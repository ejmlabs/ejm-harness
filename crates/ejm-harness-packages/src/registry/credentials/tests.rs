use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use anyhow::{Result, bail, ensure};
use ejm_harness_auth::VaultKeyStore;
use tokio_util::sync::CancellationToken;

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
const RETIRED: &str = "https://harness.meetgerry.ai/";

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
fn public_and_custom_registry_sessions_remain_origin_scoped() -> Result<()> {
    let fixture = Fixture::new()?;
    let public_origin = super::super::PUBLIC_REGISTRY_ORIGIN;
    let public_token = SecretString::new("public-registry-fixture-token");
    let custom_token = SecretString::new("custom-registry-fixture-token");
    save(&fixture.vault, &fixture.root, ORIGIN, &custom_token)?;
    save(&fixture.vault, &fixture.root, public_origin, &public_token)?;
    assert_eq!(
        load(&fixture.vault, &fixture.root, ORIGIN)?.expose_secret(),
        custom_token.expose_secret()
    );
    assert_eq!(
        load(&fixture.vault, &fixture.root, public_origin)?.expose_secret(),
        public_token.expose_secret()
    );
    delete(&fixture.vault, public_origin)?;
    assert!(load(&fixture.vault, &fixture.root, public_origin).is_err());
    assert_eq!(
        load(&fixture.vault, &fixture.root, ORIGIN)?.expose_secret(),
        custom_token.expose_secret()
    );
    Ok(())
}

#[test]
fn retired_sessions_are_not_imported_and_discard_is_local() -> Result<()> {
    let fixture = Fixture::new()?;
    let retired = SecretString::new("retired-fixture-token");
    let id = identity(RETIRED)?;
    let mut transaction = fixture.vault.lock_blocking(&CancellationToken::new())?;
    transaction.set(id, retired.clone())?;
    transaction.commit()?;
    drop(transaction);
    fixture.legacy(RETIRED)?;
    let current = SecretString::new("current-fixture-token");
    save(&fixture.vault, &fixture.root, ORIGIN, &current)?;
    assert_eq!(
        load(&fixture.vault, &fixture.root, ORIGIN)?.expose_secret(),
        current.expose_secret()
    );
    let Err(retired_load) = load(&fixture.vault, &fixture.root, RETIRED) else {
        bail!("retired origin is not loaded");
    };
    let retired_load = retired_load.to_string();
    ensure!(retired_load.contains(super::super::DISCARD_RETIRED_REGISTRY_COMMAND));
    ensure!(retired_load.contains(super::super::PUBLIC_REGISTRY_ORIGIN));
    ensure!(!retired_load.contains("migrate-credentials"));
    let Err(refused) = migrate(&fixture.vault, &fixture.root, ORIGIN) else {
        bail!("retired legacy file is not imported");
    };
    ensure!(
        refused
            .to_string()
            .contains(super::super::DISCARD_RETIRED_REGISTRY_COMMAND)
    );
    assert!(fixture.root.join(LEGACY).exists());
    assert_eq!(
        load(&fixture.vault, &fixture.root, ORIGIN)?.expose_secret(),
        current.expose_secret()
    );
    let notice = locate(&fixture.vault, &fixture.root)?;
    assert_eq!(notice.len(), 1);
    assert!(notice[0].legacy_file && notice[0].vault_record);
    assert!(!notice[0].origin.contains("retired-fixture-token"));
    let removed = discard(&fixture.vault, &fixture.root)?;
    assert_eq!(removed, notice);
    assert!(!fixture.root.join(LEGACY).exists());
    assert!(load(&fixture.vault, &fixture.root, RETIRED).is_err());
    ensure!(locate(&fixture.vault, &fixture.root)?.is_empty());
    fixture.legacy(ORIGIN)?;
    ensure!(discard(&fixture.vault, &fixture.root)?.is_empty());
    assert!(fixture.root.join(LEGACY).exists());
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

fn assert_unusable_sign_in(fixture: &Fixture) -> Result<()> {
    let path = fixture.root.join(LEGACY).display().to_string();
    let attempts = [
        load(&fixture.vault, &fixture.root, ORIGIN).map(|_| ()),
        save(
            &fixture.vault,
            &fixture.root,
            ORIGIN,
            &ejm_harness_auth::SecretString::new("new-token"),
        ),
    ];
    for result in attempts {
        let Err(error) = result else {
            bail!("unusable legacy file did not block sign-in");
        };
        let message = error.to_string();
        ensure!(message.contains(&path), "{message}");
        ensure!(
            message.contains(super::super::DISCARD_RETIRED_REGISTRY_COMMAND),
            "{message}"
        );
        ensure!(message.contains("was not sent"), "{message}");
        ensure!(!message.contains("migrate-credentials"), "{message}");
    }
    Ok(())
}

fn assert_migrate_leaves_legacy(fixture: &Fixture) -> Result<()> {
    ensure!(
        migrate(&fixture.vault, &fixture.root, ORIGIN).is_err(),
        "migrate deleted or accepted an unusable legacy file"
    );
    ensure!(
        fs::symlink_metadata(fixture.root.join(LEGACY)).is_ok(),
        "unusable legacy file was removed by migrate"
    );
    Ok(())
}

#[cfg(unix)]
fn write_legacy_mode(path: &std::path::Path, bytes: &[u8], mode: u32) -> Result<()> {
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(bytes)?;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    Ok(())
}

#[cfg(unix)]
#[test]
fn world_readable_retired_legacy_file_is_force_discarded() -> Result<()> {
    let fixture = Fixture::new()?;
    let bytes = serde_json::to_vec(&serde_json::json!({"origin":RETIRED,"token":"retired-loose"}))?;
    write_legacy_mode(&fixture.root.join(LEGACY), &bytes, 0o644)?;
    assert_unusable_sign_in(&fixture)?;
    assert_migrate_leaves_legacy(&fixture)?;
    ensure!(discard(&fixture.vault, &fixture.root)?.is_empty());
    assert!(fs::symlink_metadata(fixture.root.join(LEGACY)).is_err());
    write_legacy_mode(&fixture.root.join(LEGACY), &bytes, 0o644)?;
    ensure!(force_discard_legacy_registry_session(&fixture.root)?);
    assert!(fs::symlink_metadata(fixture.root.join(LEGACY)).is_err());
    ensure!(!force_discard_legacy_registry_session(&fixture.root)?);
    save(
        &fixture.vault,
        &fixture.root,
        ORIGIN,
        &ejm_harness_auth::SecretString::new("current-after-discard"),
    )?;
    Ok(())
}

#[cfg(unix)]
#[test]
fn a_parent_swapped_after_the_checks_cannot_redirect_the_unlink() -> Result<()> {
    let fixture = Fixture::new()?;
    let Some(base) = fixture.root.parent().map(std::path::Path::to_path_buf) else {
        bail!("fixture root has no parent");
    };
    let outside = base.join("outside");
    fs::create_dir(&outside)?;
    write_legacy_mode(&outside.join(LEGACY), b"{}", 0o600)?;
    write_legacy_mode(&fixture.root.join(LEGACY), b"{}", 0o600)?;
    let directory = PackageRoot::open(&fixture.root)?;
    // Swap the package root for a symlink to a directory outside it, after the checks.
    let moved = base.join("moved");
    fs::rename(&fixture.root, &moved)?;
    std::os::unix::fs::symlink(&outside, &fixture.root)?;
    ensure!(directory.unlink_legacy()?);
    ensure!(
        fs::symlink_metadata(outside.join(LEGACY)).is_ok(),
        "the outside file stays"
    );
    ensure!(fs::symlink_metadata(moved.join(LEGACY)).is_err());
    // A package root that is now a symlink is refused before any check runs.
    ensure!(PackageRoot::open(&fixture.root).is_err());
    ensure!(fs::symlink_metadata(outside.join(LEGACY)).is_ok());
    Ok(())
}

#[test]
fn retired_legacy_file_with_extra_field_is_force_discarded() -> Result<()> {
    let fixture = Fixture::new()?;
    let bytes = serde_json::to_vec(
        &serde_json::json!({"origin":RETIRED,"token":"retired-extra","extra":true}),
    )?;
    #[cfg(unix)]
    write_legacy_mode(&fixture.root.join(LEGACY), &bytes, 0o600)?;
    #[cfg(windows)]
    rightkit_fs::private::write_private_file(&fixture.root.join(LEGACY), &bytes)?;
    assert_unusable_sign_in(&fixture)?;
    assert_migrate_leaves_legacy(&fixture)?;
    ensure!(discard(&fixture.vault, &fixture.root)?.is_empty());
    assert!(fs::symlink_metadata(fixture.root.join(LEGACY)).is_err());
    #[cfg(unix)]
    write_legacy_mode(&fixture.root.join(LEGACY), &bytes, 0o600)?;
    #[cfg(windows)]
    rightkit_fs::private::write_private_file(&fixture.root.join(LEGACY), &bytes)?;
    ensure!(force_discard_legacy_registry_session(&fixture.root)?);
    assert!(fs::symlink_metadata(fixture.root.join(LEGACY)).is_err());
    Ok(())
}

#[cfg(unix)]
#[test]
fn symlink_legacy_session_is_unlinked_without_following() -> Result<()> {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new()?;
    let outside = fixture.root.with_file_name("outside-session");
    fs::write(&outside, b"must remain untouched")?;
    let link = fixture.root.join(LEGACY);
    symlink(&outside, &link)?;
    let Err(error) = load(&fixture.vault, &fixture.root, ORIGIN) else {
        bail!("symlink legacy file did not block sign-in");
    };
    let message = error.to_string();
    let path = link.display().to_string();
    ensure!(message.contains(&path), "{message}");
    ensure!(
        !message.contains(&outside.display().to_string()),
        "{message}"
    );
    assert_unusable_sign_in(&fixture)?;
    assert_migrate_leaves_legacy(&fixture)?;
    assert_eq!(fs::read(&outside)?, b"must remain untouched");
    assert!(fs::symlink_metadata(&link)?.is_symlink());
    ensure!(discard(&fixture.vault, &fixture.root)?.is_empty());
    assert!(fs::symlink_metadata(&link).is_err());
    assert_eq!(fs::read(&outside)?, b"must remain untouched");
    symlink(&outside, &link)?;
    ensure!(force_discard_legacy_registry_session(&fixture.root)?);
    assert!(fs::symlink_metadata(&link).is_err());
    assert_eq!(fs::read(&outside)?, b"must remain untouched");
    Ok(())
}

#[test]
fn force_discard_leaves_a_private_well_formed_legacy_file() -> Result<()> {
    let fixture = Fixture::new()?;
    ensure!(!force_discard_legacy_registry_session(&fixture.root)?);
    let source = fixture.legacy(ORIGIN)?;
    ensure!(!force_discard_legacy_registry_session(&fixture.root)?);
    assert_eq!(fs::read(fixture.root.join(LEGACY))?, source);
    fs::remove_file(fixture.root.join(LEGACY))?;
    let retired = fixture.legacy(RETIRED)?;
    ensure!(!force_discard_legacy_registry_session(&fixture.root)?);
    assert_eq!(fs::read(fixture.root.join(LEGACY))?, retired);
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
