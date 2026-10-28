//! Begin a sign-in: validate the callback, bind fresh secrets, and build the authorization URL.

use anyhow::{Context, Result, ensure};
use tokio_util::sync::CancellationToken;

use super::{
    AuthEngine, RESOURCE, SCOPES,
    attempt::{AttemptSecrets, RetryHint},
    callback::callback_redirect,
    discovery::Discovery,
    locked, timestamp,
};
use crate::{LoginAttempt, SecretString};

impl AuthEngine {
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
        let previous = locked(&self.inner.attempts)
            .remove(attempt_id)
            .context("unknown or consumed sign-in retry")?;
        ensure!(
            previous.is_live_at(timestamp()),
            "sign-in retry expired or cancelled"
        );
        let hint = locked(&previous.retry)
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
        let secrets = AttemptSecrets::new(&stored, registration_id, retry.as_ref(), redirect)?;
        let authorization_url = self.authorization_url(&discovery, &secrets, &stored.host_id)?;
        drop(transaction);
        let (id, expires_at) = self.register_attempt(secrets, &cancel)?;
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
}
