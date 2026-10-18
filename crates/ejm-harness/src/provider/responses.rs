use std::sync::Arc;

use anyhow::{Context, Result, bail, ensure};
use async_trait::async_trait;
use ejm_harness_auth::CredentialSource;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::{HttpObserver, HttpTracePhase, observe::Trace};
use super::{
    MAX_RESPONSE, PendingCall, Provider, http_client, message_content, next_sse, push_text,
    sse_stream,
};
use crate::{ModelDelta, ModelRequest, ModelResponse, ProviderContinuation, Role};

const OPENAI_BASE: &str = "https://api.openai.com/v1";
const MAX_CATALOG: usize = 1024 * 1024;

/// Provider/model support can be narrower than this set of protocol values.
pub const REASONING_EFFORTS: &[&str] = &[
    "none", "minimal", "low", "medium", "high", "xhigh", "max", "ultra",
];

/// A displayable model from the selected account, in server order.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AvailableModel {
    pub slug: String,
    pub display_name: String,
}

#[derive(Clone, Debug)]
pub struct ResponsesConfig {
    pub model: String,
    pub reasoning_effort: Option<String>,
}

impl ResponsesConfig {
    /// Validate transport options without contacting a provider.
    ///
    /// # Errors
    /// Rejects an empty model or an unknown reasoning effort.
    pub fn validate(&self) -> Result<()> {
        ensure!(!self.model.trim().is_empty(), "model is required");
        ensure!(
            self.reasoning_effort
                .as_deref()
                .is_none_or(|effort| REASONING_EFFORTS.contains(&effort)),
            "unknown reasoning effort"
        );
        Ok(())
    }
}

/// Stateless Responses transport. Credentials and refresh belong to the source.
#[derive(Clone)]
pub struct ResponsesProvider {
    config: ResponsesConfig,
    credentials: Arc<dyn CredentialSource>,
    client: reqwest::Client,
    base_url: String,
    observer: Option<Arc<dyn HttpObserver>>,
}

impl ResponsesProvider {
    /// Use the public `OpenAI` endpoint, with storage disabled on every request.
    ///
    /// # Errors
    /// Rejects an empty model or an unavailable HTTP client.
    pub fn new(config: ResponsesConfig, credentials: Arc<dyn CredentialSource>) -> Result<Self> {
        Self::build(config, credentials, OPENAI_BASE.into())
    }

    /// Use a loopback HTTP fixture without exposing credentials to arbitrary hosts.
    ///
    /// # Errors
    /// Rejects any destination other than an HTTP loopback address, an invalid
    /// model, or an unavailable HTTP client.
    pub fn for_endpoint(
        config: ResponsesConfig,
        credentials: Arc<dyn CredentialSource>,
        base_url: &str,
    ) -> Result<Self> {
        let url = reqwest::Url::parse(base_url).context("invalid fixture URL")?;
        ensure!(
            url.scheme() == "http"
                && url.host_str().is_some_and(|host| {
                    host.parse::<std::net::IpAddr>()
                        .is_ok_and(|address| address.is_loopback())
                })
                && url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none(),
            "Responses fixture URL must be an HTTP loopback address"
        );
        Self::build(config, credentials, base_url.trim_end_matches('/').into())
    }

    fn build(
        config: ResponsesConfig,
        credentials: Arc<dyn CredentialSource>,
        base_url: String,
    ) -> Result<Self> {
        config.validate()?;
        Ok(Self {
            config,
            credentials,
            client: http_client()?,
            base_url,
            observer: None,
        })
    }

    /// Add opt-in redacted body capture. The host owns bounded retention.
    #[must_use]
    pub fn with_observer(mut self, observer: Arc<dyn HttpObserver>) -> Self {
        self.observer = Some(observer);
        self
    }

    /// Override model and effort for one request while reusing the pooled client.
    ///
    /// # Errors
    /// Returns invalid option, credential, cancellation, HTTP, stream, or budget errors.
    pub async fn stream_with_config(
        &self,
        config: &ResponsesConfig,
        request: ModelRequest,
        cancel: CancellationToken,
        deltas: mpsc::Sender<ModelDelta>,
    ) -> Result<ModelResponse> {
        config.validate()?;
        let body = request_body(config, request)?;
        let token = self.credentials.access_token(cancel.child_token()).await?;
        let trace = Trace::new(self.observer.as_ref(), token.expose_secret());
        let result = self
            .send(body, token.expose_secret(), &cancel, &deltas, &trace)
            .await;
        let message = if cancel.is_cancelled() {
            "cancelled"
        } else if result.is_ok() {
            "completed"
        } else {
            "failed"
        };
        trace.emit(HttpTracePhase::Finished, message, None, false);
        result
    }

    async fn send(
        &self,
        body: Value,
        token: &str,
        cancel: &CancellationToken,
        deltas: &mpsc::Sender<ModelDelta>,
        trace: &Trace<'_>,
    ) -> Result<ModelResponse> {
        let body = serde_json::to_string(&body)?;
        let request = self
            .client
            .post(format!("{}/responses", self.base_url))
            .bearer_auth(token)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body)
            .build()?;
        trace.request(&request);
        let response = tokio::select! {
            biased;
            () = cancel.cancelled() => bail!("cancelled"),
            response = self.client.execute(request) => response.context("Responses connection failed")?,
        };
        let status = response.status();
        trace.response_head(&response);
        if !status.is_success() {
            if self.observer.is_some() {
                trace_http_error(response, cancel, trace).await;
            }
            bail!("Responses returned HTTP {}", status.as_u16());
        }
        let mut stream = sse_stream(response.bytes_stream())?;
        let mut partial = ModelResponse::default();
        while let Some(data) = next_sse(&mut stream, cancel).await? {
            trace.emit(
                HttpTracePhase::SsePayload,
                &data,
                Some(status.as_u16()),
                false,
            );
            let event: Value = serde_json::from_str(&data).context("invalid Responses event")?;
            match event["type"]
                .as_str()
                .context("Responses event has no type")?
            {
                "response.output_text.delta" | "response.refusal.delta" => {
                    let text = event["delta"].as_str().context("invalid text delta")?;
                    push_text(&mut partial, text, deltas, cancel).await?;
                }
                "response.completed" => {
                    let result = completed(&event["response"])?;
                    ensure!(
                        partial.content.is_empty() || partial.content == result.content,
                        "Responses text disagrees with its completion"
                    );
                    return Ok(result);
                }
                "response.failed" | "response.incomplete" | "error" => {
                    return Err(stream_failure(&event));
                }
                _ => {}
            }
        }
        bail!("Responses stream ended without response.completed")
    }

    /// Fetch account-specific displayable models without first selecting a model.
    ///
    /// # Errors
    /// Returns credential, cancellation, HTTP, size, or catalog validation errors.
    pub async fn available_models(
        credentials: Arc<dyn CredentialSource>,
        cancel: CancellationToken,
    ) -> Result<Vec<AvailableModel>> {
        catalog(&http_client()?, credentials.as_ref(), OPENAI_BASE, &cancel).await
    }

    /// Fetch the selected account's catalog, preserving its ordering.
    ///
    /// # Errors
    /// Returns credential, cancellation, HTTP, size, or catalog validation errors.
    pub async fn models(&self, cancel: CancellationToken) -> Result<Vec<AvailableModel>> {
        catalog(
            &self.client,
            self.credentials.as_ref(),
            &self.base_url,
            &cancel,
        )
        .await
    }
}

#[async_trait]
impl Provider for ResponsesProvider {
    async fn stream(
        &self,
        request: ModelRequest,
        cancel: CancellationToken,
        deltas: mpsc::Sender<ModelDelta>,
    ) -> Result<ModelResponse> {
        self.stream_with_config(&self.config, request, cancel, deltas)
            .await
    }
}

fn request_body(config: &ResponsesConfig, request: ModelRequest) -> Result<Value> {
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

async fn trace_http_error(
    response: reqwest::Response,
    cancel: &CancellationToken,
    trace: &Trace<'_>,
) {
    use futures_util::StreamExt;

    let status = response.status().as_u16();
    let mut stream = response.bytes_stream();
    let mut bytes = vec![];
    let deadline = tokio::time::sleep(std::time::Duration::from_secs(10));
    tokio::pin!(deadline);
    let mut truncated = false;
    loop {
        let chunk = tokio::select! {
            biased;
            () = cancel.cancelled() => { truncated = true; break; },
            () = &mut deadline => { truncated = true; break; },
            chunk = stream.next() => chunk,
        };
        let Some(chunk) = chunk else { break };
        let Ok(chunk) = chunk else {
            truncated = true;
            break;
        };
        let remaining = MAX_RESPONSE - bytes.len();
        bytes.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
        if chunk.len() > remaining {
            truncated = true;
            break;
        }
    }
    let payload = String::from_utf8_lossy(&bytes);
    truncated |= matches!(payload, std::borrow::Cow::Owned(_));
    trace.emit(
        HttpTracePhase::HttpErrorBody,
        &payload,
        Some(status),
        truncated,
    );
}

fn completed(response: &Value) -> Result<ModelResponse> {
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
    let mut ids = std::collections::HashSet::new();
    for item in output {
        match item["type"].as_str().context("output item has no type")? {
            "message" => append_message(&mut result.content, item)?,
            "function_call" => {
                ensure!(
                    result.tool_calls.len() < 16,
                    "too many Responses tool calls"
                );
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
    result.usage.input_tokens = response["usage"]["input_tokens"]
        .as_u64()
        .context("Responses input usage is missing")?;
    result.usage.output_tokens = response["usage"]["output_tokens"]
        .as_u64()
        .context("Responses output usage is missing")?;
    let cached = &response["usage"]["input_tokens_details"]["cached_tokens"];
    result.usage.cached_input_tokens = if cached.is_null() {
        None
    } else {
        Some(
            cached
                .as_u64()
                .context("Responses cached input usage is invalid")?,
        )
    };
    let reasoning = &response["usage"]["output_tokens_details"]["reasoning_tokens"];
    result.usage.reasoning_output_tokens = if reasoning.is_null() {
        None
    } else {
        Some(
            reasoning
                .as_u64()
                .context("Responses reasoning usage is invalid")?,
        )
    };
    ensure!(
        result
            .usage
            .cached_input_tokens
            .is_none_or(|cached| cached <= result.usage.input_tokens),
        "Responses cached input usage exceeds input usage"
    );
    ensure!(
        result
            .usage
            .reasoning_output_tokens
            .is_none_or(|reasoning| reasoning <= result.usage.output_tokens),
        "Responses reasoning usage exceeds output usage"
    );
    Ok(result)
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

fn stream_failure(event: &Value) -> anyhow::Error {
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

async fn catalog(
    client: &reqwest::Client,
    credentials: &dyn CredentialSource,
    base_url: &str,
    cancel: &CancellationToken,
) -> Result<Vec<AvailableModel>> {
    use futures_util::StreamExt;

    let token = credentials.access_token(cancel.child_token()).await?;
    let response = tokio::select! {
        biased;
        () = cancel.cancelled() => bail!("cancelled"),
        response = client.get(format!("{base_url}/models")).bearer_auth(token.expose_secret()).send() => response.context("model catalog connection failed")?,
    };
    ensure!(
        response.status().is_success(),
        "model catalog returned HTTP {}",
        response.status().as_u16()
    );
    let mut body = vec![];
    let mut stream = response.bytes_stream();
    loop {
        let chunk = tokio::select! {
            biased;
            () = cancel.cancelled() => bail!("cancelled"),
            chunk = stream.next() => chunk,
        };
        let Some(chunk) = chunk else { break };
        let chunk = chunk.context("model catalog stream failed")?;
        ensure!(
            body.len().saturating_add(chunk.len()) <= MAX_CATALOG,
            "model catalog exceeds byte budget"
        );
        body.extend_from_slice(&chunk);
    }
    let value: Value = serde_json::from_slice(&body).context("invalid model catalog JSON")?;
    let (models, subscription) = if let Some(models) = value.get("models") {
        (
            models.as_array().context("invalid account models array")?,
            true,
        )
    } else {
        (
            value["data"]
                .as_array()
                .context("model catalog has no models or data array")?,
            false,
        )
    };
    ensure!(models.len() <= 1024, "model catalog exceeds entry budget");
    models
        .iter()
        .filter(|model| !subscription || model["visibility"] == "list")
        .map(|model| {
            let model: AvailableModel = if subscription {
                serde_json::from_value(model.clone()).context("invalid model entry")?
            } else {
                let id = model["id"].as_str().context("model entry has no id")?;
                AvailableModel {
                    slug: id.into(),
                    display_name: id.into(),
                }
            };
            ensure!(
                !model.slug.is_empty()
                    && model.slug.len() <= 256
                    && !model.display_name.is_empty()
                    && model.display_name.len() <= 256,
                "invalid model label"
            );
            Ok(model)
        })
        .collect()
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
