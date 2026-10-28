//! Origin-pinned, bounded HTTPS transport for the provider's authentication endpoints.

use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use futures_util::StreamExt;
use reqwest::{StatusCode, Url};
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

use super::ISSUER;

const MAX_BODY: usize = 1024 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

pub(crate) struct Http {
    pub(super) client: reqwest::Client,
    pub(super) slots: Semaphore,
    pub(super) discovery_url: String,
    pub(super) allowed_origin: String,
}

pub(crate) struct HttpResponse {
    pub status: StatusCode,
    pub body: Zeroizing<Vec<u8>>,
}

impl Http {
    pub(super) fn new() -> Result<Self> {
        Ok(Self {
            client: reqwest::Client::builder()
                .https_only(true)
                .redirect(reqwest::redirect::Policy::none())
                .connect_timeout(Duration::from_secs(5))
                .timeout(REQUEST_TIMEOUT)
                .user_agent("ejm-harness-auth/0.1")
                .build()
                .context("could not initialize authentication transport")?,
            slots: Semaphore::new(4),
            discovery_url: format!("{ISSUER}/.well-known/openid-configuration"),
            allowed_origin: ISSUER.into(),
        })
    }

    pub(crate) fn endpoint(&self, value: &str) -> Result<Url> {
        ensure!(value.len() <= 2048, "authentication endpoint exceeds limit");
        let url =
            Url::parse(value).map_err(|_| anyhow::anyhow!("invalid authentication endpoint"))?;
        ensure!(
            url.origin().ascii_serialization() == self.allowed_origin
                && url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none(),
            "authentication endpoint has an unexpected origin or parameters"
        );
        Ok(url)
    }

    pub(crate) async fn get(
        &self,
        value: &str,
        cancel: &CancellationToken,
    ) -> Result<HttpResponse> {
        let url = self.endpoint(value)?;
        self.send(self.client.get(url), cancel).await
    }

    pub(super) async fn post(
        &self,
        value: &str,
        fields: &[(&str, &str)],
        cancel: &CancellationToken,
    ) -> Result<HttpResponse> {
        let url = self.endpoint(value)?;
        self.send(self.client.post(url).form(fields), cancel).await
    }

    async fn send(
        &self,
        request: reqwest::RequestBuilder,
        cancel: &CancellationToken,
    ) -> Result<HttpResponse> {
        let operation = async {
            let _permit = self.slots.acquire().await?;
            let response = request
                .send()
                .await
                .map_err(|_| anyhow::anyhow!("authentication request failed"))?;
            let status = response.status();
            let body = bounded_body(response).await?;
            Ok(HttpResponse { status, body })
        };
        tokio::select! {
            biased;
            () = cancel.cancelled() => bail!("authentication cancelled"),
            result = tokio::time::timeout(REQUEST_TIMEOUT, operation) => result.context("authentication request timed out")?,
        }
    }
}

async fn bounded_body(response: reqwest::Response) -> Result<Zeroizing<Vec<u8>>> {
    ensure!(
        response
            .content_length()
            .is_none_or(|length| length <= MAX_BODY as u64),
        "authentication response exceeds 1 MiB"
    );
    let mut stream = response.bytes_stream();
    let mut body = Zeroizing::new(Vec::new());
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| anyhow::anyhow!("authentication response interrupted"))?;
        ensure!(
            body.len() + chunk.len() <= MAX_BODY,
            "authentication response exceeds 1 MiB"
        );
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}
