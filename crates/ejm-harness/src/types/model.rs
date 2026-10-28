use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::ToolCall;

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
