mod keys;

use std::sync::Arc;

use anyhow::{Context, Result, ensure};
use jsonwebtoken::{Algorithm, Validation, decode, decode_header};
use serde::Deserialize;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::engine::{Discovery, Http, ISSUER, equal, timestamp};
use keys::KeyCache;

pub(crate) struct IdentityValidator {
    http: Arc<Http>,
    cache: Mutex<Option<KeyCache>>,
}

#[derive(Deserialize)]
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
        let claims = self
            .signed_claims(token, &discovery.jwks_uri, client, cancel)
            .await?;
        ensure_claims(&claims, client, nonce, subject)?;
        Ok(Identity {
            issuer: claims.iss,
            subject: claims.sub,
            email: claims.email,
        })
    }

    /// Decode the claims of a token with a valid RS256 signature, audience, and lifetime.
    async fn signed_claims(
        &self,
        token: &str,
        jwks_uri: &str,
        client: &str,
        cancel: &CancellationToken,
    ) -> Result<Claims> {
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
        let decoding = self.decoding_key(jwks_uri, &kid, cancel).await?;
        let mut validation = Validation::new(Algorithm::RS256);
        validation.leeway = 5;
        validation.validate_nbf = true;
        validation.set_issuer(&[ISSUER]);
        validation.set_audience(&[client]);
        validation.set_required_spec_claims(&["exp", "iat", "iss", "aud", "sub"]);
        Ok(decode::<Claims>(token, &decoding, &validation)
            .map_err(|_| anyhow::anyhow!("identity signature or claims rejected"))?
            .claims)
    }
}

/// Bind the claims to this client, and to the expected nonce and account when given.
fn ensure_claims(
    claims: &Claims,
    client: &str,
    nonce: Option<&str>,
    subject: Option<&str>,
) -> Result<()> {
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
    Ok(())
}
