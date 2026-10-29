use std::{process::Stdio, sync::Arc, time::Duration};

use anyhow::{Context, Result, anyhow, ensure};
use ejm_harness_package_spec::RuntimeKind;
use futures_util::SinkExt;
use serde_json::Value;
use tokio::{
    process::{ChildStdin, Command},
    sync::{Semaphore, mpsc, oneshot, watch},
    time::Instant,
};
use tokio_util::{
    codec::{FramedWrite, LinesCodec},
    sync::CancellationToken,
};

use super::{Frame, Pending, Request, actor::Actor};
use crate::runtime::{RuntimeConfig, RuntimeEvent};

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
        let mut child = language_command(kind, &config)
            .spawn()
            .context("start package language host")?;
        let stdin = child
            .stdin
            .take()
            .context("language host stdin unavailable")?;
        let stdout = child
            .stdout
            .take()
            .context("language host stdout unavailable")?;
        let mut stderr = child
            .stderr
            .take()
            .context("language host stderr unavailable")?;
        let (requests, incoming) = mpsc::channel(config.max_pending);
        let (frames, outgoing) = mpsc::channel(config.max_pending);
        let (done_tx, done) = watch::channel(false);
        let writer = tokio::spawn(write_frames(
            stdin,
            outgoing,
            config.max_frame_bytes,
            closed.clone(),
        ));
        let diagnostics = tokio::spawn(async move {
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
            Actor::new(frames, config, closed)
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
        let _cancel_on_drop = cancel.clone().drop_guard();
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

/// The embedded host script for one language, in its own process group where supported.
fn language_command(kind: RuntimeKind, config: &RuntimeConfig) -> Command {
    let mut command = match kind {
        RuntimeKind::Node => {
            let mut command = Command::new(&config.node);
            command.args([
                "--input-type=module",
                "--eval",
                include_str!("../../runtime/node.mjs"),
            ]);
            command
        }
        RuntimeKind::Python => {
            let mut command = Command::new(&config.python);
            command.args([
                "-I",
                "-B",
                "-u",
                "-c",
                include_str!("../../runtime/python.py"),
            ]);
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
    command
}

async fn write_frames(
    stdin: ChildStdin,
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
