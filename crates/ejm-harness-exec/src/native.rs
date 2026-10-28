//! Lazy shared native ownership. Native execution is not a sandbox.
//!
//! Hosts call [`dispatch_helper`] before ordinary startup. A protected local
//! capability authenticates bounded loopback transport. Each scope connection
//! is its private lifetime link; losing it starts cleanup in the surviving owner.

mod admission;
mod client;
mod owner;
mod placement;
mod recovery;
#[cfg(test)]
mod tests;
mod wire;

use std::{
    collections::VecDeque,
    ffi::{OsStr, OsString},
    io,
    path::{Path, PathBuf},
    process::{ExitCode, ExitStatus, Stdio},
    sync::{Arc, Mutex, Weak},
    time::Duration,
};

use anyhow::{Context, Result, ensure};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::{Mutex as AsyncMutex, watch},
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{Reservation, RestrictionState, governor::lock, process::OwnedChild};

pub use admission::{NativeOwnerAdmission, NativeOwnerPermit, NativeOwnerRequest};

/// A boxed native pipe, shared by in-process and remote children.
pub type NativeReader = Box<dyn AsyncRead + Send + Unpin>;
/// A boxed native stdin pipe, shared by in-process and remote children.
pub type NativeWriter = Box<dyn AsyncWrite + Send + Unpin>;

const HELPER_ARG: &str = "--ejm-native-owner-v2";

/// Dispatch the execution helper before argument parsing, UI, or runtime setup.
///
/// Returns `None` for ordinary invocations. Helper invocations create their own
/// small Tokio runtime and return an exit code to the host's `main` function.
#[must_use]
pub fn dispatch_helper() -> Option<Result<ExitCode>> {
    let mut args = std::env::args_os().skip(1);
    if args.next().as_deref() != Some(OsStr::new(HELPER_ARG)) {
        return None;
    }
    Some((|| {
        let root = args.next().context("native owner state root missing")?;
        let domain = args.next().context("native owner domain missing")?;
        let generation = args.next().context("native owner generation missing")?;
        let accounted = args.next().context("native owner admission missing")?;
        ensure!(args.next().is_none(), "unexpected native owner arguments");
        let domain = domain.to_str().context("invalid native owner domain")?;
        let generation = generation
            .to_str()
            .context("invalid native owner generation")?
            .parse::<Uuid>()?;
        let accounted = match accounted.to_str() {
            Some("admitted") => true,
            Some("local") => false,
            _ => anyhow::bail!("invalid native owner admission"),
        };
        placement::supported()?;
        // Keep ownership outside the runtime so teardown cannot expose a free
        // witness while a runtime thread still belongs to this helper.
        let _ownership = admission::adopt(Path::new(&root), domain)?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .max_blocking_threads(2)
            .build()
            .context("create native owner runtime")?;
        runtime.block_on(owner::serve(
            Path::new(&root),
            domain,
            generation,
            accounted,
        ))?;
        Ok(ExitCode::SUCCESS)
    })())
}

/// Explicit launch settings, including the requesting process's environment.
///
/// The final environment is private, bounded, and sent only to the authenticated
/// owner. It is never recorded in advertisements or recovery metadata.
#[derive(Clone, Serialize, Deserialize)]
pub struct NativeCommand {
    program: OsString,
    args: Vec<OsString>,
    cwd: OsString,
    env: Vec<(OsString, OsString)>,
    lifetime: Duration,
    reservation: Reservation,
    piped_stdin: bool,
}

impl NativeCommand {
    /// Snapshot the caller's cwd and environment for a new bounded launch.
    ///
    /// # Errors
    /// Returns an error when the caller's working directory is unavailable.
    pub fn new(program: impl AsRef<OsStr>) -> Result<Self> {
        Ok(Self {
            program: program.as_ref().to_owned(),
            args: Vec::new(),
            cwd: std::env::current_dir()?.into_os_string(),
            env: std::env::vars_os().collect(),
            lifetime: Duration::from_secs(120),
            reservation: Reservation::default(),
            piped_stdin: true,
        })
    }

    #[must_use]
    pub fn args(mut self, args: impl IntoIterator<Item = impl AsRef<OsStr>>) -> Self {
        self.args
            .extend(args.into_iter().map(|arg| arg.as_ref().to_owned()));
        self
    }

    #[must_use]
    pub fn current_dir(mut self, path: impl AsRef<Path>) -> Self {
        path.as_ref().as_os_str().clone_into(&mut self.cwd);
        self
    }

    #[must_use]
    pub fn env(mut self, key: impl AsRef<OsStr>, value: impl AsRef<OsStr>) -> Self {
        let mut command = self.environment_command();
        command.env(key, value);
        self.capture_environment(&command);
        self
    }

    #[must_use]
    pub fn env_remove(mut self, key: impl AsRef<OsStr>) -> Self {
        let mut command = self.environment_command();
        command.env_remove(key);
        self.capture_environment(&command);
        self
    }

    #[must_use]
    pub fn env_clear(mut self) -> Self {
        self.env.clear();
        self
    }

    #[must_use]
    pub const fn lifetime(mut self, lifetime: Duration) -> Self {
        self.lifetime = lifetime;
        self
    }

    #[must_use]
    pub fn reservation(mut self, reservation: Reservation) -> Self {
        self.reservation = reservation;
        self
    }

    #[must_use]
    pub const fn piped_stdin(mut self, piped: bool) -> Self {
        self.piped_stdin = piped;
        self
    }

    fn environment_command(&self) -> std::process::Command {
        let mut command = std::process::Command::new(&self.program);
        command
            .env_clear()
            .envs(self.env.iter().map(|(key, value)| (key, value)));
        command
    }

    fn capture_environment(&mut self, command: &std::process::Command) {
        self.env = command
            .get_envs()
            .filter_map(|(key, value)| value.map(|value| (key.to_owned(), value.to_owned())))
            .collect();
    }

    fn validate(&self) -> Result<()> {
        ensure!(!self.program.is_empty(), "native program is empty");
        ensure!(
            self.args.len() <= 4096,
            "native argument count exceeds 4096"
        );
        ensure!(
            self.env.len() <= 4096,
            "native environment count exceeds 4096"
        );
        ensure!(
            self.lifetime > Duration::ZERO && self.lifetime <= Duration::from_hours(24),
            "native lifetime must be within one day"
        );
        ensure!(
            self.reservation.memory_bytes > 0 && self.reservation.processes > 0,
            "native scope must reserve memory and processes"
        );
        let bytes = self.program.as_encoded_bytes().len()
            + self.cwd.as_encoded_bytes().len()
            + self
                .args
                .iter()
                .map(|arg| arg.as_encoded_bytes().len())
                .sum::<usize>()
            + self
                .env
                .iter()
                .map(|(key, value)| key.as_encoded_bytes().len() + value.as_encoded_bytes().len())
                .sum::<usize>();
        ensure!(
            bytes <= wire::MAX_LAUNCH_BYTES,
            "native launch exceeds private byte limit"
        );
        Ok(())
    }

    fn local_command(&self) -> tokio::process::Command {
        let mut command = tokio::process::Command::new(&self.program);
        command
            .args(&self.args)
            .current_dir(Path::new(&self.cwd))
            .env_clear()
            .envs(self.env.iter().map(|(key, value)| (key, value)))
            .stdin(if self.piped_stdin {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command
    }
}

/// An epoch and monotonic operation number. It never authorizes a replay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct NativeOperation {
    #[schemars(with = "String")]
    pub epoch: Uuid,
    pub id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NativeOperationState {
    Prepared,
    Running,
    Finished,
    Expired,
}

/// A cached view. Reading it never starts a helper or performs I/O.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct NativeSnapshot {
    #[schemars(with = "Option<String>")]
    pub epoch: Option<Uuid>,
    /// Last authenticated owner PID. This cached value does not establish liveness.
    pub owner_pid: Option<u32>,
    pub last_operation: Option<NativeOperation>,
    pub active_local_scopes: usize,
    /// Confirmed owner deaths observed by this handle. This is not a job ledger.
    pub owner_deaths: u64,
    #[schemars(with = "Option<String>")]
    pub last_dead_epoch: Option<Uuid>,
    pub owner_exclusivity: RestrictionState,
    pub owner_loss_detection: RestrictionState,
    pub descendant_cleanup: RestrictionState,
    pub owner_death_cleanup: RestrictionState,
    pub recovery: RestrictionState,
    pub domain_placement: RestrictionState,
}

impl Default for NativeSnapshot {
    fn default() -> Self {
        Self {
            epoch: None,
            owner_pid: None,
            last_operation: None,
            active_local_scopes: 0,
            owner_deaths: 0,
            last_dead_epoch: None,
            owner_exclusivity: RestrictionState::Enforced,
            owner_loss_detection: RestrictionState::Enforced,
            descendant_cleanup: if cfg!(windows) {
                RestrictionState::Enforced
            } else {
                RestrictionState::Monitored
            },
            owner_death_cleanup: if cfg!(windows) {
                RestrictionState::Enforced
            } else {
                RestrictionState::Unsupported
            },
            // Confirmed death retires future scheduling authority. It cannot
            // prove an arbitrary native tree empty or a workspace safe.
            recovery: RestrictionState::Monitored,
            domain_placement: if cfg!(windows) {
                RestrictionState::Unsupported
            } else {
                RestrictionState::Enforced
            },
        }
    }
}

#[derive(Clone)]
pub struct NativeOwner {
    inner: Arc<OwnerClient>,
    helper_executable: Option<PathBuf>,
    admission: Option<Arc<dyn NativeOwnerAdmission>>,
    selection: Option<Arc<OwnerSelection>>,
    activity: Arc<Mutex<NativeActivity>>,
}

#[derive(Default)]
struct NativeActivity {
    active_local_scopes: usize,
    owner_deaths: u64,
    last_dead_epoch: Option<Uuid>,
    dead_epochs: VecDeque<Uuid>,
}

struct OwnerSelection {
    state: Mutex<SelectedOwner>,
    prepare: AsyncMutex<()>,
}

struct SelectedOwner {
    client: Arc<OwnerClient>,
    checked: Option<tokio::time::Instant>,
}

struct OwnerClient {
    root: PathBuf,
    domain: String,
    cached: Mutex<Option<wire::Advertisement>>,
    snapshot: Mutex<NativeSnapshot>,
    startup: AsyncMutex<()>,
    recovery: AsyncMutex<RecoveryChecks>,
    readiness: AsyncMutex<Weak<Readiness>>,
}

struct Readiness {
    advertisement: wire::Advertisement,
    transport: Mutex<Option<wire::Transport>>,
}

impl Readiness {
    fn connected(&self) -> bool {
        let mut transport = lock(&self.transport);
        let Some(connection) = transport.as_ref() else {
            return false;
        };
        if matches!(connection.get_ref().try_read(&mut [0; 1]), Err(error) if error.kind() == io::ErrorKind::WouldBlock)
        {
            true
        } else {
            // This invalidates a cache only. Replacement still needs exclusive
            // OS ownership and a separately committed generation.
            transport.take();
            false
        }
    }

    fn disconnect(&self) {
        lock(&self.transport).take();
    }
}

/// A shared client keepalive bound to one authenticated helper generation.
///
/// Acquire this after queued work is prospectively eligible, then activate its
/// original admission ticket. Clones share one connection. Drop unused readiness
/// before waiting for resources again so an idle helper can release its charge.
#[derive(Clone)]
pub struct NativeOwnerReady {
    owner: NativeOwner,
    readiness: Arc<Readiness>,
}

impl NativeOwnerReady {
    #[must_use]
    pub fn owner(&self) -> &NativeOwner {
        &self.owner
    }

    #[must_use]
    pub fn generation(&self) -> Uuid {
        self.readiness.advertisement.epoch
    }

    /// Launch once in this generation, without replacing a lost helper.
    ///
    /// # Errors
    /// Returns validation, placement, transport, or launch errors. Helper loss
    /// never creates a replacement using an already admitted job reservation.
    pub async fn spawn(&self, command: NativeCommand) -> Result<NativeChild> {
        placement::supported()?;
        command.validate()?;
        ensure!(
            self.readiness.connected(),
            "native helper readiness connection closed before launch"
        );
        client::spawn(self, command).await
    }
}

#[derive(Default)]
struct RecoveryChecks {
    last_probe: Option<(Uuid, tokio::time::Instant)>,
}

impl NativeOwner {
    /// Prepare a lazy native owner handle under protected capacity state.
    ///
    /// Compatible domain keys must be shared across installed protocol versions.
    /// This constructor neither starts a process nor opens a socket.
    ///
    /// # Errors
    /// Rejects an empty or unsafe domain key.
    pub fn new(state_root: impl AsRef<Path>, domain_key: impl Into<String>) -> Result<Self> {
        let domain = domain_key.into();
        ensure!(
            !domain.is_empty()
                && domain.len() <= 128
                && domain
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-'),
            "invalid native execution domain key"
        );
        Ok(Self {
            helper_executable: None,
            admission: None,
            selection: None,
            activity: Arc::new(Mutex::new(NativeActivity::default())),
            inner: Arc::new(OwnerClient {
                root: state_root.as_ref().to_owned(),
                domain,
                cached: Mutex::new(None),
                snapshot: Mutex::new(NativeSnapshot::default()),
                startup: AsyncMutex::new(()),
                recovery: AsyncMutex::new(RecoveryChecks::default()),
                readiness: AsyncMutex::new(Weak::new()),
            }),
        })
    }

    /// Select an existing host executable that implements [`dispatch_helper`].
    ///
    /// Embedders with a managed worker may use its absolute path. The default
    /// reexecutes the current host binary. Selection starts no process.
    #[must_use]
    pub fn with_helper_executable(mut self, path: impl Into<PathBuf>) -> Self {
        self.helper_executable = Some(path.into());
        self
    }

    /// Require a separate shared helper commitment before process creation.
    #[must_use]
    pub fn with_admission(mut self, admission: Arc<dyn NativeOwnerAdmission>) -> Self {
        self.inner = self.selected_client();
        self.selection = Some(Arc::new(OwnerSelection {
            state: Mutex::new(SelectedOwner {
                client: self.inner.clone(),
                checked: None,
            }),
            prepare: AsyncMutex::new(()),
        }));
        self.admission = Some(admission);
        self
    }

    /// Establish helper readiness after eligibility and before ticket activation.
    ///
    /// The caller must bound readiness waits with cancellation or a deadline.
    /// One client shares one keepalive across all returned guards.
    ///
    /// # Errors
    /// Returns startup, accounting, placement, or authentication errors.
    pub async fn ready(&self) -> Result<NativeOwnerReady> {
        placement::supported()?;
        client::ready(&client::select(self).await?).await
    }

    #[must_use]
    pub fn snapshot(&self) -> NativeSnapshot {
        let mut snapshot = lock(&self.selected_client().snapshot).clone();
        let activity = lock(&self.activity);
        snapshot.active_local_scopes = activity.active_local_scopes;
        snapshot.owner_deaths = activity.owner_deaths;
        snapshot.last_dead_epoch = activity.last_dead_epoch;
        snapshot
    }

    /// Launch once in the current owner generation.
    ///
    /// The caller must bound the readiness wait with cancellation or a deadline.
    ///
    /// # Errors
    /// Returns a startup, domain, transport, or native launch error. A
    /// lost launch reply is uncertain and is never automatically replayed.
    pub async fn spawn(&self, command: NativeCommand) -> Result<NativeChild> {
        placement::supported()?;
        command.validate()?;
        self.ready().await?.spawn(command).await
    }

    /// Query an earlier operation in its original live generation, without effects.
    ///
    /// # Errors
    /// Rejects a replaced, unavailable, or incompatible owner. Expired IDs remain
    /// expired and are never converted into new launch attempts.
    pub async fn reconcile(&self, operation: NativeOperation) -> Result<NativeOperationState> {
        client::reconcile(&self.fixed(self.selected_client()), operation).await
    }

    fn selected_client(&self) -> Arc<OwnerClient> {
        self.selection.as_ref().map_or_else(
            || self.inner.clone(),
            |selection| lock(&selection.state).client.clone(),
        )
    }

    fn fixed(&self, inner: Arc<OwnerClient>) -> Self {
        Self {
            inner,
            helper_executable: self.helper_executable.clone(),
            admission: self.admission.clone(),
            selection: None,
            activity: self.activity.clone(),
        }
    }
}

/// Spawn through the shared owner, or keep legacy in-process ownership.
///
/// With an owner, the caller must bound readiness with cancellation or a deadline.
///
/// # Errors
/// Returns validation, transport, or operating-system launch errors.
pub async fn spawn(command: NativeCommand, owner: Option<&NativeOwner>) -> Result<NativeChild> {
    if let Some(owner) = owner {
        return owner.spawn(command).await;
    }
    command.validate()?;
    let child = crate::process::spawn_owned(command.local_command())?;
    Ok(NativeChild::Local(Box::new(child)))
}

/// Native ownership that survives caller-future cancellation in its supervisor.
pub enum NativeChild {
    Local(Box<OwnedChild>),
    #[doc(hidden)]
    Remote(RemoteChild),
}

/// Evidence available when a native scope finishes or loses its owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NativeCleanupState {
    /// Ownership remains charged until cleanup or owner death is established.
    Pending,
    /// The original owner finished supported native cleanup.
    Complete,
    /// The original owner's lifetime lock was released. Effects remain uncertain.
    OwnerDied,
}

#[doc(hidden)]
pub struct RemoteChild {
    pid: u32,
    stdin: Option<NativeWriter>,
    stdout: Option<NativeReader>,
    stderr: Option<NativeReader>,
    completion: watch::Receiver<Option<wire::Completion>>,
    cancel: CancellationToken,
}

impl NativeChild {
    /// Return cached cleanup evidence without I/O.
    ///
    /// Local children report `Pending`; a successful local `kill` confirms their
    /// supported cleanup. Owner death does not establish descendant cleanup.
    #[must_use]
    pub fn cleanup_state(&self) -> NativeCleanupState {
        match self {
            Self::Local(_) => NativeCleanupState::Pending,
            Self::Remote(child) => child
                .completion
                .borrow()
                .as_ref()
                .map_or(NativeCleanupState::Pending, |completion| completion.cleanup),
        }
    }

    #[must_use]
    pub const fn is_remote(&self) -> bool {
        matches!(self, Self::Remote(_))
    }

    #[must_use]
    pub fn id(&self) -> Option<u32> {
        match self {
            Self::Local(child) => child.id(),
            Self::Remote(child) => Some(child.pid),
        }
    }

    pub fn take_stdin(&mut self) -> Option<NativeWriter> {
        match self {
            Self::Local(child) => child
                .take_stdin()
                .map(|pipe| Box::new(pipe) as NativeWriter),
            Self::Remote(child) => child.stdin.take(),
        }
    }

    pub fn take_stdout(&mut self) -> Option<NativeReader> {
        match self {
            Self::Local(child) => child
                .take_stdout()
                .map(|pipe| Box::new(pipe) as NativeReader),
            Self::Remote(child) => child.stdout.take(),
        }
    }

    pub fn take_stderr(&mut self) -> Option<NativeReader> {
        match self {
            Self::Local(child) => child
                .take_stderr()
                .map(|pipe| Box::new(pipe) as NativeReader),
            Self::Remote(child) => child.stderr.take(),
        }
    }

    /// Wait until the owner has completed native cleanup.
    ///
    /// # Errors
    /// Owner loss reports uncertain cleanup rather than a synthetic exit status.
    pub async fn wait(&mut self) -> io::Result<ExitStatus> {
        match self {
            Self::Local(child) => child.wait().await,
            Self::Remote(child) => loop {
                if let Some(completion) = child.completion.borrow().as_ref() {
                    return completion.status();
                }
                child
                    .completion
                    .changed()
                    .await
                    .map_err(|_| io::Error::other("native owner lost; cleanup unverified"))?;
            },
        }
    }

    /// # Errors
    /// Returns the native wait error or reports uncertain owner loss.
    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        match self {
            Self::Local(child) => child.try_wait(),
            Self::Remote(child) => child
                .completion
                .borrow()
                .as_ref()
                .map(wire::Completion::status)
                .transpose(),
        }
    }

    /// # Errors
    /// Returns a native signalling error for legacy local execution.
    pub fn start_kill(&mut self) -> io::Result<()> {
        match self {
            Self::Local(child) => child.start_kill(),
            Self::Remote(child) => {
                child.cancel.cancel();
                Ok(())
            }
        }
    }

    /// # Errors
    /// Returns a native cleanup error or reports uncertain owner loss.
    pub async fn kill(&mut self) -> io::Result<ExitStatus> {
        match self {
            Self::Local(child) => child.kill().await,
            Self::Remote(child) => {
                child.cancel.cancel();
                self.wait().await
            }
        }
    }

    pub(crate) fn terminate(&mut self) -> io::Result<()> {
        match self {
            Self::Local(child) => child.terminate(),
            Self::Remote(_) => self.start_kill(),
        }
    }

    pub(crate) fn usage(&self) -> crate::monitor::ObservedUsage {
        match self {
            Self::Remote(child) => {
                child
                    .completion
                    .borrow()
                    .as_ref()
                    .map_or_else(Default::default, |completion| {
                        crate::monitor::ObservedUsage {
                            peak_memory_bytes: completion.peak_memory_bytes,
                            peak_processes: completion.peak_processes,
                            violation: completion.violation.map(wire::Violation::message),
                            sampling_failure: None,
                        }
                    })
            }
            Self::Local(_) => crate::monitor::ObservedUsage::default(),
        }
    }
}

impl Drop for RemoteChild {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}
