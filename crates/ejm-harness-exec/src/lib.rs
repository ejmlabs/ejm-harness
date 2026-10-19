//! Shared, bounded admission and native command execution.
//!
//! Native execution is not a security sandbox. Use [`Executor::capabilities`]
//! and [`RequiredCapabilities`] to require guarantees before a command starts.

mod executor;
mod governor;
mod monitor;
pub mod process;
mod processes;

use std::{collections::BTreeMap, path::PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub use executor::Executor;
pub use governor::{Governor, Lease};

/// Aggregate admission budgets shared by every session and executor.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct ResourceLimits {
    pub max_jobs: usize,
    pub max_heavy_jobs: usize,
    pub cpu_millis: u32,
    pub memory_bytes: u64,
    pub max_processes: u32,
    pub max_queue: usize,
    pub max_output_bytes: usize,
    pub max_artifact_bytes: u64,
    pub max_job_ms: u64,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_jobs: 4,
            max_heavy_jobs: 1,
            cpu_millis: 2_000,
            memory_bytes: 512 * 1024 * 1024,
            max_processes: 32,
            max_queue: 256,
            max_output_bytes: 1024 * 1024,
            max_artifact_bytes: 64 * 1024 * 1024,
            max_job_ms: 120_000,
        }
    }
}

/// Admission estimates, not a promise of OS-level resource isolation.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Reservation {
    pub cpu_millis: u32,
    pub memory_bytes: u64,
    pub processes: u32,
    pub output_bytes: usize,
    pub heavy: bool,
}

impl Default for Reservation {
    fn default() -> Self {
        Self {
            cpu_millis: 250,
            memory_bytes: 64 * 1024 * 1024,
            processes: 4,
            output_bytes: 64 * 1024,
            heavy: false,
        }
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct GovernorSnapshot {
    pub running: usize,
    pub queued: usize,
    pub heavy_running: usize,
    pub reserved_cpu_millis: u32,
    pub reserved_memory_bytes: u64,
    pub reserved_processes: u32,
    pub reserved_output_bytes: usize,
}

/// `Monitored` is observation and reactive cleanup, not a hard OS ceiling.
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RestrictionState {
    Enforced,
    Monitored,
    #[default]
    Unsupported,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ExecutorCapabilities {
    pub admission: RestrictionState,
    pub memory_limit: RestrictionState,
    pub cpu_limit: RestrictionState,
    pub process_limit: RestrictionState,
    pub output_limit: RestrictionState,
    pub artifact_limit: RestrictionState,
    pub deadline: RestrictionState,
    pub process_tree_cancellation: RestrictionState,
    pub filesystem_isolation: RestrictionState,
    pub network_isolation: RestrictionState,
}

/// A true field requires an enforced guarantee. Monitoring does not qualify.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "Each independent requirement is explicit in the stable capability wire schema."
)]
pub struct RequiredCapabilities {
    pub admission: bool,
    pub memory_limit: bool,
    pub cpu_limit: bool,
    pub process_limit: bool,
    pub output_limit: bool,
    pub artifact_limit: bool,
    pub deadline: bool,
    pub process_tree_cancellation: bool,
    pub filesystem_isolation: bool,
    pub network_isolation: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct CommandRequest {
    pub session_id: String,
    pub program: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    /// Overrides added to the calling process's inherited environment.
    pub env: BTreeMap<String, String>,
    /// Zero uses the governor's maximum. Larger values are clamped to it.
    pub timeout_ms: u64,
    pub reservation: Reservation,
    pub required: RequiredCapabilities,
}

impl Default for CommandRequest {
    fn default() -> Self {
        Self {
            session_id: "default".into(),
            program: String::new(),
            args: Vec::new(),
            cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            env: BTreeMap::new(),
            timeout_ms: 0,
            reservation: Reservation::default(),
            required: RequiredCapabilities::default(),
        }
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CommandOutput {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub truncated: bool,
    pub cancelled: bool,
    pub timed_out: bool,
    pub artifact: Option<PathBuf>,
    pub duration_ms: u64,
    /// Sampled resident memory. Zero means no sample was available.
    pub peak_memory_bytes: u64,
    /// Sampled live processes, including the command itself.
    pub peak_processes: u32,
}
