use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, ensure};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use reqwest::StatusCode;
use serde::Deserialize;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::engine::{Discovery, Http, ISSUER, equal, timestamp};

pub(crate) struct IdentityValidator {
    http: Arc<Http>,
    cache: Mutex<Option<KeyCache>>,
}

struct KeyCache {
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

#[derive(Deserialize)]
struct KeySet {
    keys: Vec<Key>,
}

#[derive(Clone, Deserialize)]
struct Claims {
    iss: String,
    sub: String,
    aud: serde_json::Value,
    exp: u64,
    iat: u64,
    nonce: Option<String>,
    azp: Option<String>,
    email: Option<String>,
}

pub(crate) struct Identity {
    pub issuer: String,
    pub subject: String,
    pub email: Option<String>,
}

impl IdentityValidator {
    pub(crate) fn new(http: Arc<Http>) -> Self {
        Self {
            http,
            cache: Mutex::new(None),
        }
    }

    pub(crate) async fn validate(
        &self,
        token: &str,
        discovery: &Discovery,
        client: &str,
        nonce: Option<&str>,
        subject: Option<&str>,
        cancel: &CancellationToken,
    ) -> Result<Identity> {
        ensure!(
            !token.is_empty() && token.len() <= 64 * 1024,
            "identity token exceeds limit"
        );
        let header =
            decode_header(token).map_err(|_| anyhow::anyhow!("invalid identity token header"))?;
        ensure!(
            header.alg == Algorithm::RS256,
            "identity token must use RS256"
        );
        let kid = header.kid.context("identity token has no key ID")?;
        ensure!(
            !kid.is_empty() && kid.len() <= 256,
            "invalid identity key ID"
        );
        let key = self.key(&discovery.jwks_uri, &kid, cancel).await?;
        let decoding = DecodingKey::from_rsa_components(
            key.n.as_deref().context("identity RSA modulus missing")?,
            key.e.as_deref().context("identity RSA exponent missing")?,
        )
        .map_err(|_| anyhow::anyhow!("invalid identity RSA key"))?;
        let mut validation = Validation::new(Algorithm::RS256);
        validation.leeway = 5;
        validation.validate_nbf = true;
        validation.set_issuer(&[ISSUER]);
        validation.set_audience(&[client]);
        validation.set_required_spec_claims(&["exp", "iat", "iss", "aud", "sub"]);
        let claims = decode::<Claims>(token, &decoding, &validation)
            .map_err(|_| anyhow::anyhow!("identity signature or claims rejected"))?
            .claims;
        ensure!(
            claims.iss == ISSUER
                && claims.iat <= timestamp().saturating_add(5)
                && claims.exp >= claims.iat,
            "identity issue time or issuer rejected"
        );
        ensure!(
            !claims.sub.is_empty() && claims.sub.len() <= 1024,
            "identity subject missing or exceeds limit"
        );
        if let Some(expected) = nonce {
            ensure!(
                claims
                    .nonce
                    .as_deref()
                    .is_some_and(|actual| equal(actual, expected)),
                "identity nonce mismatch"
            );
        }
        if let Some(expected) = subject {
            ensure!(
                claims.sub == expected,
                "sign-in returned a different account"
            );
        }
        if let Some(authorized) = claims.azp.as_deref() {
            ensure!(authorized == client, "identity authorized party mismatch");
        }
        if claims
            .aud
            .as_array()
            .is_some_and(|audiences| audiences.len() > 1)
        {
            ensure!(
                claims.azp.as_deref() == Some(client),
                "identity with multiple audiences needs client binding"
            );
        }
        ensure!(
            claims
                .email
                .as_ref()
                .is_none_or(|email| email.len() <= 1024),
            "identity email exceeds limit"
        );
        Ok(Identity {
            issuer: claims.iss,
            subject: claims.sub,
            email: claims.email,
        })
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
        let response = self.http.get(uri, cancel).await?;
        ensure!(
            response.status == StatusCode::OK,
            "identity keys unavailable"
        );
        let keys: KeySet = serde_json::from_slice(&response.body)
            .map_err(|_| anyhow::anyhow!("invalid identity key set"))?;
        ensure!(
            !keys.keys.is_empty() && keys.keys.len() <= 32,
            "identity key set exceeds limit"
        );
        let mut valid = Vec::new();
        for key in keys.keys {
            if key.kty != "RSA"
                || key.alg.as_deref().is_some_and(|alg| alg != "RS256")
                || key.usage.as_deref().is_some_and(|usage| usage != "sig")
                || key
                    .operations
                    .as_ref()
                    .is_some_and(|ops| !ops.iter().any(|op| op == "verify"))
            {
                continue;
            }
            let Some(n) = key.n.as_ref() else {
                continue;
            };
            let Some(e) = key.e.as_ref() else {
                continue;
            };
            let modulus = URL_SAFE_NO_PAD
                .decode(n)
                .map_err(|_| anyhow::anyhow!("invalid identity key modulus"))?;
            let exponent = URL_SAFE_NO_PAD
                .decode(e)
                .map_err(|_| anyhow::anyhow!("invalid identity key exponent"))?;
            ensure!(
                (256..=1024).contains(&modulus.len()) && (1..=8).contains(&exponent.len()),
                "identity RSA key size rejected"
            );
            ensure!(
                !valid.iter().any(|existing: &Key| existing.kid == key.kid),
                "duplicate identity key ID"
            );
            valid.push(key);
        }
        let selected = valid
            .iter()
            .find(|key| key.kid.as_deref() == Some(kid))
            .cloned();
        *cache = Some(KeyCache {
            uri: uri.into(),
            loaded_at: Instant::now(),
            keys: valid,
        });
        selected.context("identity key ID is not published by issuer")
    }
}
