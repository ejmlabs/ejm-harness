use std::collections::BTreeMap;

use anyhow::{Context, Result, bail, ensure};
use async_trait::async_trait;
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::{
    Provider,
    assemble::{PendingCall, block_index, push_text},
    http::{endpoint, http_client, open_stream},
    message_content,
    sse::next_sse,
};
use crate::{Message, ModelDelta, ModelRequest, ModelResponse, ToolDefinition, Usage};

#[derive(Clone)]
pub struct OpenAiConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

pub struct OpenAiProvider {
    config: OpenAiConfig,
    client: reqwest::Client,
    endpoint: String,
}

impl OpenAiProvider {
    /// Create a streaming Chat Completions adapter for an HTTPS-compatible endpoint.
    ///
    /// The base URL must use `https`, or `http` on a loopback host: exact
    /// `localhost`, an IPv4 address in `127.0.0.0/8`, or `::1`. There is no
    /// insecure override. Any scheme other than `https` skips every proxy.
    ///
    /// # Errors
    ///
    /// Returns an error when the model or base URL is invalid, or the HTTP client
    /// cannot be initialized. Construction does not contact the provider and does
    /// not place the API key on a request.
    pub fn new(config: OpenAiConfig) -> Result<Self> {
        ensure!(!config.model.trim().is_empty(), "model is required");
        let endpoint = endpoint(&config.base_url, "/chat/completions")?;
        Ok(Self {
            config,
            client: http_client(&endpoint)?,
            endpoint,
        })
    }
}

#[async_trait]
impl Provider for OpenAiProvider {
    async fn stream(
        &self,
        request: ModelRequest,
        cancel: CancellationToken,
        deltas: mpsc::Sender<ModelDelta>,
    ) -> Result<ModelResponse> {
        let body = openai_request(&self.config.model, &request);
        let mut stream = open_stream(
            self.client
                .post(&self.endpoint)
                .bearer_auth(&self.config.api_key)
                .json(&body),
            &cancel,
        )
        .await?;
        let mut result = ModelResponse::default();
        let mut calls: BTreeMap<usize, PendingCall> = BTreeMap::new();
        let mut finished = false;
        while let Some(data) = next_sse(&mut stream, &cancel).await? {
            if data == "[DONE]" {
                finished = true;
                break;
            }
            let value: Value =
                serde_json::from_str(&data).context("invalid provider stream JSON")?;
            finished |= openai_delta(&value, &mut result, &mut calls, &deltas, &cancel).await?;
        }
        ensure!(finished, "provider stream ended before a completion marker");
        result.tool_calls = calls
            .into_values()
            .map(PendingCall::finish)
            .collect::<Result<_>>()?;
        Ok(result)
    }
}

fn openai_request(model: &str, request: &ModelRequest) -> Value {
    let messages: Vec<Value> = std::iter::once(json!({"role":"system", "content":request.system}))
        .chain(request.messages.iter().map(openai_message))
        .collect();
    let mut body = json!({"model":model,"messages":messages,"stream":true,"stream_options":{"include_usage":true},"max_tokens":request.max_output_tokens});
    if !request.tools.is_empty() {
        body["tools"] = request.tools.iter().map(openai_tool).collect();
    }
    body
}

fn openai_message(message: &Message) -> Value {
    let mut value = json!({"role":message.role,"content":message_content(message)});
    if !message.tool_calls.is_empty() {
        value["tool_calls"] = message.tool_calls.iter().map(|call| json!({"id":call.id,"type":"function","function":{"name":call.name,"arguments":call.arguments.to_string()}})).collect();
    }
    if let Some(id) = &message.tool_call_id {
        value["tool_call_id"] = json!(id);
    }
    value
}

fn openai_tool(tool: &ToolDefinition) -> Value {
    json!({"type":"function","function":{"name":tool.name,"description":tool.description,"parameters":tool.input_schema}})
}

/// Apply one stream chunk. Returns true once the chunk carries a finish reason.
async fn openai_delta(
    value: &Value,
    result: &mut ModelResponse,
    calls: &mut BTreeMap<usize, PendingCall>,
    deltas: &mpsc::Sender<ModelDelta>,
    cancel: &CancellationToken,
) -> Result<bool> {
    if value.get("error").is_some() {
        bail!("provider reported a stream error");
    }
    if let Some(usage) = value.get("usage") {
        result.usage = openai_usage(usage);
    }
    let choice = &value["choices"][0];
    let finished = !choice["finish_reason"].is_null();
    if finished {
        let reason = choice["finish_reason"].as_str().unwrap_or("");
        ensure!(
            reason != "length" && reason != "content_filter",
            "provider ended with {reason}; response is incomplete"
        );
    }
    let delta = &choice["delta"];
    if let Some(text) = delta["content"].as_str() {
        push_text(result, text, deltas, cancel).await?;
    }
    if let Some(fragments) = delta["tool_calls"].as_array() {
        for fragment in fragments {
            append_fragment(calls, fragment)?;
        }
    }
    Ok(finished)
}

fn openai_usage(usage: &Value) -> Usage {
    Usage {
        input_tokens: usage["prompt_tokens"].as_u64().unwrap_or(0),
        output_tokens: usage["completion_tokens"].as_u64().unwrap_or(0),
        cached_input_tokens: usage["prompt_tokens_details"]["cached_tokens"].as_u64(),
        reasoning_output_tokens: usage["completion_tokens_details"]["reasoning_tokens"].as_u64(),
    }
}

fn append_fragment(calls: &mut BTreeMap<usize, PendingCall>, fragment: &Value) -> Result<()> {
    let index = block_index(fragment)?;
    ensure!(index < 16, "too many provider tool calls");
    let pending = calls.entry(index).or_default();
    pending.id.push_str(fragment["id"].as_str().unwrap_or(""));
    pending
        .name
        .push_str(fragment["function"]["name"].as_str().unwrap_or(""));
    pending
        .arguments
        .push_str(fragment["function"]["arguments"].as_str().unwrap_or(""));
    pending.validate_size()
}
