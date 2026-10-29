//! Resolve a usable access token, renewing it under the shared credential lock when needed.

use std::sync::Arc;

use anyhow::{Context, Result, bail, ensure};
use async_trait::async_trait;
use reqwest::StatusCode;
use tokio_util::sync::CancellationToken;

use super::{
    AuthEngine, RESOURCE,
    accounts::{ensure_issued, registration_index},
    http::HttpResponse,
    timestamp,
    token::TokenResponse,
};
use crate::{
    CredentialSource, CredentialTransaction, SecretString, StoredAuth, StoredCredentials,
    types::plan_enabled,
};

/// An access token with at least this long to live is used without renewal.
const RENEW_BEFORE_EXPIRY_SECONDS: u64 = 60;
/// While the provider defers renewal, an access token must have at least this long to live.
const MIN_DEFERRED_VALIDITY_SECONDS: u64 = 5;

struct RegistrationSource {
    engine: AuthEngine,
    id: String,
}

#[async_trait]
impl CredentialSource for RegistrationSource {
    async fn access_token(&self, cancel: CancellationToken) -> Result<SecretString> {
        self.engine.resolve(&self.id, cancel).await
    }
}

impl AuthEngine {
    #[must_use]
    pub fn source(&self, registration_id: &str) -> Arc<dyn CredentialSource> {
        Arc::new(RegistrationSource {
            engine: self.clone(),
            id: registration_id.into(),
        })
    }

    async fn resolve(&self, id: &str, cancel: CancellationToken) -> Result<SecretString> {
        let mut transaction = self.inner.store.lock(cancel.clone()).await?;
        let mut stored = transaction.load()?;
        let index = registration_index(&stored, id)?;
        let registration = &stored.registrations[index];
        ensure_issued(registration)?;
        ensure!(
            !registration.refresh_pending,
            "credential renewal was interrupted; sign in again"
        );
        let credentials = registration
            .credentials
            .as_ref()
            .context("account is signed out; sign in again")?;
        ensure!(
            plan_enabled(&credentials.scopes),
            "ChatGPT plan usage was not granted"
        );
        if access_token_is_reusable(credentials, timestamp())? {
            return credentials
                .access_token
                .clone()
                .context("account has no access token");
        }
        self.renew(&mut stored, index, transaction.as_mut(), &cancel)
            .await
    }

    /// Rotate the refresh token. The pending marker is durable before the request, so a crash
    /// mid-rotation requires a new sign-in instead of replaying the old token.
    async fn renew(
        &self,
        stored: &mut StoredAuth,
        index: usize,
        transaction: &mut dyn CredentialTransaction,
        cancel: &CancellationToken,
    ) -> Result<SecretString> {
        let registration = &stored.registrations[index];
        let previous = registration
            .credentials
            .clone()
            .context("account is signed out")?;
        let refresh = previous
            .refresh_token
            .as_ref()
            .context("account cannot renew access; sign in again")?;
        let client_id = registration.client_id.clone();
        let subject = registration.subject.clone();
        ensure_issued(registration)?;
        let discovery = self.discovery(cancel).await?;
        stored.registrations[index].refresh_pending = true;
        transaction.save(stored)?;
        let response = self
            .exchange_refresh_token(&discovery.token_endpoint, &client_id, refresh, cancel)
            .await?;
        if response.status != StatusCode::OK {
            stored.registrations[index].credentials = None;
            stored.registrations[index].refresh_pending = false;
            transaction.save(stored)?;
            bail!("credential renewal rejected; sign in again");
        }
        let mut tokens: TokenResponse = serde_json::from_slice(&response.body)
            .map_err(|_| anyhow::anyhow!("invalid credential renewal response; sign in again"))?;
        ensure!(
            tokens.refresh_token.is_some(),
            "renewal omitted replacement refresh token; sign in again"
        );
        let id_token = if let Some(token) = tokens.id_token.take() {
            self.inner
                .identity
                .validate(
                    token.expose_secret(),
                    &discovery,
                    &client_id,
                    None,
                    Some(&subject),
                    cancel,
                )
                .await?;
            token
        } else {
            previous.id_token.clone()
        };
        let next = tokens.credentials(id_token, Some(&previous.scopes))?;
        let access = next.access_token.clone();
        let enabled = plan_enabled(&next.scopes);
        stored.registrations[index].credentials = Some(next);
        stored.registrations[index].refresh_pending = false;
        transaction.save(stored)?;
        ensure!(enabled, "ChatGPT plan usage is no longer granted");
        access.context("renewal response has no access token")
    }

    async fn exchange_refresh_token(
        &self,
        token_endpoint: &str,
        client_id: &str,
        refresh: &SecretString,
        cancel: &CancellationToken,
    ) -> Result<HttpResponse> {
        self.inner
            .http
            .post(
                token_endpoint,
                &[
                    ("grant_type", "refresh_token"),
                    ("client_id", client_id),
                    ("refresh_token", refresh.expose_secret()),
                    ("resource", RESOURCE),
                ],
                cancel,
            )
            .await
    }
}

/// Whether the stored access token can be used now without renewing it. Fails when the
/// provider defers renewal but the access token has nearly expired.
fn access_token_is_reusable(credentials: &StoredCredentials, now: u64) -> Result<bool> {
    let valid_for = |seconds: u64| {
        credentials
            .expires_at
            .is_some_and(|expiry| expiry > now.saturating_add(seconds))
    };
    if valid_for(RENEW_BEFORE_EXPIRY_SECONDS) {
        return Ok(true);
    }
    if credentials
        .earliest_refresh_at
        .is_some_and(|earliest| earliest > now)
    {
        ensure!(
            valid_for(MIN_DEFERRED_VALIDITY_SECONDS),
            "access expired before permitted refresh; sign in again"
        );
        return Ok(true);
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::access_token_is_reusable;
    use crate::{SecretString, StoredCredentials};

    fn credentials(expires_at: Option<u64>, earliest_refresh_at: Option<u64>) -> StoredCredentials {
        StoredCredentials {
            access_token: Some(SecretString::new("access")),
            refresh_token: Some(SecretString::new("refresh")),
            id_token: SecretString::new("identity"),
            scopes: Vec::new(),
            expires_at,
            earliest_refresh_at,
        }
    }

    #[test]
    fn access_token_reuse_window_matches_renewal_timing() -> anyhow::Result<()> {
        let now = 1_000;
        assert!(access_token_is_reusable(
            &credentials(Some(now + 61), None),
            now
        )?);
        assert!(!access_token_is_reusable(
            &credentials(Some(now + 60), None),
            now
        )?);
        assert!(!access_token_is_reusable(&credentials(None, None), now)?);
        // While the provider defers renewal, only a token with more than five seconds left is reused.
        assert!(access_token_is_reusable(
            &credentials(Some(now + 6), Some(now + 1)),
            now
        )?);
        assert!(access_token_is_reusable(&credentials(Some(now + 5), Some(now + 1)), now).is_err());
        assert!(!access_token_is_reusable(
            &credentials(Some(now + 30), Some(now)),
            now
        )?);
        Ok(())
    }
}
