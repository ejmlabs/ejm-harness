use std::{collections::HashMap, process::Stdio, sync::Arc, time::Duration};

use anyhow::{Context, Result, anyhow, ensure};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::{
    process::{Child, Command},
    sync::{OwnedSemaphorePermit, Semaphore, mpsc, oneshot, watch},
    task::JoinHandle,
    time::Instant,
};
use tokio_util::{
    codec::{FramedRead, FramedWrite, LinesCodec},
    sync::CancellationToken,
};

use crate::runtime::{RuntimeConfig, RuntimeEvent, RuntimeKind};

struct Frame {
    line: String,
    _bytes: OwnedSemaphorePermit,
}

struct Pending {
    reply: oneshot::Sender<Result<Value>>,
    events: Option<mpsc::Sender<RuntimeEvent>>,
    cancel: CancellationToken,
    deadline: Instant,
    cancelled_at: Option<Instant>,
    failure: Option<String>,
    event_count: usize,
    event_bytes: usize,
    _slot: OwnedSemaphorePermit,
    _retained: Box<dyn Send>,
}

struct Request {
    id: String,
    method: &'static str,
    params: Value,
    pending: Pending,
}

#[derive(Deserialize)]
#[serde(untagged, deny_unknown_fields)]
enum Incoming {
    Success { id: String, result: Value },
    Failure { id: String, error: Failure },
    Notification(Notice),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Failure {
    message: String,
}

#[derive(Deserialize)]
#[serde(tag = "method", content = "params", rename_all = "snake_case")]
enum Notice {
    Progress { id: String, message: String },
    Delta { id: String, text: String },
}

pub(crate) struct Host {
    requests: mpsc::Sender<Request>,
    slots: Arc<Semaphore>,
    closed: CancellationToken,
    done: watch::Receiver<bool>,
    config: RuntimeConfig,
}

impl Host {
    pub(crate) fn spawn(
        kind: RuntimeKind,
        config: RuntimeConfig,
        closed: CancellationToken,
    ) -> Result<Arc<Self>> {
        let mut command = match kind {
            RuntimeKind::Node => {
                let mut command = Command::new(&config.node);
                command.args([
                    "--input-type=module",
                    "--eval",
                    include_str!("../runtime/node.mjs"),
                ]);
                command
            }
            RuntimeKind::Python => {
                let mut command = Command::new(&config.python);
                command.args(["-I", "-B", "-u", "-c", include_str!("../runtime/python.py")]);
                command
            }
        };
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(unix)]
        command.process_group(0);
        let mut child = command.spawn().context("start package language host")?;
        let stdin = child
            .stdin
            .take()
            .context("language host stdin unavailable")?;
        let stdout = child
            .stdout
            .take()
            .context("language host stdout unavailable")?;
        let stderr = child
            .stderr
            .take()
            .context("language host stderr unavailable")?;
        let (requests, incoming) = mpsc::channel(config.max_pending);
        let (frames, outgoing) = mpsc::channel(config.max_pending);
        let (done_tx, done) = watch::channel(false);
        let writer_closed = closed.clone();
        let maximum = config.max_frame_bytes;
        let writer = tokio::spawn(async move {
            write_frames(stdin, outgoing, maximum, writer_closed).await;
        });
        let diagnostics = tokio::spawn(async move {
            let mut stderr = stderr;
            let _ = tokio::io::copy(&mut stderr, &mut tokio::io::sink()).await;
        });
        let host = Arc::new(Self {
            requests,
            slots: Arc::new(Semaphore::new(config.max_pending)),
            closed: closed.clone(),
            done,
            config: config.clone(),
        });
        tokio::spawn(async move {
            let actor = Actor {
                frames,
                bytes: Arc::new(Semaphore::new(config.max_buffer_bytes)),
                pending: HashMap::new(),
                config,
                closed,
            };
            actor
                .supervise(child, stdout, incoming, writer, diagnostics)
                .await;
            let _ = done_tx.send(true);
        });
        Ok(host)
    }

    pub(crate) fn stopped(&self) -> bool {
        self.closed.is_cancelled()
    }

    pub(crate) async fn call<T: Send + 'static>(
        &self,
        method: &'static str,
        params: Value,
        cancel: CancellationToken,
        events: Option<mpsc::Sender<RuntimeEvent>>,
        retained: T,
    ) -> Result<Value> {
        ensure!(
            !self.stopped(),
            "package language host is stopped; reload explicitly"
        );
        ensure!(
            !cancel.is_cancelled(),
            "package request cancelled before launch"
        );
        let slot = self
            .slots
            .clone()
            .try_acquire_owned()
            .context("package language host request capacity reached")?;
        let cancel = cancel.child_token();
        let _cancel_on_drop = CancelOnDrop(cancel.clone());
        let (reply, response) = oneshot::channel();
        let request = Request {
            id: uuid::Uuid::new_v4().to_string(),
            method,
            params,
            pending: Pending {
                reply,
                events,
                cancel,
                deadline: Instant::now() + self.config.call_timeout,
                cancelled_at: None,
                failure: None,
                event_count: 0,
                event_bytes: 0,
                _slot: slot,
                _retained: Box::new(retained),
            },
        };
        self.requests
            .try_send(request)
            .map_err(|_| anyhow!("package language host request queue unavailable"))?;
        response
            .await
            .context("package host stopped before replying")?
    }

    pub(crate) async fn shutdown(&self) -> Result<()> {
        self.closed.cancel();
        let mut done = self.done.clone();
        tokio::time::timeout(self.config.cleanup_timeout, async {
            while !*done.borrow_and_update() {
                done.changed()
                    .await
                    .context("package host supervisor stopped")?;
            }
            Ok(())
        })
        .await
        .context("package host cleanup is still pending; retain the Tokio runtime")?
    }
}

struct CancelOnDrop(CancellationToken);

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

struct Actor {
    frames: mpsc::Sender<Frame>,
    bytes: Arc<Semaphore>,
    pending: HashMap<String, Pending>,
    config: RuntimeConfig,
    closed: CancellationToken,
}

impl Actor {
    async fn supervise(
        mut self,
        mut child: Child,
        stdout: tokio::process::ChildStdout,
        mut requests: mpsc::Receiver<Request>,
        writer: JoinHandle<()>,
        diagnostics: JoinHandle<()>,
    ) {
        let mut reader = FramedRead::new(
            stdout,
            LinesCodec::new_with_max_length(self.config.max_frame_bytes),
        );
        let mut timer = tokio::time::interval(Duration::from_millis(20));
        let failure = loop {
            tokio::select! {
                () = self.closed.cancelled() => break "package language host stopped".to_owned(),
                line = reader.next() => {
                    match line {
                        Some(Ok(line)) => {
                            if let Err(error) = self.incoming(&line) { break error.to_string(); }
                        }
                        Some(Err(error)) => break format!("invalid package host frame: {error}"),
                        None => break "package language host disconnected".to_owned(),
                    }
                }
                request = requests.recv() => {
                    let Some(request) = request else { break "package host requests closed".to_owned(); };
                    if let Err(error) = self.register(request) { break error.to_string(); }
                }
                _ = timer.tick() => {
                    if let Err(error) = self.expire() { break error.to_string(); }
                }
            }
        };
        self.closed.cancel();
        requests.close();
        writer.abort();
        let _ = writer.await;
        kill_group(&child);
        let _ = child.start_kill();
        // Retain requests, admission, and effect guards until actual child exit.
        let _ = child.wait().await;
        diagnostics.abort();
        let _ = diagnostics.await;
        for (_, pending) in self.pending.drain() {
            let _ = pending.reply.send(Err(anyhow!(failure.clone())));
        }
        while let Some(request) = requests.recv().await {
            let _ = request.pending.reply.send(Err(anyhow!(failure.clone())));
        }
    }

    fn register(&mut self, request: Request) -> Result<()> {
        if request.pending.cancel.is_cancelled() {
            let _ = request
                .pending
                .reply
                .send(Err(anyhow!("package request cancelled before launch")));
            return Ok(());
        }
        let value = json!({"id": request.id, "method": request.method, "params": request.params});
        self.send(&value)?;
        self.pending.insert(request.id, request.pending);
        Ok(())
    }

    fn send(&self, value: &impl Serialize) -> Result<()> {
        let line = serde_json::to_string(value)?;
        ensure!(
            line.len() < self.config.max_frame_bytes,
            "package host request exceeds frame limit"
        );
        let bytes = self
            .bytes
            .clone()
            .try_acquire_many_owned(u32::try_from(line.len() + 1)?)
            .context("package host outbound byte capacity reached")?;
        self.frames
            .try_send(Frame {
                line,
                _bytes: bytes,
            })
            .map_err(|_| anyhow!("package host outbound queue unavailable"))
    }

    fn incoming(&mut self, line: &str) -> Result<()> {
        match serde_json::from_str::<Incoming>(line).context("invalid package host message")? {
            Incoming::Success { id, result } => self.resolve(&id, Ok(result)),
            Incoming::Failure { id, error } => self.resolve(&id, Err(anyhow!(error.message))),
            Incoming::Notification(notice) => self.notify(notice),
        }
    }

    fn resolve(&mut self, id: &str, result: Result<Value>) -> Result<()> {
        let pending = self
            .pending
            .remove(id)
            .context("unknown package host response ID")?;
        let result = pending
            .failure
            .map_or(result, |failure| Err(anyhow!(failure)));
        let _ = pending.reply.send(result);
        Ok(())
    }

    fn notify(&mut self, notice: Notice) -> Result<()> {
        let (id, event) = match notice {
            Notice::Progress { id, message } => {
                ensure!(message.len() <= 2048, "package progress exceeds limit");
                (id, RuntimeEvent::Progress(message))
            }
            Notice::Delta { id, text } => {
                ensure!(text.len() <= 64 * 1024, "package text delta exceeds limit");
                (id, RuntimeEvent::Delta(text))
            }
        };
        let pending = self
            .pending
            .get_mut(&id)
            .context("unknown package notification ID")?;
        if pending.cancelled_at.is_some() {
            return Ok(());
        }
        pending.event_count += 1;
        pending.event_bytes += match &event {
            RuntimeEvent::Progress(message) => message.len(),
            RuntimeEvent::Delta(text) => text.len(),
        };
        if pending.event_count > 10_000 || pending.event_bytes > 8 * 1024 * 1024 {
            pending.failure = Some("package event budget exceeded".into());
            pending.cancel.cancel();
            return Ok(());
        }
        if let Some(events) = &pending.events
            && events.try_send(event).is_err()
        {
            pending.failure = Some("package event receiver is closed or full".into());
            pending.cancel.cancel();
        }
        Ok(())
    }

    fn expire(&mut self) -> Result<()> {
        let now = Instant::now();
        let mut cancellations = Vec::new();
        for (id, pending) in &mut self.pending {
            if let Some(at) = pending.cancelled_at {
                ensure!(
                    now.duration_since(at) < self.config.cleanup_timeout,
                    "package cancellation was not acknowledged; stopping the language host"
                );
            } else if pending.cancel.is_cancelled() || now >= pending.deadline {
                pending.failure.get_or_insert_with(|| {
                    if now >= pending.deadline {
                        "package request deadline exceeded"
                    } else {
                        "package request cancelled"
                    }
                    .into()
                });
                pending.cancelled_at = Some(now);
                cancellations.push(id.clone());
            }
        }
        for id in cancellations {
            self.send(&json!({"method":"cancel", "params":{"id":id}}))?;
        }
        Ok(())
    }
}

async fn write_frames(
    stdin: tokio::process::ChildStdin,
    mut frames: mpsc::Receiver<Frame>,
    maximum: usize,
    closed: CancellationToken,
) {
    let mut writer = FramedWrite::new(stdin, LinesCodec::new_with_max_length(maximum));
    loop {
        let frame = tokio::select! {
            () = closed.cancelled() => break,
            frame = frames.recv() => {
                let Some(frame) = frame else { break; };
                frame
            }
        };
        match tokio::time::timeout(Duration::from_secs(5), writer.send(frame.line)).await {
            Ok(Ok(())) => {}
            _ => break,
        }
    }
    closed.cancel();
}

fn kill_group(child: &Child) {
    #[cfg(unix)]
    if let Some(pid) = child.id()
        && let Ok(pid) = i32::try_from(pid)
        && let Some(pid) = rustix::process::Pid::from_raw(pid)
    {
        let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
    }
    #[cfg(not(unix))]
    let _ = child;
}
