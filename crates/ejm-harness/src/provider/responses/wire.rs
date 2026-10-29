//! The Responses wire format: request encoding and completed-response decoding.

use std::collections::HashSet;

use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};

use super::ResponsesConfig;
use crate::{
    ModelRequest, ModelResponse, ProviderContinuation, Role, Usage,
    provider::{MAX_RESPONSE, assemble::PendingCall, message_content},
};

pub(super) fn request_body(config: &ResponsesConfig, request: ModelRequest) -> Result<Value> {
    let mut input = vec![];
    for message in request.messages {
        let content = message_content(&message);
        if let Some(ProviderContinuation::OpenAiResponses { reasoning }) = message.continuation {
            ensure!(
                message.role == Role::Assistant,
                "reasoning must belong to an assistant"
            );
            for item in reasoning {
                input.push(reasoning_item(&item)?);
            }
        }
        if message.role == Role::Tool {
            let id = message
                .tool_call_id
                .context("tool result has no call identity")?;
            input.push(json!({"type":"function_call_output", "call_id":id, "output":content}));
            continue;
        }
        if !content.is_empty() {
            input.push(json!({"role":message.role, "content":content}));
        }
        for call in message.tool_calls {
            input.push(json!({"type":"function_call", "call_id":call.id, "name":call.name, "arguments":call.arguments.to_string()}));
        }
    }
    let tools: Vec<Value> = request.tools.into_iter().map(|tool| json!({"type":"function", "name":tool.name, "description":tool.description, "parameters":tool.input_schema, "strict":false})).collect();
    let mut body = json!({"model":config.model, "instructions":request.system, "input":input, "tools":tools, "max_output_tokens":request.max_output_tokens, "store":false, "stream":true, "include":["reasoning.encrypted_content"]});
    if let Some(effort) = &config.reasoning_effort {
        body["reasoning"] = json!({"effort":effort});
    }
    Ok(body)
}

/// Decode the `response` of a `response.completed` event.
pub(super) fn completed(response: &Value) -> Result<ModelResponse> {
    ensure!(
        response["status"] == "completed",
        "Responses completion has an invalid status"
    );
    let output = response["output"]
        .as_array()
        .context("Responses completion has no output")?;
    ensure!(output.len() <= 64, "too many Responses output items");
    let mut result = ModelResponse::default();
    let mut reasoning = vec![];
    let mut ids = HashSet::new();
    for item in output {
        match item["type"].as_str().context("output item has no type")? {
            "message" => append_message(&mut result.content, item)?,
            "function_call" => {
                ensure!(
                    result.tool_calls.len() < 16,
                    "too many Responses tool calls"
                );
                let call = function_call(item)?;
                ensure!(
                    ids.insert(call.id.clone()),
                    "duplicate Responses call identity"
                );
                result.tool_calls.push(call.finish()?);
            }
            "reasoning" => reasoning.push(reasoning_item(item)?),
            _ => bail!("unsupported Responses output item"),
        }
    }
    if !reasoning.is_empty() {
        let continuation = ProviderContinuation::OpenAiResponses { reasoning };
        ensure!(
            serde_json::to_vec(&continuation)?.len() <= MAX_RESPONSE,
            "reasoning exceeds continuation budget"
        );
        result.continuation = Some(continuation);
    }
    result.usage = usage(&response["usage"])?;
    Ok(result)
}

fn function_call(item: &Value) -> Result<PendingCall> {
    let call = PendingCall {
        id: item["call_id"]
            .as_str()
            .context("missing call identity")?
            .into(),
        name: item["name"].as_str().context("missing tool name")?.into(),
        arguments: item["arguments"]
            .as_str()
            .context("missing tool arguments")?
            .into(),
    };
    call.validate_size()?;
    Ok(call)
}

/// Reported usage. A missing or null cached or reasoning subset stays unknown.
fn usage(report: &Value) -> Result<Usage> {
    let input_tokens = report["input_tokens"]
        .as_u64()
        .context("Responses input usage is missing")?;
    let output_tokens = report["output_tokens"]
        .as_u64()
        .context("Responses output usage is missing")?;
    let cached_input_tokens = optional_tokens(
        &report["input_tokens_details"]["cached_tokens"],
        "Responses cached input usage is invalid",
    )?;
    let reasoning_output_tokens = optional_tokens(
        &report["output_tokens_details"]["reasoning_tokens"],
        "Responses reasoning usage is invalid",
    )?;
    ensure!(
        cached_input_tokens.is_none_or(|cached| cached <= input_tokens),
        "Responses cached input usage exceeds input usage"
    );
    ensure!(
        reasoning_output_tokens.is_none_or(|reasoning| reasoning <= output_tokens),
        "Responses reasoning usage exceeds output usage"
    );
    Ok(Usage {
        input_tokens,
        output_tokens,
        cached_input_tokens,
        reasoning_output_tokens,
    })
}

fn optional_tokens(value: &Value, invalid: &'static str) -> Result<Option<u64>> {
    if value.is_null() {
        return Ok(None);
    }
    value.as_u64().map(Some).context(invalid)
}

fn append_message(text: &mut String, item: &Value) -> Result<()> {
    ensure!(
        item["role"] == "assistant",
        "output message has an invalid role"
    );
    for part in item["content"]
        .as_array()
        .context("output message has no content")?
    {
        let field = match part["type"].as_str() {
            Some("output_text") => "text",
            Some("refusal") => "refusal",
            _ => bail!("unsupported Responses message content"),
        };
        let value = part[field].as_str().context("output text is missing")?;
        ensure!(
            text.len().saturating_add(value.len()) <= MAX_RESPONSE,
            "Responses text exceeds budget"
        );
        text.push_str(value);
    }
    Ok(())
}

fn reasoning_item(item: &Value) -> Result<Value> {
    ensure!(
        item["type"] == "reasoning",
        "invalid reasoning continuation"
    );
    ensure!(
        serde_json::to_vec(item)?.len() <= MAX_RESPONSE,
        "reasoning item exceeds budget"
    );
    let id = item["id"]
        .as_str()
        .filter(|id| !id.is_empty() && id.len() <= 256)
        .context("invalid reasoning identity")?;
    let summary = item["summary"]
        .as_array()
        .context("reasoning summary is missing")?;
    let mut result = json!({"type":"reasoning", "id":id, "summary":summary});
    let encrypted = item["encrypted_content"]
        .as_str()
        .filter(|value| !value.is_empty())
        .context("Responses omitted encrypted reasoning required for stateless replay")?;
    result["encrypted_content"] = json!(encrypted);
    Ok(result)
}

/// The error for a `response.failed`, `response.incomplete`, or `error` event.
pub(super) fn stream_failure(event: &Value) -> anyhow::Error {
    let code = event["response"]["error"]["code"]
        .as_str()
        .or_else(|| event["code"].as_str());
    match code {
        Some("subscription_sharing_usage_limit_exceeded") => {
            anyhow::anyhow!("ChatGPT plan usage limit reached; review ChatGPT Settings > Usage")
        }
        Some("subscription_sharing_usage_unavailable") => {
            anyhow::anyhow!("ChatGPT plan usage is unavailable for this account")
        }
        _ => anyhow::anyhow!("Responses failed or ended with incomplete output"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completion_rejects_impossible_and_negative_usage_subsets() -> Result<()> {
        for (usage, expected) in [
            (
                json!({"input_tokens":12,"output_tokens":4,"input_tokens_details":{"cached_tokens":13}}),
                "cached input usage exceeds",
            ),
            (
                json!({"input_tokens":12,"output_tokens":4,"output_tokens_details":{"reasoning_tokens":5}}),
                "reasoning usage exceeds",
            ),
            (
                json!({"input_tokens":12,"output_tokens":4,"input_tokens_details":{"cached_tokens":-1}}),
                "cached input usage is invalid",
            ),
            (
                json!({"input_tokens":12,"output_tokens":4,"output_tokens_details":{"reasoning_tokens":-1}}),
                "reasoning usage is invalid",
            ),
        ] {
            let error = completed(&json!({"status":"completed", "output":[], "usage":usage}))
                .err()
                .context("invalid usage subset was accepted")?;
            assert!(error.to_string().contains(expected));
        }
        Ok(())
    }

    #[test]
    fn completion_preserves_missing_null_zero_and_full_usage_subsets() -> Result<()> {
        for (usage, cached, reasoning) in [
            (json!({"input_tokens":0,"output_tokens":0}), None, None),
            (
                json!({"input_tokens":0,"output_tokens":0,"input_tokens_details":{"cached_tokens":null},"output_tokens_details":{"reasoning_tokens":null}}),
                None,
                None,
            ),
            (
                json!({"input_tokens":0,"output_tokens":0,"input_tokens_details":{"cached_tokens":0},"output_tokens_details":{"reasoning_tokens":0}}),
                Some(0),
                Some(0),
            ),
            (
                json!({"input_tokens":12,"output_tokens":4,"input_tokens_details":{"cached_tokens":12},"output_tokens_details":{"reasoning_tokens":4}}),
                Some(12),
                Some(4),
            ),
        ] {
            let result = completed(&json!({"status":"completed", "output":[], "usage":usage}))?;
            assert_eq!(result.usage.cached_input_tokens, cached);
            assert_eq!(result.usage.reasoning_output_tokens, reasoning);
        }
        Ok(())
    }
}
