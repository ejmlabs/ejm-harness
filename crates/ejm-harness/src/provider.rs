use std::{collections::BTreeMap, sync::Mutex, time::Duration};

use anyhow::{Context, Result, bail, ensure};
use async_trait::async_trait;
use futures_util::{Stream, StreamExt};
use serde_json::{Value, json};
use sse_core::{SseDecoder, SseEvent, SseStream};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::{Message, ModelDelta, ModelRequest, ModelResponse, Role, ToolCall};

mod claude;
mod observe;
mod responses;
pub use claude::{ClaudeCodeConfig, ClaudeCodeProvider};
pub use observe::{HttpObserver, HttpTraceHeader, HttpTracePhase, HttpTraceRecord};
pub use responses::{AvailableModel, REASONING_EFFORTS, ResponsesConfig, ResponsesProvider};

const MAX_RESPONSE: usize = 64 * 1024;
const MAX_SSE_FIELD: usize = 512 * 1024;
const MAX_SSE_BODY: usize = 32 * 1024 * 1024;
const MAX_SSE_EVENTS: usize = 100_000;

#[async_trait]
pub trait Provider: Send + Sync {
    /// Return canonical content as well as deltas. Do not retry effects inside a provider.
    ///
    /// # Errors
    ///
    /// Returns an error on cancellation, a closed delta receiver, provider failure,
    /// malformed or incomplete output, or an exceeded response bound.
    async fn stream(
        &self,
        request: ModelRequest,
        cancel: CancellationToken,
        deltas: mpsc::Sender<ModelDelta>,
    ) -> Result<ModelResponse>;
}

enum ReplayMode {
    Responses(std::collections::VecDeque<ModelResponse>),
    Echo,
}

pub struct ReplayProvider {
    mode: Mutex<ReplayMode>,
}

impl ReplayProvider {
    #[must_use]
    pub fn new(responses: Vec<ModelResponse>) -> Self {
        Self {
            mode: Mutex::new(ReplayMode::Responses(responses.into())),
        }
    }
    #[must_use]
    pub fn echo() -> Self {
        Self {
            mode: Mutex::new(ReplayMode::Echo),
        }
    }
}

#[async_trait]
impl Provider for ReplayProvider {
    async fn stream(
        &self,
        request: ModelRequest,
        cancel: CancellationToken,
        deltas: mpsc::Sender<ModelDelta>,
    ) -> Result<ModelResponse> {
        let response = match &mut *self
            .mode
            .lock()
            .map_err(|_| anyhow::anyhow!("replay lock poisoned"))?
        {
            ReplayMode::Responses(responses) => {
                responses.pop_front().context("replay exhausted")?
            }
            ReplayMode::Echo => ModelResponse {
                content: request
                    .messages
                    .iter()
                    .rev()
                    .find(|m| m.role == Role::User)
                    .map(|m| format!("Demo: {}", m.content))
                    .unwrap_or_default(),
                ..Default::default()
            },
        };
        tokio::select! {
            biased;
            () = cancel.cancelled() => bail!("cancelled"),
            result = deltas.send(ModelDelta::Text(response.content.clone())) => { result.context("delta receiver closed")?; }
        }
        Ok(response)
    }
}

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
    /// Create a streaming Chat Completions adapter for an HTTP-compatible endpoint.
    ///
    /// # Errors
    ///
    /// Returns an error when the model or base URL is invalid, or the HTTP client
    /// cannot be initialized. Construction does not contact the provider.
    pub fn new(config: OpenAiConfig) -> Result<Self> {
        ensure!(!config.model.trim().is_empty(), "model is required");
        let endpoint = endpoint(&config.base_url, "/chat/completions")?;
        Ok(Self {
            config,
            client: http_client()?,
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
        let body = openai_request(&self.config.model, request);
        let response = tokio::select! {
            biased;
            () = cancel.cancelled() => bail!("cancelled"),
            result = self.client.post(&self.endpoint).bearer_auth(&self.config.api_key).json(&body).send() => result.context("provider connection failed")?,
        };
        ensure!(
            response.status().is_success(),
            "provider returned HTTP {}",
            response.status().as_u16()
        );
        let mut stream = sse_stream(response.bytes_stream())?;
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

fn openai_request(model: &str, request: ModelRequest) -> Value {
    let mut messages = vec![json!({"role":"system", "content":request.system})];
    for message in request.messages {
        let mut value = json!({"role":message.role,"content":message_content(&message)});
        if !message.tool_calls.is_empty() {
            value["tool_calls"] = Value::Array(message.tool_calls.iter().map(|call| json!({"id":call.id,"type":"function","function":{"name":call.name,"arguments":call.arguments.to_string()}})).collect());
        }
        if let Some(id) = message.tool_call_id {
            value["tool_call_id"] = json!(id);
        }
        messages.push(value);
    }
    let mut body = json!({"model":model,"messages":messages,"stream":true,"stream_options":{"include_usage":true},"max_tokens":request.max_output_tokens});
    if !request.tools.is_empty() {
        body["tools"] = Value::Array(request.tools.into_iter().map(|tool| json!({"type":"function","function":{"name":tool.name,"description":tool.description,"parameters":tool.input_schema}})).collect());
    }
    body
}

fn message_content(message: &Message) -> String {
    if message.role != Role::Tool {
        return message.content.clone();
    }
    let status = if message.is_error {
        "[ejm-harness tool error]\n"
    } else {
        ""
    };
    match &message.artifact {
        Some(path) => format!(
            "{status}{}\n[ejm-harness artifact: {}]",
            message.content,
            path.display()
        ),
        None => format!("{status}{}", message.content),
    }
}

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
        result.usage.input_tokens = usage["prompt_tokens"].as_u64().unwrap_or(0);
        result.usage.output_tokens = usage["completion_tokens"].as_u64().unwrap_or(0);
        result.usage.cached_input_tokens = usage["prompt_tokens_details"]["cached_tokens"].as_u64();
        result.usage.reasoning_output_tokens =
            usage["completion_tokens_details"]["reasoning_tokens"].as_u64();
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
    if let Some(tool_calls) = delta["tool_calls"].as_array() {
        for call in tool_calls {
            let index = block_index(call)?;
            ensure!(index < 16, "too many provider tool calls");
            let pending = calls.entry(index).or_default();
            pending.id.push_str(call["id"].as_str().unwrap_or(""));
            pending
                .name
                .push_str(call["function"]["name"].as_str().unwrap_or(""));
            pending
                .arguments
                .push_str(call["function"]["arguments"].as_str().unwrap_or(""));
            pending.validate_size()?;
        }
    }
    Ok(finished)
}

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
    /// # Errors
    ///
    /// Returns an error when the model or base URL is invalid, or the HTTP client
    /// cannot be initialized. Construction does not contact the provider.
    pub fn new(config: AnthropicConfig) -> Result<Self> {
        ensure!(!config.model.trim().is_empty(), "model is required");
        let endpoint = endpoint(&config.base_url, "/messages")?;
        Ok(Self {
            config,
            client: http_client()?,
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
        let response = tokio::select! {
            biased;
            () = cancel.cancelled() => bail!("cancelled"),
            result = self.client.post(&self.endpoint).header("x-api-key", &self.config.api_key).header("anthropic-version", "2023-06-01").json(&body).send() => result.context("provider connection failed")?,
        };
        ensure!(
            response.status().is_success(),
            "provider returned HTTP {}",
            response.status().as_u16()
        );
        let mut stream = sse_stream(response.bytes_stream())?;
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
            content.push(
                json!({"type":"tool_use","id":call.id,"name":call.name,"input":call.arguments}),
            );
        }
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
        body["tools"] = Value::Array(request.tools.into_iter().map(|tool| json!({"name":tool.name,"description":tool.description,"input_schema":tool.input_schema})).collect());
    }
    Ok(body)
}

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
                calls.insert(index, PendingCall::from_anthropic(block)?);
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

fn block_index(value: &Value) -> Result<usize> {
    let index = value["index"].as_u64().context("tool block has no index")?;
    usize::try_from(index).context("tool block index exceeds platform limit")
}

fn http_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(600))
        .pool_max_idle_per_host(4)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .context("HTTP client initialization failed")
}

fn endpoint(base: &str, suffix: &str) -> Result<String> {
    let url = reqwest::Url::parse(base).context("invalid provider base URL")?;
    ensure!(
        matches!(url.scheme(), "http" | "https") && url.host_str().is_some(),
        "provider URL must use HTTP or HTTPS"
    );
    ensure!(
        url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none(),
        "provider URL must not contain credentials, query, or fragment"
    );
    Ok(if base.trim_end_matches('/').ends_with(suffix) {
        base.trim_end_matches('/').into()
    } else {
        format!("{}{suffix}", base.trim_end_matches('/'))
    })
}

#[derive(Default)]
struct PendingCall {
    id: String,
    name: String,
    arguments: String,
}
impl PendingCall {
    fn from_anthropic(block: &Value) -> Result<Self> {
        let mut call = Self {
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

    fn validate_size(&self) -> Result<()> {
        ensure!(
            self.id.len() < 256 && self.name.len() <= 128 && self.arguments.len() <= MAX_RESPONSE,
            "provider tool call exceeds limit"
        );
        Ok(())
    }

    fn finish(self) -> Result<ToolCall> {
        ensure!(
            !self.id.is_empty() && !self.name.is_empty(),
            "provider tool identity is missing"
        );
        let arguments = if self.arguments.is_empty() {
            json!({})
        } else {
            serde_json::from_str(&self.arguments).context("provider returned invalid tool JSON")?
        };
        Ok(ToolCall {
            id: self.id,
            name: self.name,
            arguments,
        })
    }
}

async fn push_text(
    result: &mut ModelResponse,
    text: &str,
    deltas: &mpsc::Sender<ModelDelta>,
    cancel: &CancellationToken,
) -> Result<()> {
    ensure!(
        result.content.len().saturating_add(text.len()) <= MAX_RESPONSE,
        "provider response exceeds byte budget"
    );
    result.content.push_str(text);
    tokio::select! { biased; () = cancel.cancelled() => bail!("cancelled"), sent = deltas.send(ModelDelta::Text(text.into())) => { sent.context("delta receiver closed")?; } }
    Ok(())
}

// Count body chunks before decoding, including comments, ignored fields, and separators.
// Count emitted library events before provider filtering, including retry and empty messages.
fn sse_stream<S, B, E>(inner: S) -> Result<impl Stream<Item = std::io::Result<SseEvent>> + Unpin>
where
    S: Stream<Item = std::result::Result<B, E>> + Unpin,
    B: AsRef<[u8]> + Unpin,
    E: std::error::Error + Send + Sync + 'static,
{
    let limit = std::num::NonZeroUsize::new(MAX_SSE_FIELD).context("SSE budget must be nonzero")?;
    let mut body_bytes = 0;
    let bounded = inner.map(move |chunk| {
        let chunk = chunk.map_err(std::io::Error::other)?;
        let bytes = chunk.as_ref().len();
        if bytes > MAX_SSE_BODY - body_bytes {
            return Err(std::io::Error::other(
                "provider SSE body exceeds 32 MiB budget",
            ));
        }
        body_bytes += bytes;
        Ok(std::io::Cursor::new(chunk))
    });
    let mut stream = SseStream::with_decoder(SseDecoder::with_limit(limit));
    stream.attach(bounded);
    Ok(stream
        .take(MAX_SSE_EVENTS + 1)
        .enumerate()
        .map(|(index, event)| {
            let event = event.map_err(std::io::Error::other)?;
            if index == MAX_SSE_EVENTS {
                return Err(std::io::Error::other(
                    "provider SSE stream exceeds 100000 decoded events",
                ));
            }
            Ok(event)
        }))
}

async fn next_sse<S, E>(stream: &mut S, cancel: &CancellationToken) -> Result<Option<String>>
where
    S: Stream<Item = std::result::Result<SseEvent, E>> + Unpin,
    E: std::error::Error + Send + Sync + 'static,
{
    loop {
        let event = tokio::select! {
            biased;
            () = cancel.cancelled() => bail!("cancelled"),
            event = stream.next() => event,
        };
        match event.transpose().context("provider SSE framing failed")? {
            Some(SseEvent::Message(event)) => return Ok(Some(event.data)),
            Some(SseEvent::Retry(_)) => {}
            None => return Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn sse_handles_split_utf8_crlf_comments_and_multiple_data_lines() -> Result<()> {
        let bytes =
            ": ping\r\ndata: {\"text\":\"雪\"}\r\n\r\ndata: first\ndata: second\n\n".as_bytes();
        let chunks = bytes.chunks(2).map(Ok::<_, std::io::Error>);
        let mut stream = sse_stream(futures_util::stream::iter(chunks))?;
        let cancel = CancellationToken::new();
        assert_eq!(
            next_sse(&mut stream, &cancel).await?,
            Some("{\"text\":\"雪\"}".into())
        );
        assert_eq!(
            next_sse(&mut stream, &cancel).await?,
            Some("first\nsecond".into())
        );
        assert!(next_sse(&mut stream, &cancel).await?.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn sse_bounds_empty_data_lines_before_the_frame_ends() -> Result<()> {
        let chunks = (0..MAX_SSE_FIELD + 2).map(|_| Ok::<_, std::io::Error>(b"data:\n".as_slice()));
        let mut stream = sse_stream(futures_util::stream::iter(chunks))?;
        let result = next_sse(&mut stream, &CancellationToken::new()).await;
        let error = result
            .err()
            .context("unbounded empty data lines were accepted")?;
        assert!(error.to_string().contains("SSE framing failed"));
        Ok(())
    }

    #[tokio::test]
    async fn sse_counts_comments_and_ignored_fields_before_decoding_an_overflowing_chunk()
    -> Result<()> {
        let mut comment = vec![b' '; 4096];
        comment[0] = b':';
        comment[4094..].copy_from_slice(b"\n\n");
        let mut ignored = comment.clone();
        ignored[0] = b'x';
        ignored[1] = b':';
        let chunks = (0..MAX_SSE_BODY / comment.len())
            .map(|index| {
                Ok::<_, std::io::Error>(if index % 2 == 0 {
                    comment.as_slice()
                } else {
                    ignored.as_slice()
                })
            })
            .chain(std::iter::once(Ok(b"data: [DONE]\n\n".as_slice())));
        let mut stream = sse_stream(futures_util::stream::iter(chunks))?;
        let error = next_sse(&mut stream, &CancellationToken::new())
            .await
            .err()
            .context("completion in an overflowing body chunk was decoded")?;
        assert!(format!("{error:#}").contains("body exceeds 32 MiB budget"));
        Ok(())
    }

    #[tokio::test]
    async fn sse_event_budget_counts_retry_and_empty_message_events() -> Result<()> {
        for frame in [b"retry: 1\n\n".as_slice(), b"data:\n\n".as_slice()] {
            let chunks = (0..=MAX_SSE_EVENTS).map(|_| Ok::<_, std::io::Error>(frame));
            let mut stream = sse_stream(futures_util::stream::iter(chunks))?;
            for _ in 0..MAX_SSE_EVENTS {
                stream.next().await.context("missing permitted event")??;
            }
            let error = stream
                .next()
                .await
                .context("missing event budget error")?
                .err()
                .context("excess decoded event was accepted")?;
            assert!(error.to_string().contains("100000 decoded events"));
            assert!(stream.next().await.is_none());
        }
        Ok(())
    }

    #[tokio::test]
    async fn sse_allows_completion_at_the_event_limit() -> Result<()> {
        let chunks = (0..MAX_SSE_EVENTS - 1)
            .map(|_| Ok::<_, std::io::Error>(b"retry: 1\n\n".as_slice()))
            .chain(std::iter::once(Ok(b"data: [DONE]\n\n".as_slice())));
        let mut stream = sse_stream(futures_util::stream::iter(chunks))?;
        let cancel = CancellationToken::new();
        assert_eq!(next_sse(&mut stream, &cancel).await?, Some("[DONE]".into()));
        assert!(next_sse(&mut stream, &cancel).await?.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn sse_cancellation_stops_a_pending_body_stream() -> Result<()> {
        let mut stream = sse_stream(futures_util::stream::pending::<
            std::io::Result<&'static [u8]>,
        >())?;
        let cancel = CancellationToken::new();
        let (result, ()) = tokio::time::timeout(Duration::from_millis(500), async {
            tokio::join!(next_sse(&mut stream, &cancel), async {
                tokio::task::yield_now().await;
                cancel.cancel();
            })
        })
        .await?;
        assert!(
            result
                .err()
                .context("cancelled SSE stream succeeded")?
                .to_string()
                .contains("cancelled")
        );
        Ok(())
    }

    #[test]
    fn endpoints_never_accept_embedded_credentials() -> Result<()> {
        assert!(endpoint("https://key:secret@host/v1", "/messages").is_err());
        assert_eq!(
            endpoint("https://api.deepseek.com", "/chat/completions")?,
            "https://api.deepseek.com/chat/completions"
        );
        Ok(())
    }
}
