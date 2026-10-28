use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{ModelResponse, ProviderContinuation, ToolCall, ToolDefinition, ToolOutput};

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

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ModelRequest {
    pub system: String,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolDefinition>,
    pub max_output_tokens: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

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
