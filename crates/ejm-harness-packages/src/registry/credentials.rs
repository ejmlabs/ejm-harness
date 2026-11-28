#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
use std::{fs, io::Read, path::Path};

use anyhow::{Context, Result, bail, ensure};
use ejm_harness_auth::{SecretId, SecretString, SecretVault};
use serde::{Deserialize, Deserializer};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

const LEGACY: &str = "registry-session.json";
const MAX_LEGACY: usize = 8192;

fn open(root: &Path) -> Result<SecretVault> {
    crate::manager::private_directory(root)?;
    SecretVault::open(root.join("credentials"))
}

fn identity(origin: &str) -> Result<SecretId> {
    SecretId::new(
        "ejm-harness-packages",
        "registry-session",
        super::normalize_origin(origin)?.as_str(),
    )
}

/// A `meetgerry.ai` session is never saved, loaded, or imported, so its token cannot be sent.
fn refuse_retired(origin: &str) -> Result<()> {
    ensure!(
        !super::normalize_origin(origin)?
            .host_str()
            .is_some_and(super::retired_registry_host),
        "{}",
        super::retired_registry_message()
    );
    Ok(())
}

fn legacy_exists(root: &Path) -> Result<bool> {
    match fs::symlink_metadata(root.join(LEGACY)) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error).context("inspect legacy registry session"),
    }
}

/// Save an origin-scoped registry bearer in the common encrypted vault.
///
/// # Errors
/// Reports an unavailable key service, unsafe state, or a required legacy handoff.
/// Call this synchronous boundary away from a UI event thread.
pub fn save_registry_token(root: &Path, origin: &str, token: &SecretString) -> Result<()> {
    save(&open(root)?, root, origin, token)
}

fn save(vault: &SecretVault, root: &Path, origin: &str, token: &SecretString) -> Result<()> {
    ensure!(
        !token.expose_secret().is_empty() && token.expose_secret().len() <= 4096,
        "invalid registry account token"
    );
    refuse_retired(origin)?;
    let id = identity(origin)?;
    let mut transaction = vault.lock_blocking(&CancellationToken::new())?;
    ensure!(
        transaction.migrated(&id) || !legacy_exists(root)?,
        "legacy registry credentials require a controlled upgrade; stop older clients and call migrate_registry_token for this same origin"
    );
    transaction.set(id.clone(), token.clone())?;
    transaction.mark_migrated(id)?;
    transaction.commit()
}

/// Load only the encrypted record for the selected registry origin.
///
/// # Errors
/// Reports missing sessions, failed key access, or an unconfirmed legacy handoff.
/// Legacy plaintext is never an automatic fallback.
pub fn load_registry_token(root: &Path, origin: &str) -> Result<SecretString> {
    load(&open(root)?, root, origin)
}

fn load(vault: &SecretVault, root: &Path, origin: &str) -> Result<SecretString> {
    refuse_retired(origin)?;
    let id = identity(origin)?;
    let transaction = vault.lock_blocking(&CancellationToken::new())?;
    if let Some(token) = transaction.get(&id) {
        return Ok(token);
    }
    ensure!(
        transaction.migrated(&id) || !legacy_exists(root)?,
        "legacy registry credentials require a controlled upgrade; stop older clients and call migrate_registry_token for this same origin"
    );
    bail!("sign in with ejm-cli package login first")
}

/// Clear the selected registry session, retaining its permanent migration marker.
///
/// # Errors
/// Reports storage failure. A leftover legacy source is never restored; retire it
/// with the controlled migration command after stopping older clients.
pub fn delete_registry_token(root: &Path, origin: &str) -> Result<()> {
    delete(&open(root)?, origin)
}

fn delete(vault: &SecretVault, origin: &str) -> Result<()> {
    let id = identity(origin)?;
    let mut transaction = vault.lock_blocking(&CancellationToken::new())?;
    transaction.delete(&id);
    transaction.mark_migrated(id)?;
    transaction.commit()
}

/// Import a legacy session only after the host confirms older clients have stopped.
///
/// Returns whether a legacy file was retired. A destination marker prevents any
/// later import after logout or replacement, even if source cleanup was interrupted.
///
/// # Errors
/// Rejects an unconfirmed handoff, unsafe source, wrong origin, or failed persistence.
/// Never call concurrently with an older client, which does not honor the vault lock.
pub fn migrate_registry_token(
    root: &Path,
    origin: &str,
    old_clients_stopped: bool,
) -> Result<bool> {
    ensure!(
        old_clients_stopped,
        "stop older clients and confirm --old-clients-stopped before registry credential migration"
    );
    migrate(&open(root)?, root, origin)
}

fn migrate(vault: &SecretVault, root: &Path, origin: &str) -> Result<bool> {
    migrate_with_before_cleanup(vault, root, origin, || Ok(()))
}

fn migrate_with_before_cleanup(
    vault: &SecretVault,
    root: &Path,
    origin: &str,
    before_cleanup: impl FnOnce() -> Result<()>,
) -> Result<bool> {
    // The legacy origin must equal this one, so a retired session is never imported.
    refuse_retired(origin)?;
    let id = identity(origin)?;
    let mut transaction = vault.lock_blocking(&CancellationToken::new())?;
    #[cfg(unix)]
    let directory = open_legacy_directory(root)?;
    #[cfg(unix)]
    let source = read_legacy(&directory)?;
    #[cfg(windows)]
    let source = read_legacy(root)?;
    let Some(source) = source else {
        return Ok(false);
    };
    #[cfg(unix)]
    let bytes = &source.bytes;
    #[cfg(windows)]
    let bytes = &source;
    let legacy: LegacySession = serde_json::from_slice(bytes)
        .map_err(|_| anyhow::anyhow!("invalid legacy registry session"))?;
    ensure!(
        identity(&legacy.origin)? == id,
        "saved registry session belongs to another origin"
    );
    if !transaction.migrated(&id) {
        ensure!(
            transaction.get(&id).is_none(),
            "registry destination already exists; preserve both records and resolve the handoff"
        );
        ensure!(
            !legacy.token.expose_secret().is_empty() && legacy.token.expose_secret().len() <= 4096,
            "invalid legacy registry session"
        );
        transaction.set(id.clone(), legacy.token.clone())?;
        transaction.mark_migrated(id.clone())?;
        transaction.commit()?;
        drop(transaction);
        // Reopen authenticated persisted state before removing the only legacy source.
        transaction = vault.lock_blocking(&CancellationToken::new())?;
        ensure!(
            transaction.migrated(&id),
            "registry migration could not be verified"
        );
        // A concurrent new client may already have replaced or deleted the record.
        // Its committed marker remains authoritative and prevents stale reimport.
    }
    before_cleanup()?;
    #[cfg(unix)]
    let cleanup = remove_legacy(&directory, &source.file);
    #[cfg(windows)]
    let cleanup = remove_legacy(root);
    cleanup.context("registry vault committed; legacy cleanup failed, retry migration with older clients stopped")?;
    Ok(true)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacySession {
    origin: String,
    #[serde(deserialize_with = "secret")]
    token: SecretString,
}

fn secret<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<SecretString, D::Error> {
    String::deserialize(deserializer).map(SecretString::new)
}

#[cfg(unix)]
fn open_legacy_directory(root: &Path) -> Result<fs::File> {
    use rustix::fs::{Mode, OFlags};

    crate::manager::private_directory(root)?;
    let before = fs::symlink_metadata(root)?;
    let directory = fs::File::from(
        rustix::fs::open(
            root,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .context("open legacy registry directory")?,
    );
    let opened = directory.metadata()?;
    ensure!(
        before.is_dir() && same_inode(&before, &opened),
        "legacy registry directory changed during open"
    );
    #[cfg(not(target_os = "macos"))]
    {
        const NON_OWNER_MODE_BITS: u32 = 6;
        ensure!(
            opened.uid() == rustix::process::getuid().as_raw(),
            "package store has another owner"
        );
        ensure!(
            opened.mode().trailing_zeros() >= NON_OWNER_MODE_BITS,
            "package store must be private (chmod 700)"
        );
    }
    #[cfg(target_os = "macos")]
    verify_pinned_owner_only(&directory).context("package store must have an owner-only ACL")?;
    Ok(directory)
}

#[cfg(unix)]
fn same_inode(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    left.dev() == right.dev() && left.ino() == right.ino()
}

#[cfg(target_os = "macos")]
fn verify_pinned_owner_only(file: &fs::File) -> Result<()> {
    let opened = file.metadata()?;
    let path = rightkit_fs::handle::final_path(file).context("resolve legacy registry handle")?;
    // The ACL verifier takes a path, so check its identity on both sides.
    let verify_identity = || -> Result<()> {
        ensure!(
            same_inode(&opened, &fs::symlink_metadata(&path)?),
            "legacy registry ACL target changed during verification"
        );
        Ok(())
    };
    verify_identity()?;
    rightkit_fs::private::verify_owner_only(&path)?;
    verify_identity()
}

#[cfg(unix)]
struct LegacySource {
    bytes: Zeroizing<Vec<u8>>,
    file: fs::File,
}

#[cfg(unix)]
fn read_legacy(directory: &fs::File) -> Result<Option<LegacySource>> {
    use rustix::{
        fs::{FileType, Mode, OFlags, fstat, openat},
        io::Errno,
    };

    let file = match openat(
        directory,
        LEGACY,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    ) {
        Ok(fd) => fs::File::from(fd),
        Err(Errno::NOENT) => return Ok(None),
        Err(error) => return Err(error).context("cannot open protected legacy registry session"),
    };
    let inspected = fstat(&file)?;
    ensure!(
        FileType::from_raw_mode(inspected.st_mode) == FileType::RegularFile
            && inspected.st_nlink == 1
            && inspected.st_mode & 0o7777 == 0o600
            && inspected.st_uid == rustix::process::geteuid().as_raw(),
        "legacy registry session must be a private owner-only regular file"
    );
    #[cfg(target_os = "macos")]
    verify_pinned_owner_only(&file)?;
    let bytes = read_bounded(&file)?;
    #[cfg(target_os = "macos")]
    verify_pinned_owner_only(&file)?;
    verify_legacy_entry(
        directory,
        &inspected,
        "legacy registry session changed during read",
    )?;
    Ok(Some(LegacySource { bytes, file }))
}

#[cfg(unix)]
fn verify_legacy_entry(
    directory: &fs::File,
    inspected: &rustix::fs::Stat,
    changed: &'static str,
) -> Result<()> {
    use rustix::fs::{AtFlags, statat};

    let current = statat(directory, LEGACY, AtFlags::SYMLINK_NOFOLLOW).context(changed)?;
    ensure!(
        current.st_dev == inspected.st_dev && current.st_ino == inspected.st_ino,
        "{changed}"
    );
    Ok(())
}

#[cfg(windows)]
fn read_legacy(root: &Path) -> Result<Option<Zeroizing<Vec<u8>>>> {
    let directory = rightkit_fs::StateDir::open(root)?;
    if !directory.exists(LEGACY)? {
        return Ok(None);
    }
    let file = directory.open_read(
        LEGACY,
        &rightkit_fs::handle::ReadOptions::new()
            .require_single_link(true)
            .share_read_only(true),
    )?;
    rightkit_fs::private::verify_owner_only(&root.join(LEGACY))?;
    Ok(Some(read_bounded(&file)?))
}

fn read_bounded(file: &fs::File) -> Result<Zeroizing<Vec<u8>>> {
    ensure!(
        file.metadata()?.len() <= u64::try_from(MAX_LEGACY)?,
        "legacy registry session exceeds byte limit"
    );
    let mut bytes = Zeroizing::new(Vec::new());
    file.take(u64::try_from(MAX_LEGACY + 1)?)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= MAX_LEGACY,
        "legacy registry session exceeds byte limit"
    );
    Ok(bytes)
}

#[cfg(unix)]
fn remove_legacy(directory: &fs::File, inspected: &fs::File) -> Result<()> {
    use rustix::fs::{AtFlags, fstat, unlinkat};

    // The final stat and unlink are not atomic for a writer inside the private directory.
    // Pinning the directory prevents redirection through the parent.
    verify_legacy_entry(
        directory,
        &fstat(inspected)?,
        "legacy registry session changed before cleanup",
    )?;
    unlinkat(directory, LEGACY, AtFlags::empty())?;
    directory.sync_all()?;
    Ok(())
}

#[cfg(windows)]
fn remove_legacy(root: &Path) -> Result<()> {
    ensure!(
        rightkit_fs::StateDir::open(root)?.remove(LEGACY)?,
        "legacy registry session disappeared during cleanup"
    );
    Ok(())
}

#[cfg(test)]
mod tests;
