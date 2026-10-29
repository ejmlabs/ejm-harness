use std::fmt;

use anyhow::{Context, Result, ensure};
use tokio::sync::OwnedMutexGuard;
use zeroize::Zeroizing;

use super::{
    LEGACY_STATE_FILE, MAX_ENCRYPTED, MAX_SECRET, MAX_SECRETS, VAULT_FILE,
    format::{decrypt, encode, seal},
    state::{Migration, SecretId, VaultState},
};
use crate::{
    SecretString, StoredAuth,
    store::{files::RawTransaction, wire},
};

/// Exclusive snapshot. Changes persist only after an explicit commit.
pub struct VaultTransaction {
    raw: Option<RawTransaction>,
    memory: Option<OwnedMutexGuard<VaultState>>,
    identity: String,
    key: Option<Zeroizing<Vec<u8>>>,
    pub(super) state: VaultState,
}

impl fmt::Debug for VaultTransaction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("VaultTransaction([REDACTED])")
    }
}

impl VaultTransaction {
    pub(super) fn memory(guard: OwnedMutexGuard<VaultState>) -> Self {
        Self {
            state: guard.clone(),
            memory: Some(guard),
            raw: None,
            identity: String::new(),
            key: None,
        }
    }

    pub(super) fn file(
        raw: RawTransaction,
        identity: String,
        key: Zeroizing<Vec<u8>>,
        state: VaultState,
    ) -> Self {
        Self {
            raw: Some(raw),
            memory: None,
            identity,
            key: Some(key),
            state,
        }
    }

    /// Return an exact scoped match with redacted diagnostics.
    #[must_use]
    pub fn get(&self, id: &SecretId) -> Option<SecretString> {
        self.state.secrets.get(id).cloned()
    }

    /// Identity strings for one product and purpose. Secret values are not returned.
    #[must_use]
    pub fn identities(&self, product: &str, purpose: &str) -> Vec<String> {
        self.state
            .secrets
            .keys()
            .filter(|id| id.product == product && id.purpose == purpose)
            .map(|id| id.identity.clone())
            .collect()
    }
    /// Stage a bounded scoped secret.
    ///
    /// # Errors
    /// Rejects invalid scope, empty/oversized values, or the 128-record limit.
    pub fn set(&mut self, id: SecretId, value: SecretString) -> Result<()> {
        id.validate()?;
        ensure!(
            !value.expose_secret().is_empty() && value.expose_secret().len() <= MAX_SECRET,
            "credential secret must contain 1 to 16384 bytes"
        );
        ensure!(
            self.state.secrets.contains_key(&id) || self.state.secrets.len() < MAX_SECRETS,
            "credential vault exceeds 128 host secrets"
        );
        self.state.secrets.insert(id, value);
        Ok(())
    }
    /// Stage deletion while retaining every migration marker.
    pub fn delete(&mut self, id: &SecretId) {
        self.state.secrets.remove(id);
    }
    /// Whether this identity completed controlled legacy migration.
    #[must_use]
    pub fn migrated(&self, id: &SecretId) -> bool {
        self.state.markers.contains(id)
    }
    /// Stage a permanent marker, committed atomically with the imported secret.
    ///
    /// # Errors
    /// Rejects invalid identifiers or more than 128 markers.
    pub fn mark_migrated(&mut self, id: SecretId) -> Result<()> {
        id.validate()?;
        ensure!(
            self.state.markers.contains(&id) || self.state.markers.len() < MAX_SECRETS,
            "credential vault exceeds 128 migration markers"
        );
        self.state.markers.insert(id);
        Ok(())
    }
    /// Atomically save the complete snapshot, retaining lock ownership.
    ///
    /// # Errors
    /// Reports bounds, encryption or persistence errors. Failure after replacement
    /// leaves persistence uncertain; never replay a token exchange.
    pub fn commit(&mut self) -> Result<()> {
        let plaintext = encode(&self.state)?;
        if let Some(memory) = &mut self.memory {
            **memory = self.state.clone();
            return Ok(());
        }
        let raw = self.storage()?;
        let file = seal(plaintext, self.key()?, &self.identity)?;
        raw.write(VAULT_FILE, &file, MAX_ENCRYPTED)
    }

    pub(crate) fn provider(&self) -> StoredAuth {
        self.state.provider.clone().unwrap_or_default()
    }
    pub(crate) fn save_provider(&mut self, state: &StoredAuth) -> Result<()> {
        let _ = wire::encode(state)?;
        self.state.provider = Some(state.clone());
        self.commit()
    }
    pub(crate) fn migrate_provider(&mut self) -> Result<()> {
        let Some(raw) = &self.raw else {
            return Ok(());
        };
        let source = raw.read(LEGACY_STATE_FILE, wire::MAX_STORE_BYTES)?;
        let barrier = source
            .as_deref()
            .is_some_and(|bytes| wire::is_barrier(bytes));
        if self.state.migration == Migration::Complete {
            ensure!(
                barrier,
                "legacy credential state reappeared after migration; stop older clients and preserve the vault"
            );
            return Ok(());
        }
        if barrier {
            ensure!(
                self.state.migration == Migration::Prepared,
                "credential migration barrier has no prepared vault"
            );
        } else {
            self.import_legacy_provider(source.as_ref())?;
        }
        self.state.migration = Migration::Complete;
        self.commit()
    }

    /// Persist the legacy file's state encrypted, then publish the version barrier.
    /// A crash before the barrier leaves the legacy file authoritative.
    fn import_legacy_provider(&mut self, source: Option<&Zeroizing<Vec<u8>>>) -> Result<()> {
        self.state.provider = source.map(|bytes| wire::decode(bytes)).transpose()?;
        self.state.migration = Migration::Prepared;
        self.commit()?;
        self.verify_saved()?;
        let host_id = self
            .state
            .provider
            .as_ref()
            .map_or("encrypted-vault", |state| state.host_id.as_str());
        self.storage()?.write(
            LEGACY_STATE_FILE,
            &wire::barrier(host_id)?,
            wire::MAX_STORE_BYTES,
        )
    }

    pub(super) fn verify_saved(&self) -> Result<()> {
        let bytes = self
            .storage()?
            .read(VAULT_FILE, MAX_ENCRYPTED)?
            .context("saved vault is missing")?;
        let restored = decrypt(&bytes, self.key()?, &self.identity)?;
        ensure!(
            *encode(&restored)? == *encode(&self.state)?,
            "saved credential vault could not be verified"
        );
        Ok(())
    }

    pub(super) fn storage(&self) -> Result<&RawTransaction> {
        self.raw
            .as_ref()
            .context("credential transaction has no storage")
    }

    fn key(&self) -> Result<&[u8]> {
        self.key
            .as_ref()
            .map(|key| key.as_slice())
            .context("credential transaction has no key")
    }
}
