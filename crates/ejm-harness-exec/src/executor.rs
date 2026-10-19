use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    process::{ExitStatus, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail, ensure};
use tokio::{
    fs::{File, OpenOptions},
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    process::Command,
    sync::Mutex as AsyncMutex,
    task::JoinSet,
    time::{Instant as TokioInstant, sleep_until, timeout},
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{
    CommandOutput, CommandRequest, ExecutorCapabilities, Governor, Lease, RequiredCapabilities,
    RestrictionState,
    governor::{ARTIFACT_OVERHEAD, lock},
    process::{OwnedChild, spawn_owned},
    processes,
};

const TERMINATE_GRACE: Duration = Duration::from_millis(100);
const DRAIN_GRACE: Duration = Duration::from_secs(1);
const READ_BYTES: usize = 8192;
const MAX_INPUT_BYTES: usize = 8 * 1024 * 1024;

#[derive(Default)]
struct ProcessInput {
    bytes: Option<Vec<u8>>,
    env_remove: Vec<String>,
}

#[derive(Clone)]
pub struct Executor {
    governor: Arc<Governor>,
    artifact_root: Arc<PathBuf>,
}

impl Executor {
    /// Create an executor whose artifacts share the governor's retained quota.
    ///
    /// # Errors
    /// Returns an error if the dedicated artifact directory is unavailable,
    /// is a symlink, or already exceeds the retained artifact quota.
    pub fn new(governor: Arc<Governor>, artifact_root: impl AsRef<Path>) -> Result<Self> {
        let artifact_root = artifact_root.as_ref();
        std::fs::create_dir_all(artifact_root).context("create executor artifact directory")?;
        let metadata = std::fs::symlink_metadata(artifact_root)?;
        ensure!(
            metadata.is_dir() && !metadata.file_type().is_symlink(),
            "artifact root must be a directory, not a symlink"
        );
        let artifact_root = artifact_root.canonicalize()?;
        governor.register_artifact_root(&artifact_root)?;
        Ok(Self {
            governor,
            artifact_root: Arc::new(artifact_root),
        })
    }

    #[must_use]
    pub fn capabilities(&self) -> ExecutorCapabilities {
        use RestrictionState::{Enforced, Monitored, Unsupported};
        ExecutorCapabilities {
            admission: Enforced,
            memory_limit: if processes::supported() {
                Monitored
            } else {
                Unsupported
            },
            cpu_limit: Unsupported,
            process_limit: if processes::supported() {
                Monitored
            } else {
                Unsupported
            },
            output_limit: Enforced,
            artifact_limit: Enforced,
            // The deadline triggers termination; the OS may defer a kill while
            // a process is in an uninterruptible kernel operation.
            deadline: Monitored,
            process_tree_cancellation: Monitored,
            filesystem_isolation: Unsupported,
            network_isolation: Unsupported,
        }
    }

    /// Admit and supervise one bounded command until its output is drained.
    ///
    /// # Errors
    /// Returns an error for invalid requests, unavailable required restrictions,
    /// failed admission, process or output I/O failures, or sampled budget abuse.
    pub async fn run(
        &self,
        request: CommandRequest,
        cancel: CancellationToken,
    ) -> Result<CommandOutput> {
        self.run_with_retained(request, cancel, ()).await
    }

    /// Feed private input to a supervised command and close its stdin at EOF.
    ///
    /// Input is limited to 8 MiB and is never included in command artifacts or
    /// serialized requests. Environment removals apply before explicit overrides
    /// in `request.env`. Writing input and draining output run concurrently.
    ///
    /// # Errors
    /// Returns an error for oversized input, invalid environment names, or any
    /// request, admission, execution, or output error returned by [`Self::run`].
    pub async fn run_with_input(
        &self,
        request: CommandRequest,
        input: Vec<u8>,
        env_remove: Vec<String>,
        cancel: CancellationToken,
    ) -> Result<CommandOutput> {
        ensure!(
            input.len() <= MAX_INPUT_BYTES,
            "command input exceeds 8 MiB"
        );
        ensure!(
            env_remove
                .iter()
                .all(|key| !key.is_empty() && !key.contains(['=', '\0'])),
            "invalid command environment removal"
        );
        self.run_configured(
            request,
            ProcessInput {
                bytes: Some(input),
                env_remove,
            },
            cancel,
            (),
        )
        .await
    }

    /// Retain host ownership, such as a workspace write guard, through cleanup.
    ///
    /// The retained value moves into the supervisor after admission. Dropping
    /// the caller cannot release it while its child or drainers are still live.
    ///
    /// # Errors
    /// Returns the same request, admission, execution, and output errors as [`Self::run`].
    pub async fn run_with_retained<T: Send + 'static>(
        &self,
        request: CommandRequest,
        cancel: CancellationToken,
        retained: T,
    ) -> Result<CommandOutput> {
        self.run_configured(request, ProcessInput::default(), cancel, retained)
            .await
    }

    async fn run_configured<T: Send + 'static>(
        &self,
        request: CommandRequest,
        input: ProcessInput,
        cancel: CancellationToken,
        retained: T,
    ) -> Result<CommandOutput> {
        validate_request(&request)?;
        check_required(&request.required, &self.capabilities())?;
        let start = Instant::now();
        let max_ms = self.governor.limits.max_job_ms;
        let timeout_ms = if request.timeout_ms == 0 {
            max_ms
        } else {
            request.timeout_ms.min(max_ms)
        };
        let deadline = TokioInstant::now() + Duration::from_millis(timeout_ms);
        let lease = tokio::select! {
            biased;
            () = cancel.cancelled() => return Ok(unlaunched_output(start, true, false)),
            () = sleep_until(deadline) => return Ok(unlaunched_output(start, false, true)),
            lease = self.governor.acquire(&request.session_id, request.reservation.clone(), &cancel) => lease?,
        };

        // Own the process and the lease in a separate task. Dropping the caller's
        // future cancels it, but cannot drop those resources before cleanup.
        let local_cancel = cancel.child_token();
        let _cancel_on_drop = CancelOnDrop(local_cancel.clone());
        let executor = self.clone();
        tokio::spawn(async move {
            let result = executor
                .supervise(request, input, local_cancel, deadline, start, lease)
                .await;
            drop(retained);
            result
        })
        .await
        .context("command supervisor failed")?
    }

    async fn supervise(
        &self,
        request: CommandRequest,
        input: ProcessInput,
        cancel: CancellationToken,
        deadline: TokioInstant,
        start: Instant,
        _lease: Lease,
    ) -> Result<CommandOutput> {
        if cancel.is_cancelled() {
            return Ok(unlaunched_output(start, true, false));
        }
        if TokioInstant::now() >= deadline {
            return Ok(unlaunched_output(start, false, true));
        }

        let mut child = spawn_command(&request, &input)?;
        let pid = child.id().context("spawned command has no process ID")?;
        let observed =
            self.governor
                .monitor
                .register(pid, request.reservation.clone(), cancel.clone());
        let capture = Arc::new(Mutex::new(Capture::new(request.reservation.output_bytes)));
        let spool = Arc::new(AsyncMutex::new(Spool {
            governor: self.governor.clone(),
            root: self.artifact_root.clone(),
            file: None,
            path: None,
            truncated: false,
        }));
        let mut drainers = start_pipes(&mut child, &capture, &spool, input.bytes)?;

        let mut cancelled = false;
        let mut timed_out = false;
        let mut failure = None;
        let status: Option<ExitStatus> = loop {
            tokio::select! {
                biased;
                () = cancel.cancelled() => { cancelled = true; break None; },
                () = sleep_until(deadline) => { timed_out = true; break None; },
                status = child.wait() => {
                    match status {
                        Ok(status) => break Some(status),
                        Err(error) => { failure = Some(anyhow::Error::from(error).context("wait for command")); break None; },
                    }
                },
                result = drainers.join_next(), if !drainers.is_empty() => {
                    if let Some(error) = drain_error(result) {
                        failure = Some(error);
                        break None;
                    }
                },
            }
        };

        let status = reap(&mut child, status).await;
        observed.kill_known_descendants().await;
        let drained = timeout(DRAIN_GRACE, async {
            while let Some(result) = drainers.join_next().await {
                if let Some(error) = drain_error(Some(result)) {
                    failure.get_or_insert(error);
                }
            }
        })
        .await
        .is_ok();
        if !drained {
            drainers.abort_all();
            while drainers.join_next().await.is_some() {}
        }
        let usage = observed.usage();
        drop(observed);
        let status = status?;
        if let Some(message) = usage.violation {
            bail!(message);
        }
        if let Some(error) = failure {
            return Err(error);
        }
        let mut spool = spool.lock().await;
        if let Some(file) = &mut spool.file {
            file.flush().await.context("flush command artifact")?;
        }
        let (stdout, stderr, capture_truncated) = lock(&capture).take();
        Ok(CommandOutput {
            exit_code: status.code(),
            stdout,
            stderr,
            truncated: capture_truncated || spool.truncated || !drained,
            cancelled,
            timed_out,
            artifact: spool.path.clone(),
            duration_ms: millis(start.elapsed()),
            peak_memory_bytes: usage.peak_memory_bytes,
            peak_processes: usage.peak_processes,
        })
    }
}

fn spawn_command(request: &CommandRequest, input: &ProcessInput) -> Result<OwnedChild> {
    let mut command = Command::new(&request.program);
    for key in &input.env_remove {
        command.env_remove(key);
    }
    command
        .args(&request.args)
        .current_dir(&request.cwd)
        .envs(&request.env)
        .stdin(if input.bytes.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    spawn_owned(command).with_context(|| format!("launch command {}", request.program))
}

fn start_pipes(
    child: &mut OwnedChild,
    capture: &Arc<Mutex<Capture>>,
    spool: &Arc<AsyncMutex<Spool>>,
    input: Option<Vec<u8>>,
) -> Result<JoinSet<Result<()>>> {
    let mut drainers = JoinSet::new();
    for (reader, stream) in [
        (
            Box::new(child.take_stdout().context("stdout pipe missing")?)
                as Box<dyn AsyncRead + Send + Unpin>,
            Stream::Stdout,
        ),
        (
            Box::new(child.take_stderr().context("stderr pipe missing")?)
                as Box<dyn AsyncRead + Send + Unpin>,
            Stream::Stderr,
        ),
    ] {
        drainers.spawn(drain(reader, stream, capture.clone(), spool.clone()));
    }
    if let Some(bytes) = input {
        let mut stdin = child.take_stdin().context("stdin pipe missing")?;
        drainers.spawn(async move {
            // A command may close stdin on authentication failure. Its exit
            // status remains authoritative while output continues to drain.
            match stdin.write_all(&bytes).await {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
                Err(error) => Err(error).context("write command input"),
            }
        });
    }
    Ok(drainers)
}

async fn reap(child: &mut OwnedChild, status: Option<ExitStatus>) -> Result<ExitStatus> {
    let status = if let Some(status) = status {
        Ok(status)
    } else {
        let _ = child.terminate();
        if let Ok(result) = timeout(TERMINATE_GRACE, child.wait()).await {
            result.context("reap terminated command")
        } else {
            child.kill().await.context("reap killed command")
        }
    };
    // A command's background children cannot outlive its execution scope.
    let cleanup = child.kill().await.context("clean up command descendants");
    let status = status?;
    cleanup?;
    Ok(status)
}

fn validate_request(request: &CommandRequest) -> Result<()> {
    ensure!(
        !request.program.is_empty(),
        "command program must not be empty"
    );
    ensure!(
        !request.program.contains('\0'),
        "command program contains NUL"
    );
    ensure!(
        request.args.iter().all(|arg| !arg.contains('\0')),
        "command argument contains NUL"
    );
    ensure!(
        request.cwd.is_dir(),
        "command working directory does not exist"
    );
    ensure!(
        request.reservation.processes > 0,
        "commands must reserve at least one process"
    );
    ensure!(
        request.reservation.memory_bytes > 0,
        "commands must reserve memory"
    );
    ensure!(
        request.reservation.cpu_millis > 0,
        "commands must reserve CPU"
    );
    ensure!(
        request.reservation.output_bytes > 0,
        "commands must reserve output"
    );
    for (key, value) in &request.env {
        ensure!(
            !key.is_empty() && !key.contains(['=', '\0']) && !value.contains('\0'),
            "invalid command environment entry"
        );
    }
    Ok(())
}

fn check_required(
    required: &RequiredCapabilities,
    capabilities: &ExecutorCapabilities,
) -> Result<()> {
    for (name, required, available) in [
        ("admission", required.admission, capabilities.admission),
        (
            "memory_limit",
            required.memory_limit,
            capabilities.memory_limit,
        ),
        ("cpu_limit", required.cpu_limit, capabilities.cpu_limit),
        (
            "process_limit",
            required.process_limit,
            capabilities.process_limit,
        ),
        (
            "output_limit",
            required.output_limit,
            capabilities.output_limit,
        ),
        (
            "artifact_limit",
            required.artifact_limit,
            capabilities.artifact_limit,
        ),
        ("deadline", required.deadline, capabilities.deadline),
        (
            "process_tree_cancellation",
            required.process_tree_cancellation,
            capabilities.process_tree_cancellation,
        ),
        (
            "filesystem_isolation",
            required.filesystem_isolation,
            capabilities.filesystem_isolation,
        ),
        (
            "network_isolation",
            required.network_isolation,
            capabilities.network_isolation,
        ),
    ] {
        ensure!(
            !required || available == RestrictionState::Enforced,
            "required restriction {name} is not enforced by native execution ({available:?})"
        );
    }
    Ok(())
}

fn unlaunched_output(start: Instant, cancelled: bool, timed_out: bool) -> CommandOutput {
    CommandOutput {
        cancelled,
        timed_out,
        duration_ms: millis(start.elapsed()),
        ..CommandOutput::default()
    }
}

fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

fn drain_error(
    result: Option<std::result::Result<Result<()>, tokio::task::JoinError>>,
) -> Option<anyhow::Error> {
    match result {
        Some(Ok(Err(error))) => Some(error),
        Some(Err(error)) => Some(anyhow::Error::from(error).context("command pipe task failed")),
        _ => None,
    }
}

struct CancelOnDrop(CancellationToken);

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

#[derive(Clone, Copy)]
enum Stream {
    Stdout,
    Stderr,
}

impl Stream {
    fn name(self) -> &'static str {
        match self {
            Self::Stdout => "stdout",
            Self::Stderr => "stderr",
        }
    }
}

struct Tail {
    bytes: VecDeque<u8>,
    limit: usize,
    truncated: bool,
}

impl Tail {
    fn new(limit: usize) -> Self {
        Self {
            bytes: VecDeque::new(),
            limit,
            truncated: false,
        }
    }

    fn append(&mut self, bytes: &[u8]) {
        if bytes.len() >= self.limit {
            self.truncated |= !self.bytes.is_empty() || bytes.len() > self.limit;
            self.bytes.clear();
            self.bytes.extend(&bytes[bytes.len() - self.limit..]);
        } else {
            let excess = (self.bytes.len() + bytes.len()).saturating_sub(self.limit);
            self.truncated |= excess > 0;
            self.bytes.drain(..excess);
            self.bytes.extend(bytes);
        }
    }

    fn take(&mut self) -> (String, bool) {
        let bytes: Vec<_> = self.bytes.drain(..).collect();
        let mut text = String::from_utf8_lossy(&bytes).into_owned();
        // Replacement characters can expand invalid UTF-8. Preserve the same
        // byte budget in the public strings, including those replacements.
        if text.len() > self.limit {
            let mut start = text.len() - self.limit;
            while !text.is_char_boundary(start) {
                start += 1;
            }
            text.drain(..start);
            self.truncated = true;
        }
        (text, self.truncated)
    }
}

struct Capture {
    stdout: Tail,
    stderr: Tail,
}

impl Capture {
    fn new(limit: usize) -> Self {
        Self {
            stdout: Tail::new(limit.div_ceil(2)),
            stderr: Tail::new(limit / 2),
        }
    }

    fn append(&mut self, stream: Stream, bytes: &[u8]) {
        match stream {
            Stream::Stdout => self.stdout.append(bytes),
            Stream::Stderr => self.stderr.append(bytes),
        }
    }

    fn take(&mut self) -> (String, String, bool) {
        let (stdout, out_truncated) = self.stdout.take();
        let (stderr, err_truncated) = self.stderr.take();
        (stdout, stderr, out_truncated || err_truncated)
    }
}

struct Spool {
    governor: Arc<Governor>,
    root: Arc<PathBuf>,
    file: Option<File>,
    path: Option<PathBuf>,
    truncated: bool,
}

impl Spool {
    async fn append(&mut self, stream: Stream, bytes: &[u8]) -> Result<()> {
        if self.truncated || bytes.is_empty() {
            return Ok(());
        }
        if self.file.is_none() {
            let overhead = self.governor.claim_artifact_bytes(ARTIFACT_OVERHEAD);
            if overhead < ARTIFACT_OVERHEAD {
                self.governor.release_artifact_bytes(overhead);
                self.truncated = true;
                return Ok(());
            }
            let path = self.root.join(format!("exec-{}.log", Uuid::new_v4()));
            match create_private(&path).await {
                Ok(file) => {
                    self.file = Some(file);
                    self.path = Some(path);
                }
                Err(error) => {
                    self.governor.release_artifact_bytes(overhead);
                    return Err(error).context("create command artifact");
                }
            }
        }
        let header = format!("\n[{} {} bytes]\n", stream.name(), bytes.len());
        let total = header.len() + bytes.len();
        let granted = usize::try_from(self.governor.claim_artifact_bytes(total as u64))
            .context("artifact byte grant exceeds addressable memory")?;
        self.truncated = granted < total;
        if granted == 0 {
            return Ok(());
        }
        let file = self
            .file
            .as_mut()
            .context("artifact file missing after creation")?;
        let header_bytes = header.as_bytes();
        file.write_all(&header_bytes[..granted.min(header_bytes.len())])
            .await
            .context("write command artifact header")?;
        if granted > header_bytes.len() {
            file.write_all(&bytes[..granted - header_bytes.len()])
                .await
                .context("write command artifact output")?;
        }
        Ok(())
    }
}

async fn create_private(path: &Path) -> std::io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    options.open(path).await
}

async fn drain(
    mut reader: Box<dyn AsyncRead + Send + Unpin>,
    stream: Stream,
    capture: Arc<Mutex<Capture>>,
    spool: Arc<AsyncMutex<Spool>>,
) -> Result<()> {
    let mut bytes = [0_u8; READ_BYTES];
    loop {
        let count = reader
            .read(&mut bytes)
            .await
            .context("read command output")?;
        if count == 0 {
            return Ok(());
        }
        lock(&capture).append(stream, &bytes[..count]);
        spool.lock().await.append(stream, &bytes[..count]).await?;
    }
}
