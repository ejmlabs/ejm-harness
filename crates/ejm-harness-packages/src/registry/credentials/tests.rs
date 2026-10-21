use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use anyhow::Result;
use ejm_harness_auth::VaultKeyStore;

use super::*;

#[derive(Default)]
struct Keys(Mutex<Option<SecretString>>);
impl VaultKeyStore for Keys {
    fn get(&self, _: &str) -> Result<Option<SecretString>> {
        Ok(self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("fixture key lock"))?
            .clone())
    }
    fn set(&self, _: &str, key: &SecretString) -> Result<()> {
        *self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("fixture key lock"))? = Some(key.clone());
        Ok(())
    }
}

const ORIGIN: &str = "https://registry.example.invalid/";

struct Fixture {
    _directory: tempfile::TempDir,
    root: PathBuf,
    vault: SecretVault,
    keys: Arc<Keys>,
}
impl Fixture {
    fn new() -> Result<Self> {
        let directory = tempfile::tempdir()?;
        let root = directory.path().join("packages");
        crate::manager::private_directory(&root)?;
        let keys = Arc::new(Keys::default());
        let vault = SecretVault::open_with_key_store(root.join("credentials"), keys.clone())?;
        Ok(Self {
            _directory: directory,
            root,
            vault,
            keys,
        })
    }
    fn legacy(&self, origin: &str) -> Result<Vec<u8>> {
        let bytes = serde_json::to_vec(
            &serde_json::json!({"origin":origin,"token":"legacy-fixture-token"}),
        )?;
        #[cfg(unix)]
        {
            use std::{io::Write, os::unix::fs::OpenOptionsExt};
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(self.root.join(LEGACY))?;
            file.write_all(&bytes)?;
        }
        #[cfg(windows)]
        rightkit_fs::private::write_private_file(&self.root.join(LEGACY), &bytes)?;
        Ok(bytes)
    }
}

#[test]
fn encrypted_registry_sessions_are_origin_bound_and_deletion_is_permanent() -> Result<()> {
    let fixture = Fixture::new()?;
    let token = SecretString::new("registry-fixture-token");
    save(&fixture.vault, &fixture.root, ORIGIN, &token)?;
    assert_eq!(
        load(&fixture.vault, &fixture.root, ORIGIN)?.expose_secret(),
        token.expose_secret()
    );
    assert!(load(&fixture.vault, &fixture.root, "https://other.example/").is_err());
    let encrypted = fs::read(fixture.root.join("credentials/vault.bin"))?;
    assert!(
        !encrypted
            .windows(token.expose_secret().len())
            .any(|bytes| bytes == token.expose_secret().as_bytes())
    );
    assert!(!fixture.root.join(LEGACY).exists());
    assert!(!format!("{token:?}").contains("registry-fixture-token"));
    delete(&fixture.vault, ORIGIN)?;
    fixture.legacy(ORIGIN)?;
    assert!(load(&fixture.vault, &fixture.root, ORIGIN).is_err());
    assert!(migrate(&fixture.vault, &fixture.root, ORIGIN)?);
    assert!(load(&fixture.vault, &fixture.root, ORIGIN).is_err());
    assert!(!fixture.root.join(LEGACY).exists());
    Ok(())
}

#[test]
fn legacy_handoff_requires_confirmation_and_exact_origin() -> Result<()> {
    let fixture = Fixture::new()?;
    let source = fixture.legacy(ORIGIN)?;
    assert!(migrate_registry_token(&fixture.root, ORIGIN, false).is_err());
    assert!(fixture.keys.get("fixture")?.is_none());
    assert!(load(&fixture.vault, &fixture.root, ORIGIN).is_err());
    assert!(
        save(
            &fixture.vault,
            &fixture.root,
            ORIGIN,
            &SecretString::new("new-token")
        )
        .is_err()
    );
    assert!(migrate(&fixture.vault, &fixture.root, "https://other.example/").is_err());
    assert_eq!(fs::read(fixture.root.join(LEGACY))?, source);
    assert!(migrate(&fixture.vault, &fixture.root, ORIGIN)?);
    assert_eq!(
        load(&fixture.vault, &fixture.root, ORIGIN)?.expose_secret(),
        "legacy-fixture-token"
    );
    assert!(!migrate(&fixture.vault, &fixture.root, ORIGIN)?);
    Ok(())
}

#[test]
fn interrupted_legacy_cleanup_never_overwrites_a_replacement_or_resets_a_lost_key() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.legacy(ORIGIN)?;
    // This is the persisted boundary after import but before source retirement.
    let id = identity(ORIGIN)?;
    let mut transaction = fixture.vault.lock_blocking(&CancellationToken::new())?;
    transaction.set(id.clone(), SecretString::new("newer-fixture-token"))?;
    transaction.mark_migrated(id)?;
    transaction.commit()?;
    drop(transaction);
    assert!(migrate(&fixture.vault, &fixture.root, ORIGIN)?);
    assert_eq!(
        load(&fixture.vault, &fixture.root, ORIGIN)?.expose_secret(),
        "newer-fixture-token"
    );
    let source = fixture.legacy(ORIGIN)?;
    *fixture
        .keys
        .0
        .lock()
        .map_err(|_| anyhow::anyhow!("fixture key lock"))? = None;
    assert!(migrate(&fixture.vault, &fixture.root, ORIGIN).is_err());
    assert_eq!(fs::read(fixture.root.join(LEGACY))?, source);
    assert!(fixture.keys.get("fixture")?.is_none());
    Ok(())
}

#[cfg(unix)]
#[test]
fn unsafe_legacy_source_is_not_imported_or_deleted() -> Result<()> {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new()?;
    let outside = fixture.root.with_file_name("outside");
    fs::write(&outside, b"must remain untouched")?;
    symlink(&outside, fixture.root.join(LEGACY))?;
    assert!(migrate(&fixture.vault, &fixture.root, ORIGIN).is_err());
    assert_eq!(fs::read(&outside)?, b"must remain untouched");
    assert!(fs::symlink_metadata(fixture.root.join(LEGACY))?.is_symlink());
    Ok(())
}

#[cfg(target_os = "linux")]
#[test]
fn legacy_fifo_is_rejected_without_waiting_or_changing_credentials() -> Result<()> {
    use std::{
        os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt},
        sync::mpsc::{self, RecvTimeoutError},
        thread,
        time::Duration,
    };

    use rustix::fs::{CWD, Mode, OFlags, mkfifoat};

    let fixture = Fixture::new()?;
    let token = SecretString::new("preserved-registry-fixture-token");
    save(&fixture.vault, &fixture.root, ORIGIN, &token)?;
    let encrypted = fs::read(fixture.root.join("credentials/vault.bin"))?;
    let path = fixture.root.join(LEGACY);
    mkfifoat(CWD, &path, Mode::RUSR | Mode::WUSR)?;
    let before = fs::symlink_metadata(&path)?;

    let (finished, completion) = mpsc::channel();
    let rescue_path = path.clone();
    let watchdog = thread::spawn(move || -> Result<bool> {
        match completion.recv_timeout(Duration::from_secs(2)) {
            Ok(()) => Ok(false),
            Err(RecvTimeoutError::Disconnected) => bail!("FIFO fixture lost completion"),
            Err(RecvTimeoutError::Timeout) => {
                // Release a regressed blocking open so the test fails instead of hanging.
                let reader = fs::OpenOptions::new()
                    .read(true)
                    .custom_flags(OFlags::NONBLOCK.bits().cast_signed())
                    .open(&rescue_path)?;
                let writer = fs::OpenOptions::new()
                    .write(true)
                    .custom_flags(OFlags::NONBLOCK.bits().cast_signed())
                    .open(&rescue_path)?;
                drop(writer);
                drop(reader);
                Ok(true)
            }
        }
    });
    let result = migrate(&fixture.vault, &fixture.root, ORIGIN);
    let _ = finished.send(());
    let rescued = watchdog
        .join()
        .map_err(|_| anyhow::anyhow!("FIFO fixture watchdog panicked"))??;

    assert!(result.is_err());
    let after = fs::symlink_metadata(&path)?;
    assert!(after.file_type().is_fifo());
    assert_eq!((after.dev(), after.ino()), (before.dev(), before.ino()));
    assert_eq!(
        fs::read(fixture.root.join("credentials/vault.bin"))?,
        encrypted
    );
    assert_eq!(
        load(&fixture.vault, &fixture.root, ORIGIN)?.expose_secret(),
        token.expose_secret()
    );
    assert!(!rescued, "legacy FIFO waited for the fixture writer");
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn legacy_allow_acl_is_not_imported_or_deleted() -> Result<()> {
    let fixture = Fixture::new()?;
    let source = fixture.legacy(ORIGIN)?;
    ensure!(
        std::process::Command::new("/bin/chmod")
            .args(["+a", "everyone allow read"])
            .arg(fixture.root.join(LEGACY))
            .status()?
            .success(),
        "cannot add native fixture ACL"
    );
    assert!(migrate(&fixture.vault, &fixture.root, ORIGIN).is_err());
    assert_eq!(fs::read(fixture.root.join(LEGACY))?, source);
    Ok(())
}
