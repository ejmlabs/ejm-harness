use std::{
    collections::HashMap,
    fs,
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::Duration,
};

use anyhow::{Context, Result, ensure};
use base64::engine::general_purpose::STANDARD;
use tokio_util::sync::CancellationToken;

use super::*;
use crate::{CredentialStore, FileStore, SecretString, StoredCredentials, StoredRegistration};

#[derive(Default)]
struct Keys {
    values: Mutex<HashMap<String, SecretString>>,
    writes: AtomicUsize,
}

impl VaultKeyStore for Keys {
    fn get(&self, id: &str) -> Result<Option<SecretString>> {
        Ok(self
            .values
            .lock()
            .map_err(|_| anyhow::anyhow!("test key lock poisoned"))?
            .get(id)
            .cloned())
    }
    fn set(&self, id: &str, value: &SecretString) -> Result<()> {
        self.writes.fetch_add(1, Ordering::SeqCst);
        self.values
            .lock()
            .map_err(|_| anyhow::anyhow!("test key lock poisoned"))?
            .insert(id.into(), value.clone());
        Ok(())
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum KeyCall {
    Read,
    Save,
}

struct KeyGate {
    entered: tokio::sync::oneshot::Sender<()>,
    release: mpsc::Receiver<()>,
}

struct BlockedKeys {
    inner: Arc<Keys>,
    call: KeyCall,
    gate: Mutex<Option<KeyGate>>,
}

struct ReleaseKeyCall(mpsc::Sender<()>);

impl Drop for ReleaseKeyCall {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}

impl BlockedKeys {
    fn new(
        inner: Arc<Keys>,
        call: KeyCall,
    ) -> (
        Arc<Self>,
        tokio::sync::oneshot::Receiver<()>,
        ReleaseKeyCall,
    ) {
        let (entered, reached) = tokio::sync::oneshot::channel();
        let (release, wait) = mpsc::channel();
        (
            Arc::new(Self {
                inner,
                call,
                gate: Mutex::new(Some(KeyGate {
                    entered,
                    release: wait,
                })),
            }),
            reached,
            ReleaseKeyCall(release),
        )
    }

    fn block(&self, call: KeyCall) -> Result<()> {
        if self.call == call {
            let gate = self
                .gate
                .lock()
                .map_err(|_| anyhow::anyhow!("test key gate poisoned"))?
                .take();
            if let Some(gate) = gate {
                let _ = gate.entered.send(());
                gate.release
                    .recv_timeout(Duration::from_secs(5))
                    .context("blocked test key call was not released")?;
            }
        }
        Ok(())
    }
}

impl VaultKeyStore for BlockedKeys {
    fn get(&self, id: &str) -> Result<Option<SecretString>> {
        self.block(KeyCall::Read)?;
        self.inner.get(id)
    }

    fn set(&self, id: &str, value: &SecretString) -> Result<()> {
        self.inner.set(id, value)?;
        self.block(KeyCall::Save)
    }
}

async fn released_lock(vault: &SecretVault) -> Result<Prepared> {
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let Some(prepared) = vault.try_lock()? {
                return Ok(prepared);
            }
            tokio::task::yield_now().await;
        }
    })
    .await?
}

fn sample() -> StoredAuth {
    StoredAuth {
        host_id: "test-host".into(),
        registrations: vec![StoredRegistration {
            id: "registration".into(),
            issuer: "https://issuer.example".into(),
            subject: "subject".into(),
            client_id: "client".into(),
            email: Some("test@example.com".into()),
            credentials: Some(StoredCredentials {
                access_token: Some(SecretString::new("fixture-access")),
                refresh_token: Some(SecretString::new("fixture-refresh")),
                id_token: SecretString::new("fixture-identity"),
                scopes: vec!["openid".into()],
                expires_at: Some(100),
                earliest_refresh_at: Some(90),
            }),
            refresh_pending: true,
            authorization_revision: 7,
        }],
    }
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    #[cfg(unix)]
    {
        use std::{fs::OpenOptions, io::Write, os::unix::fs::OpenOptionsExt};
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    #[cfg(windows)]
    rightkit_fs::private::write_private_file(path, bytes)?;
    Ok(())
}

fn id() -> Result<SecretId> {
    SecretId::new(
        "fixture-product",
        "api-key",
        "https://one.example/account-a",
    )
}

#[tokio::test]
async fn shared_vault_scopes_atomic_commit_and_permanent_markers() -> Result<()> {
    let root = tempfile::tempdir()?;
    let path = root.path().join("vault");
    let keys = Arc::new(Keys::default());
    let vault = SecretVault::open_with_key_store(&path, keys.clone())?;
    let mut transaction = vault.lock(CancellationToken::new()).await?;
    let secret_id = id()?;
    transaction.set(
        secret_id.clone(),
        SecretString::new("fixture-bearer-keep-private"),
    )?;
    transaction.mark_migrated(secret_id.clone())?;
    transaction.commit()?;
    transaction.set(
        SecretId::new("fixture-product", "uncommitted", "one")?,
        SecretString::new("must-not-commit"),
    )?;
    assert!(!format!("{vault:?} {transaction:?}").contains("fixture-bearer"));
    drop(transaction);
    let ciphertext = fs::read(path.join("vault.bin"))?;
    assert!(
        !ciphertext
            .windows(b"fixture-bearer".len())
            .any(|v| v == b"fixture-bearer")
    );
    let mut reopened = SecretVault::open_with_key_store(&path, keys.clone())?
        .lock(CancellationToken::new())
        .await?;
    assert_eq!(
        reopened
            .get(&secret_id)
            .context("missing saved secret")?
            .expose_secret(),
        "fixture-bearer-keep-private"
    );
    for other in [
        SecretId::new("other-product", "api-key", "https://one.example/account-a")?,
        SecretId::new("fixture-product", "relay", "https://one.example/account-a")?,
        SecretId::new(
            "fixture-product",
            "api-key",
            "https://two.example/account-a",
        )?,
        SecretId::new(
            "fixture-product",
            "api-key",
            "https://one.example/account-b",
        )?,
        SecretId::new("fixture-product", "uncommitted", "one")?,
    ] {
        assert!(reopened.get(&other).is_none());
    }
    reopened.delete(&secret_id);
    reopened.commit()?;
    drop(reopened);
    let reopened = vault.lock(CancellationToken::new()).await?;
    assert!(reopened.get(&secret_id).is_none());
    assert!(reopened.migrated(&secret_id));
    assert_eq!(keys.writes.load(Ordering::SeqCst), 1);
    Ok(())
}

#[tokio::test]
async fn nested_credential_parents_are_created_and_reopened() -> Result<()> {
    let root = tempfile::tempdir()?;
    let path = root.path().join("new-state").join("nested").join("auth");
    assert!(!root.path().join("new-state").exists());
    let keys = Arc::new(Keys::default());
    let secret_id = id()?;
    let vault = SecretVault::open_with_key_store(&path, keys.clone())?;
    let mut transaction = vault.lock(CancellationToken::new()).await?;
    transaction.set(secret_id.clone(), SecretString::new("nested-path-secret"))?;
    transaction.commit()?;
    drop(transaction);
    drop(vault);

    let reopened = SecretVault::open_with_key_store(&path, keys.clone())?
        .lock(CancellationToken::new())
        .await?;
    assert_eq!(
        reopened
            .get(&secret_id)
            .context("missing secret after opening a new nested state directory")?
            .expose_secret(),
        "nested-path-secret"
    );
    assert_eq!(keys.writes.load(Ordering::SeqCst), 1);
    Ok(())
}

#[tokio::test]
async fn key_bootstrap_is_exclusive_and_lock_wait_cancels() -> Result<()> {
    let root = tempfile::tempdir()?;
    let path = root.path().join("vault");
    let keys = Arc::new(Keys::default());
    let first = SecretVault::open_with_key_store(&path, keys.clone())?;
    let second = SecretVault::open_with_key_store(&path, keys.clone())?;
    let mut held = first.lock(CancellationToken::new()).await?;
    held.set(id()?, SecretString::new("value"))?;
    held.commit()?;
    let cancel = CancellationToken::new();
    let waiting = second.lock(cancel.clone());
    tokio::pin!(waiting);
    assert!(
        tokio::time::timeout(Duration::from_millis(40), &mut waiting)
            .await
            .is_err()
    );
    cancel.cancel();
    assert!(
        tokio::time::timeout(Duration::from_millis(250), waiting)
            .await?
            .is_err()
    );
    drop(held);
    let transaction = second.lock(CancellationToken::new()).await?;
    assert!(transaction.get(&id()?).is_some());
    assert_eq!(keys.writes.load(Ordering::SeqCst), 1);
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn blocked_lock_times_out_without_changing_committed_state() -> Result<()> {
    let vault = SecretVault::memory();
    let mut held = vault.lock(CancellationToken::new()).await?;
    held.set(id()?, SecretString::new("committed-value"))?;
    held.commit()?;
    let waiting = vault.lock(CancellationToken::new());
    tokio::pin!(waiting);
    assert!(futures_util::poll!(&mut waiting).is_pending());
    tokio::time::advance(Duration::from_secs(31)).await;
    assert!(waiting.await.is_err());
    drop(held);
    assert_eq!(
        vault
            .lock(CancellationToken::new())
            .await?
            .get(&id()?)
            .context("missing committed secret")?
            .expose_secret(),
        "committed-value"
    );
    Ok(())
}

async fn blocked_key_read_stops_waiting(cancel_wait: bool) -> Result<()> {
    let root = tempfile::tempdir()?;
    let path = root.path().join("vault");
    let keys = Arc::new(Keys::default());
    let mut held = SecretVault::open_with_key_store(&path, keys.clone())?
        .lock(CancellationToken::new())
        .await?;
    held.set(id()?, SecretString::new("committed-before-key-wait"))?;
    held.commit()?;
    drop(held);
    let ciphertext = fs::read(path.join("vault.bin"))?;
    let (blocked, entered, release) = BlockedKeys::new(keys.clone(), KeyCall::Read);
    let vault = SecretVault::open_with_key_store(&path, blocked)?;
    let cancel = CancellationToken::new();
    let waiting = tokio::spawn({
        let vault = vault.clone();
        let cancel = cancel.clone();
        async move { vault.lock(cancel).await }
    });
    tokio::time::timeout(Duration::from_secs(2), entered).await??;
    if cancel_wait {
        cancel.cancel();
    } else {
        // Pause only after the real blocking worker has reached the injected boundary.
        tokio::time::pause();
        tokio::time::advance(LOCK_WAIT).await;
        // Let rounded deadlines and the completion guard progress while the
        // blocking key call still inhibits paused-clock auto-advance.
        tokio::time::resume();
    }
    let outcome = tokio::time::timeout(Duration::from_secs(1), waiting).await;
    assert!(
        outcome??.is_err(),
        "native key wait must not expose a transaction"
    );
    assert!(
        vault.try_lock()?.is_none(),
        "native call must retain ownership"
    );
    assert_eq!(fs::read(path.join("vault.bin"))?, ciphertext);
    drop(release);
    let held = released_lock(&vault).await?;
    assert_eq!(fs::read(path.join("vault.bin"))?, ciphertext);
    assert_eq!(keys.writes.load(Ordering::SeqCst), 1);
    drop(held);
    let transaction = vault.lock(CancellationToken::new()).await?;
    assert_eq!(
        transaction
            .get(&id()?)
            .context("committed secret missing")?
            .expose_secret(),
        "committed-before-key-wait"
    );
    Ok(())
}

#[tokio::test]
async fn native_key_read_cancels_without_releasing_the_inflight_lock() -> Result<()> {
    blocked_key_read_stops_waiting(true).await
}

#[tokio::test]
async fn native_key_read_obeys_the_original_lock_deadline() -> Result<()> {
    blocked_key_read_stops_waiting(false).await
}

async fn interrupted_native_key_save_leaves_recoverable_bootstrap(drop_wait: bool) -> Result<()> {
    let root = tempfile::tempdir()?;
    let path = root.path().join("vault");
    let keys = Arc::new(Keys::default());
    let (blocked, entered, release) = BlockedKeys::new(keys.clone(), KeyCall::Save);
    let vault = SecretVault::open_with_key_store(&path, blocked)?;
    let cancel = CancellationToken::new();
    let waiting = tokio::spawn({
        let vault = vault.clone();
        let cancel = cancel.clone();
        async move { vault.lock(cancel).await }
    });
    tokio::time::timeout(Duration::from_secs(2), entered).await??;
    assert_eq!(keys.writes.load(Ordering::SeqCst), 1);
    if drop_wait {
        waiting.abort();
        let error = tokio::time::timeout(Duration::from_secs(1), waiting)
            .await?
            .err()
            .context("caller task was not aborted")?;
        ensure!(
            error.is_cancelled(),
            "aborted caller did not report cancellation"
        );
    } else {
        cancel.cancel();
        assert!(
            tokio::time::timeout(Duration::from_secs(1), waiting)
                .await??
                .is_err()
        );
    }
    assert!(
        vault.try_lock()?.is_none(),
        "native call must retain ownership"
    );
    drop(release);
    let held = released_lock(&vault).await?;
    assert_eq!(fs::read(path.join(".bootstrap"))?, BOOTSTRAP);
    assert!(!path.join("vault.bin").exists());
    drop(held);
    let mut transaction = vault.lock(CancellationToken::new()).await?;
    assert!(!path.join(".bootstrap").exists());
    transaction.set(id()?, SecretString::new("after-native-key-recovery"))?;
    transaction.commit()?;
    assert_eq!(keys.writes.load(Ordering::SeqCst), 1);
    Ok(())
}

#[tokio::test]
async fn cancelled_native_key_save_leaves_recoverable_bootstrap() -> Result<()> {
    interrupted_native_key_save_leaves_recoverable_bootstrap(false).await
}

#[tokio::test]
async fn abandoned_native_key_save_leaves_recoverable_bootstrap() -> Result<()> {
    interrupted_native_key_save_leaves_recoverable_bootstrap(true).await
}

#[tokio::test]
async fn lost_replaced_keys_tampering_and_unknown_formats_never_reset() -> Result<()> {
    let root = tempfile::tempdir()?;
    let path = root.path().join("vault");
    let keys = Arc::new(Keys::default());
    let vault = SecretVault::open_with_key_store(&path, keys.clone())?;
    let mut transaction = vault.lock(CancellationToken::new()).await?;
    transaction.set(id()?, SecretString::new("fixture-secret"))?;
    transaction.commit()?;
    drop(transaction);
    let original = fs::read(path.join("vault.bin"))?;
    let saved_keys = keys
        .values
        .lock()
        .map_err(|_| anyhow::anyhow!("test lock"))?
        .clone();
    keys.values
        .lock()
        .map_err(|_| anyhow::anyhow!("test lock"))?
        .clear();
    assert!(vault.lock(CancellationToken::new()).await.is_err());
    assert_eq!(keys.writes.load(Ordering::SeqCst), 1);
    assert_eq!(fs::read(path.join("vault.bin"))?, original);
    let replaced = saved_keys
        .keys()
        .map(|id| (id.clone(), SecretString::new(STANDARD.encode([9u8; 32]))))
        .collect();
    *keys
        .values
        .lock()
        .map_err(|_| anyhow::anyhow!("test lock"))? = replaced;
    assert!(vault.lock(CancellationToken::new()).await.is_err());
    *keys
        .values
        .lock()
        .map_err(|_| anyhow::anyhow!("test lock"))? = saved_keys;
    let mut tampered = original.clone();
    let last = tampered.last_mut().context("empty ciphertext")?;
    *last ^= 1;
    for bad in [
        tampered,
        b"EJMVLT99untrusted unsupported envelope".to_vec(),
        original[..20].to_vec(),
    ] {
        write_private(&path.join("vault.bin"), &bad)?;
        assert!(vault.lock(CancellationToken::new()).await.is_err());
        assert_eq!(fs::read(path.join("vault.bin"))?, bad);
    }
    write_private(&path.join("vault.bin"), &original)?;
    assert!(
        vault
            .lock(CancellationToken::new())
            .await?
            .get(&id()?)
            .is_some()
    );
    fs::remove_file(path.join("vault.bin"))?;
    assert!(vault.lock(CancellationToken::new()).await.is_err());
    assert_eq!(keys.writes.load(Ordering::SeqCst), 1);
    Ok(())
}

#[tokio::test]
async fn vault_ciphertext_is_bound_to_its_native_path() -> Result<()> {
    let root = tempfile::tempdir()?;
    let keys = Arc::new(Keys::default());
    let first = SecretVault::open_with_key_store(root.path().join("first"), keys.clone())?;
    let second = SecretVault::open_with_key_store(root.path().join("second"), keys.clone())?;
    let mut transaction = first.lock(CancellationToken::new()).await?;
    transaction.set(id()?, SecretString::new("first-value"))?;
    transaction.commit()?;
    drop(transaction);
    let first_key = keys
        .values
        .lock()
        .map_err(|_| anyhow::anyhow!("test lock"))?
        .values()
        .next()
        .context("missing key")?
        .clone();
    let mut transaction = second.lock(CancellationToken::new()).await?;
    transaction.commit()?;
    drop(transaction);
    for value in keys
        .values
        .lock()
        .map_err(|_| anyhow::anyhow!("test lock"))?
        .values_mut()
    {
        *value = first_key.clone();
    }
    write_private(
        &root.path().join("second/vault.bin"),
        &fs::read(root.path().join("first/vault.bin"))?,
    )?;
    assert!(second.lock(CancellationToken::new()).await.is_err());
    Ok(())
}

#[tokio::test]
async fn provider_migration_reuses_legacy_lock_and_rejects_old_readers() -> Result<()> {
    let root = tempfile::tempdir()?;
    let path = root.path().join("auth");
    let keys = Arc::new(Keys::default());
    let store = FileStore::open_with_key_store(&path, keys.clone())?;
    write_private(&path.join("state.json"), &wire::encode(&sample())?)?;
    let held = fs::File::open(path.join(".lock"))?;
    held.lock()?;
    #[cfg(unix)]
    let inode = {
        use std::os::unix::fs::MetadataExt;
        held.metadata()?.ino()
    };
    let waiting = store.lock(CancellationToken::new());
    tokio::pin!(waiting);
    assert!(
        tokio::time::timeout(Duration::from_millis(40), &mut waiting)
            .await
            .is_err()
    );
    let mut latest = sample();
    latest.registrations[0].authorization_revision = 9;
    write_private(&path.join("state.json"), &wire::encode(&latest)?)?;
    drop(held);
    let mut transaction = waiting.await?;
    let restored = transaction.load()?;
    assert!(restored.registrations[0].refresh_pending);
    assert_eq!(restored.registrations[0].authorization_revision, 9);
    assert!(wire::is_barrier(&fs::read(path.join("state.json"))?));
    assert!(wire::decode(&fs::read(path.join("state.json"))?).is_err());
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        assert_eq!(fs::metadata(path.join(".lock"))?.ino(), inode);
    }
    let mut logged_out = restored;
    logged_out.registrations[0].credentials = None;
    transaction.save(&logged_out)?;
    drop(transaction);
    write_private(&path.join("state.json"), &wire::encode(&sample())?)?;
    assert!(store.lock(CancellationToken::new()).await.is_err());
    Ok(())
}

#[tokio::test]
async fn interrupted_provider_handoff_reconciles_source_before_activation() -> Result<()> {
    let root = tempfile::tempdir()?;
    let path = root.path().join("auth");
    let keys = Arc::new(Keys::default());
    let vault = SecretVault::open_with_key_store(&path, keys.clone())?;
    write_private(&path.join("state.json"), &wire::encode(&sample())?)?;
    let mut prepared = vault.lock(CancellationToken::new()).await?;
    // Prepared ciphertext without a barrier is not the active provider source.
    prepared.state.provider = Some(sample());
    prepared.state.migration = Migration::Prepared;
    prepared.commit()?;
    drop(prepared);
    let mut latest = sample();
    latest.registrations[0].authorization_revision = 42;
    write_private(&path.join("state.json"), &wire::encode(&latest)?)?;
    let store = FileStore::open_with_key_store(&path, keys.clone())?;
    let mut transaction = store.lock(CancellationToken::new()).await?;
    assert_eq!(
        transaction.load()?.registrations[0].authorization_revision,
        42
    );
    drop(transaction);
    fs::remove_file(path.join("vault.bin"))?;
    assert!(store.lock(CancellationToken::new()).await.is_err());
    assert_eq!(keys.writes.load(Ordering::SeqCst), 1);
    Ok(())
}

#[tokio::test]
async fn provider_handoff_after_barrier_uses_prepared_ciphertext() -> Result<()> {
    let root = tempfile::tempdir()?;
    let path = root.path().join("auth");
    let keys = Arc::new(Keys::default());
    let vault = SecretVault::open_with_key_store(&path, keys.clone())?;
    let mut transaction = vault.lock(CancellationToken::new()).await?;
    let mut latest = sample();
    latest.registrations[0].authorization_revision = 88;
    transaction.state.provider = Some(latest);
    transaction.state.migration = Migration::Prepared;
    transaction.commit()?;
    drop(transaction);
    write_private(&path.join("state.json"), &wire::barrier("test-host")?)?;
    let store = FileStore::open_with_key_store(&path, keys)?;
    let mut transaction = store.lock(CancellationToken::new()).await?;
    assert_eq!(
        transaction.load()?.registrations[0].authorization_revision,
        88
    );
    assert!(transaction.load()?.registrations[0].refresh_pending);
    Ok(())
}

#[test]
fn unsupported_nested_provider_versions_are_rejected() -> Result<()> {
    let state = VaultState {
        provider: Some(sample()),
        ..VaultState::default()
    };
    let mut bytes: serde_json::Value = serde_json::from_slice(&encode(&state)?)?;
    bytes["provider"]["version"] = serde_json::json!(2);
    assert!(decode(&serde_json::to_vec(&bytes)?).is_err());
    Ok(())
}

#[tokio::test]
#[ignore = "uses the native user credential service; run explicitly on each supported OS"]
async fn native_os_key_service_round_trip() -> Result<()> {
    let root = tempfile::tempdir()?;
    let vault = SecretVault::open(root.path().join("native-key-fixture"))?;
    let identity = match &vault.backend {
        Backend::File { identity, .. } => identity.clone(),
        Backend::Memory(_) => bail!("fixture requires persistent storage"),
    };
    let result = async {
        let mut transaction = vault.lock(CancellationToken::new()).await?;
        transaction.set(id()?, SecretString::new("nonsecret-native-fixture"))?;
        transaction.commit()?;
        drop(transaction);
        let transaction = vault.lock(CancellationToken::new()).await?;
        ensure!(
            transaction.get(&id()?).is_some(),
            "native fixture did not reopen"
        );
        Ok::<_, anyhow::Error>(())
    }
    .await;
    let cleanup = native_entry(&identity)?.delete_credential();
    result?;
    cleanup.map_err(|_| anyhow::anyhow!("native fixture key cleanup failed"))?;
    Ok(())
}

#[tokio::test]
async fn interrupted_key_bootstrap_reuses_the_key_before_admitting_credentials() -> Result<()> {
    struct InterruptedKeys(Keys);
    impl VaultKeyStore for InterruptedKeys {
        fn get(&self, id: &str) -> Result<Option<SecretString>> {
            self.0.get(id)
        }
        fn set(&self, id: &str, key: &SecretString) -> Result<()> {
            self.0.set(id, key)?;
            bail!("fixture: OS key saved but completion was interrupted")
        }
    }
    let root = tempfile::tempdir()?;
    let path = root.path().join("auth");
    let keys = Arc::new(InterruptedKeys(Keys::default()));
    let vault = SecretVault::open_with_key_store(&path, keys.clone())?;
    assert!(vault.lock(CancellationToken::new()).await.is_err());
    assert_eq!(fs::read(path.join(".bootstrap"))?, BOOTSTRAP);
    assert!(!path.join("vault.bin").exists());
    let mut transaction = vault.lock(CancellationToken::new()).await?;
    assert!(!path.join(".bootstrap").exists());
    assert!(transaction.get(&id()?).is_none());
    transaction.set(id()?, SecretString::new("fixture-after-bootstrap"))?;
    transaction.commit()?;
    drop(transaction);
    assert_eq!(keys.0.writes.load(Ordering::SeqCst), 1);
    fs::remove_file(path.join("vault.bin"))?;
    assert!(vault.lock(CancellationToken::new()).await.is_err());
    assert_eq!(keys.0.writes.load(Ordering::SeqCst), 1);
    Ok(())
}

#[cfg(any(windows, target_os = "macos"))]
#[tokio::test]
async fn native_case_aliases_share_the_same_vault_key() -> Result<()> {
    let root = tempfile::tempdir()?;
    let path = root.path().join("MixedVault");
    let alias = root.path().join("mixedvault");
    let keys = Arc::new(Keys::default());
    let vault = SecretVault::open_with_key_store(&path, keys.clone())?;
    if !alias.try_exists()? {
        // A case-sensitive volume has no alias to exercise.
        return Ok(());
    }
    let mut transaction = vault.lock(CancellationToken::new()).await?;
    transaction.set(id()?, SecretString::new("case-alias-fixture"))?;
    transaction.commit()?;
    drop(transaction);
    let alias = SecretVault::open_with_key_store(&alias, keys.clone())?;
    let transaction = alias.lock(CancellationToken::new()).await?;
    assert_eq!(
        transaction
            .get(&id()?)
            .context("fixture missing")?
            .expose_secret(),
        "case-alias-fixture"
    );
    assert_eq!(keys.writes.load(Ordering::SeqCst), 1);
    Ok(())
}

#[tokio::test]
async fn memory_mode_bounds_and_provider_validation_preserve_committed_state() -> Result<()> {
    let vault = SecretVault::memory();
    let mut transaction = vault.lock(CancellationToken::new()).await?;
    assert!(
        transaction
            .set(id()?, SecretString::new("x".repeat(16385)))
            .is_err()
    );
    for n in 0..128 {
        transaction.set(
            SecretId::new("test", "scope", &n.to_string())?,
            SecretString::new("value"),
        )?;
    }
    assert!(
        transaction
            .set(id()?, SecretString::new("overflow"))
            .is_err()
    );
    transaction.commit()?;
    transaction.save_provider(&sample())?;
    let mut invalid = sample();
    invalid.registrations.push(invalid.registrations[0].clone());
    assert!(transaction.save_provider(&invalid).is_err());
    invalid = sample();
    invalid.registrations[0]
        .credentials
        .as_mut()
        .context("missing credentials")?
        .access_token = Some(SecretString::new("\"".repeat(1024 * 1024)));
    assert!(transaction.save_provider(&invalid).is_err());
    drop(transaction);
    assert_eq!(
        vault
            .lock(CancellationToken::new())
            .await?
            .provider()
            .registrations
            .len(),
        1
    );
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn unsafe_native_paths_and_replaced_lock_fail_closed() -> Result<()> {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let root = tempfile::tempdir()?;
    let path = root.path().join("vault");
    let keys = Arc::new(Keys::default());
    let vault = SecretVault::open_with_key_store(&path, keys.clone())?;
    let outside = root.path().join("outside");
    write_private(&outside, b"outside must remain unchanged")?;
    symlink(&outside, path.join("vault.bin"))?;
    assert!(vault.lock(CancellationToken::new()).await.is_err());
    assert_eq!(fs::read(&outside)?, b"outside must remain unchanged");
    fs::remove_file(path.join("vault.bin"))?;
    fs::hard_link(&outside, path.join("vault.bin"))?;
    assert!(vault.lock(CancellationToken::new()).await.is_err());
    fs::remove_file(path.join("vault.bin"))?;
    fs::rename(path.join(".lock"), path.join("old-lock"))?;
    write_private(&path.join(".lock"), b"")?;
    assert!(vault.lock(CancellationToken::new()).await.is_err());
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    assert!(SecretVault::open_with_key_store(&path, keys).is_err());
    Ok(())
}

#[tokio::test]
async fn stable_lock_excludes_another_native_process() -> Result<()> {
    const PROBE: &str = "EJM_VAULT_TEST_LOCK_PATH";
    const HELD: &str = "EJM_VAULT_TEST_LOCK_HELD";
    if let Some(path) = std::env::var_os(PROBE) {
        let lock = fs::File::open(path)?;
        let attempt = lock.try_lock();
        if std::env::var_os(HELD).is_some() {
            ensure!(
                matches!(attempt, Err(fs::TryLockError::WouldBlock)),
                "other process acquired held lock"
            );
        } else {
            attempt?;
        }
        return Ok(());
    }
    let root = tempfile::tempdir()?;
    let path = root.path().join("vault");
    let vault = SecretVault::open_with_key_store(&path, Arc::new(Keys::default()))?;
    let transaction = vault.lock(CancellationToken::new()).await?;
    let run = |held: bool| -> Result<_> {
        let mut command = tokio::process::Command::new(std::env::current_exe()?);
        command
            .args([
                "--exact",
                "store::vault::tests::stable_lock_excludes_another_native_process",
            ])
            .env(PROBE, path.join(".lock"))
            .env_remove(HELD)
            .kill_on_drop(true);
        if held {
            command.env(HELD, "1");
        }
        Ok(command)
    };
    assert!(
        tokio::time::timeout(Duration::from_secs(5), run(true)?.output())
            .await??
            .status
            .success()
    );
    drop(transaction);
    assert!(
        tokio::time::timeout(Duration::from_secs(5), run(false)?.output())
            .await??
            .status
            .success()
    );
    Ok(())
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn macos_allow_acls_and_inherited_creation_are_rejected() -> Result<()> {
    use std::{os::unix::fs::PermissionsExt, process::Command};
    let root = tempfile::tempdir()?;
    let path = root.path().join("vault");
    let keys = Arc::new(Keys::default());
    let vault = SecretVault::open_with_key_store(&path, keys.clone())?;
    drop(vault.lock(CancellationToken::new()).await?);
    for name in ["vault.bin", ".lock"] {
        let entry = path.join(name);
        ensure!(
            Command::new("/bin/chmod")
                .args(["+a", "everyone allow read"])
                .arg(&entry)
                .status()?
                .success(),
            "cannot add native fixture ACL"
        );
        assert_eq!(fs::metadata(&entry)?.permissions().mode() & 0o777, 0o600);
        assert!(vault.lock(CancellationToken::new()).await.is_err());
        ensure!(
            Command::new("/bin/chmod")
                .arg("-N")
                .arg(&entry)
                .status()?
                .success(),
            "cannot remove native fixture ACL"
        );
    }
    ensure!(
        Command::new("/bin/chmod")
            .args([
                "+a",
                "everyone allow read,search,file_inherit,directory_inherit"
            ])
            .arg(root.path())
            .status()?
            .success(),
        "cannot add inherited native fixture ACL"
    );
    let pending = root.path().join("new-vault");
    assert!(SecretVault::open_with_key_store(&pending, keys.clone()).is_err());
    assert!(!pending.join(".lock").exists());
    assert_eq!(keys.writes.load(Ordering::SeqCst), 1);
    Ok(())
}

#[test]
#[ignore = "explicit local vault measurement; uses a synthetic injected key"]
fn vault_save_reopen_profile() -> Result<()> {
    // Keep OS key-service latency separate from repeatable file/format costs.
    // Run this exact test alone under the native process resource profiler.
    let full = std::env::var_os("EJM_HARNESS_VAULT_PROFILE_FULL").is_some();
    let root = tempfile::tempdir()?;
    let path = root.path().join("profile");
    let keys = Arc::new(Keys::default());
    let cancel = CancellationToken::new();
    let vault = SecretVault::open_with_key_store(&path, keys.clone())?;
    let mut transaction = vault.lock_blocking(&cancel)?;
    let count = if full { 128 } else { 1 };
    let value = SecretString::new("x".repeat(if full { 16384 } else { 1024 }));
    for number in 0..count {
        let id = SecretId::new("profile", "session", &number.to_string())?;
        transaction.set(id.clone(), value.clone())?;
        transaction.mark_migrated(id)?;
    }
    if full {
        let registration = sample().registrations.remove(0);
        let mut provider = StoredAuth {
            host_id: "profile".into(),
            registrations: (0..64)
                .map(|number| StoredRegistration {
                    id: number.to_string(),
                    ..registration.clone()
                })
                .collect(),
        };
        let unused = wire::MAX_STORE_BYTES - wire::encode(&provider)?.len();
        let credentials = provider.registrations[0]
            .credentials
            .as_mut()
            .context("fixture credentials missing")?;
        credentials.id_token =
            SecretString::new("x".repeat(unused + credentials.id_token.expose_secret().len()));
        assert_eq!(wire::encode(&provider)?.len(), wire::MAX_STORE_BYTES);
        transaction.state.provider = Some(provider);
    }
    let plaintext_bytes = encode(&transaction.state)?.len();
    transaction.commit()?;
    drop(transaction);
    let mut samples = Vec::with_capacity(33);
    let last_id = SecretId::new("profile", "session", &(count - 1).to_string())?;
    for _ in 0..33 {
        let start = std::time::Instant::now();
        let mut transaction = vault.lock_blocking(&cancel)?;
        transaction.commit()?;
        drop(transaction);
        let reopened = SecretVault::open_with_key_store(&path, keys.clone())?;
        let transaction = reopened.lock_blocking(&cancel)?;
        assert_eq!(
            transaction
                .get(&last_id)
                .context("profile record missing")?
                .expose_secret(),
            value.expose_secret()
        );
        assert!(transaction.migrated(&last_id));
        drop(transaction);
        samples.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    samples.sort_by(f64::total_cmp);
    println!(
        "vault-profile full={full} samples=33 plaintext_bytes={plaintext_bytes} encrypted_bytes={} median_ms={:.3} p95_ms={:.3}",
        fs::metadata(path.join("vault.bin"))?.len(),
        samples[16],
        samples[31]
    );
    Ok(())
}
