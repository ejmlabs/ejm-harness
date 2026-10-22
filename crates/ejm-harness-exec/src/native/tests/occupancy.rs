//! Opt-in control-state saturation against an actual helper, without commands.

use std::{
    collections::HashSet,
    fs::{File, TryLockError},
    io,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result, bail, ensure};
use futures_util::{StreamExt, stream};
use rustix::process::{Pid, Resource, getrlimit, test_kill_process};
use tokio::time::{Instant, sleep, timeout, timeout_at};

use super::super::{
    NativeOperation, NativeOperationState, NativeOwner, NativeOwnerReady,
    wire::{self, Action, Advertisement, Domain, Hello, Reply, Transport},
};
use crate::{governor::lock, processes::Sampler};

const HELPER_ENV: &str = "EJM_NATIVE_OCCUPANCY_HELPER";
const PIPELINE: usize = 16;
const SATURATION_WINDOW: Duration = Duration::from_secs(4);
const CLEANUP_WINDOW: Duration = Duration::from_secs(8);

struct Fixture {
    owner: NativeOwner,
    advertisement: Advertisement,
    domain: Domain,
    ready: Option<NativeOwnerReady>,
    holds: Vec<Transport>,
    scopes: Vec<(NativeOperation, Transport)>,
    stalled: Vec<Transport>,
    operations: Vec<NativeOperation>,
    birth: u64,
    started: Instant,
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires EJM_NATIVE_OCCUPANCY_HELPER and room for 1312 control connections"]
async fn real_helper_control_state_saturation() -> Result<()> {
    let helper = helper_path()?;
    preflight_descriptors()?;
    let temporary = tempfile::tempdir()?;
    let root = temporary.path().canonicalize()?.join("capacity");
    let directory = wire::directory(&root, "occupancy")?;
    let witness = wire::stable_lock(&directory, "owner.lock")?;
    let owner = NativeOwner::new(&root, "occupancy")?.with_helper_executable(helper);
    let setup = timeout(Duration::from_secs(8), Fixture::start(owner))
        .await
        .context("control-state helper startup exceeded its deadline")
        .and_then(std::convert::identity);
    let mut fixture = match setup {
        Ok(fixture) => fixture,
        Err(error) => {
            let retained = temporary.keep();
            return Err(error.context(format!(
                "helper startup cleanup is unverified; retained {}",
                retained.display()
            )));
        }
    };
    let result = timeout(Duration::from_secs(25), fixture.exercise())
        .await
        .context("control-state fixture exceeded its overall deadline")
        .and_then(std::convert::identity);
    let cleanup = fixture.cleanup(&witness).await;
    if let Err(error) = cleanup {
        let retained = temporary.keep();
        bail!(
            "control-state result: {result:?}; cleanup unverified: {error:#}; retained {}",
            retained.display()
        );
    }
    result
}

fn helper_path() -> Result<PathBuf> {
    let path = PathBuf::from(std::env::var_os(HELPER_ENV).with_context(|| {
        format!(
            "set {HELPER_ENV} to the absolute path of a test helper implementing dispatch_helper"
        )
    })?);
    ensure!(path.is_absolute(), "{HELPER_ENV} must be absolute");
    let metadata = path.metadata().context("inspect explicit test helper")?;
    ensure!(
        metadata.is_file() && metadata.permissions().mode() & 0o111 != 0,
        "{HELPER_ENV} must name an executable file"
    );
    path.canonicalize().context("resolve explicit test helper")
}

fn preflight_descriptors() -> Result<()> {
    let directory = if cfg!(target_os = "linux") {
        Path::new("/proc/self/fd")
    } else {
        Path::new("/dev/fd")
    };
    let open =
        std::fs::read_dir(directory)?.try_fold(0, |count, entry| entry.map(|_| count + 1))?;
    let required = u64::try_from(
        open + wire::MAX_READY_CLIENTS + wire::MAX_SCOPES + wire::MAX_HANDSHAKES + 128,
    )?;
    let limit = getrlimit(Resource::Nofile).current;
    ensure!(
        limit.is_none_or(|limit| limit >= required),
        "control-state fixture needs a descriptor limit of at least {required}"
    );
    println!(
        "{}",
        serde_json::json!({
            "native_owner_occupancy": "descriptor_preflight",
            "client_pid": std::process::id(),
            "descriptor_soft_limit": limit,
            "open_descriptor_entries": open,
            "required_descriptors": required,
        })
    );
    Ok(())
}

impl Fixture {
    async fn start(owner: NativeOwner) -> Result<Self> {
        let started = Instant::now();
        let domain = wire::observe_domain().await?;
        let ready = owner.ready().await?;
        let advertisement = lock(&owner.inner.cached)
            .clone()
            .context("started helper has no cached advertisement")?;
        ensure!(
            advertisement.epoch == ready.generation(),
            "helper epoch changed"
        );
        let process = Sampler::default().observe(advertisement.pid)?;
        ensure!(!process.zombie, "started helper is already a zombie");
        let fixture = Self {
            owner,
            advertisement,
            domain,
            ready: Some(ready),
            holds: Vec::with_capacity(wire::MAX_READY_CLIENTS - 1),
            scopes: Vec::with_capacity(wire::MAX_SCOPES),
            stalled: Vec::with_capacity(wire::MAX_HANDSHAKES),
            operations: Vec::with_capacity(wire::MAX_SCOPES + 2),
            birth: process.start,
            started,
        };
        fixture.stage("started", 0, 0);
        Ok(fixture)
    }

    fn stage(&self, name: &str, completed: usize, proven_handshakes: usize) {
        println!(
            "{}",
            serde_json::json!({
                "native_owner_occupancy": name,
                "client_pid": std::process::id(),
                "helper_pid": self.advertisement.pid,
                "epoch": self.advertisement.epoch,
                "helper_birth_native": self.birth,
                "elapsed_ms": self.started.elapsed().as_millis(),
                "ready_clients": self.holds.len() + usize::from(self.ready.is_some()),
                "prepared_scopes": self.scopes.len(),
                "completed_capacity_verified": completed,
                "stalled_connections": self.stalled.len(),
                "proven_handshakes": proven_handshakes,
            })
        );
    }

    async fn exercise(&mut self) -> Result<()> {
        let newest = self.completed_fifo().await?;
        self.fill_readiness(newest).await?;
        timeout(SATURATION_WINDOW, self.saturate_scopes_and_handshakes())
            .await
            .context("occupancy was not proved before the five-second production deadline")??;
        self.drain_scopes().await?;
        self.stage("scopes_drained", wire::MAX_COMPLETED, wire::MAX_HANDSHAKES);
        drop(self.holds.pop());
        self.holds.push(wait_hold(&self.advertisement).await?);
        let (operation, transport) = prepare(&self.advertisement, &self.domain).await?;
        self.operations.push(operation);
        drop(transport);
        wait_terminal(&self.owner, operation, false).await?;
        self.operations.clear();
        self.stage(
            "fresh_control_usable",
            wire::MAX_COMPLETED,
            wire::MAX_HANDSHAKES,
        );
        Ok(())
    }

    async fn completed_fifo(&self) -> Result<NativeOperation> {
        let mut first = None;
        let mut newest = None;
        for index in 0..=wire::MAX_COMPLETED {
            let (operation, transport) = prepare(&self.advertisement, &self.domain).await?;
            ensure!(
                operation.epoch == self.advertisement.epoch,
                "wrong operation epoch"
            );
            ensure!(
                newest.is_none_or(|previous: NativeOperation| operation.id > previous.id),
                "operation IDs did not increase"
            );
            first.get_or_insert(operation);
            newest = Some(operation);
            drop(transport);
            wait_terminal(&self.owner, operation, false).await?;
            if index + 1 == wire::MAX_COMPLETED {
                ensure!(
                    self.owner
                        .reconcile(first.context("missing oldest ID")?)
                        .await?
                        == NativeOperationState::Finished,
                    "oldest ID expired before completed capacity"
                );
                self.stage("completed_full", wire::MAX_COMPLETED, 0);
            }
        }
        ensure!(
            self.owner
                .reconcile(first.context("missing oldest ID")?)
                .await?
                == NativeOperationState::Expired,
            "completed ring did not evict its oldest finished ID"
        );
        let newest = newest.context("missing newest ID")?;
        ensure!(
            self.owner.reconcile(newest).await? == NativeOperationState::Finished,
            "completed ring lost its newest finished ID"
        );
        self.stage("completed_eviction_proven", wire::MAX_COMPLETED, 0);
        Ok(newest)
    }

    async fn fill_readiness(&mut self, newest: NativeOperation) -> Result<()> {
        let advertisement = &self.advertisement;
        let mut pending = stream::iter(0..wire::MAX_READY_CLIENTS - 1)
            .map(|_| exchange(advertisement, None, Action::Hold))
            .buffer_unordered(PIPELINE);
        while let Some(result) = pending.next().await {
            let (transport, reply) = result?;
            ensure!(matches!(reply, Reply::Held), "readiness pool filled early");
            self.holds.push(transport);
        }
        let (_, reply) = exchange(advertisement, None, Action::Hold).await?;
        ensure!(
            matches!(reply, Reply::Rejected(_)),
            "readiness overflow was accepted"
        );
        ensure!(
            self.owner.reconcile(newest).await? == NativeOperationState::Finished,
            "query failed while readiness was full"
        );
        drop(self.holds.pop());
        self.holds.push(wait_hold(advertisement).await?);
        self.stage("readiness_full_and_reused", wire::MAX_COMPLETED, 0);
        Ok(())
    }

    async fn saturate_scopes_and_handshakes(&mut self) -> Result<()> {
        let advertisement = &self.advertisement;
        let domain = &self.domain;
        let mut pending = stream::iter(0..wire::MAX_SCOPES)
            .map(|_| prepare(advertisement, domain))
            .buffer_unordered(PIPELINE);
        let mut unique = HashSet::with_capacity(wire::MAX_SCOPES);
        while let Some(result) = pending.next().await {
            let (operation, transport) = result?;
            ensure!(
                operation.epoch == advertisement.epoch,
                "scope epoch changed"
            );
            ensure!(unique.insert(operation.id), "duplicate operation ID");
            self.operations.push(operation);
            self.scopes.push((operation, transport));
        }
        ensure!(
            self.scopes.len() == wire::MAX_SCOPES,
            "scope count is unproved"
        );
        let (_, reply) = exchange(advertisement, Some(domain.clone()), Action::Prepare).await?;
        ensure!(
            matches!(reply, Reply::Rejected(_)),
            "scope overflow was accepted"
        );
        drop(pending);
        self.check_scope_extremes().await?;
        self.stage("scopes_full", wire::MAX_COMPLETED, 0);
        self.prove_handshakes().await?;

        let (released, transport) = self.scopes.pop().context("missing scope to replace")?;
        drop(transport);
        wait_terminal(&self.owner, released, false).await?;
        let (replacement, transport) = prepare(&self.advertisement, &self.domain).await?;
        ensure!(unique.insert(replacement.id), "reused operation ID");
        ensure!(
            replacement.epoch == self.advertisement.epoch,
            "replacement epoch changed"
        );
        self.operations.push(replacement);
        self.scopes.push((replacement, transport));
        self.check_scope_extremes().await?;
        self.stage(
            "scope_slot_reused",
            wire::MAX_COMPLETED,
            wire::MAX_HANDSHAKES,
        );
        Ok(())
    }

    async fn prove_handshakes(&mut self) -> Result<()> {
        let advertisement = &self.advertisement;
        for _ in 0..wire::MAX_HANDSHAKES {
            self.stalled.push(wire::connect(advertisement).await?);
        }
        self.stage("handshake_connections_pending", wire::MAX_COMPLETED, 0);
        require_handshake_disconnect(advertisement).await?;
        self.stage("handshake_overflow_disconnected", wire::MAX_COMPLETED, 0);
        // Let the external PID observer sample this short plateau within the real deadline.
        sleep(Duration::from_millis(100)).await;
        let mut authentication = stream::iter(self.stalled.drain(..))
            .map(|mut transport| async move {
                wire::send(&mut transport, &hello(advertisement, None, Action::Hold)).await?;
                let reply = wire::receive::<Reply>(&mut transport).await?;
                ensure!(
                    matches!(reply, Reply::Rejected(_)),
                    "stalled Hello was not authenticated"
                );
                Ok::<(), anyhow::Error>(())
            })
            .buffer_unordered(PIPELINE);
        let mut accepted = 0;
        while let Some(result) = authentication.next().await {
            result?;
            accepted += 1;
        }
        ensure!(
            accepted == wire::MAX_HANDSHAKES,
            "accepted handshake count changed"
        );
        drop(authentication);
        self.check_scope_extremes().await?;
        self.stage("handshake_limit_proven", wire::MAX_COMPLETED, accepted);
        Ok(())
    }

    async fn check_scope_extremes(&self) -> Result<()> {
        for operation in [
            self.scopes
                .iter()
                .map(|(operation, _)| operation)
                .min_by_key(|operation| operation.id),
            self.scopes
                .iter()
                .map(|(operation, _)| operation)
                .max_by_key(|operation| operation.id),
        ] {
            ensure!(
                self.owner
                    .reconcile(*operation.context("empty scope pool")?)
                    .await?
                    == NativeOperationState::Prepared,
                "a scope expired before exact saturation could be proved"
            );
        }
        Ok(())
    }

    async fn drain_scopes(&mut self) -> Result<()> {
        self.scopes.clear();
        let mut pending = stream::iter(self.operations.iter().copied())
            .map(|operation| wait_terminal(&self.owner, operation, true))
            .buffer_unordered(PIPELINE);
        timeout(Duration::from_secs(5), async {
            while let Some(result) = pending.next().await {
                result?;
            }
            Ok::<(), anyhow::Error>(())
        })
        .await
        .context("accepted scopes did not drain")??;
        drop(pending);
        self.operations.clear();
        Ok(())
    }

    async fn cleanup(&mut self, witness: &File) -> Result<()> {
        self.stalled.clear();
        self.holds.clear();
        self.scopes.clear();
        let drained = self.drain_scopes().await;
        drop(self.ready.take());
        self.stage("links_dropped", 0, 0);
        let exit = timeout(CLEANUP_WINDOW, async {
            loop {
                match witness.try_lock() {
                    Ok(()) => break,
                    Err(TryLockError::WouldBlock) => sleep(Duration::from_millis(10)).await,
                    Err(TryLockError::Error(error)) => return Err(error.into()),
                }
            }
            wait_identity_exit(self.advertisement.pid, self.birth).await
        })
        .await
        .context("helper lifetime witness or exact identity did not end")?;
        exit?;
        self.stage("helper_exit_proven", 0, 0);
        drained
    }
}

fn hello(advertisement: &Advertisement, domain: Option<Domain>, action: Action) -> Hello {
    Hello {
        protocol: wire::PROTOCOL,
        epoch: advertisement.epoch,
        capability: advertisement.capability,
        domain_key: advertisement.domain.clone(),
        domain,
        action,
    }
}

async fn exchange(
    advertisement: &Advertisement,
    domain: Option<Domain>,
    action: Action,
) -> Result<(Transport, Reply)> {
    let mut transport = wire::connect(advertisement).await?;
    wire::send(&mut transport, &hello(advertisement, domain, action)).await?;
    let reply = wire::receive(&mut transport).await?;
    Ok((transport, reply))
}

async fn prepare(
    advertisement: &Advertisement,
    domain: &Domain,
) -> Result<(NativeOperation, Transport)> {
    let (transport, reply) = exchange(advertisement, Some(domain.clone()), Action::Prepare).await?;
    match reply {
        Reply::Prepared(operation) => Ok((operation, transport)),
        Reply::Rejected(reason) => bail!("native Prepare rejected: {reason}"),
        _ => bail!("native Prepare returned an unexpected reply"),
    }
}

async fn require_handshake_disconnect(advertisement: &Advertisement) -> Result<()> {
    let mut transport = wire::connect(advertisement).await?;
    timeout(Duration::from_millis(300), async {
        if let Err(error) =
            wire::send(&mut transport, &hello(advertisement, None, Action::Hold)).await
        {
            if error
                .chain()
                .any(|cause| cause.downcast_ref::<io::Error>().is_some_and(is_disconnect))
            {
                return Ok(());
            }
            return Err(error.context("overflow Hello failed without a proved disconnect"));
        }
        match transport.next().await {
            None => Ok(()),
            Some(Err(error)) if is_disconnect(&error) => Ok(()),
            Some(Err(error)) => Err(error.into()),
            Some(Ok(_)) => bail!("33rd Hello received a frame; handshake occupancy is unproved"),
        }
    })
    .await
    .context("33rd Hello did not disconnect before its proof deadline")?
}

fn is_disconnect(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::BrokenPipe
            | io::ErrorKind::ConnectionAborted
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::NotConnected
            | io::ErrorKind::UnexpectedEof
    )
}

async fn wait_hold(advertisement: &Advertisement) -> Result<Transport> {
    let deadline = Instant::now() + Duration::from_secs(1);
    timeout_at(deadline, async {
        loop {
            let (transport, reply) = exchange(advertisement, None, Action::Hold).await?;
            match reply {
                Reply::Held => return Ok(transport),
                Reply::Rejected(_) => sleep(Duration::from_millis(5)).await,
                _ => bail!("native Hold returned an unexpected reply"),
            }
        }
    })
    .await
    .context("closed readiness slot was not reusable")?
}

async fn wait_terminal(
    owner: &NativeOwner,
    operation: NativeOperation,
    expired: bool,
) -> Result<()> {
    timeout(Duration::from_secs(2), async {
        loop {
            match owner.reconcile(operation).await? {
                NativeOperationState::Finished => return Ok(()),
                NativeOperationState::Expired if expired => return Ok(()),
                NativeOperationState::Prepared => sleep(Duration::from_millis(2)).await,
                state => bail!("unexpected terminal state for {operation:?}: {state:?}"),
            }
        }
    })
    .await
    .context("closed prepared scope did not reach a terminal state")?
}

async fn wait_identity_exit(pid: u32, birth: u64) -> Result<()> {
    let native_pid = Pid::from_raw(i32::try_from(pid)?).context("invalid helper PID")?;
    let mut sampler = Sampler::default();
    loop {
        if let Ok(process) = sampler.observe(pid)
            && process.start != birth
        {
            return Ok(());
        }
        if test_kill_process(native_pid) == Err(rustix::io::Errno::SRCH) {
            return Ok(());
        }
        sleep(Duration::from_millis(10)).await;
    }
}
