use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    io::{self, Write},
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail, ensure};
use aws_lc_rs::{
    aead::{AES_256_GCM, Aad, Nonce, RandomizedNonceKey},
    digest::{SHA256, digest},
    rand::fill,
};
use base64::{
    Engine as _,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, OwnedMutexGuard};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

use super::{
    files::{PrivateDirectory, RawTransaction},
    wire::{self, WireAuth, WireSecret},
};
use crate::{SecretString, StoredAuth};

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

const MAGIC: &[u8; 8] = b"EJMVLT01";
const MAX_PLAIN: usize = 4 * 1024 * 1024;
const MAX_ENCRYPTED: usize = 8 * 1024 * 1024;
const MAX_SECRETS: usize = 128;
const MAX_SECRET: usize = 16 * 1024;
const LOCK_WAIT: Duration = Duration::from_secs(30);
const LOCK_RETRY: Duration = Duration::from_millis(25);
const KEY_SERVICE: &str = "com.ejmlabs.harness.vault.v1";
const BOOTSTRAP: &[u8; 8] = b"EJMBOOT1";

/// Exact product, purpose, and origin/account identity of a host credential.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecretId {
    product: String,
    purpose: String,
    identity: String,
}

impl SecretId {
    /// Construct a scoped identifier. Consumers include origin and account in identity.
    ///
    /// # Errors
    /// Rejects empty, oversized, or control-containing components.
    pub fn new(product: &str, purpose: &str, identity: &str) -> Result<Self> {
        let id = Self {
            product: product.into(),
            purpose: purpose.into(),
            identity: identity.into(),
        };
        id.validate()?;
        Ok(id)
    }
    fn validate(&self) -> Result<()> {
        for (value, max) in [
            (&self.product, 128),
            (&self.purpose, 128),
            (&self.identity, 1024),
        ] {
            ensure!(
                !value.is_empty() && value.len() <= max && !value.chars().any(char::is_control),
                "invalid credential namespace"
            );
        }
        Ok(())
    }
}

impl fmt::Debug for SecretId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretId([REDACTED])")
    }
}

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

struct NativeKeys;

fn native_entry(vault_id: &str) -> Result<keyring_core::Entry> {
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

#[derive(Clone, Default)]
struct VaultState {
    provider: Option<StoredAuth>,
    secrets: BTreeMap<SecretId, SecretString>,
    markers: BTreeSet<SecretId>,
    migration: Migration,
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Migration {
    #[default]
    None,
    Prepared,
    Complete,
}

/// Exclusive snapshot. Changes persist only after an explicit commit.
pub struct VaultTransaction {
    raw: Option<RawTransaction>,
    memory: Option<OwnedMutexGuard<VaultState>>,
    identity: String,
    key: Option<Zeroizing<Vec<u8>>>,
    state: VaultState,
}

impl fmt::Debug for SecretVault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretVault([REDACTED])")
    }
}
impl fmt::Debug for VaultTransaction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("VaultTransaction([REDACTED])")
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

enum Prepared {
    File(RawTransaction),
    Memory(OwnedMutexGuard<VaultState>),
}

fn check_lock_wait(cancel: &CancellationToken, timed_out: bool) -> Result<()> {
    ensure!(!cancel.is_cancelled(), "credential store lock cancelled");
    ensure!(!timed_out, "credential store lock timed out");
    Ok(())
}

fn decode_vault(
    secret: &SecretString,
    encoded: Option<Zeroizing<Vec<u8>>>,
    identity: &str,
) -> Result<(Zeroizing<Vec<u8>>, VaultState)> {
    ensure!(
        secret.expose_secret().len() <= 128,
        "invalid vault key record"
    );
    let key = Zeroizing::new(
        STANDARD
            .decode(secret.expose_secret())
            .map_err(|_| anyhow::anyhow!("invalid vault key record"))?,
    );
    ensure!(key.len() == 32, "invalid vault key record");
    let state = encoded.map_or_else(
        || Ok(VaultState::default()),
        |encoded| decrypt(&encoded, &key, identity),
    )?;
    Ok((key, state))
}

fn finish_lock(
    backend: Backend,
    prepared: Prepared,
    check_wait: impl Fn() -> Result<()>,
) -> Result<VaultTransaction> {
    check_wait()?;
    match (backend, prepared) {
        (Backend::Memory(_), Prepared::Memory(guard)) => Ok(VaultTransaction {
            state: guard.clone(),
            memory: Some(guard),
            raw: None,
            identity: String::new(),
            key: None,
        }),
        (Backend::File { keys, identity, .. }, Prepared::File(raw)) => {
            let encoded = raw.read("vault.bin", MAX_ENCRYPTED)?;
            let bootstrap = raw.read(".bootstrap", BOOTSTRAP.len())?;
            ensure!(
                bootstrap.as_deref().is_none_or(|bytes| bytes == BOOTSTRAP),
                "invalid credential bootstrap marker; preserve the vault and key"
            );
            check_wait()?;
            let existing = keys.get(&identity);
            check_wait()?;
            let existing = existing.map_err(|_| {
                anyhow::anyhow!(
                    "vault key service unavailable; unlock macOS Keychain or Windows Credential Manager, or start an unlocked Linux Secret Service in the user's D-Bus session"
                )
            })?;
            ensure!(
                encoded.is_none() || existing.is_some(),
                "existing vault key is missing; preserve the vault and recover the original OS key record"
            );
            ensure!(
                encoded.is_some() || existing.is_none() || bootstrap.is_some(),
                "existing credential vault data is missing; preserve the OS key and recover the original encrypted vault"
            );
            let new_vault = encoded.is_none();
            let legacy = raw.read("state.json", wire::MAX_STORE_BYTES)?;
            ensure!(
                !legacy
                    .as_deref()
                    .is_some_and(|bytes| wire::is_barrier(bytes))
                    || encoded.is_some(),
                "migrated credential vault is missing; preserve state and recover the original vault"
            );
            if new_vault && bootstrap.is_none() {
                // Persist before creating a key. No credential transaction is exposed
                // until this marker has been durably retired.
                check_wait()?;
                raw.write(".bootstrap", BOOTSTRAP, BOOTSTRAP.len())?;
            }
            let secret = if let Some(secret) = existing {
                secret
            } else {
                let mut bytes = Zeroizing::new([0u8; 32]);
                fill(bytes.as_mut()).map_err(|_| anyhow::anyhow!("cannot generate vault key"))?;
                let secret = SecretString::new(STANDARD.encode(bytes.as_ref()));
                check_wait()?;
                let saved = keys.set(&identity, &secret);
                check_wait()?;
                saved.map_err(|_| {
                    anyhow::anyhow!("cannot save vault key in OS credential service")
                })?;
                check_wait()?;
                let confirmed = keys.get(&identity);
                check_wait()?;
                let confirmed = confirmed
                    .map_err(|_| anyhow::anyhow!("cannot verify saved vault key"))?
                    .context("saved vault key is missing")?;
                ensure!(
                    bool::from(subtle::ConstantTimeEq::ct_eq(
                        secret.expose_secret().as_bytes(),
                        confirmed.expose_secret().as_bytes()
                    )),
                    "saved vault key could not be verified"
                );
                secret
            };
            let (key, state) = decode_vault(&secret, encoded, &identity)?;
            let mut transaction = VaultTransaction {
                raw: Some(raw),
                memory: None,
                identity,
                key: Some(key),
                state,
            };
            if new_vault {
                // Once a key exists, absence of ciphertext is never treated as
                // an empty store. Finish bootstrap before exposing a transaction.
                check_wait()?;
                transaction.commit()?;
                check_wait()?;
                transaction.verify_saved()?;
            }
            if new_vault || bootstrap.is_some() {
                check_wait()?;
                transaction
                    .raw
                    .as_ref()
                    .context("credential transaction has no storage")?
                    .remove(".bootstrap", BOOTSTRAP.len())?;
            }
            check_wait()?;
            Ok(transaction)
        }
        _ => bail!("invalid credential transaction backend"),
    }
}

impl VaultTransaction {
    /// Return an exact scoped match with redacted diagnostics.
    #[must_use]
    pub fn get(&self, id: &SecretId) -> Option<SecretString> {
        self.state.secrets.get(id).cloned()
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
        let encoded = encode(&self.state)?;
        if let Some(memory) = &mut self.memory {
            **memory = self.state.clone();
            return Ok(());
        }
        let raw = self
            .raw
            .as_ref()
            .context("credential transaction has no storage")?;
        let key = self
            .key
            .as_ref()
            .context("credential transaction has no key")?;
        let mut encrypted = encoded;
        let algorithm = RandomizedNonceKey::new(&AES_256_GCM, key.as_slice())
            .map_err(|_| anyhow::anyhow!("invalid vault key"))?;
        let aad = additional_data(&self.identity);
        let nonce = algorithm
            .seal_in_place_append_tag(Aad::from(&aad), &mut *encrypted)
            .map_err(|_| anyhow::anyhow!("cannot encrypt credential vault"))?;
        let mut file = Vec::with_capacity(MAGIC.len() + nonce.as_ref().len() + encrypted.len());
        file.extend_from_slice(MAGIC);
        file.extend_from_slice(nonce.as_ref());
        file.extend_from_slice(&encrypted);
        raw.write("vault.bin", &file, MAX_ENCRYPTED)
    }
    pub(super) fn provider(&self) -> StoredAuth {
        self.state.provider.clone().unwrap_or_default()
    }
    pub(super) fn save_provider(&mut self, state: &StoredAuth) -> Result<()> {
        let _ = wire::encode(state)?;
        self.state.provider = Some(state.clone());
        self.commit()
    }
    pub(super) fn migrate_provider(&mut self) -> Result<()> {
        let Some(raw) = &self.raw else {
            return Ok(());
        };
        let source = raw.read("state.json", wire::MAX_STORE_BYTES)?;
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
            self.state.provider = source
                .as_deref()
                .map(|bytes| wire::decode(bytes))
                .transpose()?;
            self.state.migration = Migration::Prepared;
            self.commit()?;
            self.verify_saved()?;
            let host_id = self
                .state
                .provider
                .as_ref()
                .map_or("encrypted-vault", |state| state.host_id.as_str());
            self.raw
                .as_ref()
                .context("credential transaction has no storage")?
                .write(
                    "state.json",
                    &wire::barrier(host_id)?,
                    wire::MAX_STORE_BYTES,
                )?;
        }
        self.state.migration = Migration::Complete;
        self.commit()
    }
    fn verify_saved(&self) -> Result<()> {
        let raw = self
            .raw
            .as_ref()
            .context("credential transaction has no storage")?;
        let bytes = raw
            .read("vault.bin", MAX_ENCRYPTED)?
            .context("saved vault is missing")?;
        let key = self
            .key
            .as_ref()
            .context("credential transaction has no key")?;
        let restored = decrypt(&bytes, key, &self.identity)?;
        ensure!(
            *encode(&restored)? == *encode(&self.state)?,
            "saved credential vault could not be verified"
        );
        Ok(())
    }
}

fn additional_data(identity: &str) -> Vec<u8> {
    let mut aad = MAGIC.to_vec();
    aad.extend_from_slice(identity.as_bytes());
    aad
}

fn decrypt(encoded: &[u8], key: &[u8], identity: &str) -> Result<VaultState> {
    ensure!(
        encoded.len() >= 36 && encoded.len() <= MAX_ENCRYPTED,
        "invalid credential vault size"
    );
    ensure!(
        &encoded[..8] == MAGIC,
        "unsupported credential vault format"
    );
    let nonce = Nonce::try_assume_unique_for_key(&encoded[8..20])
        .map_err(|_| anyhow::anyhow!("invalid credential vault nonce"))?;
    let algorithm = RandomizedNonceKey::new(&AES_256_GCM, key)
        .map_err(|_| anyhow::anyhow!("invalid vault key"))?;
    let mut plaintext = Zeroizing::new(encoded[20..].to_vec());
    let length = algorithm
        .open_in_place(nonce, Aad::from(additional_data(identity)), &mut plaintext)
        .map_err(|_| {
            anyhow::anyhow!(
                "credential vault authentication failed; preserve the vault and original OS key"
            )
        })?
        .len();
    ensure!(length <= MAX_PLAIN, "credential vault exceeds 4 MiB");
    plaintext.truncate(length);
    decode(&plaintext)
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireVault {
    version: u8,
    provider: Option<WireAuth>,
    secrets: Vec<WireEntry>,
    markers: Vec<SecretId>,
    migration: Migration,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireEntry {
    id: SecretId,
    value: WireSecret,
}
struct BoundedJson(Zeroizing<Vec<u8>>);
impl Write for BoundedJson {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > MAX_PLAIN.saturating_sub(self.0.len()) {
            return Err(io::Error::new(
                io::ErrorKind::FileTooLarge,
                "credential vault exceeds 4 MiB",
            ));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn encode(state: &VaultState) -> Result<Zeroizing<Vec<u8>>> {
    if let Some(provider) = &state.provider {
        let _ = wire::encode(provider)?;
    }
    let wire = WireVault {
        version: 1,
        provider: state.provider.as_ref().map(WireAuth::from_state),
        secrets: state
            .secrets
            .iter()
            .map(|(id, value)| WireEntry {
                id: id.clone(),
                value: WireSecret(value.clone()),
            })
            .collect(),
        markers: state.markers.iter().cloned().collect(),
        migration: state.migration,
    };
    let mut writer = BoundedJson(Zeroizing::new(Vec::new()));
    serde_json::to_writer(&mut writer, &wire)
        .map_err(|_| anyhow::anyhow!("credential vault exceeds supported JSON bounds"))?;
    Ok(writer.0)
}
fn decode(bytes: &[u8]) -> Result<VaultState> {
    let wire: WireVault = serde_json::from_slice(bytes)
        .map_err(|_| anyhow::anyhow!("credential vault contains invalid state"))?;
    ensure!(
        wire.version == 1,
        "unsupported credential vault state version"
    );
    ensure!(
        wire.secrets.len() <= MAX_SECRETS && wire.markers.len() <= MAX_SECRETS,
        "credential vault exceeds namespace limits"
    );
    let mut state = VaultState {
        provider: wire.provider.map(WireAuth::into_state).transpose()?,
        migration: wire.migration,
        ..VaultState::default()
    };
    if let Some(provider) = &state.provider {
        let _ = wire::encode(provider)?;
    }
    for entry in wire.secrets {
        entry.id.validate()?;
        ensure!(
            !entry.value.0.expose_secret().is_empty()
                && entry.value.0.expose_secret().len() <= MAX_SECRET,
            "invalid credential secret size"
        );
        ensure!(
            state.secrets.insert(entry.id, entry.value.0).is_none(),
            "duplicate credential namespace"
        );
    }
    for marker in wire.markers {
        marker.validate()?;
        ensure!(
            state.markers.insert(marker),
            "duplicate credential migration marker"
        );
    }
    Ok(state)
}
