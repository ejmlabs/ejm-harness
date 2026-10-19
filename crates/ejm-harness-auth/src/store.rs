use crate::StoredAuth;
use anyhow::Result;
use async_trait::async_trait;
use std::{path::Path, sync::Arc};
use tokio_util::sync::CancellationToken;
mod files;
mod vault;
mod wire;
pub use vault::{SecretId, SecretVault, VaultKeyStore, VaultTransaction};
/// Serializes credential reads and updates across host processes.
#[async_trait]
pub trait CredentialStore: Send + Sync {
    /// Retain exclusive ownership until the transaction is dropped.
    ///
    /// # Errors
    /// Reports cancellation, timeout, unsafe paths, or unavailable/corrupt storage.
    async fn lock(&self, cancel: CancellationToken) -> Result<Box<dyn CredentialTransaction>>;
}
/// A credential transaction retains its lock through network exchanges.
pub trait CredentialTransaction: Send {
    /// Read provider state, or empty state before first use.
    ///
    /// # Errors
    /// Reports unsafe, corrupt, oversized, or unsupported state.
    fn load(&mut self) -> Result<StoredAuth>;
    /// Atomically replace and synchronize provider state.
    ///
    /// # Errors
    /// Reports invalid state or failed durable persistence. Failure after replacement
    /// leaves the save outcome uncertain.
    fn save(&mut self, state: &StoredAuth) -> Result<()>;
}
/// Compatibility entry point for provider state in the shared encrypted vault.
#[derive(Clone, Debug)]
pub struct FileStore(SecretVault);
impl FileStore {
    /// Explicit provider storage that lasts only for this process.
    #[must_use]
    pub fn memory() -> Self {
        Self(SecretVault::memory())
    }

    /// Open a protected vault, reusing the provider's stable legacy lock.
    ///
    /// # Errors
    /// Reports unsafe native paths. Key access and migration happen under the lock.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Ok(Self(SecretVault::open(path)?))
    }
    /// Open provider storage with an injected protected key boundary.
    ///
    /// # Errors
    /// Reports unsafe or unavailable native paths.
    pub fn open_with_key_store(
        path: impl AsRef<Path>,
        keys: Arc<dyn VaultKeyStore>,
    ) -> Result<Self> {
        Ok(Self(SecretVault::open_with_key_store(path, keys)?))
    }
}
#[async_trait]
impl CredentialStore for FileStore {
    async fn lock(&self, cancel: CancellationToken) -> Result<Box<dyn CredentialTransaction>> {
        let mut transaction = self.0.lock(cancel).await?;
        transaction.migrate_provider()?;
        Ok(Box::new(transaction))
    }
}
impl CredentialTransaction for VaultTransaction {
    fn load(&mut self) -> Result<StoredAuth> {
        Ok(self.provider())
    }
    fn save(&mut self, state: &StoredAuth) -> Result<()> {
        self.save_provider(state)
    }
}
