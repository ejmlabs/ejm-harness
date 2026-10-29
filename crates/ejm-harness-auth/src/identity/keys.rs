//! The issuer's signing keys: fetched once, validated, and cached briefly.

use std::time::{Duration, Instant};

use anyhow::{Context, Result, ensure};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use jsonwebtoken::DecodingKey;
use reqwest::StatusCode;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use super::IdentityValidator;

pub(super) struct KeyCache {
    uri: String,
    loaded_at: Instant,
    keys: Vec<Key>,
}

#[derive(Clone, Deserialize)]
struct Key {
    kid: Option<String>,
    kty: String,
    alg: Option<String>,
    #[serde(rename = "use")]
    usage: Option<String>,
    #[serde(rename = "key_ops")]
    operations: Option<Vec<String>>,
    n: Option<String>,
    e: Option<String>,
}

impl Key {
    /// An RSA key that may verify RS256 signatures. The issuer's other keys are ignored.
    fn verifies_rs256(&self) -> bool {
        self.kty == "RSA"
            && self.alg.as_deref().is_none_or(|alg| alg == "RS256")
            && self.usage.as_deref().is_none_or(|usage| usage == "sig")
            && self
                .operations
                .as_ref()
                .is_none_or(|ops| ops.iter().any(|op| op == "verify"))
    }
}

#[derive(Deserialize)]
struct KeySet {
    keys: Vec<Key>,
}

impl IdentityValidator {
    pub(super) async fn decoding_key(
        &self,
        uri: &str,
        kid: &str,
        cancel: &CancellationToken,
    ) -> Result<DecodingKey> {
        let key = self.key(uri, kid, cancel).await?;
        DecodingKey::from_rsa_components(
            key.n.as_deref().context("identity RSA modulus missing")?,
            key.e.as_deref().context("identity RSA exponent missing")?,
        )
        .map_err(|_| anyhow::anyhow!("invalid identity RSA key"))
    }

    async fn key(&self, uri: &str, kid: &str, cancel: &CancellationToken) -> Result<Key> {
        let mut cache = tokio::select! {
            () = cancel.cancelled() => anyhow::bail!("authentication cancelled"),
            cache = self.cache.lock() => cache,
        };
        if let Some(cached) = cache.as_ref()
            && cached.uri == uri
            && cached.loaded_at.elapsed() < Duration::from_secs(300)
            && let Some(key) = cached
                .keys
                .iter()
                .find(|key| key.kid.as_deref() == Some(kid))
        {
            return Ok(key.clone());
        }
        let keys = self.fetch_keys(uri, cancel).await?;
        let selected = keys
            .iter()
            .find(|key| key.kid.as_deref() == Some(kid))
            .cloned();
        *cache = Some(KeyCache {
            uri: uri.into(),
            loaded_at: Instant::now(),
            keys,
        });
        selected.context("identity key ID is not published by issuer")
    }

    /// The issuer's usable verification keys, each bounded and with a unique key ID.
    async fn fetch_keys(&self, uri: &str, cancel: &CancellationToken) -> Result<Vec<Key>> {
        let response = self.http.get(uri, cancel).await?;
        ensure!(
            response.status == StatusCode::OK,
            "identity keys unavailable"
        );
        let published: KeySet = serde_json::from_slice(&response.body)
            .map_err(|_| anyhow::anyhow!("invalid identity key set"))?;
        ensure!(
            !published.keys.is_empty() && published.keys.len() <= 32,
            "identity key set exceeds limit"
        );
        let mut valid: Vec<Key> = Vec::new();
        for key in published.keys {
            if !key.verifies_rs256() {
                continue;
            }
            let (Some(n), Some(e)) = (key.n.as_deref(), key.e.as_deref()) else {
                continue;
            };
            ensure_rsa_size(n, e)?;
            ensure!(
                !valid.iter().any(|existing| existing.kid == key.kid),
                "duplicate identity key ID"
            );
            valid.push(key);
        }
        Ok(valid)
    }
}

fn ensure_rsa_size(modulus: &str, exponent: &str) -> Result<()> {
    let modulus = URL_SAFE_NO_PAD
        .decode(modulus)
        .map_err(|_| anyhow::anyhow!("invalid identity key modulus"))?;
    let exponent = URL_SAFE_NO_PAD
        .decode(exponent)
        .map_err(|_| anyhow::anyhow!("invalid identity key exponent"))?;
    ensure!(
        (256..=1024).contains(&modulus.len()) && (1..=8).contains(&exponent.len()),
        "identity RSA key size rejected"
    );
    Ok(())
}
