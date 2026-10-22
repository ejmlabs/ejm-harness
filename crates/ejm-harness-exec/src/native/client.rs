use std::{
    fs::{File, TryLockError},
    process::Stdio,
    sync::Arc,
    time::Duration,
};

use anyhow::{Context, Result, bail, ensure};
use futures_util::{SinkExt, StreamExt};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::{Notify, watch},
    time::Instant,
};
use tokio_util::sync::CancellationToken;

use super::{
    HELPER_ARG, NativeChild, NativeCleanupState, NativeCommand, NativeOperation,
    NativeOperationState, NativeOwner, NativeOwnerReady, NativeOwnerRequest, Readiness,
    RemoteChild, recovery,
    wire::{self, Action, Advertisement, Completion, Hello, Launch, Reply, Transport},
};
use crate::governor::lock;

pub(super) const DOMAIN_CHECK_INTERVAL: Duration = Duration::from_millis(250);

pub(super) async fn select(owner: &NativeOwner) -> Result<NativeOwner> {
    let Some(selection) = &owner.selection else {
        return Ok(owner.clone());
    };
    if let Some(selected) = recently_selected(owner) {
        return Ok(selected);
    }
    let _prepare = selection.prepare.lock().await;
    if let Some(selected) = recently_selected(owner) {
        return Ok(selected);
    }
    let admission = owner
        .admission
        .clone()
        .context("native domain selection has no admission adapter")?;
    let domain = tokio::task::spawn_blocking(move || admission.domain_key())
        .await
        .context("select current native execution domain")??;
    let mut state = lock(&selection.state);
    if let Some(domain) = domain
        && domain != state.client.domain
    {
        state.client = NativeOwner::new(&state.client.root, domain)?.inner;
    }
    state.checked = Some(Instant::now());
    Ok(owner.fixed(state.client.clone()))
}

fn recently_selected(owner: &NativeOwner) -> Option<NativeOwner> {
    let selection = owner.selection.as_ref()?;
    let state = lock(&selection.state);
    state
        .checked
        .is_some_and(|checked| checked.elapsed() < DOMAIN_CHECK_INTERVAL)
        .then(|| owner.fixed(state.client.clone()))
}

async fn advertisement(owner: &NativeOwner) -> Result<Advertisement> {
    let cached = lock(&owner.inner.cached).clone();
    if let Some(ad) = cached {
        return Ok(ad);
    }
    let _startup = owner.inner.startup.lock().await;
    let cached = lock(&owner.inner.cached).clone();
    if let Some(ad) = cached {
        return Ok(ad);
    }
    let root = owner.inner.root.clone();
    let key = owner.inner.domain.clone();
    let (directory, startup) = tokio::task::spawn_blocking(move || -> Result<_> {
        let directory = wire::directory(&root, &key)?;
        let startup = wire::stable_lock(&directory, "startup.lock")?;
        Ok((directory, startup))
    })
    .await??;
    // A peer can still be waiting for helper admission. The caller bounds this
    // pre-launch wait; transport deadlines do not limit startup-lock contention.
    loop {
        match startup.try_lock() {
            Ok(()) => break,
            Err(TryLockError::WouldBlock) => {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            Err(TryLockError::Error(error)) => return Err(error.into()),
        }
    }
    // No command has been submitted. An idle helper may retire while its
    // launcher is unscheduled; retry that verified clean exit only once.
    for _ in 0..2 {
        let ownership = wire::stable_lock(&directory, "owner.lock")?;
        loop {
            match ownership.try_lock() {
                Ok(()) => break,
                Err(TryLockError::WouldBlock) => {}
                Err(TryLockError::Error(error)) => return Err(error.into()),
            }
            let existing = directory
                .read_opt(wire::DISCOVERY, 4096)?
                .map(|bytes| serde_json::from_slice::<Advertisement>(&bytes))
                .transpose()?;
            if let Some(ad) = &existing {
                ensure!(
                    ad.domain == owner.inner.domain,
                    "native owner domain mismatch"
                );
                if wire::connect(ad).await.is_ok() {
                    require_accounted(owner, ad)?;
                    cache(owner, ad);
                    return Ok(ad.clone());
                }
            }
            match ownership.try_lock() {
                Ok(()) => break,
                Err(TryLockError::WouldBlock) => {
                    ensure!(
                        existing.is_none_or(|ad| ad.protocol == wire::PROTOCOL),
                        "native owner protocol mismatch"
                    );
                    // Idle retirement closes the endpoint before runtime teardown
                    // releases ownership. Only contact or real release ends this
                    // pre-launch wait, bounded by the caller.
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
                Err(TryLockError::Error(error)) => return Err(error.into()),
            }
        }
        if let Some(ad) = start_helper(owner, &directory, ownership).await? {
            cache(owner, &ad);
            return Ok(ad);
        }
    }
    bail!("native execution helper repeatedly retired before readiness")
}

async fn start_helper(
    owner: &NativeOwner,
    directory: &rightkit_fs::StateDir,
    ownership: File,
) -> Result<Option<Advertisement>> {
    let generation = uuid::Uuid::new_v4();
    let admission = owner.admission.clone();
    let request = NativeOwnerRequest::new(
        owner.inner.root.clone(),
        owner.inner.domain.clone(),
        generation,
        directory.path().join("owner.lock"),
    );
    let (ownership, accounted) = tokio::task::spawn_blocking(move || -> Result<_> {
        let accounted = if let Some(admission) = admission {
            let permit = admission.admit(&request)?;
            ensure!(
                permit.generation == generation,
                "native helper admission generation mismatch"
            );
            true
        } else {
            false
        };
        // Cancellation may discard this result. Closing the locked description
        // then supplies recovery proof; the permit itself never refunds a row.
        Ok((ownership, accounted))
    })
    .await
    .context("admit native execution helper")??;
    // Shared admission has its own bounded wait. Give process startup a fresh
    // transport window after that commitment, within the caller's deadline.
    let deadline = Instant::now() + wire::TIMEOUT;
    let executable = owner
        .helper_executable
        .clone()
        .map_or_else(std::env::current_exe, Ok)?;
    let mut helper_command = tokio::process::Command::new(executable);
    helper_command
        .arg(HELPER_ARG)
        .arg(&owner.inner.root)
        .arg(&owner.inner.domain)
        .arg(generation.to_string())
        .arg(if accounted { "admitted" } else { "local" })
        .stdin(Stdio::from(ownership.try_clone()?))
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(unix)]
    helper_command.process_group(0);
    let mut helper = helper_command
        .spawn()
        .context("start native execution owner; host must call dispatch_helper before startup")?;
    // Never unlock a transferred open file description. The child already
    // holds its duplicate, including if paused before dispatch_helper runs.
    drop(helper_command);
    drop(ownership);
    let child_pid = helper.id();
    let ad = loop {
        if let Some(status) = helper.try_wait()? {
            ensure!(
                status.success(),
                "native execution helper failed before startup ({status}); check early dispatch"
            );
            return Ok(None);
        }
        if let Some(bytes) = directory.read_opt(wire::DISCOVERY, 4096)? {
            let ad: Advertisement = serde_json::from_slice(&bytes)?;
            if Some(ad.pid) == child_pid
                && ad.epoch == generation
                && ad.accounted == accounted
                && wire::connect(&ad).await.is_ok()
            {
                break ad;
            }
        }
        ensure!(
            Instant::now() < deadline,
            "native execution owner startup timed out"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    tokio::spawn(async move {
        let _ = helper.wait().await;
    });
    Ok(Some(ad))
}

fn require_accounted(owner: &NativeOwner, ad: &Advertisement) -> Result<()> {
    ensure!(
        owner.admission.is_none() || ad.accounted,
        "live native helper has no shared capacity commitment; drain it before coordinated use"
    );
    Ok(())
}

pub(super) async fn ready(owner: &NativeOwner) -> Result<NativeOwnerReady> {
    let mut shared = owner.inner.readiness.lock().await;
    if let Some(readiness) = shared.upgrade()
        && readiness.connected()
    {
        require_accounted(owner, &readiness.advertisement)?;
        return Ok(NativeOwnerReady {
            owner: owner.clone(),
            readiness,
        });
    }
    let mut ad = advertisement(owner).await?;
    require_accounted(owner, &ad)?;
    let transport = if let Ok(transport) = hold(owner, &ad).await {
        transport
    } else {
        // This connection cannot submit a launch. Rediscovery is safe only
        // before the readiness guard and dependent leases exist.
        lock(&owner.inner.cached).take();
        ad = advertisement(owner).await?;
        require_accounted(owner, &ad)?;
        hold(owner, &ad).await?
    };
    let readiness = Arc::new(Readiness {
        advertisement: ad,
        transport: std::sync::Mutex::new(Some(transport)),
    });
    *shared = Arc::downgrade(&readiness);
    Ok(NativeOwnerReady {
        owner: owner.clone(),
        readiness,
    })
}

async fn hold(owner: &NativeOwner, ad: &Advertisement) -> Result<Transport> {
    let mut transport = handshake(owner, ad, Action::Hold).await?;
    match wire::receive(&mut transport).await? {
        Reply::Held => Ok(transport),
        Reply::Rejected(error) => bail!(error),
        _ => bail!("unexpected native readiness reply"),
    }
}

fn cache(owner: &NativeOwner, ad: &Advertisement) {
    *lock(&owner.inner.cached) = Some(ad.clone());
    let mut snapshot = lock(&owner.inner.snapshot);
    snapshot.epoch = Some(ad.epoch);
    snapshot.owner_pid = Some(ad.pid);
}

async fn handshake(owner: &NativeOwner, ad: &Advertisement, action: Action) -> Result<Transport> {
    let domain = match &action {
        Action::Prepare => Some(wire::observe_domain().await?),
        Action::Hold | Action::Query(_) => None,
    };
    let mut transport = wire::connect(ad).await?;
    wire::send(
        &mut transport,
        &Hello {
            protocol: wire::PROTOCOL,
            epoch: ad.epoch,
            capability: ad.capability,
            domain_key: owner.inner.domain.clone(),
            domain,
            action,
        },
    )
    .await?;
    transport
        .codec_mut()
        .set_max_frame_length(wire::MAX_FRAME_BYTES);
    Ok(transport)
}

pub(super) async fn reconcile(
    owner: &NativeOwner,
    operation: NativeOperation,
) -> Result<NativeOperationState> {
    // Reconciliation never starts or replaces an owner.
    let ad = lock(&owner.inner.cached)
        .clone()
        .context("native owner generation is unknown")?;
    ensure!(
        ad.epoch == operation.epoch,
        "native owner generation changed; replay forbidden"
    );
    query(owner, &ad, operation).await
}

pub(super) async fn query(
    owner: &NativeOwner,
    ad: &Advertisement,
    operation: NativeOperation,
) -> Result<NativeOperationState> {
    let mut transport = handshake(owner, ad, Action::Query(operation)).await?;
    match wire::receive(&mut transport).await? {
        Reply::State(state) => Ok(state),
        Reply::Rejected(error) => bail!(error),
        _ => bail!("unexpected native operation reply"),
    }
}

async fn prepare(owner: &NativeOwner, ad: &Advertisement) -> Result<(Transport, Reply)> {
    let mut transport = handshake(owner, ad, Action::Prepare).await?;
    let reply = wire::receive(&mut transport).await?;
    Ok((transport, reply))
}

pub(super) async fn spawn(
    ready: &NativeOwnerReady,
    mut command: NativeCommand,
) -> Result<NativeChild> {
    command.cwd = std::path::absolute(&command.cwd)?.into_os_string();
    let started = Instant::now();
    let piped_stdin = command.piped_stdin;
    let owner = ready.owner.clone();
    let ad = ready.readiness.advertisement.clone();
    let (mut transport, prepared) = prepare(&owner, &ad)
        .await
        .inspect_err(|_| ready.readiness.disconnect())?;
    let operation = match prepared {
        Reply::Prepared(operation) => operation,
        Reply::Rejected(error) => bail!(error),
        _ => bail!("unexpected native prepare reply"),
    };
    ensure!(
        operation.epoch == ad.epoch,
        "native owner generation changed"
    );
    lock(&owner.inner.snapshot).last_operation = Some(operation);
    // This is the only submission. Any subsequent failure must be reconciled
    // against this epoch and ID. Replacement never resubmits the command.
    let mut command = command;
    command.lifetime = command.lifetime.saturating_sub(started.elapsed());
    command.validate()?;
    let pid = match submit(&mut transport, operation, command).await {
        Ok(pid) => pid,
        Err(error) => {
            // Effects may already exist. EOF first triggers cleanup, then keep
            // the caller's admission charged until evidence resolves ownership.
            drop(transport);
            ready.readiness.disconnect();
            let _ = recovery::resolve(&owner, &ad, operation).await;
            return Err(error.context("native launch failed or reply uncertain; replay forbidden"));
        }
    };
    let transport = wire::streaming(transport);
    let (stdin, input) = tokio::io::duplex(wire::PIPE_BYTES);
    let (stdout, output) = tokio::io::duplex(wire::PIPE_BYTES);
    let (stderr, errors) = tokio::io::duplex(wire::PIPE_BYTES);
    let (completion_tx, completion) = watch::channel(None);
    let cancel = CancellationToken::new();
    lock(&owner.activity).active_local_scopes += 1;
    tokio::spawn(relay(
        transport,
        RemoteScope {
            owner,
            ad,
            operation,
        },
        input,
        output,
        errors,
        cancel.clone(),
        completion_tx,
    ));
    Ok(NativeChild::Remote(RemoteChild {
        pid,
        stdin: piped_stdin.then(|| Box::new(stdin) as super::NativeWriter),
        stdout: Some(Box::new(stdout)),
        stderr: Some(Box::new(stderr)),
        completion,
        cancel,
    }))
}

async fn submit(
    transport: &mut Transport,
    operation: NativeOperation,
    command: NativeCommand,
) -> Result<u32> {
    let domain = wire::observe_domain().await?;
    wire::send(
        transport,
        &Launch {
            operation,
            command,
            domain,
        },
    )
    .await?;
    match wire::receive(transport).await? {
        Reply::Started { pid } => Ok(pid),
        Reply::Rejected(error) => bail!(error),
        _ => bail!("unexpected native launch reply"),
    }
}

struct RemoteScope {
    owner: NativeOwner,
    ad: Advertisement,
    operation: NativeOperation,
}

async fn relay(
    transport: Transport,
    scope: RemoteScope,
    mut input: tokio::io::DuplexStream,
    mut output: tokio::io::DuplexStream,
    mut errors: tokio::io::DuplexStream,
    lifetime_cancel: CancellationToken,
    completion_tx: watch::Sender<Option<Completion>>,
) {
    let (mut writer, mut reader) = transport.split();
    let acknowledgement = std::sync::Arc::new(Notify::new());
    let input_ack = acknowledgement.clone();
    let write_cancel = lifetime_cancel.clone();
    let sender = tokio::spawn(async move {
        let mut bytes = [0; wire::PIPE_BYTES];
        loop {
            let packet = tokio::select! {
                biased;
                () = write_cancel.cancelled() => {
                    let _ = writer.send(wire::packet(b'K',&[])).await;
                    return;
                },
                count = input.read(&mut bytes) => match count {
                    Ok(0) => { let _ = writer.send(wire::packet(b'C',&[])).await; break; },
                    Ok(count) => wire::packet(b'I',&bytes[..count]), Err(_) => break,
                },
            };
            if writer.send(packet).await.is_err() {
                return;
            }
            tokio::select! {
                biased;
                ()=write_cancel.cancelled()=>{let _=writer.send(wire::packet(b'K',&[])).await;return;},
                ()=input_ack.notified()=>{},
            }
        }
        write_cancel.cancelled().await;
        let _ = writer.send(wire::packet(b'K', &[])).await;
    });
    let mut finished = None;
    while let Some(Ok(packet)) = reader.next().await {
        let Some((&tag, data)) = packet.split_first() else {
            break;
        };
        match tag {
            b'O' => {
                tokio::select! { biased; ()=lifetime_cancel.cancelled()=>{}, result=output.write_all(data)=>{if result.is_err(){lifetime_cancel.cancel();}} }
            }
            b'E' => {
                tokio::select! { biased; ()=lifetime_cancel.cancelled()=>{}, result=errors.write_all(data)=>{if result.is_err(){lifetime_cancel.cancel();}} }
            }
            b'A' if data.is_empty() => acknowledgement.notify_one(),
            b'F' => {
                finished = serde_json::from_slice::<Completion>(data)
                    .ok()
                    .filter(|completion| completion.cleanup == NativeCleanupState::Complete);
                break;
            }
            _ => break,
        }
    }
    sender.abort();
    let _ = sender.await;
    // Drop both halves before Query. Otherwise the old owner may still wait for
    // EOF on this lifetime connection and never enter its cleanup path.
    drop(reader);
    drop(output);
    drop(errors);
    let completion = if let Some(completion) = finished {
        completion
    } else {
        recovery::resolve(&scope.owner, &scope.ad, scope.operation).await
    };
    let _ = completion_tx.send(Some(completion));
    lock(&scope.owner.activity).active_local_scopes -= 1;
}
