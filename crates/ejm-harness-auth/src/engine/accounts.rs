//! Saved account registrations: lookup, validity, and public status.

use anyhow::{Context, Result, ensure};
use tokio_util::sync::CancellationToken;

use super::{AuthEngine, DYNAMIC_CLIENT, ISSUER};
use crate::{RegistrationStatus, StoredAuth, StoredRegistration};

impl AuthEngine {
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
        let index = registration_index(&stored, registration_id)?;
        Ok(stored.registrations[index].status())
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
}

pub(super) fn registration_index(stored: &StoredAuth, id: &str) -> Result<usize> {
    stored
        .registrations
        .iter()
        .position(|entry| entry.id == id)
        .context("unknown account registration")
}

/// Reject a saved registration that was not issued to this provider with a real client ID.
pub(super) fn ensure_issued(registration: &StoredRegistration) -> Result<()> {
    ensure!(
        registration.issuer == ISSUER && valid_client(&registration.client_id),
        "invalid saved account registration"
    );
    Ok(())
}

pub(super) fn valid_client(value: &str) -> bool {
    !value.is_empty()
        && value != DYNAMIC_CLIENT
        && value.len() <= 256
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}
