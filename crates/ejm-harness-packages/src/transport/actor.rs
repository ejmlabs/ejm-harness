use std::{collections::HashMap, sync::Arc, time::Duration};

use anyhow::{Context, Result, anyhow, ensure};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::{
    process::{Child, ChildStdout},
    sync::{Semaphore, mpsc},
    task::JoinHandle,
    time::Instant,
};
use tokio_util::{
    codec::{FramedRead, LinesCodec},
    sync::CancellationToken,
};

use super::{Frame, Pending, Request};
use crate::runtime::{RuntimeConfig, RuntimeEvent};

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

/// Owns one host process's protocol state: pending calls, deadlines, and cleanup.
pub(super) struct Actor {
    frames: mpsc::Sender<Frame>,
    bytes: Arc<Semaphore>,
    pending: HashMap<String, Pending>,
    config: RuntimeConfig,
    closed: CancellationToken,
}

impl Actor {
    pub(super) fn new(
        frames: mpsc::Sender<Frame>,
        config: RuntimeConfig,
        closed: CancellationToken,
    ) -> Self {
        Self {
            frames,
            bytes: Arc::new(Semaphore::new(config.max_buffer_bytes)),
            pending: HashMap::new(),
            config,
            closed,
        }
    }

    pub(super) async fn supervise(
        mut self,
        mut child: Child,
        stdout: ChildStdout,
        mut requests: mpsc::Receiver<Request>,
        writer: JoinHandle<()>,
        diagnostics: JoinHandle<()>,
    ) {
        let failure = self.serve(stdout, &mut requests).await;
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

    /// Handle host output, new requests, and deadlines until the host stops; returns why.
    async fn serve(
        &mut self,
        stdout: ChildStdout,
        requests: &mut mpsc::Receiver<Request>,
    ) -> String {
        let mut reader = FramedRead::new(
            stdout,
            LinesCodec::new_with_max_length(self.config.max_frame_bytes),
        );
        let mut timer = tokio::time::interval(Duration::from_millis(20));
        loop {
            tokio::select! {
                () = self.closed.cancelled() => return "package language host stopped".to_owned(),
                line = reader.next() => {
                    match line {
                        Some(Ok(line)) => {
                            if let Err(error) = self.incoming(&line) { return error.to_string(); }
                        }
                        Some(Err(error)) => return format!("invalid package host frame: {error}"),
                        None => return "package language host disconnected".to_owned(),
                    }
                }
                request = requests.recv() => {
                    let Some(request) = request else { return "package host requests closed".to_owned(); };
                    if let Err(error) = self.register(request) { return error.to_string(); }
                }
                _ = timer.tick() => {
                    if let Err(error) = self.expire() { return error.to_string(); }
                }
            }
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
