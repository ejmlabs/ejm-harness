use std::fmt;

use anyhow::{Result, ensure};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

/// A secret with explicit access, redacted diagnostics, and zeroed owned storage on drop.
#[derive(Clone)]
pub struct SecretString(Zeroizing<String>);

impl SecretString {
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(Zeroizing::new(value.into()))
    }

    #[must_use]
    pub fn expose_secret(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretString([REDACTED])")
    }
}

#[async_trait]
pub trait CredentialSource: Send + Sync {
    /// Resolve a usable bearer token, renewing it when needed.
    ///
    /// # Errors
    /// Returns an error for cancellation, missing grants, or unavailable credentials.
    async fn access_token(&self, cancel: CancellationToken) -> Result<SecretString>;
}

pub struct StaticCredential {
    token: SecretString,
}

impl StaticCredential {
    #[must_use]
    pub fn new(token: SecretString) -> Self {
        Self { token }
    }
}

#[async_trait]
impl CredentialSource for StaticCredential {
    async fn access_token(&self, cancel: CancellationToken) -> Result<SecretString> {
        ensure!(!cancel.is_cancelled(), "credential request cancelled");
        ensure!(
            !self.token.expose_secret().is_empty(),
            "credential is empty"
        );
        Ok(self.token.clone())
    }
}

#[derive(Clone, Debug)]
pub struct AuthConfig {
    pub agent_name: String,
}

#[derive(Clone, Debug)]
pub struct LoginAttempt {
    pub id: String,
    pub authorization_url: SecretString,
    pub redirect_uri: String,
    /// Expiration as Unix seconds.
    pub expires_at: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RegistrationStatus {
    pub id: String,
    pub issuer: String,
    pub subject: String,
    pub client_id: String,
    pub email: Option<String>,
    pub signed_in: bool,
    pub plan_enabled: bool,
    pub expires_at: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LogoutResult {
    pub remote_revoked: bool,
}

/// A rejected authorization code can be retried with the issued client registration.
#[derive(Clone, Debug)]
pub struct RetryRequired {
    pub attempt_id: String,
}

impl fmt::Display for RetryRequired {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("authorization code expired or was rejected; retry sign-in")
    }
}

impl std::error::Error for RetryRequired {}

/// Sensitive state, available only to an injected credential store.
#[derive(Clone, Debug, Default)]
pub struct StoredAuth {
    pub host_id: String,
    pub registrations: Vec<StoredRegistration>,
}

#[derive(Clone, Debug)]
pub struct StoredRegistration {
    pub id: String,
    pub issuer: String,
    pub subject: String,
    pub client_id: String,
    pub email: Option<String>,
    pub credentials: Option<StoredCredentials>,
    /// Sign-out invalidates authorizations begun under an earlier revision.
    pub authorization_revision: u64,
    /// A durable marker prevents reuse after an interrupted rotating-token exchange.
    pub refresh_pending: bool,
}

#[derive(Clone, Debug)]
pub struct StoredCredentials {
    pub access_token: Option<SecretString>,
    pub refresh_token: Option<SecretString>,
    pub id_token: SecretString,
    pub scopes: Vec<String>,
    pub expires_at: Option<u64>,
    pub earliest_refresh_at: Option<u64>,
}

impl StoredRegistration {
    pub(crate) fn status(&self) -> RegistrationStatus {
        let usable = self.credentials.as_ref().filter(|_| !self.refresh_pending);
        RegistrationStatus {
            id: self.id.clone(),
            issuer: self.issuer.clone(),
            subject: self.subject.clone(),
            client_id: self.client_id.clone(),
            email: self.email.clone(),
            signed_in: usable.is_some(),
            plan_enabled: usable.is_some_and(|credentials| {
                credentials.access_token.is_some() && plan_enabled(&credentials.scopes)
            }),
            expires_at: usable.and_then(|credentials| credentials.expires_at),
        }
    }
}

pub(crate) fn plan_enabled(scopes: &[String]) -> bool {
    ["resource.invoke", "chatgpt.tokens.use.direct"]
        .iter()
        .all(|required| scopes.iter().any(|scope| scope == required))
}
