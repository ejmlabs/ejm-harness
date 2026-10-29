use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result, bail, ensure};
use tokio_util::sync::CancellationToken;

use super::{
    process::{Operation, ProcessResult, supervise},
    status::ClaudeCodeStatus,
};

/// Thin auth adapter for an installed, unmodified official Claude Code binary.
/// Hosts choose its executable and own terminal suspension around [`Self::login`].
#[derive(Clone)]
pub struct ClaudeCodeClient {
    program: PathBuf,
    login_timeout: Duration,
    status_timeout: Duration,
}

impl ClaudeCodeClient {
    #[must_use]
    pub fn new(program: PathBuf) -> Self {
        Self {
            program,
            login_timeout: Duration::from_secs(600),
            status_timeout: Duration::from_secs(15),
        }
    }

    #[must_use]
    pub fn program(&self) -> &Path {
        &self.program
    }

    /// Set a shorter process deadline for hosts and deterministic fixtures.
    ///
    /// # Errors
    /// Returns an error for zero or a timeout longer than ten minutes.
    pub fn with_timeout(mut self, timeout: Duration) -> Result<Self> {
        ensure!(
            !timeout.is_zero() && timeout <= Duration::from_secs(600),
            "Claude Code auth timeout must be positive and at most ten minutes"
        );
        self.login_timeout = timeout;
        self.status_timeout = timeout.min(Duration::from_secs(15));
        Ok(self)
    }

    /// Inspect bounded nonsecret account metadata through the official client.
    ///
    /// # Errors
    /// Returns an error for an unavailable client, cancellation, timeout, invalid
    /// status, oversized output, or an unexpected client failure.
    pub async fn status(&self, cancel: CancellationToken) -> Result<ClaudeCodeStatus> {
        let output = self.run(Operation::Status, cancel).await?;
        output.require_finished()?;
        ClaudeCodeStatus::from_client_output(output.exit_code, &output.stdout)
    }

    /// Run the official client's subscription sign-in with inherited terminal IO.
    ///
    /// The host must suspend its terminal reader first. The client owns browser
    /// interaction and credential storage. Child output is never captured.
    ///
    /// # Errors
    /// Returns an error on cancellation, timeout, client failure, or when the
    /// resulting status is not the client's own subscription authentication.
    pub async fn login(&self, cancel: CancellationToken) -> Result<ClaudeCodeStatus> {
        let output = self.run(Operation::Login, cancel.clone()).await?;
        output.require_finished()?;
        ensure!(output.exit_code == Some(0), "Claude Code sign-in failed");
        let status = self.status(cancel).await?;
        status.require_subscription()?;
        Ok(status)
    }

    /// Delegate sign-out to the official client without reading its credentials.
    /// A committed sign-out is returned even when cancellation races completion.
    ///
    /// # Errors
    /// Returns an error for client failure, timeout, or cancellation when local
    /// sign-out cannot be confirmed. Raw child output is never returned.
    pub async fn logout(&self, cancel: CancellationToken) -> Result<ClaudeCodeStatus> {
        let output = self.run(Operation::Logout, cancel).await?;
        if output.exit_code == Some(0) {
            return Ok(ClaudeCodeStatus::default());
        }
        if output.cancelled || output.timed_out {
            // Sign-out may already be committed. Preserve the observable change.
            if let Ok(status) = self.status(CancellationToken::new()).await
                && !status.logged_in
            {
                return Ok(status);
            }
        }
        output.require_finished()?;
        bail!("Claude Code sign-out failed")
    }

    async fn run(&self, operation: Operation, cancel: CancellationToken) -> Result<ProcessResult> {
        ensure!(
            !self.program.as_os_str().is_empty(),
            "Claude Code executable is required"
        );
        ensure!(
            std::env::var_os("CLAUDE_CONFIG_DIR").is_none_or(|path| Path::new(&path).is_absolute()),
            "CLAUDE_CONFIG_DIR must be absolute for consistent official-client account selection"
        );
        ensure!(!cancel.is_cancelled(), "Claude Code operation cancelled");
        let local_cancel = cancel.child_token();
        let _guard = CancelOnDrop(local_cancel.clone());
        let program = self.program.clone();
        let timeout = if operation.captures_output() {
            self.status_timeout
        } else {
            self.login_timeout
        };
        tokio::spawn(async move { supervise(&program, operation, timeout, &local_cancel).await })
            .await
            .context("Claude Code auth supervisor failed")?
    }
}

struct CancelOnDrop(CancellationToken);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}
