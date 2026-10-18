use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Assistant,
    Tool,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Message {
    pub role: Role,
    pub content: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// Tool failure evidence retained independently of its display text.
    #[serde(default)]
    pub is_error: bool,
    /// Optional tool artifact that remains available when observation text is omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact: Option<PathBuf>,
    /// Bounded opaque state returned by the same provider for a later tool turn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continuation: Option<ProviderContinuation>,
}

impl Message {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: content.into(),
            tool_calls: vec![],
            tool_call_id: None,
            is_error: false,
            artifact: None,
            continuation: None,
        }
    }

    #[must_use]
    pub fn assistant(response: &ModelResponse) -> Self {
        Self {
            role: Role::Assistant,
            content: response.content.clone(),
            tool_calls: response.tool_calls.clone(),
            tool_call_id: None,
            is_error: false,
            artifact: None,
            continuation: response.continuation.clone(),
        }
    }

    #[must_use]
    pub fn tool(call_id: impl Into<String>, output: &ToolOutput) -> Self {
        Self {
            role: Role::Tool,
            content: output.content.clone(),
            tool_calls: vec![],
            tool_call_id: Some(call_id.into()),
            is_error: output.is_error,
            artifact: output.artifact.clone(),
            continuation: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Effect {
    Read,
    Write,
    External,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub effect: Effect,
    pub version: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct ToolOutput {
    pub content: String,
    #[serde(default)]
    pub data: Value,
    #[serde(default)]
    pub is_error: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact: Option<PathBuf>,
}

impl ToolOutput {
    pub fn text(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            ..Self::default()
        }
    }

    pub fn error(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            is_error: true,
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    /// Reported subset of input tokens served from the provider's cache.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached_input_tokens: Option<u64>,
    /// Reported subset of output tokens spent on reasoning.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_output_tokens: Option<u64>,
}

impl Usage {
    pub fn add(&mut self, other: &Self) {
        self.input_tokens = self.input_tokens.saturating_add(other.input_tokens);
        self.output_tokens = self.output_tokens.saturating_add(other.output_tokens);
        self.cached_input_tokens =
            add_reported(self.cached_input_tokens, other.cached_input_tokens);
        self.reasoning_output_tokens =
            add_reported(self.reasoning_output_tokens, other.reasoning_output_tokens);
    }
}

fn add_reported(left: Option<u64>, right: Option<u64>) -> Option<u64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.saturating_add(right)),
        (left, right) => left.or(right),
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ModelRequest {
    pub system: String,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolDefinition>,
    pub max_output_tokens: u32,
}

#[derive(Clone, Debug)]
pub enum ModelDelta {
    Text(String),
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct ModelResponse {
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub tool_calls: Vec<ToolCall>,
    #[serde(default)]
    pub usage: Usage,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continuation: Option<ProviderContinuation>,
}

/// Provider state that must survive a stateless tool exchange and journal replay.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "provider", rename_all = "snake_case")]
pub enum ProviderContinuation {
    /// Encrypted reasoning items are passed only to the Responses adapter.
    OpenAiResponses { reasoning: Vec<Value> },
}

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
pub struct Event {
    pub sequence: u64,
    pub session_id: String,
    pub run_id: Option<String>,
    pub timestamp_ms: u64,
    #[serde(flatten)]
    pub kind: EventKind,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct HarnessSnapshot {
    pub active_agents: usize,
    pub model_requests: usize,
    pub waiting_model_requests: usize,
    pub tools: ejm_harness_exec::GovernorSnapshot,
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

pub(crate) fn timestamp_ms() -> u64 {
    u64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
    )
    .unwrap_or(u64::MAX)
}

pub(crate) fn pending_calls(messages: &[Message]) -> Vec<ToolCall> {
    let mut pending = std::collections::BTreeMap::new();
    for message in messages {
        for call in &message.tool_calls {
            pending.insert(call.id.clone(), call.clone());
        }
        if let Some(id) = &message.tool_call_id {
            pending.remove(id);
        }
    }
    pending.into_values().collect()
}

pub(crate) fn truncate_utf8(value: &mut String, max: usize) -> bool {
    if value.len() <= max {
        return false;
    }
    let mut end = max.min(value.len());
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value.truncate(end);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reused_legacy_ids_are_reconciled_in_message_order() {
        let call = ToolCall {
            id: "legacy".into(),
            name: "effect".into(),
            arguments: serde_json::json!({}),
        };
        let response = ModelResponse {
            tool_calls: vec![call],
            ..Default::default()
        };
        let history = vec![
            Message::assistant(&response),
            Message::tool("legacy", &ToolOutput::text("done")),
            Message::assistant(&response),
        ];
        assert_eq!(pending_calls(&history).len(), 1);
    }

    #[test]
    fn legacy_messages_default_tool_metadata_and_round_trip_new_evidence() -> anyhow::Result<()> {
        let legacy: Message = serde_json::from_value(serde_json::json!({
            "role":"tool", "content":"done", "tool_call_id":"legacy"
        }))?;
        assert!(!legacy.is_error);
        assert!(legacy.artifact.is_none());
        assert!(legacy.continuation.is_none());

        let output = ToolOutput {
            artifact: Some(PathBuf::from("artifacts/failed-command.log")),
            ..ToolOutput::error("command exited with status 1")
        };
        let message = Message::tool("failed", &output);
        let restored: Message = serde_json::from_slice(&serde_json::to_vec(&message)?)?;
        assert_eq!(restored.role, Role::Tool);
        assert_eq!(restored.tool_call_id.as_deref(), Some("failed"));
        assert_eq!(restored.content, output.content);
        assert!(restored.is_error);
        assert_eq!(restored.artifact, output.artifact);
        for message in [
            Message::user("inspect"),
            Message::assistant(&ModelResponse::default()),
        ] {
            assert!(!message.is_error);
            assert!(message.artifact.is_none());
            assert!(serde_json::to_value(message)?.get("artifact").is_none());
        }
        Ok(())
    }
}
