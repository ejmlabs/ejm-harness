//! Sign-out: clear local credentials first, then try to revoke the refresh token remotely.

use std::time::Duration;

use anyhow::{Context, Result};
use reqwest::StatusCode;
use tokio_util::sync::CancellationToken;

use super::{AuthEngine, accounts::registration_index};
use crate::{LogoutResult, SecretString};

impl AuthEngine {
    /// Clear local tokens and attempt idempotent remote session revocation.
    ///
    /// # Errors
    /// Returns an error if local credential storage cannot be updated.
    pub async fn logout(
        &self,
        registration_id: &str,
        cancel: CancellationToken,
    ) -> Result<LogoutResult> {
        let (client_id, refresh) = self.clear_credentials(registration_id, &cancel).await?;
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

    /// Clearing first makes a crash or cancelled revocation fail closed across processes.
    /// Returns the client ID and refresh token to revoke, after releasing the credential lock.
    async fn clear_credentials(
        &self,
        registration_id: &str,
        cancel: &CancellationToken,
    ) -> Result<(String, Option<SecretString>)> {
        let mut transaction = self.inner.store.lock(cancel.clone()).await?;
        let mut stored = transaction.load()?;
        let index = registration_index(&stored, registration_id)?;
        let registration = &stored.registrations[index];
        let refresh = registration
            .credentials
            .as_ref()
            .and_then(|credentials| credentials.refresh_token.clone());
        let client_id = registration.client_id.clone();
        stored.registrations[index].authorization_revision = registration
            .authorization_revision
            .checked_add(1)
            .context("account authorization revision exhausted")?;
        stored.registrations[index].credentials = None;
        stored.registrations[index].refresh_pending = false;
        transaction.save(&stored)?;
        Ok((client_id, refresh))
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
}
