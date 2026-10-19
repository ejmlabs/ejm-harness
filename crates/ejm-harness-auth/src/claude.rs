//! Authentication stays inside the unmodified official Claude Code client.

use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    process::{ExitStatus, Stdio},
    time::Duration,
};

use anyhow::{Context, Result, bail, ensure};
#[cfg(unix)]
use rustix::process::{Pid, Signal, kill_process_group};
use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::{Child, Command},
    task::JoinSet,
};
use tokio_util::sync::CancellationToken;

const MAX_STATUS_BYTES: usize = 16 * 1024;
const MAX_METADATA_BYTES: usize = 512;
type ReaderOutput = Result<(bool, Vec<u8>)>;

/// Resolve the official client without a shell. An explicit override is unchanged.
/// Otherwise prefer the native install in the user's `.local/bin`, then `PATH`.
/// Windows automatic lookup requires `claude.exe`, so batch shims cannot win.
/// A missing client retains its native command name for the normal launch error.
#[must_use]
pub fn resolve_claude_code_program(configured: Option<&Path>) -> PathBuf {
    configured.map_or_else(
        || {
            discover_program(
                std::env::home_dir().as_deref(),
                std::env::var_os("PATH").as_deref(),
            )
        },
        Path::to_path_buf,
    )
}

fn discover_program(home: Option<&Path>, paths: Option<&OsStr>) -> PathBuf {
    let name = if cfg!(windows) {
        "claude.exe"
    } else {
        "claude"
    };
    home.and_then(|home| which::which(home.join(".local").join("bin").join(name)).ok())
        .or_else(|| which::which_in_global(name, paths).ok()?.next())
        .unwrap_or_else(|| PathBuf::from(name))
}

/// Ambient credentials and routing overrides removed for subscription access.
/// The official client still owns its credential store and managed policy.
pub const CLAUDE_CODE_ENV_OVERRIDES: &[&str] = &[
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_BASE_URL",
    "ANTHROPIC_CUSTOM_HEADERS",
    "ANTHROPIC_MODEL",
    "ANTHROPIC_SMALL_FAST_MODEL",
    "ANTHROPIC_DEFAULT_OPUS_MODEL",
    "ANTHROPIC_DEFAULT_SONNET_MODEL",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL",
    "CLAUDE_CODE_OAUTH_TOKEN",
    "CLAUDE_CODE_OAUTH_REFRESH_TOKEN",
    "CLAUDE_CODE_OAUTH_SCOPES",
    "CLAUDE_CODE_OAUTH_TOKEN_FILE_DESCRIPTOR",
    "CLAUDE_CODE_API_KEY_FILE_DESCRIPTOR",
    "CLAUDE_CODE_USE_BEDROCK",
    "CLAUDE_CODE_USE_VERTEX",
    "CLAUDE_CODE_USE_FOUNDRY",
    "CLAUDE_CODE_SIMPLE",
    "CLAUDECODE",
];

/// Allowlisted metadata from `claude auth status --json`.
/// Subscription tokens and unknown client fields are never retained here.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeCodeStatus {
    pub logged_in: bool,
    #[serde(default)]
    pub auth_method: String,
    pub email: Option<String>,
    pub subscription_type: Option<String>,
}

impl ClaudeCodeStatus {
    /// Decode bounded, nonsecret status captured by a host-owned supervisor.
    ///
    /// # Errors
    /// Returns an error for a failed client, oversized or malformed JSON,
    /// inconsistent sign-in state, or invalid account metadata. Raw output is
    /// never included in errors. The caller must first reject cancelled,
    /// timed-out and truncated process results.
    pub fn from_client_output(exit_code: Option<i32>, stdout: &[u8]) -> Result<Self> {
        ensure!(
            matches!(exit_code, Some(0 | 1)),
            "Claude Code account status failed"
        );
        ensure!(
            stdout.len() <= MAX_STATUS_BYTES,
            "Claude Code auth output exceeds limit"
        );
        let mut status: Self = serde_json::from_slice(stdout)
            .map_err(|_| anyhow::anyhow!("invalid Claude Code account status"))?;
        ensure!(
            status.logged_in == (exit_code == Some(0)),
            "inconsistent Claude Code account status"
        );
        status.auth_method = match status.auth_method.as_str() {
            "none" | "claude.ai" | "oauth_token" | "api_key" | "api_key_helper" | "third_party" => {
                status.auth_method
            }
            _ => "unknown".into(),
        };
        for field in [&status.email, &status.subscription_type]
            .into_iter()
            .flatten()
        {
            ensure!(
                field.len() <= MAX_METADATA_BYTES && !field.chars().any(char::is_control),
                "invalid Claude Code account metadata"
            );
        }
        Ok(status)
    }

    /// Require the official client's own Claude subscription sign-in.
    ///
    /// # Errors
    /// Returns an error for signed-out, API-key, token-override and cloud sources.
    pub fn require_subscription(&self) -> Result<()> {
        ensure!(
            self.logged_in,
            "Claude Code is signed out; sign in through the official client"
        );
        ensure!(
            self.auth_method == "claude.ai",
            "Claude subscription access requires the official client's Claude account sign-in; API-key and third-party sources use a separate provider"
        );
        Ok(())
    }
}

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
        let client = self.clone();
        tokio::spawn(async move { client.supervise(operation, local_cancel).await })
            .await
            .context("Claude Code auth supervisor failed")?
    }

    async fn supervise(
        &self,
        operation: Operation,
        cancel: CancellationToken,
    ) -> Result<ProcessResult> {
        ensure!(!cancel.is_cancelled(), "Claude Code operation cancelled");
        let mut command = Command::new(&self.program);
        for key in CLAUDE_CODE_ENV_OVERRIDES {
            command.env_remove(key);
        }
        command.args(operation.args());
        let captured = !matches!(operation, Operation::Login);
        if captured {
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
        let (mut child, mut group) = ProcessGroup::spawn(command, captured).context(
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
        let timeout = if captured {
            self.status_timeout
        } else {
            self.login_timeout
        };
        let deadline = tokio::time::sleep(timeout);
        tokio::pin!(deadline);
        let mut result = ProcessResult::default();
        let mut failure = None;
        loop {
            tokio::select! {
                biased;
                status = child.wait() => {
                    match status {
                        Ok(status) => result.exit_code = status.code(),
                        Err(error) => failure = Some(anyhow::Error::from(error).context("wait for Claude Code auth")),
                    }
                    break;
                }
                () = cancel.cancelled() => { result.cancelled = true; break; }
                () = &mut deadline => { result.timed_out = true; break; }
                item = readers.join_next(), if !readers.is_empty() => {
                    match reader_result(item) {
                        Ok((true, bytes)) => result.stdout = bytes,
                        Ok((false, _)) => {},
                        Err(error) => { failure = Some(error); break; }
                    }
                }
            }
        }
        // A normal root exit can leave background descendants and open pipes.
        // Windows retains its Job until membership confirms an empty tree.
        let status = group.reap(&mut child).await;
        let drained = tokio::time::timeout(Duration::from_secs(1), async {
            while let Some(item) = readers.join_next().await {
                match reader_result(Some(item)) {
                    Ok((true, bytes)) => result.stdout = bytes,
                    Ok((false, _)) => {}
                    Err(error) => {
                        failure.get_or_insert(error);
                    }
                }
            }
        })
        .await
        .is_ok();
        readers.abort_all();
        while readers.join_next().await.is_some() {}
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
}

#[derive(Clone, Copy)]
enum Operation {
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
}

#[derive(Default)]
struct ProcessResult {
    exit_code: Option<i32>,
    stdout: Vec<u8>,
    cancelled: bool,
    timed_out: bool,
}

impl ProcessResult {
    fn require_finished(&self) -> Result<()> {
        ensure!(!self.cancelled, "Claude Code operation cancelled");
        ensure!(!self.timed_out, "Claude Code operation timed out");
        Ok(())
    }
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
    Ok((stdout, if stdout { bytes } else { Vec::new() }))
}

fn reader_result(item: Option<Result<ReaderOutput, tokio::task::JoinError>>) -> ReaderOutput {
    item.context("Claude Code auth reader ended unexpectedly")?
        .context("Claude Code auth reader failed")?
}

struct CancelOnDrop(CancellationToken);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

struct ProcessGroup {
    #[cfg(unix)]
    pid: Option<u32>,
    #[cfg(windows)]
    native: processkit::ProcessGroup,
}
impl ProcessGroup {
    fn spawn(mut command: Command, captured: bool) -> Result<(Child, Self)> {
        command.kill_on_drop(true);
        #[cfg(unix)]
        if captured {
            command.process_group(0);
        }
        #[cfg(windows)]
        let native = processkit::ProcessGroup::new()?;
        #[cfg(windows)]
        let child = native.spawn(command)?;
        #[cfg(not(windows))]
        let child = command.spawn()?;
        let group = Self {
            #[cfg(unix)]
            pid: if captured { child.id() } else { None },
            #[cfg(windows)]
            native,
        };
        #[cfg(not(unix))]
        let _ = captured;
        Ok((child, group))
    }

    fn kill(&self) -> Result<()> {
        #[cfg(unix)]
        if let Some(pid) = self
            .pid
            .and_then(|pid| i32::try_from(pid).ok())
            .and_then(Pid::from_raw)
        {
            match kill_process_group(pid, Signal::KILL) {
                Ok(()) | Err(rustix::io::Errno::SRCH) => {}
                Err(error) => return Err(error.into()),
            }
        }
        #[cfg(windows)]
        self.native.kill_all()?;
        Ok(())
    }

    async fn reap(&mut self, child: &mut Child) -> Result<ExitStatus> {
        let group = self.kill();
        let root = child.start_kill();
        #[cfg(windows)]
        loop {
            if self
                .native
                .members()
                .is_ok_and(|members| members.is_empty())
            {
                break;
            }
            let _ = self.native.kill_all();
            let _ = child.start_kill();
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        let status = child.wait().await;
        group.context("terminate Claude Code auth process group")?;
        root.context("terminate Claude Code auth child")?;
        let status = status.context("reap Claude Code auth")?;
        #[cfg(unix)]
        {
            // Cleanup consumed this identity. Drop must not signal a reused group.
            self.pid = None;
        }
        Ok(status)
    }
}
impl Drop for ProcessGroup {
    fn drop(&mut self) {
        let _ = self.kill();
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ClaudeCodeStatus, MAX_STATUS_BYTES, discover_program, resolve_claude_code_program,
    };
    use std::path::Path;

    #[test]
    fn official_client_discovery_preserves_override_and_prefers_native_install()
    -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let home = root.path().join("home with space");
        let native_dir = home.join(".local/bin");
        let path_dir = root.path().join("path");
        std::fs::create_dir_all(&native_dir)?;
        std::fs::create_dir(&path_dir)?;
        let name = if cfg!(windows) {
            "claude.exe"
        } else {
            "claude"
        };
        let native = native_dir.join(name);
        let fallback = path_dir.join(name);
        for file in [&native, &fallback] {
            std::fs::write(file, [])?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o700))?;
            }
        }
        // A batch shim must not precede the native executable in automatic lookup.
        #[cfg(windows)]
        std::fs::write(path_dir.join("claude.cmd"), "@exit /b 1\r\n")?;
        let paths = std::env::join_paths([path_dir])?;
        let explicit = Path::new("unavailable/custom-client");
        assert_eq!(resolve_claude_code_program(Some(explicit)), explicit);
        assert_eq!(discover_program(Some(&home), Some(&paths)), native);
        std::fs::remove_file(&native)?;
        assert_eq!(discover_program(Some(&home), Some(&paths)), fallback);
        std::fs::remove_file(fallback)?;
        assert_eq!(discover_program(Some(&home), Some(&paths)), Path::new(name));
        Ok(())
    }

    #[test]
    fn supervised_status_decoder_enforces_its_byte_boundary() -> anyhow::Result<()> {
        let mut output = br#"{"loggedIn":false,"authMethod":"none"}"#.to_vec();
        output.resize(MAX_STATUS_BYTES, b' ');
        assert!(!ClaudeCodeStatus::from_client_output(Some(1), &output)?.logged_in);
        output.push(b' ');
        assert!(ClaudeCodeStatus::from_client_output(Some(1), &output).is_err());
        Ok(())
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn successful_auth_reap_retires_the_unix_group_identity() -> anyhow::Result<()> {
        let mut command = tokio::process::Command::new(std::env::current_exe()?);
        command
            .arg("--list")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        let (mut child, mut group) = super::ProcessGroup::spawn(command, true)?;
        assert!(group.pid.is_some(), "captured child must own its group");
        tokio::time::timeout(std::time::Duration::from_secs(3), group.reap(&mut child)).await??;
        assert!(
            child.id().is_none(),
            "successful cleanup must reap its child"
        );
        assert!(group.pid.is_none(), "completed ownership must be retired");
        Ok(())
    }
}
