mod catalog;
mod wire;

use std::sync::Arc;

use anyhow::{Context, Result, bail, ensure};
use async_trait::async_trait;
use ejm_harness_auth::CredentialSource;
use futures_util::StreamExt;
use serde_json::Value;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use wire::{completed, request_body, stream_failure};

use super::{
    HttpObserver, HttpTracePhase, MAX_RESPONSE, Provider,
    assemble::push_text,
    cancellable,
    http::http_client,
    observe::Trace,
    sse::{next_sse, sse_stream},
};
use crate::{ModelDelta, ModelRequest, ModelResponse};

pub use catalog::AvailableModel;

const OPENAI_BASE: &str = "https://api.openai.com/v1";

/// Provider/model support can be narrower than this set of protocol values.
pub const REASONING_EFFORTS: &[&str] = &[
    "none", "minimal", "low", "medium", "high", "xhigh", "max", "ultra",
];

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
    /// The fixture URL is not `https`, so its client uses no proxy.
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
            client: http_client(&base_url)?,
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
            .send(&body, token.expose_secret(), &cancel, &deltas, &trace)
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
        body: &Value,
        token: &str,
        cancel: &CancellationToken,
        deltas: &mpsc::Sender<ModelDelta>,
        trace: &Trace<'_>,
    ) -> Result<ModelResponse> {
        let response = self.post(body, token, cancel, trace).await?;
        let status = response.status().as_u16();
        let mut stream = sse_stream(response.bytes_stream())?;
        let mut partial = ModelResponse::default();
        while let Some(data) = next_sse(&mut stream, cancel).await? {
            trace.emit(HttpTracePhase::SsePayload, &data, Some(status), false);
            if let Some(result) = handle_event(&data, &mut partial, deltas, cancel).await? {
                return Ok(result);
            }
        }
        bail!("Responses stream ended without response.completed")
    }

    /// Send the request and return the response once its status is a success.
    async fn post(
        &self,
        body: &Value,
        token: &str,
        cancel: &CancellationToken,
        trace: &Trace<'_>,
    ) -> Result<reqwest::Response> {
        let request = self
            .client
            .post(format!("{}/responses", self.base_url))
            .bearer_auth(token)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(serde_json::to_string(body)?)
            .build()?;
        trace.request(&request);
        let response = cancellable(cancel, self.client.execute(request))
            .await?
            .context("Responses connection failed")?;
        let status = response.status();
        trace.response_head(&response);
        if !status.is_success() {
            if self.observer.is_some() {
                trace_http_error(response, cancel, trace).await;
            }
            bail!("Responses returned HTTP {}", status.as_u16());
        }
        Ok(response)
    }

    /// Fetch account-specific displayable models without first selecting a model.
    ///
    /// # Errors
    /// Returns credential, cancellation, HTTP, size, or catalog validation errors.
    pub async fn available_models(
        credentials: Arc<dyn CredentialSource>,
        cancel: CancellationToken,
    ) -> Result<Vec<AvailableModel>> {
        catalog::fetch(
            &http_client(OPENAI_BASE)?,
            credentials.as_ref(),
            OPENAI_BASE,
            &cancel,
        )
        .await
    }

    /// Fetch the selected account's catalog, preserving its ordering.
    ///
    /// # Errors
    /// Returns credential, cancellation, HTTP, size, or catalog validation errors.
    pub async fn models(&self, cancel: CancellationToken) -> Result<Vec<AvailableModel>> {
        catalog::fetch(
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

/// Apply one SSE payload. Returns the canonical response once `response.completed` arrives.
async fn handle_event(
    data: &str,
    partial: &mut ModelResponse,
    deltas: &mpsc::Sender<ModelDelta>,
    cancel: &CancellationToken,
) -> Result<Option<ModelResponse>> {
    let event: Value = serde_json::from_str(data).context("invalid Responses event")?;
    match event["type"]
        .as_str()
        .context("Responses event has no type")?
    {
        "response.output_text.delta" | "response.refusal.delta" => {
            let text = event["delta"].as_str().context("invalid text delta")?;
            push_text(partial, text, deltas, cancel).await?;
        }
        "response.completed" => {
            let result = completed(&event["response"])?;
            ensure!(
                partial.content.is_empty() || partial.content == result.content,
                "Responses text disagrees with its completion"
            );
            return Ok(Some(result));
        }
        "response.failed" | "response.incomplete" | "error" => return Err(stream_failure(&event)),
        _ => {}
    }
    Ok(None)
}

async fn trace_http_error(
    response: reqwest::Response,
    cancel: &CancellationToken,
    trace: &Trace<'_>,
) {
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
