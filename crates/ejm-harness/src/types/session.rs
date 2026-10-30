use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::Usage;

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, Default)]
pub struct SessionOptions {
    pub cwd: PathBuf,
    #[serde(default)]
    pub title: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Idle,
    Running,
    Interrupted,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Session {
    pub id: String,
    pub title: String,
    pub cwd: PathBuf,
    pub created_at_ms: u64,
    pub status: SessionStatus,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Completed,
    Cancelled,
    Failed,
    Interrupted,
    BudgetExhausted,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RunSummary {
    pub run_id: String,
    pub status: RunStatus,
    pub steps: u32,
    pub text: String,
    pub usage: Usage,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct HarnessSnapshot {
    pub active_agents: usize,
    pub model_requests: usize,
    pub waiting_model_requests: usize,
    pub tools: ejm_harness_exec::GovernorSnapshot,
}
