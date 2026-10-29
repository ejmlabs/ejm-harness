use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{RunSummary, ToolCall, ToolOutput, Usage};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Event {
    pub sequence: u64,
    pub session_id: String,
    pub run_id: Option<String>,
    pub timestamp_ms: u64,
    #[serde(flatten)]
    pub kind: EventKind,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventKind {
    RunStarted {
        prompt: String,
    },
    ModelStarted {
        step: u32,
    },
    TextDelta {
        text: String,
    },
    ToolQueued {
        call: ToolCall,
    },
    ToolStarted {
        call: ToolCall,
    },
    ToolFinished {
        call_id: String,
        output: ToolOutput,
    },
    Usage {
        usage: Usage,
    },
    RunFinished {
        summary: RunSummary,
    },
    Steered {
        text: String,
    },
    Queued {
        text: String,
    },
    Progress {
        call_id: String,
        message: String,
    },
    ContextTrimmed {
        before_bytes: usize,
        after_bytes: usize,
    },
}
