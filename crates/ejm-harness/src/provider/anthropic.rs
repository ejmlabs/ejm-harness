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
use crate::{Message, ModelDelta, ModelRequest, ModelResponse, Role, ToolDefinition};

#[derive(Clone)]
pub struct AnthropicConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

pub struct AnthropicProvider {
    config: AnthropicConfig,
    client: reqwest::Client,
    endpoint: String,
}

impl AnthropicProvider {
    /// Create a streaming Anthropic Messages adapter.
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
    pub fn new(config: AnthropicConfig) -> Result<Self> {
        ensure!(!config.model.trim().is_empty(), "model is required");
        let endpoint = endpoint(&config.base_url, "/messages")?;
        Ok(Self {
            config,
            client: http_client(&endpoint)?,
            endpoint,
        })
    }
}

#[async_trait]
impl Provider for AnthropicProvider {
    async fn stream(
        &self,
        request: ModelRequest,
        cancel: CancellationToken,
        deltas: mpsc::Sender<ModelDelta>,
    ) -> Result<ModelResponse> {
        let body = anthropic_request(&self.config.model, request)?;
        let mut stream = open_stream(
            self.client
                .post(&self.endpoint)
                .header("x-api-key", &self.config.api_key)
                .header("anthropic-version", "2023-06-01")
                .json(&body),
            &cancel,
        )
        .await?;
        let mut result = ModelResponse::default();
        let mut calls: BTreeMap<usize, PendingCall> = BTreeMap::new();
        let mut stopped = false;
        while let Some(data) = next_sse(&mut stream, &cancel).await? {
            let value: Value =
                serde_json::from_str(&data).context("invalid provider stream JSON")?;
            if anthropic_delta(&value, &mut result, &mut calls, &deltas, &cancel).await? {
                stopped = true;
                break;
            }
        }
        ensure!(stopped, "provider stream ended before message_stop");
        result.tool_calls = calls
            .into_values()
            .map(PendingCall::finish)
            .collect::<Result<_>>()?;
        Ok(result)
    }
}

fn anthropic_request(model: &str, request: ModelRequest) -> Result<Value> {
    let mut messages: Vec<Value> = vec![];
    for message in request.messages {
        let (role, content) = anthropic_blocks(message);
        if let Some(last) = messages.last_mut().filter(|last| last["role"] == role) {
            last["content"]
                .as_array_mut()
                .context("invalid internal message")?
                .extend(content);
        } else {
            messages.push(json!({"role":role,"content":content}));
        }
    }
    let mut body = json!({"model":model,"system":request.system,"messages":messages,"stream":true,"max_tokens":request.max_output_tokens});
    if !request.tools.is_empty() {
        body["tools"] = request.tools.iter().map(anthropic_tool).collect();
    }
    Ok(body)
}

/// The role and content blocks of one message. Consecutive same-role messages are merged by the caller.
fn anthropic_blocks(message: Message) -> (&'static str, Vec<Value>) {
    let text = message_content(&message);
    let role = if message.role == Role::Assistant {
        "assistant"
    } else {
        "user"
    };
    let mut content = vec![];
    if let Some(call_id) = message.tool_call_id {
        content.push(
            json!({"type":"tool_result","tool_use_id":call_id,"content":text,"is_error":message.is_error}),
        );
    } else if !text.is_empty() {
        content.push(json!({"type":"text","text":text}));
    }
    for call in message.tool_calls {
        content
            .push(json!({"type":"tool_use","id":call.id,"name":call.name,"input":call.arguments}));
    }
    (role, content)
}

fn anthropic_tool(tool: &ToolDefinition) -> Value {
    json!({"name":tool.name,"description":tool.description,"input_schema":tool.input_schema})
}

/// Apply one stream event. Returns true at `message_stop`.
async fn anthropic_delta(
    value: &Value,
    result: &mut ModelResponse,
    calls: &mut BTreeMap<usize, PendingCall>,
    deltas: &mpsc::Sender<ModelDelta>,
    cancel: &CancellationToken,
) -> Result<bool> {
    match value["type"].as_str().unwrap_or("") {
        "message_start" => {
            result.usage.input_tokens = value["message"]["usage"]["input_tokens"]
                .as_u64()
                .unwrap_or(0);
        }
        "content_block_start" => {
            let block = &value["content_block"];
            if block["type"] == "tool_use" {
                let index = block_index(value)?;
                ensure!(calls.len() < 16, "too many provider tool calls");
                ensure!(!calls.contains_key(&index), "duplicate tool block index");
                calls.insert(index, started_call(block)?);
            }
            if let Some(text) = block["text"].as_str() {
                push_text(result, text, deltas, cancel).await?;
            }
        }
        "content_block_delta" => {
            let delta = &value["delta"];
            if let Some(text) = delta["text"].as_str() {
                push_text(result, text, deltas, cancel).await?;
            }
            if let Some(arguments) = delta["partial_json"].as_str() {
                let index = block_index(value)?;
                let call = calls
                    .get_mut(&index)
                    .context("tool delta preceded its start")?;
                call.arguments.push_str(arguments);
                call.validate_size()?;
            }
        }
        "message_delta" => {
            let reason = value["delta"]["stop_reason"].as_str().unwrap_or("");
            ensure!(
                reason != "max_tokens",
                "provider output token budget exhausted; response is incomplete"
            );
            result.usage.output_tokens = value["usage"]["output_tokens"].as_u64().unwrap_or(0);
        }
        "message_stop" => return Ok(true),
        "error" => bail!("provider reported a stream error"),
        _ => {}
    }
    Ok(false)
}

/// A `tool_use` block as it starts, carrying any input the start event already holds.
fn started_call(block: &Value) -> Result<PendingCall> {
    let mut call = PendingCall {
        id: block["id"].as_str().unwrap_or("").into(),
        name: block["name"].as_str().unwrap_or("").into(),
        arguments: String::new(),
    };
    if let Some(input) = block.get("input") {
        let input_object = input.as_object().context("tool input must be an object")?;
        if !input_object.is_empty() {
            call.arguments = input.to_string();
        }
    }
    call.validate_size()?;
    Ok(call)
}
