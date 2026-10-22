use std::{
    collections::{BTreeMap, VecDeque},
    net::Ipv4Addr,
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use anyhow::{Context, Result, ensure};
use futures_util::{SinkExt, StreamExt};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::{Semaphore, mpsc},
    task::JoinSet,
    time::{Instant, sleep_until},
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::{
    NativeCleanupState, NativeOperation, NativeOperationState,
    wire::{self, Action, Advertisement, Completion, Hello, Launch, Reply, Transport},
};
use crate::{governor::lock, monitor::ProcessMonitor, process::spawn_owned};

#[derive(Default)]
struct Operations {
    next: u64,
    active: BTreeMap<u64, NativeOperationState>,
    completed: VecDeque<u64>,
}

impl Operations {
    fn finish(&mut self, id: u64) {
        self.active.remove(&id);
        self.completed.push_back(id);
        if self.completed.len() > wire::MAX_COMPLETED {
            self.completed.pop_front();
        }
    }
    fn query(&self, id: u64) -> NativeOperationState {
        self.active.get(&id).cloned().unwrap_or_else(|| {
            if self.completed.contains(&id) {
                NativeOperationState::Finished
            } else {
                NativeOperationState::Expired
            }
        })
    }
}

pub(super) async fn serve(root: &Path, key: &str, generation: Uuid, accounted: bool) -> Result<()> {
    super::placement::supported()?;
    let directory = wire::directory(root, key).context("open protected native owner state")?;
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .context("bind native owner loopback transport")?;
    let ad = Arc::new(Advertisement {
        protocol: wire::PROTOCOL,
        epoch: generation,
        capability: Uuid::new_v4(),
        pid: std::process::id(),
        port: listener.local_addr()?.port(),
        domain: key.to_owned(),
        accounted,
    });
    directory.write(wire::DISCOVERY, &serde_json::to_vec(ad.as_ref())?)?;
    let operations = Arc::new(Mutex::new(Operations::default()));
    let monitor = Arc::new(ProcessMonitor::default());
    let handshakes = Arc::new(Semaphore::new(wire::MAX_HANDSHAKES));
    let ready_clients = Arc::new(Semaphore::new(wire::MAX_READY_CLIENTS));
    let scopes = Arc::new(Semaphore::new(wire::MAX_SCOPES));
    let mut clients = JoinSet::new();
    let mut idle = Instant::now() + wire::IDLE;
    loop {
        tokio::select! {
            biased;
            result = clients.join_next(), if !clients.is_empty() => {
                let _ = result; idle = Instant::now()+wire::IDLE;
            },
            () = sleep_until(idle), if clients.is_empty() => break,
            accepted = listener.accept() => {
                let (stream,peer) = accepted?;
                if !peer.ip().is_loopback() { continue; }
                let Ok(permit) = handshakes.clone().try_acquire_owned() else { continue; };
                stream.set_nodelay(true)?;
                let ad = ad.clone(); let operations=operations.clone(); let monitor=monitor.clone(); let scopes=scopes.clone(); let ready_clients=ready_clients.clone();
                clients.spawn(async move {
                    let mut transport=wire::framed(stream);
                    wire::authenticate_server(&mut transport, &ad).await?;
                    let hello:Hello=wire::receive(&mut transport).await?;
                    ensure!(hello.protocol==wire::PROTOCOL && hello.epoch==ad.epoch && hello.capability==ad.capability && hello.domain_key==ad.domain,"native authentication or generation mismatch");
                    transport.codec_mut().set_max_frame_length(wire::MAX_FRAME_BYTES);
                    drop(permit);
                    match hello.action {
                        Action::Hold => {
                            let Ok(_ready) = ready_clients.try_acquire_owned() else { return wire::send(&mut transport,&Reply::Rejected("native client readiness limit reached".into())).await; };
                            wire::send(&mut transport,&Reply::Held).await?;
                            // The authenticated client connection pins this
                            // generation without per-agent tasks or polling.
                            let _ = transport.next().await;
                            Ok(())
                        },
                        Action::Query(operation) => {
                            ensure!(operation.epoch==ad.epoch,"native operation generation mismatch");
                            let state=lock(&operations).query(operation.id);
                            wire::send(&mut transport,&Reply::State(state)).await
                        },
                        Action::Prepare => {
                            let domain=hello.domain.context("native launch placement is missing")?;
                            if domain!=wire::observe_domain().await? {
                                return wire::send(&mut transport,&Reply::Rejected("native execution domain mismatch; placement cannot be preserved".into())).await;
                            }
                            let Ok(_scope) = scopes.try_acquire_owned() else { return wire::send(&mut transport,&Reply::Rejected("native scope limit reached".into())).await; };
                            let id={ let mut state=lock(&operations); state.next=state.next.checked_add(1).context("native operation identity exhausted")?; let id=state.next; state.active.insert(id,NativeOperationState::Prepared); id };
                            let operation=NativeOperation {epoch:ad.epoch,id};
                            let result=run(transport,operation,&domain,&operations,&monitor).await;
                            lock(&operations).finish(id);
                            result
                        }
                    }
                });
            }
        }
    }
    // dispatch_helper retains the inherited lifetime witness through runtime
    // teardown, after every readiness connection and native scope has drained.
    Ok(())
}

async fn run(
    mut transport: Transport,
    operation: NativeOperation,
    domain: &wire::Domain,
    operations: &Mutex<Operations>,
    monitor: &Arc<ProcessMonitor>,
) -> Result<()> {
    wire::send(&mut transport, &Reply::Prepared(operation)).await?;
    let launch: Launch = wire::receive(&mut transport).await?;
    ensure!(
        launch.operation == operation,
        "native launch operation mismatch"
    );
    launch.command.validate()?;
    ensure!(
        launch.domain == *domain && launch.domain == wire::observe_domain().await?,
        "native execution domain changed before launch; placement cannot be preserved"
    );
    let command = launch.command;
    let mut child = match spawn_owned(command.local_command()) {
        Ok(child) => child,
        Err(error) => {
            return wire::send(
                &mut transport,
                &Reply::Rejected(format!("native process launch failed: {error}")),
            )
            .await;
        }
    };
    let (lifetime, reservation) = (command.lifetime, command.reservation.clone());
    drop(command);
    lock(operations)
        .active
        .insert(operation.id, NativeOperationState::Running);
    let pid = child.id().context("native child PID missing")?;
    let cancel = CancellationToken::new();
    let observed = monitor.register(pid, reservation, cancel.clone());
    // Any failed acknowledgement closes the private scope link and takes the
    // same cleanup path as a client crash. There is never another spawn.
    let connected = wire::send(&mut transport, &Reply::Started { pid })
        .await
        .is_ok();
    let ScopePipes {
        mut writer,
        mut frames,
        mut pipes,
        mut controls,
        live,
    } = connect_pipes(&mut child, wire::streaming(transport), connected, &cancel);
    let deadline = Instant::now() + lifetime;
    let mut status = None;
    if connected {
        loop {
            tokio::select! {
                biased;
                ()=cancel.cancelled()=>break,
                ()=sleep_until(deadline)=>break,
                result=child.wait()=>{status=Some(result);break;},
                frame=frames.recv(), if !frames.is_closed() || !frames.is_empty()=>{
                    if let Some(frame)=frame {
                        let result=tokio::select! { ()=cancel.cancelled()=>break, ()=sleep_until(deadline)=>break, result=tokio::time::timeout(wire::TIMEOUT,writer.send(frame))=>result };
                        if !matches!(result,Ok(Ok(()))) {live.store(false,Ordering::Relaxed);break;}
                    }
                }
            }
        }
    }
    let cleanup = finish_child(&mut child).await;
    observed.kill_known_descendants().await;
    controls.abort_all();
    while controls.join_next().await.is_some() {}
    let usage = observed.usage();
    let status = status.unwrap_or(Ok(cleanup));
    // Keep a slow or dead consumer from retaining output buffers or scope state.
    let drain_deadline = Instant::now() + Duration::from_secs(1);
    while live.load(Ordering::Relaxed) && Instant::now() < drain_deadline {
        let frame = tokio::time::timeout_at(drain_deadline, frames.recv()).await;
        match frame {
            Ok(Some(frame)) => {
                if !matches!(
                    tokio::time::timeout_at(drain_deadline, writer.send(frame)).await,
                    Ok(Ok(()))
                ) {
                    break;
                }
            }
            _ => break,
        }
    }
    pipes.abort_all();
    while pipes.join_next().await.is_some() {}
    let error = status.as_ref().err().map(ToString::to_string);
    let completion = Completion {
        cleanup: NativeCleanupState::Complete,
        raw_status: status.as_ref().ok().map(|status| wire::raw_status(*status)),
        error: error.or_else(|| usage.sampling_error()),
        peak_memory_bytes: usage.peak_memory_bytes,
        peak_processes: usage.peak_processes,
        violation: usage.violation.map(wire::Violation::from_message),
    };
    if live.load(Ordering::Relaxed) {
        let _ = tokio::time::timeout(
            wire::TIMEOUT,
            writer.send(wire::packet(b'F', &serde_json::to_vec(&completion)?)),
        )
        .await;
    }
    Ok(())
}

async fn read_pipe(
    mut reader: impl AsyncRead + Unpin,
    tag: u8,
    frames: mpsc::Sender<bytes::Bytes>,
) -> Result<()> {
    let mut buffer = [0; wire::PIPE_BYTES];
    loop {
        let count = reader.read(&mut buffer).await?;
        if count == 0 {
            return Ok(());
        }
        if frames
            .send(wire::packet(tag, &buffer[..count]))
            .await
            .is_err()
        {
            return Ok(());
        }
    }
}

async fn finish_child(child: &mut crate::process::OwnedChild) -> std::process::ExitStatus {
    loop {
        if let Ok(status) = child.kill().await {
            return status;
        }
        // A failed native query is not evidence of cleanup. Keep the scope,
        // reservation, and pipes owned while termination is retried.
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

struct ScopePipes {
    writer: futures_util::stream::SplitSink<Transport, bytes::Bytes>,
    frames: mpsc::Receiver<bytes::Bytes>,
    pipes: JoinSet<Result<()>>,
    controls: JoinSet<Result<()>>,
    live: Arc<AtomicBool>,
}

fn connect_pipes(
    child: &mut crate::process::OwnedChild,
    transport: Transport,
    connected: bool,
    cancel: &CancellationToken,
) -> ScopePipes {
    let (outgoing, frames) = mpsc::channel::<bytes::Bytes>(4);
    let mut pipes = JoinSet::new();
    let mut controls: JoinSet<Result<()>> = JoinSet::new();
    if let Some(stdout) = child.take_stdout() {
        pipes.spawn(read_pipe(stdout, b'O', outgoing.clone()));
    }
    if let Some(stderr) = child.take_stderr() {
        pipes.spawn(read_pipe(stderr, b'E', outgoing.clone()));
    }
    let (writer, mut reader) = transport.split();
    let (input, mut inputs) = mpsc::channel::<bytes::BytesMut>(1);
    let input_cancel = cancel.clone();
    let live = Arc::new(AtomicBool::new(connected));
    let read_live = live.clone();
    controls.spawn(async move {
        let mut input = Some(input);
        while let Some(Ok(frame)) = reader.next().await {
            match frame.first() {
                Some(b'I') if frame.len() <= wire::PIPE_BYTES + 1 => {
                    if input
                        .as_ref()
                        .is_none_or(|input| input.try_send(frame).is_err())
                    {
                        break;
                    }
                }
                Some(b'C') if frame.len() == 1 => {
                    input = None;
                }
                Some(b'K') if frame.len() == 1 => {
                    input_cancel.cancel();
                    return Ok(());
                }
                _ => break,
            }
        }
        read_live.store(false, Ordering::Relaxed);
        input_cancel.cancel();
        Ok(())
    });
    if let Some(mut stdin) = child.take_stdin() {
        let acknowledgements = outgoing.clone();
        controls.spawn(async move {
            while let Some(frame) = inputs.recv().await {
                if stdin.write_all(&frame[1..]).await.is_err() {
                    break;
                }
                if acknowledgements
                    .send(wire::packet(b'A', &[]))
                    .await
                    .is_err()
                {
                    break;
                }
            }
            Ok(())
        });
    }
    drop(outgoing);
    ScopePipes {
        writer,
        frames,
        pipes,
        controls,
        live,
    }
}
