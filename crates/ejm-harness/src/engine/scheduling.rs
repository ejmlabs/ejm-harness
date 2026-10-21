use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Cached local run counts. Each managed run occupies at most one listed phase.
///
/// These counts describe engine work, not OS CPU use, native child liveness, or
/// a host-wide total. Reading them performs no journal or host observation I/O.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SchedulingSnapshot {
    /// Runs retained by the engine, including waiting and finishing runs.
    pub managed_runs: usize,
    /// Managed runs outside the explicit waiting, preparation and service phases.
    /// This includes journal and hook work; it is not an OS runnable-thread count.
    pub runnable_runs: usize,
    /// Runs waiting for a model slot, coordinated model credit or context memory.
    pub model_waiting: usize,
    /// Runs preparing admitted context, hooks and a provider request.
    pub model_preparing: usize,
    /// Runs waiting for provider selection or coordinated provider quota.
    pub provider_waiting: usize,
    /// Runs inside a provider callback, including streamed-output backpressure.
    pub model_running: usize,
    /// Runs waiting to enter the bounded tool-dispatch pool.
    pub tool_dispatch_waiting: usize,
    /// Runs waiting for their workspace mutation lock.
    pub workspace_waiting: usize,
    /// Runs waiting for a tool reservation in the shared governor.
    pub tool_resource_waiting: usize,
    /// Runs inside a tool callback. Self-admitting tools may wait internally.
    /// Callback completion does not prove that uncertain native work has exited.
    pub tool_running: usize,
}

#[derive(Clone, Default)]
pub(super) struct Scheduling(Arc<Mutex<SchedulingSnapshot>>);

#[derive(Clone, Copy)]
pub(super) enum Phase {
    Managed,
    ModelWaiting,
    ModelPreparing,
    ProviderWaiting,
    ModelRunning,
    ToolDispatchWaiting,
    WorkspaceWaiting,
    ToolResourceWaiting,
    ToolRunning,
}

impl Phase {
    fn count(self, snapshot: &mut SchedulingSnapshot) -> &mut usize {
        match self {
            Self::Managed => &mut snapshot.managed_runs,
            Self::ModelWaiting => &mut snapshot.model_waiting,
            Self::ModelPreparing => &mut snapshot.model_preparing,
            Self::ProviderWaiting => &mut snapshot.provider_waiting,
            Self::ModelRunning => &mut snapshot.model_running,
            Self::ToolDispatchWaiting => &mut snapshot.tool_dispatch_waiting,
            Self::WorkspaceWaiting => &mut snapshot.workspace_waiting,
            Self::ToolResourceWaiting => &mut snapshot.tool_resource_waiting,
            Self::ToolRunning => &mut snapshot.tool_running,
        }
    }
}

impl Scheduling {
    fn counts(&self) -> MutexGuard<'_, SchedulingSnapshot> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(super) fn enter(&self, phase: Phase) -> Activity {
        let mut counts = self.counts();
        let count = phase.count(&mut counts);
        *count = count.saturating_add(1);
        Activity {
            scheduling: self.clone(),
            phase,
        }
    }

    pub(super) fn snapshot(&self) -> SchedulingSnapshot {
        let mut snapshot = *self.counts();
        let occupied = snapshot.model_waiting
            + snapshot.model_preparing
            + snapshot.provider_waiting
            + snapshot.model_running
            + snapshot.tool_dispatch_waiting
            + snapshot.workspace_waiting
            + snapshot.tool_resource_waiting
            + snapshot.tool_running;
        snapshot.runnable_runs = snapshot.managed_runs.saturating_sub(occupied);
        snapshot
    }
}

pub(super) struct Activity {
    scheduling: Scheduling,
    phase: Phase,
}

impl Drop for Activity {
    fn drop(&mut self) {
        let mut counts = self.scheduling.counts();
        let count = self.phase.count(&mut counts);
        *count = count.saturating_sub(1);
    }
}
