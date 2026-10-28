//! Pending sign-in attempts: their one-use secrets, retry hints, and registry.

use std::sync::{Arc, Mutex};

use anyhow::{Result, ensure};
use oauth2::{CsrfToken, PkceCodeChallenge};
use reqwest::Url;
use tokio_util::sync::CancellationToken;

use super::{
    AuthEngine, DYNAMIC_CLIENT,
    accounts::{ensure_issued, registration_index},
    locked, timestamp,
};
use crate::{SecretString, StoredAuth};

const MAX_ATTEMPTS: usize = 8;
const ATTEMPT_SECONDS: u64 = 600;

pub(super) struct Attempt {
    pub(super) secrets: Mutex<Option<AttemptSecrets>>,
    pub(super) retry: Mutex<Option<RetryHint>>,
    pub(super) cancel: CancellationToken,
    pub(super) expires_at: u64,
}

impl Attempt {
    pub(super) fn is_live_at(&self, now: u64) -> bool {
        self.expires_at > now && !self.cancel.is_cancelled()
    }
}

pub(super) struct RetryHint {
    pub(super) client_id: String,
    pub(super) registration_id: Option<String>,
    pub(super) expected_subject: Option<String>,
    pub(super) expected_revision: Option<u64>,
}

pub(super) struct AttemptSecrets {
    pub(super) state: SecretString,
    pub(super) nonce: SecretString,
    pub(super) verifier: SecretString,
    pub(super) challenge: PkceCodeChallenge,
    pub(super) redirect_uri: Url,
    pub(super) client_id: String,
    pub(super) new_registration: bool,
    pub(super) registration_id: Option<String>,
    pub(super) expected_subject: Option<String>,
    pub(super) expected_revision: Option<u64>,
}

impl AttemptSecrets {
    /// Fresh PKCE, state, and nonce bound to the selected registration or retry hint.
    pub(super) fn new(
        stored: &StoredAuth,
        registration_id: Option<&str>,
        retry: Option<&RetryHint>,
        redirect_uri: Url,
    ) -> Result<Self> {
        let saved_id =
            registration_id.or_else(|| retry.and_then(|hint| hint.registration_id.as_deref()));
        let registration = saved_id
            .map(|id| registration_index(stored, id))
            .transpose()?
            .map(|index| &stored.registrations[index]);
        if let Some(registration) = registration {
            ensure_issued(registration)?;
        }
        let client_id = retry.map_or_else(
            || registration.map_or_else(|| DYNAMIC_CLIENT.into(), |entry| entry.client_id.clone()),
            |hint| hint.client_id.clone(),
        );
        if let (Some(hint), Some(registration)) = (retry, registration) {
            ensure!(
                hint.client_id == registration.client_id
                    && hint.expected_subject.as_deref() == Some(&registration.subject)
                    && hint.expected_revision == Some(registration.authorization_revision),
                "retry account changed during authorization"
            );
        }
        let (challenge, verifier) = PkceCodeChallenge::new_random_sha256_len(64);
        Ok(Self {
            state: SecretString::new(CsrfToken::new_random_len(32).into_secret()),
            nonce: SecretString::new(CsrfToken::new_random_len(32).into_secret()),
            verifier: SecretString::new(verifier.into_secret()),
            challenge,
            redirect_uri,
            client_id,
            new_registration: registration.is_none() && retry.is_none(),
            registration_id: saved_id.map(str::to_owned),
            expected_subject: registration.map(|entry| entry.subject.clone()),
            expected_revision: registration.map(|entry| entry.authorization_revision),
        })
    }
}

/// Removes a completing attempt unless it was rejected in a way that allows a retry.
pub(super) struct AttemptGuard {
    pub(super) engine: AuthEngine,
    pub(super) id: String,
}

impl Drop for AttemptGuard {
    fn drop(&mut self) {
        let mut attempts = locked(&self.engine.inner.attempts);
        let retrying = attempts.get(&self.id).is_some_and(|attempt| {
            !attempt.cancel.is_cancelled() && locked(&attempt.retry).is_some()
        });
        if !retrying && let Some(attempt) = attempts.remove(&self.id) {
            attempt.cancel.cancel();
        }
    }
}

impl AuthEngine {
    /// Track a new attempt, pruning dead ones first. Returns its ID and expiry.
    pub(super) fn register_attempt(
        &self,
        secrets: AttemptSecrets,
        cancel: &CancellationToken,
    ) -> Result<(String, u64)> {
        let now = timestamp();
        let expires_at = now.saturating_add(ATTEMPT_SECONDS);
        let id = uuid::Uuid::new_v4().to_string();
        let mut attempts = locked(&self.inner.attempts);
        attempts.retain(|_, attempt| {
            let live = attempt.is_live_at(now);
            if !live {
                attempt.cancel.cancel();
            }
            live
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
        Ok((id, expires_at))
    }

    /// Cancel a pending or exchanging sign-in attempt.
    #[must_use]
    pub fn cancel(&self, attempt_id: &str) -> bool {
        let Some(attempt) = locked(&self.inner.attempts).remove(attempt_id) else {
            return false;
        };
        attempt.cancel.cancel();
        true
    }
}
