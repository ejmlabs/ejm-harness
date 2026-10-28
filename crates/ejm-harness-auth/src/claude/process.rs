//! Supervise one official client process: bounded output, a deadline, and tree cleanup.

use std::{path::Path, process::Stdio, time::Duration};

use anyhow::{Context, Result, ensure};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::{Child, Command},
    task::{JoinError, JoinSet},
};
use tokio_util::sync::CancellationToken;

use super::{MAX_STATUS_BYTES, process_group::ProcessGroup, program::CLAUDE_CODE_ENV_OVERRIDES};

/// `Some` holds captured stdout. Stderr is read only to bound it, and is discarded.
type ReaderOutput = Result<Option<Vec<u8>>>;

#[derive(Clone, Copy)]
pub(super) enum Operation {
    Login,
    Status,
    Logout,
}

impl Operation {
    fn args(self) -> &'static [&'static str] {
        match self {
            Self::Login => &["auth", "login", "--claudeai"],
            Self::Status => &["auth", "status", "--json"],
            Self::Logout => &["auth", "logout"],
        }
    }

    /// Login keeps the inherited terminal. Every other operation is captured and bounded.
    pub(super) fn captures_output(self) -> bool {
        !matches!(self, Self::Login)
    }
}

#[derive(Default)]
pub(super) struct ProcessResult {
    pub(super) exit_code: Option<i32>,
    pub(super) stdout: Vec<u8>,
    pub(super) cancelled: bool,
    pub(super) timed_out: bool,
}

impl ProcessResult {
    pub(super) fn require_finished(&self) -> Result<()> {
        ensure!(!self.cancelled, "Claude Code operation cancelled");
        ensure!(!self.timed_out, "Claude Code operation timed out");
        Ok(())
    }

    fn record(&mut self, item: Option<Result<ReaderOutput, JoinError>>) -> Result<()> {
        if let Some(bytes) = reader_result(item)? {
            self.stdout = bytes;
        }
        Ok(())
    }
}

pub(super) async fn supervise(
    program: &Path,
    operation: Operation,
    timeout: Duration,
    cancel: &CancellationToken,
) -> Result<ProcessResult> {
    ensure!(!cancel.is_cancelled(), "Claude Code operation cancelled");
    let captured = operation.captures_output();
    let (mut child, mut group) = ProcessGroup::spawn(command(program, operation), captured)
        .context(
            "start official Claude Code client; install it or configure its executable path",
        )?;
    let mut readers = JoinSet::new();
    if captured {
        readers.spawn(read_bounded(
            child
                .stdout
                .take()
                .context("Claude Code stdout pipe missing")?,
            true,
        ));
        readers.spawn(read_bounded(
            child
                .stderr
                .take()
                .context("Claude Code stderr pipe missing")?,
            false,
        ));
    }
    let mut result = ProcessResult::default();
    let mut failure = wait(&mut child, &mut readers, &mut result, timeout, cancel).await;
    // A normal root exit can leave background descendants and open pipes.
    // Windows retains its Job until membership confirms an empty tree.
    let status = group.reap(&mut child).await;
    let drained = drain(&mut readers, &mut result, &mut failure).await;
    let status = status?;
    if result.exit_code.is_none() {
        result.exit_code = status.code();
    }
    ensure!(drained, "Claude Code auth output did not close");
    if let Some(error) = failure {
        return Err(error);
    }
    Ok(result)
}

fn command(program: &Path, operation: Operation) -> Command {
    let mut command = Command::new(program);
    for key in CLAUDE_CODE_ENV_OVERRIDES {
        command.env_remove(key);
    }
    command.args(operation.args());
    if operation.captures_output() {
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
    } else {
        // Retain the foreground terminal's process group for canonical input.
        command
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());
    }
    command
}

/// Wait for exit, cancellation, the deadline, or a failed reader. The outcome lands in
/// `result`. A failure is returned for the caller to report after cleanup.
async fn wait(
    child: &mut Child,
    readers: &mut JoinSet<ReaderOutput>,
    result: &mut ProcessResult,
    timeout: Duration,
    cancel: &CancellationToken,
) -> Option<anyhow::Error> {
    let deadline = tokio::time::sleep(timeout);
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            biased;
            status = child.wait() => {
                return match status {
                    Ok(status) => {
                        result.exit_code = status.code();
                        None
                    }
                    Err(error) => Some(anyhow::Error::from(error).context("wait for Claude Code auth")),
                };
            }
            () = cancel.cancelled() => { result.cancelled = true; return None; }
            () = &mut deadline => { result.timed_out = true; return None; }
            item = readers.join_next(), if !readers.is_empty() => {
                if let Err(error) = result.record(item) {
                    return Some(error);
                }
            }
        }
    }
}

/// Give the readers a second to finish after cleanup, then abort them. Reports whether they closed.
async fn drain(
    readers: &mut JoinSet<ReaderOutput>,
    result: &mut ProcessResult,
    failure: &mut Option<anyhow::Error>,
) -> bool {
    let drained = tokio::time::timeout(Duration::from_secs(1), async {
        while let Some(item) = readers.join_next().await {
            if let Err(error) = result.record(Some(item)) {
                failure.get_or_insert(error);
            }
        }
    })
    .await
    .is_ok();
    readers.abort_all();
    while readers.join_next().await.is_some() {}
    drained
}

async fn read_bounded(reader: impl AsyncRead + Unpin, stdout: bool) -> ReaderOutput {
    let mut bytes = Vec::new();
    reader
        .take((MAX_STATUS_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .await
        .context("read Claude Code auth output")?;
    ensure!(
        bytes.len() <= MAX_STATUS_BYTES,
        "Claude Code auth output exceeds limit"
    );
    Ok(stdout.then_some(bytes))
}

fn reader_result(item: Option<Result<ReaderOutput, JoinError>>) -> ReaderOutput {
    item.context("Claude Code auth reader ended unexpectedly")?
        .context("Claude Code auth reader failed")?
}
