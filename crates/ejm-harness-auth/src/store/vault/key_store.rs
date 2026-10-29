use anyhow::{Result, bail, ensure};
use zeroize::Zeroizing;

use crate::SecretString;

const KEY_SERVICE: &str = "com.ejmlabs.harness.vault.v1";

/// Injected synchronous OS key boundary, called while the stable vault lock is held.
/// An in-flight method cannot be interrupted; asynchronous callers can stop waiting.
pub trait VaultKeyStore: Send + Sync {
    /// Read an existing base64-encoded 32-byte key; only absence returns `None`.
    ///
    /// # Errors
    /// Reports locked, unavailable, ambiguous, or invalid native records.
    fn get(&self, vault_id: &str) -> Result<Option<SecretString>>;
    /// Persist the key for a new vault and make it available to a subsequent read.
    ///
    /// # Errors
    /// Reports native storage failures. Callers never replace an existing vault key.
    fn set(&self, vault_id: &str, key: &SecretString) -> Result<()>;
}

pub(super) struct NativeKeys;

pub(super) fn native_entry(vault_id: &str) -> Result<keyring_core::Entry> {
    use keyring_core::api::CredentialStoreApi as _;
    #[cfg(target_os = "macos")]
    let store = apple_native_keyring_store::keychain::Store::new();
    #[cfg(target_os = "windows")]
    let store = windows_native_keyring_store::Store::new();
    #[cfg(target_os = "linux")]
    let store = dbus_secret_service_keyring_store::Store::new();
    #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
    return store.and_then(|store| store.build(KEY_SERVICE, vault_id, None))
        .map_err(|_| anyhow::anyhow!("OS credential service unavailable; unlock Keychain or Credential Manager, or start an unlocked Linux Secret Service"));
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    bail!("no native credential key adapter for this platform")
}

impl VaultKeyStore for NativeKeys {
    fn get(&self, vault_id: &str) -> Result<Option<SecretString>> {
        match native_entry(vault_id)?.get_secret() {
            Ok(bytes) => {
                let bytes = Zeroizing::new(bytes);
                let value = std::str::from_utf8(&bytes)
                    .map_err(|_| anyhow::anyhow!("invalid vault key record"))?;
                ensure!(value.len() <= 128, "invalid vault key record");
                Ok(Some(SecretString::new(value)))
            }
            Err(keyring_core::Error::NoEntry) => Ok(None),
            Err(_) => bail!("vault key is unavailable; unlock the OS credential service and retry"),
        }
    }
    fn set(&self, vault_id: &str, key: &SecretString) -> Result<()> {
        native_entry(vault_id)?
            .set_secret(key.expose_secret().as_bytes())
            .map_err(|_| anyhow::anyhow!("cannot save vault key in OS credential service"))
    }
}
