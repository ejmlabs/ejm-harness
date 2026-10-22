//! Host-owned capacity policy. Hot admission uses cached grants, never SQLite.

mod control;
mod helper;
mod learning;
mod provider;
mod provider_registry;
mod registry;
#[cfg(test)]
mod telemetry;
#[cfg(test)]
mod tests;

pub use control::{LaneSnapshot, TimingSummary};
pub use provider::ProviderSnapshot;

use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard, Weak, mpsc},
    time::{Duration, Instant},
};

use anyhow::{Context, Result, ensure};
use ejm_harness_exec::{
    AdmissionPolicy, Governor, GovernorSnapshot, HostObservations, HostSampler, Lease,
    ObservationAvailability, OperatingSnapshot, OperatingTarget, ResourceLimits,
    native::{NativeOwner, NativeSnapshot},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use crate::{HarnessConfig, ModelRequest, ProviderCapacity, types::timestamp_ms};
use control::{Outcome, Pressure, Timings, Tuner, workload_class};
use registry::{Budget, Registry, Renewal};

const CONTACT_TIME: Duration = Duration::from_secs(8);
const CONTROL_INTERVAL: Duration = Duration::from_secs(2);
const BASELINE_MEMORY: u64 = 32 * 1024 * 1024;
const DISPATCH_MEMORY: u64 = 512 * 1024;

/// Capacity policy belongs to the local owner, never to generated content.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CapacityMode {
    #[default]
    Auto,
    Fixed,
}

/// Optional values are explicit hard ceilings. Omission is not a zero budget.
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct CapacityPolicy {
    pub mode: CapacityMode,
    pub max_sessions: Option<usize>,
    pub max_model_requests: Option<usize>,
    pub max_jobs: Option<usize>,
    pub max_memory_bytes: Option<u64>,
    pub max_cpu_millis: Option<u32>,
    pub max_heavy_jobs: Option<usize>,
    pub max_processes: Option<u32>,
    pub max_output_bytes: Option<usize>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LearningState {
    #[default]
    Cold,
    Learned,
    Unavailable,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LearningStatus {
    pub state: LearningState,
    pub samples: u64,
    pub profile_count: usize,
}

/// A cheap cached snapshot. Reading it performs no probes or control work.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CapacitySnapshot {
    pub mode: CapacityMode,
    pub scope: String,
    pub coordinated: bool,
    pub hard_model_requests: usize,
    pub model_target: usize,
    pub model_running: usize,
    pub model_waiting: usize,
    pub hard_resources: ResourceLimits,
    pub operating: OperatingSnapshot,
    pub observations: HostObservations,
    pub reason: Option<String>,
    pub learning: LearningStatus,
    pub models: LaneSnapshot,
    pub tools: LaneSnapshot,
    pub provider: ProviderSnapshot,
    pub native: NativeSnapshot,
    #[serde(default)]
    pub native_helpers: NativeHelperSnapshot,
}

/// Cached coarse helper commitments. Counts include pending and draining owners.
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct NativeHelperSnapshot {
    pub occupied: usize,
    pub pending: usize,
    pub committed: usize,
    pub memory_bytes: u64,
    /// Process slots, including the helper runtime's threads on Linux.
    pub processes: u32,
}

/// Prepared before packages/providers, then shared with the final harness.
#[derive(Clone)]
pub struct CapacityContext {
    inner: Arc<Inner>,
}

pub(crate) struct PreparedProvider {
    capacity: CapacityContext,
    prepared: provider::PreparedProvider,
    domain: Option<String>,
}

impl PreparedProvider {
    pub(crate) fn commit(self) {
        self.prepared.commit();
        self.capacity.set_learning_domain(self.domain);
    }
}

struct Inner {
    governor: Arc<Governor>,
    mode: CapacityMode,
    hard_sessions: usize,
    hard_models: usize,
    context_bytes: u64,
    hard: ResourceLimits,
    dispatch_slots: usize,
    state: Mutex<State>,
    changed: Notify,
    wake: mpsc::SyncSender<()>,
    provider: provider::ProviderGate,
    owner: String,
    _baseline: Lease,
    _lifetime: Arc<std::fs::File>,
    native: NativeOwner,
}

impl Drop for Inner {
    fn drop(&mut self) {
        // A retained Executor must never spend this allowance after the last
        // capacity context releases its OS lifetime witness.
        self.governor.park_admission();
    }
}

struct State {
    observations: HostObservations,
    model_target: usize,
    model_running: usize,
    model_waiting: usize,
    contact: Option<Instant>,
    reason: Option<String>,
    generation: u64,
    learning: LearningStatus,
    tuner: Tuner,
    learning_domain: Option<String>,
    native_deaths: u64,
    native_helpers: NativeHelperSnapshot,
}

/// The model count is distinct from context memory and native execution slots.
pub(crate) struct ModelLease {
    inner: Arc<Inner>,
}

impl Drop for ModelLease {
    fn drop(&mut self) {
        let mut state = lock(&self.inner.state);
        state.model_running = state.model_running.saturating_sub(1);
        drop(state);
        self.inner.changed.notify_one();
    }
}

/// Observe a model attempt without owning an admission or memory lease.
pub(crate) struct ModelWork {
    inner: Arc<Inner>,
    started: Instant,
    preparing: Option<Instant>,
    provider_waiting: Option<Instant>,
    serving: Option<Instant>,
    timings: Timings,
    class: usize,
    cancel: CancellationToken,
    success: bool,
}

impl ModelWork {
    pub(crate) fn complete(&mut self) {
        self.success = true;
    }

    pub(crate) fn prepare_started(&mut self) {
        self.timings.queue = self.started.elapsed();
        self.preparing = Some(Instant::now());
    }

    pub(crate) fn prepared(&mut self, bytes: u64) {
        self.timings.preparation = self
            .preparing
            .take()
            .map_or(Duration::ZERO, |start| start.elapsed());
        self.class = workload_class(bytes);
        self.provider_waiting = Some(Instant::now());
    }

    pub(crate) fn provider_started(&mut self) {
        self.timings.provider_wait = self
            .provider_waiting
            .take()
            .map_or(Duration::ZERO, |start| start.elapsed());
        self.serving = Some(Instant::now());
    }
}

impl Drop for ModelWork {
    fn drop(&mut self) {
        if self.preparing.is_none() && self.provider_waiting.is_none() && self.serving.is_none() {
            self.timings.queue = self.started.elapsed();
        }
        if let Some(start) = self.preparing {
            self.timings.preparation = start.elapsed();
        }
        if let Some(start) = self.provider_waiting {
            self.timings.provider_wait = start.elapsed();
        }
        self.timings.service = self.serving.map_or(Duration::ZERO, |start| start.elapsed());
        let outcome = if self.success {
            Outcome::Successful
        } else if self.cancel.is_cancelled() {
            Outcome::Cancelled
        } else {
            Outcome::Failed
        };
        lock(&self.inner.state)
            .tuner
            .record_model(outcome, &self.timings, self.class);
    }
}

pub(crate) struct ToolWork {
    inner: Arc<Inner>,
    started: Instant,
    running: Option<Instant>,
    class: usize,
    cancel: CancellationToken,
    success: bool,
}

impl ToolWork {
    pub(crate) fn running(&mut self, bytes: u64) {
        self.class = workload_class(bytes);
        self.running = Some(Instant::now());
    }
    pub(crate) fn complete(&mut self, success: bool) {
        self.success = success;
    }
}

impl Drop for ToolWork {
    fn drop(&mut self) {
        let now = Instant::now();
        let timings = Timings {
            queue: self.running.map_or_else(
                || self.started.elapsed(),
                |start| start.duration_since(self.started),
            ),
            preparation: Duration::ZERO,
            provider_wait: Duration::ZERO,
            service: self
                .running
                .map_or(Duration::ZERO, |start| now.duration_since(start)),
        };
        let outcome = if self.success {
            Outcome::Successful
        } else if self.cancel.is_cancelled() {
            Outcome::Cancelled
        } else {
            Outcome::Failed
        };
        lock(&self.inner.state)
            .tuner
            .record_tool(outcome, &timings, self.class);
    }
}

struct ModelWaiter(Arc<Inner>);
impl Drop for ModelWaiter {
    fn drop(&mut self) {
        let mut state = lock(&self.0.state);
        state.model_waiting = state.model_waiting.saturating_sub(1);
    }
}

impl CapacityContext {
    /// Resolve hard ceilings and prepare coordinated capacity before package load.
    /// Call on a blocking worker when the host has an input or async loop.
    ///
    /// # Errors
    /// Rejects invalid ceilings, unsafe shared state, unsupported observations,
    /// protocol mismatch, or unavailable bounded registration.
    pub fn prepare(config: &mut HarnessConfig, policy: CapacityPolicy) -> Result<Self> {
        Self::prepare_in(config, policy, &registry_path()?)
    }

    /// Prepare capacity with an explicit helper executable for an embedding host.
    /// The executable must dispatch [`ejm_harness_exec::native::dispatch_helper`]
    /// before ordinary startup. No helper starts during preparation.
    ///
    /// # Errors
    /// Has the same validation and protected-state errors as [`Self::prepare`].
    pub fn prepare_with_native_helper(
        config: &mut HarnessConfig,
        policy: CapacityPolicy,
        helper: &Path,
    ) -> Result<Self> {
        Self::prepare_native_in(config, policy, &registry_path()?, Some(helper))
    }

    /// Prepare an isolated fixture budget. Never use this for production hosts.
    /// All fixture processes must use the same private root to test coordination.
    ///
    /// # Errors
    /// Has the same validation and protected-state errors as [`Self::prepare`].
    #[cfg(feature = "test-support")]
    pub fn prepare_isolated(
        config: &mut HarnessConfig,
        policy: CapacityPolicy,
        root: &Path,
    ) -> Result<Self> {
        Self::prepare_in(config, policy, root)
    }

    /// Prepare an isolated fixture budget with an explicit native helper.
    ///
    /// # Errors
    /// Has the same errors as [`Self::prepare_with_native_helper`].
    #[cfg(feature = "test-support")]
    pub fn prepare_isolated_with_native_helper(
        config: &mut HarnessConfig,
        policy: CapacityPolicy,
        root: &Path,
        helper: &Path,
    ) -> Result<Self> {
        Self::prepare_native_in(config, policy, root, Some(helper))
    }

    fn prepare_in(config: &mut HarnessConfig, policy: CapacityPolicy, root: &Path) -> Result<Self> {
        Self::prepare_native_in(config, policy, root, None)
    }

    fn prepare_native_in(
        config: &mut HarnessConfig,
        policy: CapacityPolicy,
        root: &Path,
        helper: Option<&Path>,
    ) -> Result<Self> {
        let root = std::path::absolute(root)?;
        let root = root.as_path();
        let mut sampler = HostSampler::default();
        let observations = sampler.sample();
        resolve(config, &policy, &observations)?;
        let native = NativeOwner::new(root, execution_domain(&observations))?;
        let native = if let Some(path) = helper {
            native.with_helper_executable(path)
        } else {
            native
        };
        let governor = Arc::new(Governor::new_with_policy(
            config.resources.clone(),
            match policy.mode {
                CapacityMode::Auto => AdmissionPolicy::Automatic,
                CapacityMode::Fixed => AdmissionPolicy::Fixed,
            },
        )?);
        let (dispatch_slots, floor) = runtime_baseline(&config.resources)?;
        let owner = uuid::Uuid::new_v4().to_string();
        let mut registry = Registry::open(root)?;
        let environment =
            learning::environment_key(&observations, u64::try_from(config.context_max_bytes)?);
        let learning = learning::Learning::open(&mut registry, &environment, None, timestamp_ms());
        let desired_models = if policy.mode == CapacityMode::Auto {
            config.max_model_requests.min(8)
        } else {
            config.max_model_requests
        };
        let desired_jobs = if policy.mode == CapacityMode::Auto {
            config.resources.max_jobs.min(4)
        } else {
            config.resources.max_jobs
        };
        let target = Budget::requested(&config.resources, desired_models, desired_jobs);
        let grant = registry.register_with_floor(&owner, target, floor, &observations)?;
        governor.install_target(
            1,
            grant.target(&config.resources),
            Some(Instant::now() + CONTACT_TIME),
        )?;
        let baseline_bytes = floor.memory_bytes;
        let Some(baseline) = governor.try_acquire_memory("runtime-baseline", baseline_bytes)?
        else {
            registry.release_departed(&governor)?;
            anyhow::bail!(
                "waiting for shared capacity to fit the runtime and dispatch baseline ({baseline_bytes} bytes)"
            );
        };
        let (wake, wakeups) = mpsc::sync_channel(1);
        let native = native.with_admission(Arc::new(helper::NativeAdmission::new(
            root,
            &owner,
            wake.clone(),
        )));
        let native_helpers = registry.native_helpers()?;
        let inner = Arc::new(Inner {
            governor,
            mode: policy.mode,
            hard_sessions: config.max_sessions,
            hard_models: config.max_model_requests,
            context_bytes: u64::try_from(config.context_max_bytes)?,
            hard: config.resources.clone(),
            dispatch_slots,
            state: Mutex::new(State {
                observations,
                model_target: grant.models,
                model_running: 0,
                model_waiting: 0,
                contact: Some(Instant::now() + CONTACT_TIME),
                reason: (grant.models == 0).then(|| "waiting for shared host capacity".into()),
                generation: 1,
                learning: learning.status(),
                tuner: Tuner::new(desired_models, desired_jobs),
                learning_domain: None,
                native_deaths: 0,
                native_helpers,
            }),
            changed: Notify::new(),
            provider: provider::ProviderGate::new(
                owner.clone(),
                config.max_model_requests,
                wake.clone(),
            ),
            wake,
            owner,
            _baseline: baseline,
            _lifetime: registry.lifetime()?,
            native,
        });
        let weak = Arc::downgrade(&inner);
        let governor = inner.governor.clone();
        std::thread::Builder::new()
            .name("ejm-capacity".into())
            .spawn(move || coordinate(&weak, registry, sampler, &governor, &wakeups, learning))
            .context("start bounded host capacity coordinator")?;
        Ok(Self { inner })
    }

    #[must_use]
    pub fn governor(&self) -> Arc<Governor> {
        self.inner.governor.clone()
    }

    /// Share lazy native execution ownership with this capacity participant.
    /// Reading this handle starts no helper and performs no I/O.
    #[must_use]
    pub fn native_owner(&self) -> NativeOwner {
        self.inner.native.clone()
    }

    pub(crate) fn validate_config(&self, config: &HarnessConfig) -> Result<()> {
        ensure!(
            self.inner.hard_sessions == config.max_sessions
                && self.inner.hard_models == config.max_model_requests
                && self.inner.context_bytes == u64::try_from(config.context_max_bytes)?
                && serde_json::to_value(&self.inner.hard)?
                    == serde_json::to_value(&config.resources)?,
            "configuration differs from prepared capacity"
        );
        Ok(())
    }

    pub(crate) fn dispatch_slots(&self) -> usize {
        self.inner.dispatch_slots
    }

    /// Coalesce an idle-to-busy transition before acquiring from the shared
    /// governor. This performs no probing, allocation, or registry I/O.
    pub fn notify_demand(&self) {
        let _ = self.inner.wake.try_send(());
    }

    pub(crate) fn set_learning_domain(&self, domain: Option<String>) {
        lock(&self.inner.state).learning_domain = domain;
        self.notify_demand();
    }

    /// Configure nonsecret provider scope and ceilings before model work starts.
    /// Quota identity comes from the host, independently of credential contents.
    ///
    /// # Errors
    /// Rejects invalid policy or a change while streams or quota waits are active.
    pub fn configure_provider(&self, policy: ProviderCapacity) -> Result<()> {
        self.prepare_provider(policy)?.commit();
        Ok(())
    }

    pub(crate) fn prepare_provider(&self, policy: ProviderCapacity) -> Result<PreparedProvider> {
        let domain = policy.domain.as_ref().map(|domain| domain.id().to_owned());
        Ok(PreparedProvider {
            capacity: self.clone(),
            prepared: self.inner.provider.prepare(policy)?,
            domain,
        })
    }

    /// Request a local owner-authorized cooldown clear. This does not reset
    /// prepaid provider usage or retry an earlier request.
    pub fn clear_provider_cooldown(&self) {
        self.inner.provider.request_clear_cooldown();
    }

    pub(crate) async fn acquire_provider(
        &self,
        request: &ModelRequest,
        cancel: &CancellationToken,
    ) -> Result<provider::ProviderLease> {
        self.inner.provider.acquire(request, cancel).await
    }

    #[must_use]
    pub fn snapshot(&self) -> CapacitySnapshot {
        let state = lock(&self.inner.state);
        let mut observations = state.observations.clone();
        observations.refresh_age();
        let valid = state.contact.is_some_and(|until| Instant::now() < until);
        CapacitySnapshot {
            mode: self.inner.mode,
            scope: "current OS user and visible host/container ancestors".into(),
            coordinated: valid,
            hard_model_requests: self.inner.hard_models,
            model_target: state.model_target,
            model_running: state.model_running,
            model_waiting: state.model_waiting,
            hard_resources: self.inner.hard.clone(),
            operating: self.inner.governor.operating_snapshot(),
            observations,
            reason: if valid {
                state.reason.clone()
            } else {
                Some("host coordination unavailable; new work is parked".into())
            },
            learning: state.learning.clone(),
            models: state.tuner.model_snapshot(),
            tools: state.tuner.tool_snapshot(),
            provider: self.inner.provider.snapshot(),
            native: self.inner.native.snapshot(),
            native_helpers: state.native_helpers,
        }
    }

    pub(crate) fn model_work(&self, cancel: &CancellationToken, started: Instant) -> ModelWork {
        ModelWork {
            inner: self.inner.clone(),
            started,
            preparing: None,
            provider_waiting: None,
            serving: None,
            timings: Timings::default(),
            class: 0,
            cancel: cancel.clone(),
            success: false,
        }
    }

    pub(crate) async fn acquire_model(&self, cancel: &CancellationToken) -> Result<ModelLease> {
        lock(&self.inner.state).model_waiting += 1;
        self.notify_demand();
        let _waiter = ModelWaiter(self.inner.clone());
        loop {
            let changed = self.inner.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            {
                let mut state = lock(&self.inner.state);
                if !cancel.is_cancelled()
                    && state.contact.is_some_and(|until| Instant::now() < until)
                    && state.model_running < state.model_target
                {
                    state.model_running += 1;
                    return Ok(ModelLease {
                        inner: self.inner.clone(),
                    });
                }
            }
            tokio::select! {
                biased;
                () = cancel.cancelled() => anyhow::bail!("cancelled while waiting for model capacity"),
                () = &mut changed => {}
            }
        }
    }

    pub(crate) fn tool_work(&self, cancel: &CancellationToken, started: Instant) -> ToolWork {
        self.notify_demand();
        ToolWork {
            inner: self.inner.clone(),
            started,
            running: None,
            class: 0,
            cancel: cancel.clone(),
            success: false,
        }
    }

    pub(crate) async fn context_memory(
        &self,
        session: &str,
        cancel: &CancellationToken,
    ) -> Result<Lease> {
        // Request copies, serialization buffers, tools and the bounded output tail.
        let bytes = self
            .inner
            .context_bytes
            .saturating_mul(3)
            .saturating_add(64 * 1024 * 1024);
        self.inner
            .governor
            .acquire_memory(session, bytes, cancel)
            .await
    }
}

fn runtime_baseline(limits: &ResourceLimits) -> Result<(usize, OperatingTarget)> {
    // A selected call is at most 64 KiB on the wire. The pool includes its
    // bounded parsed/clone estimate and never holds complete response batches.
    let by_memory = usize::try_from(limits.memory_bytes / 8 / DISPATCH_MEMORY)
        .unwrap_or(usize::MAX)
        .max(1);
    let slots = limits
        .max_queue
        .saturating_sub(32)
        .clamp(1, 128)
        .min(by_memory);
    let memory_bytes = BASELINE_MEMORY
        .checked_add(u64::try_from(slots)?.saturating_mul(DISPATCH_MEMORY))
        .context("runtime baseline memory overflow")?;
    ensure!(
        memory_bytes <= limits.memory_bytes,
        "runtime and dispatch baseline ({memory_bytes} bytes) exceeds the memory ceiling ({} bytes)",
        limits.memory_bytes
    );
    Ok((
        slots,
        OperatingTarget {
            memory_bytes,
            ..OperatingTarget::default()
        },
    ))
}

fn execution_domain(observations: &HostObservations) -> String {
    let mut digest = Sha256::new();
    digest.update(b"ejm-native-domain-v1");
    digest.update(std::env::consts::OS.as_bytes());
    digest.update(std::env::consts::ARCH.as_bytes());
    digest.update(
        observations
            .cpu_millis
            .value
            .unwrap_or_default()
            .to_le_bytes(),
    );
    digest.update(
        observations
            .memory_bytes
            .value
            .unwrap_or_default()
            .to_le_bytes(),
    );
    digest.update(
        observations
            .process_limit
            .value
            .unwrap_or_default()
            .to_le_bytes(),
    );
    digest.update(
        observations
            .descriptor_limit
            .value
            .unwrap_or_default()
            .to_le_bytes(),
    );
    for domain in &observations.domains {
        for id in [Some(domain.id.as_str()), domain.parent.as_deref()] {
            let value = id.unwrap_or_default();
            digest.update(u64::try_from(value.len()).unwrap_or(u64::MAX).to_le_bytes());
            digest.update(value.as_bytes());
        }
        digest.update(domain.cpu_millis.unwrap_or_default().to_le_bytes());
        digest.update(domain.memory_bytes.unwrap_or_default().to_le_bytes());
        digest.update(domain.max_processes.unwrap_or_default().to_le_bytes());
    }
    format!("{:x}", digest.finalize())
}

/// Reset aggregate hints only. Active grants and provider charges are separate.
///
/// # Errors
/// Returns a protected-state or bounded SQLite error without touching sessions.
pub fn reset_capacity_learning() -> Result<usize> {
    learning::reset(&mut Registry::open(&registry_path()?)?)
}

fn registry_path() -> Result<PathBuf> {
    #[cfg(windows)]
    let home = std::env::var_os("LOCALAPPDATA")
        .context("LOCALAPPDATA is required for shared capacity state")?;
    #[cfg(not(windows))]
    let home = std::env::var_os("HOME").context("HOME is required for shared capacity state")?;
    Ok(PathBuf::from(home).join(".ejm-harness-capacity"))
}

fn resolve(
    config: &mut HarnessConfig,
    policy: &CapacityPolicy,
    observations: &HostObservations,
) -> Result<()> {
    let cpu = observations
        .cpu_millis
        .value
        .context("effective CPU capacity is unavailable")?;
    let memory = observations
        .memory_bytes
        .value
        .context("effective memory capacity is unavailable")?;
    let usable = memory.saturating_sub((memory / 8).max(256 * 1024 * 1024));
    let auto = policy.mode == CapacityMode::Auto;
    config.max_sessions = policy.max_sessions.unwrap_or(if auto { 4096 } else { 100 });
    config.max_model_requests = policy
        .max_model_requests
        .unwrap_or(if auto { 256 } else { 8 });
    config.resources.max_jobs = policy.max_jobs.unwrap_or(if auto {
        usize::try_from(cpu.div_ceil(250))?.clamp(1, 128)
    } else {
        4
    });
    config.resources.memory_bytes = policy
        .max_memory_bytes
        .unwrap_or(if auto { usable / 2 } else { 512 * 1024 * 1024 })
        .min(usable);
    config.resources.cpu_millis = policy
        .max_cpu_millis
        .unwrap_or(if auto {
            cpu
        } else {
            config.resources.cpu_millis
        })
        .min(cpu);
    config.resources.max_heavy_jobs = policy.max_heavy_jobs.unwrap_or(if auto {
        usize::try_from(cpu.div_ceil(1_000))?.min(config.resources.max_jobs)
    } else {
        config.resources.max_heavy_jobs
    });
    config.resources.max_processes = policy.max_processes.unwrap_or(if auto {
        u32::try_from(config.resources.max_jobs)?.saturating_mul(4)
    } else {
        config.resources.max_processes
    });
    config.resources.max_output_bytes = policy.max_output_bytes.unwrap_or(if auto {
        config.resources.max_jobs.saturating_mul(64 * 1024)
    } else {
        config.resources.max_output_bytes
    });
    ensure!(
        (1..=4096).contains(&config.max_sessions),
        "managed agent ceiling must be 1..4096"
    );
    ensure!(
        (1..=256).contains(&config.max_model_requests),
        "model ceiling must be 1..256"
    );
    ensure!(
        (1..=128).contains(&config.resources.max_jobs),
        "local job ceiling must be 1..128"
    );
    ensure!(
        config.resources.memory_bytes >= BASELINE_MEMORY,
        "memory ceiling cannot fit the 32 MiB runtime baseline"
    );
    if let Some(limit) = policy.max_memory_bytes {
        ensure!(
            limit >= BASELINE_MEMORY,
            "explicit memory ceiling cannot fit the runtime baseline"
        );
    }
    Ok(())
}

fn coordinate(
    weak: &Weak<Inner>,
    mut registry: Registry,
    mut sampler: HostSampler,
    governor: &Arc<Governor>,
    wakeups: &mpsc::Receiver<()>,
    mut learning: learning::Learning,
) {
    let mut last_cycle = Instant::now();
    loop {
        let _ = wakeups.recv_timeout(CONTROL_INTERVAL);
        // Demand bursts coalesce. Shared transactions remain below two per
        // second without putting a database operation on ordinary local work.
        if let Some(delay) = Duration::from_millis(500).checked_sub(last_cycle.elapsed()) {
            std::thread::sleep(delay);
        }
        let elapsed = last_cycle.elapsed();
        last_cycle = Instant::now();
        let Some(inner) = weak.upgrade() else {
            let used = governor.snapshot();
            if used.running == 0
                && used.queued == 0
                && governor.operating_snapshot().memory_leases == 0
            {
                learning.flush(&mut registry, timestamp_ms(), true);
                if registry.release_departed(governor).is_ok() {
                    break;
                }
            }
            continue;
        };
        let admission_revision = inner.governor.admission_revision();
        let native_deaths = inner.native.snapshot().owner_deaths;
        let observations = sampler.sample();
        let usage = inner.governor.snapshot();
        let pressure = pressure(&observations, elapsed);
        let environment = learning::environment_key(&observations, inner.context_bytes);
        let (domain, evidence_counts) = {
            let state = lock(&inner.state);
            (state.learning_domain.clone(), state.tuner.class_samples())
        };
        let reset_evidence = learning.refresh(
            &mut registry,
            &environment,
            domain.as_deref(),
            timestamp_ms(),
            evidence_counts,
        );
        let mut state = lock(&inner.state);
        state.observations = observations.clone();
        if pressure != Pressure::Clear || reset_evidence {
            state.tuner.cancel_probes();
        }
        let recovering = recover_native_owner(&inner, &mut state, &mut learning, native_deaths);
        let waiting = state.model_waiting;
        if reset_evidence {
            state.tuner.reset_evidence();
        }
        let backoff = prepare_cycle_feedback(
            &inner,
            &mut state,
            &mut learning,
            elapsed,
            [waiting > 0, usage.queued > 0],
            pressure,
        );
        let jobs = if recovering { 1 } else { state.tuner.jobs() };
        let requested = Budget::requested(&inner.hard, state.tuner.models(), jobs);
        let previous_granted = [
            state.model_target,
            governor.operating_snapshot().target.max_jobs,
        ];
        drop(state);
        let result = renew_grant(
            &inner,
            &mut registry,
            admission_revision,
            requested,
            &usage,
            &observations,
            waiting,
        );
        if result.is_err() || inner.provider.coordinate(&mut registry).is_err() {
            inner.provider.failed();
        }
        publish_grant(
            &inner,
            &registry,
            &result,
            previous_granted[0],
            requested,
            pressure,
        );
        record_cycle_feedback(
            &inner,
            &mut learning,
            elapsed,
            [waiting > 0, usage.queued > 0],
            (!backoff).then_some(pressure),
            (requested, previous_granted),
            &result,
        );
        if let Ok(helpers) = registry.native_helpers() {
            lock(&inner.state).native_helpers = helpers;
        }
        learning.flush(&mut registry, timestamp_ms(), false);
        lock(&inner.state).learning = learning.status();
    }
}

fn prepare_cycle_feedback(
    inner: &Inner,
    state: &mut State,
    learning: &mut learning::Learning,
    elapsed: Duration,
    demand: [bool; 2],
    pressure: Pressure,
) -> bool {
    if inner.mode != CapacityMode::Auto {
        return false;
    }
    let backoff = pressure == Pressure::Busy || state.tuner.needs_backoff();
    if backoff {
        // A current or pending Busy window cannot grow. Reduce before
        // renewal so fast wakes preserve the two-window response bound.
        state.tuner.tick(
            elapsed,
            demand,
            pressure,
            [0; 2],
            [inner.hard_models, inner.hard.max_jobs],
        );
    } else {
        learning.apply_hints(
            &mut state.tuner,
            [inner.hard_models, inner.hard.max_jobs],
            pressure == Pressure::Clear,
        );
    }
    backoff
}

fn record_cycle_feedback(
    inner: &Inner,
    learning: &mut learning::Learning,
    elapsed: Duration,
    demand: [bool; 2],
    pressure: Option<Pressure>,
    (requested, previous_granted): (Budget, [usize; 2]),
    result: &Result<Renewal>,
) {
    let mut state = lock(&inner.state);
    if inner.mode == CapacityMode::Auto && result.is_err() {
        state.tuner.cancel_probes();
        state.tuner.reset_evidence();
    }
    if inner.mode == CapacityMode::Auto
        && let Some(pressure) = pressure
    {
        // A newly installed grant cannot validate work completed under the
        // prior target. Exclude that transition interval from probe evidence.
        // None means backoff already consumed this cycle's elapsed time.
        let (granted, feedback_pressure) = if let Ok(renewal) = result {
            state.tuner.observe_plan(
                [requested.models, requested.jobs],
                [renewal.planned.models, renewal.planned.jobs],
                renewal.plan_epoch,
                renewal.plan_refreshed,
            );
            (
                [
                    previous_granted[0].min(renewal.granted.models),
                    previous_granted[1].min(renewal.granted.jobs),
                ],
                pressure,
            )
        } else {
            ([0; 2], pressure.max(Pressure::Unknown))
        };
        state.tuner.tick(
            elapsed,
            demand,
            feedback_pressure,
            granted,
            [inner.hard_models, inner.hard.max_jobs],
        );
    }
    learning.observe(&state.tuner);
}

fn recover_native_owner(
    inner: &Inner,
    state: &mut State,
    learning: &mut learning::Learning,
    deaths: u64,
) -> bool {
    if deaths <= state.native_deaths {
        return false;
    }
    state.native_deaths = deaths;
    learning.native_owner_lost(state.tuner.class_samples());
    if inner.mode == CapacityMode::Auto {
        state.tuner.apply_hint(1, 1);
    }
    true
}

fn renew_grant(
    inner: &Inner,
    registry: &mut Registry,
    admission_revision: u64,
    requested: Budget,
    usage: &GovernorSnapshot,
    observations: &HostObservations,
    waiting: usize,
) -> Result<Renewal> {
    let running = lock(&inner.state).model_running;
    registry.renew_with_floor(
        &inner.owner,
        requested,
        running,
        waiting,
        usage,
        inner.governor.admission_demand(),
        inner.governor.admission_floor(),
        inner.governor.admission_activity(),
        observations,
        || {
            let mut state = lock(&inner.state);
            state.model_target = 0;
            install_grant(
                inner,
                &mut state,
                admission_revision,
                OperatingTarget::default(),
            )
        },
        |grant| {
            let mut state = lock(&inner.state);
            install_grant(
                inner,
                &mut state,
                admission_revision,
                grant.target(&inner.hard),
            )?;
            state.model_target = grant.models;
            state.contact = Some(Instant::now() + CONTACT_TIME);
            inner.changed.notify_waiters();
            Ok((inner.governor.snapshot(), state.model_running))
        },
    )
}

fn install_grant(
    inner: &Inner,
    state: &mut State,
    admission_revision: u64,
    target: OperatingTarget,
) -> Result<()> {
    ensure!(
        inner.native.snapshot().owner_deaths == state.native_deaths,
        "native owner changed after sampling; refresh observations before admission"
    );
    let generation = state
        .generation
        .checked_add(1)
        .context("capacity generation exhausted")?;
    inner.governor.install_target_for_revision(
        admission_revision,
        generation,
        target,
        Some(Instant::now() + CONTACT_TIME),
    )?;
    state.generation = generation;
    Ok(())
}

fn publish_grant(
    inner: &Inner,
    registry: &Registry,
    result: &Result<Renewal>,
    previous_target: usize,
    requested: Budget,
    pressure: Pressure,
) {
    let mut state = lock(&inner.state);
    if let Ok(renewal) = result {
        let grant = renewal.granted;
        let changed = previous_target != grant.models || state.contact.is_none();
        state.model_target = grant.models;
        state.contact = Some(Instant::now() + CONTACT_TIME);
        state.reason = if let Some(reason) = registry.reason() {
            Some(reason)
        } else if pressure == Pressure::Busy {
            Some("resource pressure; targets are draining".into())
        } else if pressure == Pressure::Unknown {
            Some("incomplete observations; upward probes are paused".into())
        } else {
            (grant.models < requested.models || grant.jobs < requested.jobs).then(|| {
                "sharing host capacity with other processes or draining reservations".into()
            })
        };
        if changed {
            inner.changed.notify_waiters();
        }
    } else if let Err(error) = result {
        state.reason = Some(format!(
            "shared capacity renewal failed ({}); admission expires within eight seconds",
            renewal_failure_category(error)
        ));
        inner.changed.notify_waiters();
    }
}

fn renewal_failure_category(error: &anyhow::Error) -> &'static str {
    if let Some(database) = error.downcast_ref::<rusqlite::Error>() {
        if matches!(
            database,
            rusqlite::Error::SqliteFailure(failure, _)
                if matches!(failure.code,
                    rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked)
        ) {
            "database contention"
        } else {
            "database failure"
        }
    } else if error.downcast_ref::<std::io::Error>().is_some() {
        "filesystem failure"
    } else {
        "admission failure"
    }
}

fn pressure(observations: &HostObservations, elapsed: Duration) -> Pressure {
    let critical = observations.memory_bytes.is_fresh()
        && observations.available_memory_bytes.is_fresh()
        && observations.cpu_millis.is_fresh()
        && observations.cpu_usage_percent.is_fresh()
        && observations.domain_visibility == ObservationAvailability::Measured;
    if observations.available_memory_bytes.is_fresh()
        && observations
            .available_memory_bytes
            .value
            .is_some_and(|free| free < BASELINE_MEMORY * 4)
        || observations.cpu_usage_percent.is_fresh()
            && observations
                .cpu_usage_percent
                .value
                .is_some_and(|load| load >= 95.0)
        || elapsed > CONTROL_INTERVAL * 2
    {
        return Pressure::Busy;
    }
    if critical
        && observations
            .cpu_usage_percent
            .value
            .is_some_and(|load| load < 95.0)
    {
        Pressure::Clear
    } else {
        Pressure::Unknown
    }
}

pub(super) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
