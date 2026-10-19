use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail, ensure};
use async_trait::async_trait;
use futures_util::StreamExt;
use oauth2::{CsrfToken, PkceCodeChallenge};
use reqwest::{StatusCode, Url};
use serde::{Deserialize, Deserializer};
use subtle::ConstantTimeEq;
use tokio::sync::{Mutex as AsyncMutex, Semaphore};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

use crate::{
    AuthConfig, CredentialSource, CredentialStore, CredentialTransaction, LoginAttempt,
    LogoutResult, RegistrationStatus, RetryRequired, SecretString, StoredAuth, StoredCredentials,
    StoredRegistration, identity::IdentityValidator, types::plan_enabled,
};

pub(crate) const ISSUER: &str = "https://auth.openai.com";
const RESOURCE: &str = "https://api.openai.com/v1";
const DYNAMIC_CLIENT: &str = "dynamic_agent_client";
const SCOPES: &str =
    "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct";
const MAX_BODY: usize = 1024 * 1024;
const MAX_ATTEMPTS: usize = 8;
const ATTEMPT_SECONDS: u64 = 600;

#[derive(Clone)]
pub struct AuthEngine {
    inner: Arc<Inner>,
}

struct Inner {
    config: AuthConfig,
    store: Arc<dyn CredentialStore>,
    http: Arc<Http>,
    identity: IdentityValidator,
    discovery: AsyncMutex<Option<Discovery>>,
    attempts: Mutex<HashMap<String, Arc<Attempt>>>,
}

struct Attempt {
    secrets: Mutex<Option<AttemptSecrets>>,
    retry: Mutex<Option<RetryHint>>,
    cancel: CancellationToken,
    expires_at: u64,
}

struct RetryHint {
    client_id: String,
    registration_id: Option<String>,
    expected_subject: Option<String>,
    expected_revision: Option<u64>,
}

struct AttemptSecrets {
    state: SecretString,
    nonce: SecretString,
    verifier: SecretString,
    challenge: PkceCodeChallenge,
    redirect_uri: Url,
    client_id: String,
    new_registration: bool,
    registration_id: Option<String>,
    expected_subject: Option<String>,
    expected_revision: Option<u64>,
}

struct AttemptGuard {
    engine: AuthEngine,
    id: String,
}

impl Drop for AttemptGuard {
    fn drop(&mut self) {
        let mut attempts = self
            .engine
            .inner
            .attempts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let retrying = attempts.get(&self.id).is_some_and(|attempt| {
            !attempt.cancel.is_cancelled()
                && attempt
                    .retry
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .is_some()
        });
        if !retrying && let Some(attempt) = attempts.remove(&self.id) {
            attempt.cancel.cancel();
        }
    }
}

#[derive(Clone, Deserialize)]
pub(crate) struct Discovery {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub jwks_uri: String,
    pub revocation_endpoint: Option<String>,
}

pub(crate) struct Http {
    client: reqwest::Client,
    slots: Semaphore,
    discovery_url: String,
    allowed_origin: String,
}

pub(crate) struct HttpResponse {
    pub status: StatusCode,
    pub body: Zeroizing<Vec<u8>>,
}

impl Http {
    fn new() -> Result<Self> {
        Ok(Self {
            client: reqwest::Client::builder()
                .https_only(true)
                .redirect(reqwest::redirect::Policy::none())
                .connect_timeout(Duration::from_secs(5))
                .timeout(Duration::from_secs(20))
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

    async fn post(
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
            ensure!(
                response
                    .content_length()
                    .is_none_or(|length| length <= MAX_BODY as u64),
                "authentication response exceeds 1 MiB"
            );
            let mut stream = response.bytes_stream();
            let mut body = Zeroizing::new(Vec::new());
            while let Some(chunk) = stream.next().await {
                let chunk =
                    chunk.map_err(|_| anyhow::anyhow!("authentication response interrupted"))?;
                ensure!(
                    body.len() + chunk.len() <= MAX_BODY,
                    "authentication response exceeds 1 MiB"
                );
                body.extend_from_slice(&chunk);
            }
            Ok(HttpResponse { status, body })
        };
        tokio::select! {
            biased;
            () = cancel.cancelled() => bail!("authentication cancelled"),
            result = tokio::time::timeout(Duration::from_secs(20), operation) => result.context("authentication request timed out")?,
        }
    }
}

impl AuthEngine {
    /// Create a `ChatGPT` public-client engine. The host supplies private credential storage.
    ///
    /// # Errors
    /// Rejects an empty app name or an unavailable HTTP transport.
    pub fn new(config: AuthConfig, store: Arc<dyn CredentialStore>) -> Result<Self> {
        ensure!(
            !config.agent_name.trim().is_empty() && config.agent_name.len() <= 128,
            "invalid agent name"
        );
        let http = Arc::new(Http::new()?);
        Ok(Self {
            inner: Arc::new(Inner {
                config,
                store,
                identity: IdentityValidator::new(http.clone()),
                http,
                discovery: AsyncMutex::new(None),
                attempts: Mutex::new(HashMap::new()),
            }),
        })
    }

    /// Prepare an authorization URL after the host starts its loopback listener.
    ///
    /// # Errors
    /// Rejects invalid callbacks, unavailable storage, or more than eight live attempts.
    pub async fn start(
        &self,
        redirect_uri: &str,
        registration_id: Option<&str>,
        cancel: CancellationToken,
    ) -> Result<LoginAttempt> {
        self.begin(redirect_uri, registration_id, None, cancel)
            .await
    }

    /// Retry a rejected code with its issued client ID and fresh authorization secrets.
    ///
    /// # Errors
    /// Rejects unknown, consumed, cancelled, or expired retry hints and unsafe callbacks.
    pub async fn retry(
        &self,
        attempt_id: &str,
        redirect_uri: &str,
        cancel: CancellationToken,
    ) -> Result<LoginAttempt> {
        callback_redirect(redirect_uri)?;
        let previous = self
            .inner
            .attempts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(attempt_id)
            .context("unknown or consumed sign-in retry")?;
        ensure!(
            previous.expires_at > timestamp() && !previous.cancel.is_cancelled(),
            "sign-in retry expired or cancelled"
        );
        let hint = previous
            .retry
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
            .context("sign-in attempt does not require retry")?;
        previous.cancel.cancel();
        self.begin(redirect_uri, None, Some(hint), cancel).await
    }

    async fn begin(
        &self,
        redirect_uri: &str,
        registration_id: Option<&str>,
        retry: Option<RetryHint>,
        cancel: CancellationToken,
    ) -> Result<LoginAttempt> {
        ensure!(!cancel.is_cancelled(), "sign-in cancelled");
        let redirect = callback_redirect(redirect_uri)?;
        let discovery = self.discovery(&cancel).await?;
        let mut transaction = self.inner.store.lock(cancel.clone()).await?;
        let mut stored = transaction.load()?;
        if stored.host_id.is_empty() {
            stored.host_id = format!("urn:uuid:{}", uuid::Uuid::new_v4());
            transaction.save(&stored)?;
        }
        let saved_id = registration_id.or_else(|| {
            retry
                .as_ref()
                .and_then(|hint| hint.registration_id.as_deref())
        });
        let registration = saved_id
            .map(|id| {
                stored
                    .registrations
                    .iter()
                    .find(|entry| entry.id == id)
                    .context("unknown account registration")
            })
            .transpose()?;
        if let Some(registration) = registration {
            ensure!(
                registration.issuer == ISSUER && valid_client(&registration.client_id),
                "invalid saved account registration"
            );
        }
        let client_id = retry.as_ref().map_or_else(
            || registration.map_or_else(|| DYNAMIC_CLIENT.into(), |entry| entry.client_id.clone()),
            |hint| hint.client_id.clone(),
        );
        if let (Some(hint), Some(registration)) = (&retry, registration) {
            ensure!(
                hint.client_id == registration.client_id
                    && hint.expected_subject.as_deref() == Some(&registration.subject)
                    && hint.expected_revision == Some(registration.authorization_revision),
                "retry account changed during authorization"
            );
        }
        let (challenge, verifier) = PkceCodeChallenge::new_random_sha256_len(64);
        let secrets = AttemptSecrets {
            state: SecretString::new(CsrfToken::new_random_len(32).into_secret()),
            nonce: SecretString::new(CsrfToken::new_random_len(32).into_secret()),
            verifier: SecretString::new(verifier.into_secret()),
            challenge,
            redirect_uri: redirect,
            client_id,
            new_registration: registration.is_none() && retry.is_none(),
            registration_id: saved_id.map(str::to_owned),
            expected_subject: registration.map(|entry| entry.subject.clone()),
            expected_revision: registration.map(|entry| entry.authorization_revision),
        };
        let authorization_url = self.authorization_url(&discovery, &secrets, &stored.host_id)?;
        drop(transaction);
        let now = timestamp();
        let expires_at = now.saturating_add(ATTEMPT_SECONDS);
        let id = uuid::Uuid::new_v4().to_string();
        let mut attempts = self
            .inner
            .attempts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        attempts.retain(|_, attempt| {
            if attempt.expires_at <= now || attempt.cancel.is_cancelled() {
                attempt.cancel.cancel();
                false
            } else {
                true
            }
        });
        ensure!(
            attempts.len() < MAX_ATTEMPTS,
            "at most eight sign-in attempts may be pending"
        );
        attempts.insert(
            id.clone(),
            Arc::new(Attempt {
                secrets: Mutex::new(Some(secrets)),
                retry: Mutex::new(None),
                cancel: cancel.child_token(),
                expires_at,
            }),
        );
        Ok(LoginAttempt {
            id,
            authorization_url,
            redirect_uri: redirect_uri.into(),
            expires_at,
        })
    }

    fn authorization_url(
        &self,
        discovery: &Discovery,
        pending: &AttemptSecrets,
        host: &str,
    ) -> Result<SecretString> {
        let mut url = self
            .inner
            .http
            .endpoint(&discovery.authorization_endpoint)?;
        {
            let mut query = url.query_pairs_mut();
            query.extend_pairs([
                ("client_id", pending.client_id.as_str()),
                ("ext_agent_host_id", host),
                ("response_type", "code"),
                ("redirect_uri", pending.redirect_uri.as_str()),
                ("scope", SCOPES),
                ("resource", RESOURCE),
                ("state", pending.state.expose_secret()),
                ("nonce", pending.nonce.expose_secret()),
                ("code_challenge_method", pending.challenge.method().as_str()),
                ("code_challenge", pending.challenge.as_str()),
            ]);
            if pending.new_registration {
                query.append_pair("agent_name_hint", &self.inner.config.agent_name);
            }
        }
        // Omit token and email hints so hosts can safely open or explicitly display this URL.
        Ok(SecretString::new(url.to_string()))
    }

    /// Consume one callback, verify identity, and atomically save that account's credentials.
    ///
    /// # Errors
    /// Rejects reused attempts, state or identity mismatches, missing grants, and HTTP failures.
    pub async fn complete(
        &self,
        attempt_id: &str,
        callback_uri: &str,
        cancel: CancellationToken,
    ) -> Result<RegistrationStatus> {
        let attempt = self
            .inner
            .attempts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(attempt_id)
            .cloned()
            .context("unknown or completed sign-in attempt")?;
        ensure!(
            attempt.expires_at > timestamp() && !attempt.cancel.is_cancelled(),
            "sign-in attempt expired or cancelled"
        );
        let pending = attempt
            .secrets
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
            .context("sign-in callback was already consumed")?;
        let _guard = AttemptGuard {
            engine: self.clone(),
            id: attempt_id.into(),
        };
        let operation = self.complete_pending(attempt_id, &attempt, pending, callback_uri, &cancel);
        tokio::select! {
            biased;
            () = attempt.cancel.cancelled() => bail!("sign-in attempt cancelled"),
            () = cancel.cancelled() => bail!("sign-in attempt cancelled"),
            result = operation => result,
        }
    }

    async fn complete_pending(
        &self,
        attempt_id: &str,
        attempt: &Arc<Attempt>,
        pending: AttemptSecrets,
        callback_uri: &str,
        cancel: &CancellationToken,
    ) -> Result<RegistrationStatus> {
        let callback = parse_callback(&pending, callback_uri)?;
        let discovery = self.discovery(cancel).await?;
        let response = self
            .exchange(&pending, &callback, &discovery, cancel)
            .await?;
        if response.status != StatusCode::OK {
            #[derive(Deserialize)]
            struct Rejection {
                error: String,
            }
            if serde_json::from_slice::<Rejection>(&response.body)
                .is_ok_and(|rejection| rejection.error == "invalid_grant")
            {
                ensure!(
                    !cancel.is_cancelled() && !attempt.cancel.is_cancelled(),
                    "sign-in cancelled"
                );
                *attempt
                    .retry
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(RetryHint {
                    client_id: callback.client_id,
                    registration_id: pending.registration_id,
                    expected_subject: pending.expected_subject,
                    expected_revision: pending.expected_revision,
                });
                return Err(RetryRequired {
                    attempt_id: attempt_id.into(),
                }
                .into());
            }
            bail!("authorization code exchange rejected; start a fresh sign-in");
        }
        let mut tokens: TokenResponse = serde_json::from_slice(&response.body)
            .map_err(|_| anyhow::anyhow!("invalid authorization token response"))?;
        let id_token = tokens
            .id_token
            .take()
            .context("authorization response has no identity token")?;
        let identity = self
            .inner
            .identity
            .validate(
                id_token.expose_secret(),
                &discovery,
                &callback.client_id,
                Some(pending.nonce.expose_secret()),
                pending.expected_subject.as_deref(),
                cancel,
            )
            .await?;
        let credentials = tokens.credentials(id_token, None)?;
        let mut transaction = self.inner.store.lock(cancel.clone()).await?;
        let mut stored = transaction.load()?;
        let index = if let Some(id) = pending.registration_id {
            let index = stored
                .registrations
                .iter()
                .position(|entry| entry.id == id)
                .context("account registration was removed")?;
            let selected = &stored.registrations[index];
            ensure!(
                selected.issuer == identity.issuer
                    && selected.subject == identity.subject
                    && selected.client_id == callback.client_id
                    && Some(selected.authorization_revision) == pending.expected_revision,
                "sign-in account changed during authorization"
            );
            index
        } else {
            ensure!(
                stored.registrations.len() < 64,
                "at most 64 account registrations may be saved"
            );
            stored.registrations.push(StoredRegistration {
                id: uuid::Uuid::new_v4().to_string(),
                issuer: identity.issuer,
                subject: identity.subject,
                client_id: callback.client_id,
                email: identity.email.clone(),
                credentials: None,
                authorization_revision: 0,
                refresh_pending: false,
            });
            stored.registrations.len() - 1
        };
        stored.registrations[index].email = identity.email;
        stored.registrations[index].credentials = Some(credentials);
        stored.registrations[index].refresh_pending = false;
        self.commit_attempt(attempt_id, attempt, cancel, transaction.as_mut(), &stored)?;
        Ok(stored.registrations[index].status())
    }

    async fn exchange(
        &self,
        pending: &AttemptSecrets,
        callback: &Callback,
        discovery: &Discovery,
        cancel: &CancellationToken,
    ) -> Result<HttpResponse> {
        self.inner
            .http
            .post(
                &discovery.token_endpoint,
                &[
                    ("grant_type", "authorization_code"),
                    ("client_id", &callback.client_id),
                    ("code", callback.code.expose_secret()),
                    ("code_verifier", pending.verifier.expose_secret()),
                    ("redirect_uri", pending.redirect_uri.as_str()),
                    ("resource", RESOURCE),
                ],
                cancel,
            )
            .await
    }

    fn commit_attempt(
        &self,
        id: &str,
        attempt: &Arc<Attempt>,
        cancel: &CancellationToken,
        transaction: &mut dyn CredentialTransaction,
        stored: &StoredAuth,
    ) -> Result<()> {
        // Share cancellation's lock so saving and cancellation have a defined order.
        let attempts = self
            .inner
            .attempts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        ensure!(
            attempts
                .get(id)
                .is_some_and(|current| Arc::ptr_eq(current, attempt))
                && !attempt.cancel.is_cancelled()
                && !cancel.is_cancelled()
                && attempt.expires_at > timestamp(),
            "sign-in expired or cancelled before save"
        );
        transaction.save(stored)
    }

    /// Cancel a pending or exchanging sign-in attempt.
    #[must_use]
    pub fn cancel(&self, attempt_id: &str) -> bool {
        let pending = self
            .inner
            .attempts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(attempt_id);
        if let Some(attempt) = pending {
            attempt.cancel.cancel();
            true
        } else {
            false
        }
    }

    /// Read public account metadata without exposing credentials.
    ///
    /// # Errors
    /// Returns an error for unknown accounts, cancellation, or unavailable storage.
    pub async fn status(
        &self,
        registration_id: &str,
        cancel: CancellationToken,
    ) -> Result<RegistrationStatus> {
        let mut transaction = self.inner.store.lock(cancel).await?;
        let stored = transaction.load()?;
        Ok(stored
            .registrations
            .iter()
            .find(|entry| entry.id == registration_id)
            .context("unknown account registration")?
            .status())
    }

    /// List separate account and client registrations. Email is display metadata only.
    ///
    /// # Errors
    /// Returns an error for cancellation or unavailable storage.
    pub async fn registrations(
        &self,
        cancel: CancellationToken,
    ) -> Result<Vec<RegistrationStatus>> {
        let mut transaction = self.inner.store.lock(cancel).await?;
        Ok(transaction
            .load()?
            .registrations
            .iter()
            .map(StoredRegistration::status)
            .collect())
    }

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
        let index = stored
            .registrations
            .iter()
            .position(|entry| entry.id == id)
            .context("unknown account registration")?;
        let registration = &stored.registrations[index];
        ensure!(
            registration.issuer == ISSUER && valid_client(&registration.client_id),
            "invalid saved account registration"
        );
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
        let now = timestamp();
        if credentials
            .expires_at
            .is_some_and(|expiry| expiry > now.saturating_add(60))
        {
            return credentials
                .access_token
                .clone()
                .context("account has no access token");
        }
        if credentials
            .earliest_refresh_at
            .is_some_and(|earliest| earliest > now)
        {
            ensure!(
                credentials
                    .expires_at
                    .is_some_and(|expiry| expiry > now.saturating_add(5)),
                "access expired before permitted refresh; sign in again"
            );
            return credentials
                .access_token
                .clone()
                .context("account has no access token");
        }
        self.renew(&mut stored, index, transaction.as_mut(), &cancel)
            .await
    }

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
        ensure!(
            registration.issuer == ISSUER && valid_client(&client_id),
            "invalid saved account registration"
        );
        let discovery = self.discovery(cancel).await?;
        stored.registrations[index].refresh_pending = true;
        transaction.save(stored)?;
        let response = self
            .inner
            .http
            .post(
                &discovery.token_endpoint,
                &[
                    ("grant_type", "refresh_token"),
                    ("client_id", &client_id),
                    ("refresh_token", refresh.expose_secret()),
                    ("resource", RESOURCE),
                ],
                cancel,
            )
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

    /// Clear local tokens and attempt idempotent remote session revocation.
    ///
    /// # Errors
    /// Returns an error if local credential storage cannot be updated.
    pub async fn logout(
        &self,
        registration_id: &str,
        cancel: CancellationToken,
    ) -> Result<LogoutResult> {
        let mut transaction = self.inner.store.lock(cancel.clone()).await?;
        let mut stored = transaction.load()?;
        let index = stored
            .registrations
            .iter()
            .position(|entry| entry.id == registration_id)
            .context("unknown account registration")?;
        let registration = &stored.registrations[index];
        let refresh = registration
            .credentials
            .as_ref()
            .and_then(|credentials| credentials.refresh_token.clone());
        let client_id = registration.client_id.clone();
        // Clearing first makes a crash or cancelled revocation fail closed across processes.
        stored.registrations[index].authorization_revision = registration
            .authorization_revision
            .checked_add(1)
            .context("account authorization revision exhausted")?;
        stored.registrations[index].credentials = None;
        stored.registrations[index].refresh_pending = false;
        transaction.save(&stored)?;
        drop(transaction);
        let remote_revoked = match refresh {
            Some(refresh) => tokio::time::timeout(
                Duration::from_secs(25),
                self.revoke(&client_id, &refresh, &cancel),
            )
            .await
            .unwrap_or(Ok(false))
            .unwrap_or(false),
            None => true,
        };
        Ok(LogoutResult { remote_revoked })
    }

    async fn revoke(
        &self,
        client_id: &str,
        token: &SecretString,
        cancel: &CancellationToken,
    ) -> Result<bool> {
        let discovery = self.discovery(cancel).await?;
        let endpoint = discovery
            .revocation_endpoint
            .context("revocation endpoint unavailable")?;
        for attempt in 0..3 {
            let response = self
                .inner
                .http
                .post(
                    &endpoint,
                    &[
                        ("token", token.expose_secret()),
                        ("token_type_hint", "refresh_token"),
                        ("client_id", client_id),
                    ],
                    cancel,
                )
                .await;
            match response {
                Ok(response) if response.status == StatusCode::OK => return Ok(true),
                Ok(response) if !response.status.is_server_error() => return Ok(false),
                _ if attempt == 2 => return Ok(false),
                _ => {}
            }
            tokio::select! {
                () = cancel.cancelled() => return Ok(false),
                () = tokio::time::sleep(Duration::from_millis(250 << attempt)) => {}
            }
        }
        Ok(false)
    }

    async fn discovery(&self, cancel: &CancellationToken) -> Result<Discovery> {
        let mut cache = tokio::select! {
            () = cancel.cancelled() => bail!("authentication cancelled"),
            cache = self.inner.discovery.lock() => cache,
        };
        if let Some(discovery) = cache.as_ref() {
            return Ok(discovery.clone());
        }
        let response = self
            .inner
            .http
            .get(&self.inner.http.discovery_url, cancel)
            .await?;
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
            self.inner.http.endpoint(value)?;
        }
        if let Some(endpoint) = &discovery.revocation_endpoint {
            self.inner.http.endpoint(endpoint)?;
        }
        *cache = Some(discovery.clone());
        Ok(discovery)
    }
}

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

#[derive(Deserialize)]
struct TokenResponse {
    #[serde(default, deserialize_with = "optional_secret")]
    access_token: Option<SecretString>,
    #[serde(default, deserialize_with = "optional_secret")]
    refresh_token: Option<SecretString>,
    #[serde(default, deserialize_with = "optional_secret")]
    id_token: Option<SecretString>,
    token_type: Option<String>,
    expires_in: Option<u64>,
    scope: Option<String>,
    earliest_refresh_at: Option<u64>,
}

impl TokenResponse {
    fn credentials(
        self,
        id_token: SecretString,
        previous_scopes: Option<&[String]>,
    ) -> Result<StoredCredentials> {
        for token in [self.access_token.as_ref(), self.refresh_token.as_ref()]
            .into_iter()
            .flatten()
        {
            ensure!(
                !token.expose_secret().is_empty() && token.expose_secret().len() <= 64 * 1024,
                "token field is missing or exceeds limit"
            );
        }
        if self.access_token.is_some() {
            ensure!(
                self.token_type
                    .as_deref()
                    .is_some_and(|kind| kind.eq_ignore_ascii_case("Bearer")),
                "unsupported access token type"
            );
        }
        let scopes = if let Some(scope) = self.scope {
            ensure!(scope.len() <= 8192, "granted scopes exceed limit");
            let scopes = scope
                .split_ascii_whitespace()
                .map(str::to_owned)
                .collect::<Vec<_>>();
            ensure!(scopes.len() <= 64, "too many granted scopes");
            scopes
        } else {
            previous_scopes
                .context("authorization response omitted granted scopes")?
                .to_vec()
        };
        let expires_at = if self.access_token.is_some() {
            let expires = self.expires_in.context("access token expiry missing")?;
            ensure!(
                (1..=86_400).contains(&expires),
                "invalid access token expiry"
            );
            Some(timestamp().saturating_add(expires))
        } else {
            None
        };
        ensure!(
            !plan_enabled(&scopes) || self.access_token.is_some(),
            "plan permission has no access token"
        );
        Ok(StoredCredentials {
            access_token: self.access_token,
            refresh_token: self.refresh_token,
            id_token,
            scopes,
            expires_at,
            earliest_refresh_at: self.earliest_refresh_at,
        })
    }
}

fn optional_secret<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Option<SecretString>, D::Error> {
    Option::<String>::deserialize(deserializer).map(|value| value.map(SecretString::new))
}

struct Callback {
    client_id: String,
    code: SecretString,
}

fn parse_callback(pending: &AttemptSecrets, callback: &str) -> Result<Callback> {
    ensure!(callback.len() <= 16 * 1024, "callback exceeds limit");
    let url =
        Url::parse(callback).map_err(|_| anyhow::anyhow!("invalid authorization callback"))?;
    ensure!(
        url.fragment().is_none() && url.username().is_empty() && url.password().is_none(),
        "unexpected callback parameters"
    );
    let mut base = url.clone();
    base.set_query(None);
    ensure!(
        base == pending.redirect_uri,
        "callback URI does not match authorization attempt"
    );
    let mut values = HashMap::new();
    for (key, value) in url.query_pairs() {
        ensure!(
            values.len() < 16
                && values
                    .insert(key.into_owned(), value.into_owned())
                    .is_none(),
            "duplicate or excessive callback parameters"
        );
    }
    let state = values.get("state").context("callback state missing")?;
    ensure!(
        equal(state, pending.state.expose_secret()),
        "callback state mismatch"
    );
    ensure!(
        !values.contains_key("error"),
        "authorization was declined or failed"
    );
    let client_id = if pending.new_registration {
        let issued = values
            .get("client_id")
            .context("registration callback omitted issued client ID")?;
        ensure!(
            valid_client(issued),
            "registration callback has invalid issued client ID"
        );
        issued.clone()
    } else {
        ensure!(
            values
                .get("client_id")
                .is_none_or(|issued| issued == &pending.client_id),
            "callback client ID differs from selected account"
        );
        pending.client_id.clone()
    };
    let code = values
        .remove("code")
        .context("authorization code missing")?;
    ensure!(
        !code.is_empty() && code.len() <= 4096,
        "invalid authorization code"
    );
    Ok(Callback {
        client_id,
        code: SecretString::new(code),
    })
}

fn callback_redirect(value: &str) -> Result<Url> {
    ensure!(value.len() <= 2048, "callback URI exceeds limit");
    let url = Url::parse(value).map_err(|_| anyhow::anyhow!("invalid callback URI"))?;
    ensure!(
        url.scheme() == "http"
            && url.host_str() == Some("127.0.0.1")
            && url.port().is_some_and(|port| port > 0)
            && url.path() == "/auth/callback"
            && url.query().is_none()
            && url.fragment().is_none()
            && url.username().is_empty()
            && url.password().is_none(),
        "callback must be http://127.0.0.1:<port>/auth/callback"
    );
    Ok(url)
}

fn valid_client(value: &str) -> bool {
    !value.is_empty()
        && value != DYNAMIC_CLIENT
        && value.len() <= 256
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

pub(crate) fn equal(left: &str, right: &str) -> bool {
    bool::from(left.as_bytes().ct_eq(right.as_bytes()))
}

pub(crate) fn timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
