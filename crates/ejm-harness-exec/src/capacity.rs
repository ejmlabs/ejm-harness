//! Cached admission targets. Host coordination remains in the harness core.

use std::time::{Duration, Instant};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{GovernorSnapshot, ResourceLimits};

/// Queue policy chosen by the owner, never by generated agent content.
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AdmissionPolicy {
    /// Preserve the existing session-head ordering.
    #[default]
    Fixed,
    /// Fill spare capacity while bounding bypass of an older session head.
    Automatic,
}

/// Adjustable budgets beneath the governor's immutable hard limits.
///
/// Zero is valid and parks admission for that resource. Shrinking a target does
/// not revoke running leases or erase reservations that exceed the new target.
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct OperatingTarget {
    pub max_jobs: usize,
    pub max_heavy_jobs: usize,
    pub cpu_millis: u32,
    pub memory_bytes: u64,
    pub max_processes: u32,
    pub max_output_bytes: usize,
}

impl OperatingTarget {
    #[must_use]
    pub const fn from_limits(limits: &ResourceLimits) -> Self {
        Self {
            max_jobs: limits.max_jobs,
            max_heavy_jobs: limits.max_heavy_jobs,
            cpu_millis: limits.cpu_millis,
            memory_bytes: limits.memory_bytes,
            max_processes: limits.max_processes,
            max_output_bytes: limits.max_output_bytes,
        }
    }

    pub(crate) fn debt(self, used: &GovernorSnapshot) -> Self {
        Self {
            max_jobs: used.running.saturating_sub(self.max_jobs),
            max_heavy_jobs: used.heavy_running.saturating_sub(self.max_heavy_jobs),
            cpu_millis: used.reserved_cpu_millis.saturating_sub(self.cpu_millis),
            memory_bytes: used.reserved_memory_bytes.saturating_sub(self.memory_bytes),
            max_processes: used.reserved_processes.saturating_sub(self.max_processes),
            max_output_bytes: used
                .reserved_output_bytes
                .saturating_sub(self.max_output_bytes),
        }
    }
}

/// Bounded local state, separate from the legacy [`GovernorSnapshot`] shape.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OperatingSnapshot {
    pub policy: AdmissionPolicy,
    pub generation: u64,
    pub target: OperatingTarget,
    pub debt: OperatingTarget,
    /// Contexts and persistent hosts reserve memory without active job slots.
    pub memory_leases: usize,
    /// False after the cached coordination contact deadline expires.
    pub contact_valid: bool,
    pub contact_remaining: Option<Duration>,
}

pub(crate) struct OperatingState {
    pub policy: AdmissionPolicy,
    pub generation: u64,
    pub target: OperatingTarget,
    pub contact_deadline: Option<Instant>,
    pub memory_leases: usize,
}

impl OperatingState {
    pub fn contact_valid(&self, now: Instant) -> bool {
        self.contact_deadline.is_none_or(|deadline| now < deadline)
    }
}
