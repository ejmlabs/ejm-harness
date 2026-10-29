//! Finish a sign-in: exchange the callback code, verify identity, and save credentials.

use std::sync::Arc;

use anyhow::{Context, Result, bail, ensure};
use reqwest::StatusCode;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use super::{
    AuthEngine, RESOURCE,
    attempt::{Attempt, AttemptGuard, AttemptSecrets, RetryHint},
    callback::{Callback, parse_callback},
    discovery::Discovery,
    http::HttpResponse,
    lock, timestamp,
    token::TokenResponse,
};
use crate::{
    CredentialTransaction, RegistrationStatus, RetryRequired, StoredAuth, StoredRegistration,
    identity::Identity,
};

impl AuthEngine {
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
        let attempt = lock(&self.inner.attempts)
            .get(attempt_id)
            .cloned()
            .context("unknown or completed sign-in attempt")?;
        ensure!(
            attempt.is_live_at(timestamp()),
            "sign-in attempt expired or cancelled"
        );
        let pending = lock(&attempt.secrets)
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
            .exchange_code(&pending, &callback, &discovery, cancel)
            .await?;
        if response.status != StatusCode::OK {
            return Err(rejected_exchange(
                attempt_id,
                attempt,
                pending,
                callback.client_id,
                &response.body,
                cancel,
            ));
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
        let index = registration_slot(&mut stored, &pending, &identity, &callback.client_id)?;
        let registration = &mut stored.registrations[index];
        registration.email = identity.email;
        registration.credentials = Some(credentials);
        registration.refresh_pending = false;
        self.commit_attempt(attempt_id, attempt, cancel, transaction.as_mut(), &stored)?;
        Ok(stored.registrations[index].status())
    }

    async fn exchange_code(
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
        let attempts = lock(&self.inner.attempts);
        ensure!(
            attempts
                .get(id)
                .is_some_and(|current| Arc::ptr_eq(current, attempt))
                && !cancel.is_cancelled()
                && attempt.is_live_at(timestamp()),
            "sign-in expired or cancelled before save"
        );
        transaction.save(stored)
    }
}

/// The error for a non-OK code exchange. An `invalid_grant` rejection records a retry hint
/// so the host can retry with the issued client ID instead of starting over.
fn rejected_exchange(
    attempt_id: &str,
    attempt: &Attempt,
    pending: AttemptSecrets,
    client_id: String,
    body: &[u8],
    cancel: &CancellationToken,
) -> anyhow::Error {
    #[derive(Deserialize)]
    struct Rejection {
        error: String,
    }
    if !serde_json::from_slice::<Rejection>(body)
        .is_ok_and(|rejection| rejection.error == "invalid_grant")
    {
        return anyhow::anyhow!("authorization code exchange rejected; start a fresh sign-in");
    }
    if cancel.is_cancelled() || attempt.cancel.is_cancelled() {
        return anyhow::anyhow!("sign-in cancelled");
    }
    *lock(&attempt.retry) = Some(RetryHint {
        client_id,
        registration_id: pending.registration_id,
        expected_subject: pending.expected_subject,
        expected_revision: pending.expected_revision,
    });
    RetryRequired {
        attempt_id: attempt_id.into(),
    }
    .into()
}

/// The registration this sign-in updates: the selected one, if it is unchanged, or a new blank one.
fn registration_slot(
    stored: &mut StoredAuth,
    pending: &AttemptSecrets,
    identity: &Identity,
    client_id: &str,
) -> Result<usize> {
    if let Some(id) = pending.registration_id.as_deref() {
        let index = stored
            .registrations
            .iter()
            .position(|entry| entry.id == id)
            .context("account registration was removed")?;
        let selected = &stored.registrations[index];
        ensure!(
            selected.issuer == identity.issuer
                && selected.subject == identity.subject
                && selected.client_id == client_id
                && Some(selected.authorization_revision) == pending.expected_revision,
            "sign-in account changed during authorization"
        );
        return Ok(index);
    }
    ensure!(
        stored.registrations.len() < 64,
        "at most 64 account registrations may be saved"
    );
    stored.registrations.push(StoredRegistration {
        id: uuid::Uuid::new_v4().to_string(),
        issuer: identity.issuer.clone(),
        subject: identity.subject.clone(),
        client_id: client_id.into(),
        email: None,
        credentials: None,
        authorization_revision: 0,
        refresh_pending: false,
    });
    Ok(stored.registrations.len() - 1)
}
