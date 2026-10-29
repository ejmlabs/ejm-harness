mod format;
mod key_store;
mod lock;
mod state;
mod transaction;

use std::{
    fmt,
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Result, bail};
use aws_lc_rs::digest::{SHA256, digest};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use self::{
    key_store::NativeKeys,
    lock::{Prepared, check_lock_wait, finish_lock},
    state::VaultState,
};
use super::files::PrivateDirectory;

pub use self::{key_store::VaultKeyStore, state::SecretId, transaction::VaultTransaction};

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

const MAX_ENCRYPTED: usize = 8 * 1024 * 1024;
const MAX_SECRETS: usize = 128;
const MAX_SECRET: usize = 16 * 1024;
const LOCK_WAIT: Duration = Duration::from_secs(30);
const LOCK_RETRY: Duration = Duration::from_millis(25);
const BOOTSTRAP: &[u8; 8] = b"EJMBOOT1";
const VAULT_FILE: &str = "vault.bin";
const BOOTSTRAP_FILE: &str = ".bootstrap";
const LEGACY_STATE_FILE: &str = "state.json";

/// The common encrypted credential store for native hosts.
#[derive(Clone)]
pub struct SecretVault {
    backend: Backend,
}

#[derive(Clone)]
enum Backend {
    File {
        directory: PrivateDirectory,
        keys: Arc<dyn VaultKeyStore>,
        identity: String,
    },
    Memory(Arc<Mutex<VaultState>>),
}

impl fmt::Debug for SecretVault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretVault([REDACTED])")
    }
}

impl SecretVault {
    /// Open protected persistent storage. Key access is deferred until locking.
    ///
    /// # Errors
    /// Rejects unsafe or unavailable native paths.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_with_key_store(path, Arc::new(NativeKeys))
    }

    /// Open storage with an explicit key boundary for deterministic host fixtures.
    ///
    /// # Errors
    /// Rejects unsafe or unavailable native paths.
    pub fn open_with_key_store(
        path: impl AsRef<Path>,
        keys: Arc<dyn VaultKeyStore>,
    ) -> Result<Self> {
        let directory = PrivateDirectory::open(path.as_ref())?;
        let identity = URL_SAFE_NO_PAD
            .encode(digest(&SHA256, directory.path().as_os_str().as_encoded_bytes()).as_ref());
        Ok(Self {
            backend: Backend::File {
                directory,
                keys,
                identity,
            },
        })
    }

    /// Explicit memory-only storage. Nothing is persisted or imported.
    #[must_use]
    pub fn memory() -> Self {
        Self {
            backend: Backend::Memory(Arc::new(Mutex::new(VaultState::default()))),
        }
    }

    /// Acquire ownership, including native key setup, with bounded cancellable waiting.
    /// An in-flight native call retains the lock until it returns, even if this wait ends.
    ///
    /// # Errors
    /// Reports cancellation, timeout, key-service failure, or corrupt/unsafe state.
    pub async fn lock(&self, cancel: CancellationToken) -> Result<VaultTransaction> {
        let deadline = tokio::time::Instant::now() + LOCK_WAIT;
        let worker_cancel = cancel.child_token();
        let _cancel_on_drop = worker_cancel.clone().drop_guard();
        loop {
            check_lock_wait(&cancel, tokio::time::Instant::now() >= deadline)?;
            if let Some(prepared) = self.try_lock()? {
                let backend = self.backend.clone();
                let mut operation = tokio::task::spawn_blocking(move || {
                    finish_lock(backend, prepared, || {
                        check_lock_wait(&worker_cancel, tokio::time::Instant::now() >= deadline)
                    })
                });
                let result = tokio::select! {
                    biased;
                    () = cancel.cancelled() => bail!("credential store lock cancelled"),
                    () = tokio::time::sleep_until(deadline) => bail!("credential store lock timed out"),
                    result = &mut operation => result,
                };
                check_lock_wait(&cancel, tokio::time::Instant::now() >= deadline)?;
                return result
                    .map_err(|_| anyhow::anyhow!("credential store operation interrupted"))?;
            }
            tokio::select! {
                biased;
                () = cancel.cancelled() => bail!("credential store lock cancelled"),
                () = tokio::time::sleep_until(deadline.min(tokio::time::Instant::now() + LOCK_RETRY)) => {},
            }
        }
    }

    /// Acquire at a synchronous host boundary. Do not call on a UI event thread.
    /// Native key calls cannot be interrupted; cancellation and the deadline are
    /// checked before and after them, while the calling thread retains ownership.
    ///
    /// # Errors
    /// Reports the same failures as [`Self::lock`].
    pub fn lock_blocking(&self, cancel: &CancellationToken) -> Result<VaultTransaction> {
        let deadline = Instant::now() + LOCK_WAIT;
        loop {
            check_lock_wait(cancel, Instant::now() >= deadline)?;
            if let Some(prepared) = self.try_lock()? {
                let transaction = finish_lock(self.backend.clone(), prepared, || {
                    check_lock_wait(cancel, Instant::now() >= deadline)
                })?;
                check_lock_wait(cancel, Instant::now() >= deadline)?;
                return Ok(transaction);
            }
            std::thread::sleep(LOCK_RETRY.min(deadline.saturating_duration_since(Instant::now())));
        }
    }

    fn try_lock(&self) -> Result<Option<Prepared>> {
        match &self.backend {
            Backend::File { directory, .. } => Ok(directory.try_lock()?.map(Prepared::File)),
            Backend::Memory(state) => Ok(Arc::clone(state)
                .try_lock_owned()
                .ok()
                .map(Prepared::Memory)),
        }
    }
}
