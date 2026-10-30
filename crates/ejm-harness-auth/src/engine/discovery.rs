//! The issuer's discovery document, fetched once and checked against the pinned origin.

use anyhow::{Result, bail, ensure};
use reqwest::StatusCode;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use super::{AuthEngine, ISSUER};

#[derive(Clone, Deserialize)]
pub(crate) struct Discovery {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub jwks_uri: String,
    pub revocation_endpoint: Option<String>,
}

impl AuthEngine {
    pub(super) async fn discovery(&self, cancel: &CancellationToken) -> Result<Discovery> {
        let mut cache = tokio::select! {
            () = cancel.cancelled() => bail!("authentication cancelled"),
            cache = self.inner.discovery.lock() => cache,
        };
        if let Some(discovery) = cache.as_ref() {
            return Ok(discovery.clone());
        }
        let discovery = self.fetch_discovery(cancel).await?;
        *cache = Some(discovery.clone());
        Ok(discovery)
    }

    async fn fetch_discovery(&self, cancel: &CancellationToken) -> Result<Discovery> {
        let http = &self.inner.http;
        let response = http.get(&http.discovery_url, cancel).await?;
        ensure!(
            response.status == StatusCode::OK,
            "authentication discovery unavailable"
        );
        let discovery: Discovery = serde_json::from_slice(&response.body)
            .map_err(|_| anyhow::anyhow!("invalid authentication discovery"))?;
        ensure!(
            discovery.issuer == ISSUER,
            "unexpected authentication issuer"
        );
        for value in [
            &discovery.authorization_endpoint,
            &discovery.token_endpoint,
            &discovery.jwks_uri,
        ] {
            http.endpoint(value)?;
        }
        if let Some(endpoint) = &discovery.revocation_endpoint {
            http.endpoint(endpoint)?;
        }
        Ok(discovery)
    }
}
