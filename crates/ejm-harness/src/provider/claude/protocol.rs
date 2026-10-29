//! The JSON contract with the official client: stdin input, response schema, and result decoding.

use std::path::Path;

use anyhow::{Context, Result, anyhow, ensure};
use ejm_harness_exec::CommandOutput;
use schemars::{JsonSchema, generate::SchemaSettings};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use super::MAX_PROTOCOL_OUTPUT;
use crate::{
    ModelRequest, ModelResponse, Role, ToolCall, ToolDefinition, Usage, provider::MAX_RESPONSE,
};

pub(super) const MAX_INPUT: usize = 8 * 1024 * 1024;
pub(super) const MAX_CALLS: usize = 16;

#[derive(Serialize)]
struct ReadableMessage<'a> {
    role: &'a Role,
    content: &'a str,
    tool_calls: &'a [ToolCall],
    tool_call_id: Option<&'a str>,
    is_error: bool,
    artifact: Option<&'a Path>,
}

pub(super) fn request_input(request: &ModelRequest) -> Result<Vec<u8>> {
    #[derive(Serialize)]
    struct Input<'a> {
        system: &'a str,
        messages: Vec<ReadableMessage<'a>>,
        tools: &'a [ToolDefinition],
    }
    let input = serde_json::to_vec(&Input {
        system: &request.system,
        messages: request
            .messages
            .iter()
            .map(|message| ReadableMessage {
                role: &message.role,
                content: &message.content,
                tool_calls: &message.tool_calls,
                tool_call_id: message.tool_call_id.as_deref(),
                is_error: message.is_error,
                artifact: message.artifact.as_deref(),
            })
            .collect(),
        tools: &request.tools,
    })?;
    ensure!(
        input.len() <= MAX_INPUT,
        "Claude Code input exceeds 8 MiB budget"
    );
    Ok(input)
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct StructuredResponse {
    content: String,
    tool_calls: Vec<StructuredToolCall>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct StructuredToolCall {
    name: String,
    arguments_json: String,
}

pub(super) fn response_schema(tools: &[ToolDefinition]) -> Result<Value> {
    let mut settings = SchemaSettings::draft07();
    settings.inline_subschemas = true;
    let mut schema = serde_json::to_value(
        settings
            .into_generator()
            .into_root_schema_for::<StructuredResponse>(),
    )?;
    let calls = &mut schema["properties"]["tool_calls"];
    if tools.is_empty() {
        calls["const"] = json!([]);
    } else {
        calls["items"]["properties"]["name"]["enum"] =
            Value::Array(tools.iter().map(|tool| json!(tool.name)).collect());
    }
    Ok(schema)
}

#[derive(Deserialize)]
struct ClientResult {
    #[serde(rename = "type")]
    kind: String,
    subtype: String,
    is_error: bool,
    structured_output: Option<Value>,
    usage: Option<ClientUsage>,
}

#[derive(Deserialize)]
struct ClientUsage {
    #[serde(rename = "input_tokens")]
    input: u64,
    #[serde(rename = "output_tokens")]
    output: u64,
    #[serde(rename = "cache_creation_input_tokens")]
    cache_creation: Option<u64>,
    #[serde(rename = "cache_read_input_tokens")]
    cache_read: Option<u64>,
}

impl ClientUsage {
    fn into_usage(self) -> Result<Usage> {
        let input_tokens = self
            .input
            .checked_add(self.cache_creation.unwrap_or(0))
            .and_then(|total| total.checked_add(self.cache_read.unwrap_or(0)))
            .context("Claude Code input usage exceeds limit")?;
        Ok(Usage {
            input_tokens,
            output_tokens: self.output,
            cached_input_tokens: self.cache_read,
            reasoning_output_tokens: None,
        })
    }
}

pub(super) fn decode_response(output: &CommandOutput, schema: &Value) -> Result<ModelResponse> {
    let result = client_result(output)?;
    let structured = structured_response(result.structured_output, schema)?;
    ensure!(
        structured.content.len() <= MAX_RESPONSE,
        "Claude Code content exceeds byte budget"
    );
    ensure!(
        structured.tool_calls.len() <= MAX_CALLS,
        "Claude Code tool proposals exceed count budget"
    );
    let tool_calls = structured
        .tool_calls
        .into_iter()
        .map(proposed_call)
        .collect::<Result<Vec<_>>>()?;
    Ok(ModelResponse {
        content: structured.content,
        tool_calls,
        usage: result
            .usage
            .context("Claude Code result has no usage")?
            .into_usage()?,
        continuation: None,
    })
}

/// The client's result envelope, once the process ended cleanly within its output budget.
fn client_result(output: &CommandOutput) -> Result<ClientResult> {
    ensure!(!output.cancelled, "Claude Code request cancelled");
    ensure!(!output.timed_out, "Claude Code request timed out");
    ensure!(
        output.exit_code == Some(0),
        "Claude Code request failed; check official client sign-in and policy"
    );
    ensure!(
        !output.truncated && output.stdout.len() <= MAX_PROTOCOL_OUTPUT,
        "Claude Code output exceeds protocol budget"
    );
    let result: ClientResult = serde_json::from_str(&output.stdout)
        .map_err(|_| anyhow!("Claude Code returned invalid result JSON"))?;
    ensure!(
        result.kind == "result" && result.subtype == "success" && !result.is_error,
        "Claude Code did not return a successful result"
    );
    Ok(result)
}

fn structured_response(structured: Option<Value>, schema: &Value) -> Result<StructuredResponse> {
    let structured = structured.context("Claude Code result has no structured output")?;
    let validator = jsonschema::validator_for(schema)
        .map_err(|_| anyhow!("invalid Claude Code response schema"))?;
    ensure!(
        validator.is_valid(&structured),
        "Claude Code structured output does not match its schema"
    );
    serde_json::from_value(structured)
        .map_err(|_| anyhow!("Claude Code returned invalid structured output"))
}

fn proposed_call(call: StructuredToolCall) -> Result<ToolCall> {
    ensure!(
        !call.name.is_empty() && call.name.len() <= 64 && call.arguments_json.len() <= MAX_RESPONSE,
        "Claude Code tool proposal exceeds byte budget"
    );
    let arguments: Value = serde_json::from_str(&call.arguments_json)
        .map_err(|_| anyhow!("Claude Code returned invalid tool argument JSON"))?;
    Ok(ToolCall {
        id: Uuid::new_v4().to_string(),
        name: call.name,
        arguments,
    })
}
