use std::{fs, io::Read, path::Path};

use anyhow::{Context, Result, bail, ensure};
use ejm_harness_auth::{SecretId, SecretString, SecretVault, VaultTransaction};
use serde::{Deserialize, Deserializer};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

const LEGACY: &str = "registry-session.json";
const MAX_LEGACY: usize = 8192;

fn open(root: &Path) -> Result<SecretVault> {
    crate::manager::private_directory(root)?;
    SecretVault::open(root.join("credentials"))
}

const PRODUCT: &str = "ejm-harness-packages";
const PURPOSE: &str = "registry-session";

fn identity(origin: &str) -> Result<SecretId> {
    SecretId::new(PRODUCT, PURPOSE, super::normalize_origin(origin)?.as_str())
}

fn origin_is_retired(origin: &str) -> Result<bool> {
    Ok(super::normalize_origin(origin)?
        .host_str()
        .is_some_and(super::retired_registry_host))
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
    ensure!(
        !origin_is_retired(origin)?,
        "{}",
        super::retired_registry_message()
    );
    let id = identity(origin)?;
    let mut transaction = vault.lock_blocking(&CancellationToken::new())?;
    legacy_gate(&transaction, root, &id)?;
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
    ensure!(
        !origin_is_retired(origin)?,
        "{}",
        super::retired_registry_message()
    );
    let id = identity(origin)?;
    let transaction = vault.lock_blocking(&CancellationToken::new())?;
    if let Some(token) = transaction.get(&id) {
        return Ok(token);
    }
    legacy_gate(&transaction, root, &id)?;
    bail!("sign in with ejm-cli package login first")
}

fn legacy_gate(transaction: &VaultTransaction, root: &Path, id: &SecretId) -> Result<()> {
    if transaction.migrated(id) {
        return Ok(());
    }
    let Some(legacy) = read_legacy_session(root)? else {
        return Ok(());
    };
    if origin_is_retired(&legacy.origin)? {
        if identity(&legacy.origin)? == *id {
            bail!("{}", super::retired_registry_message());
        }
        return Ok(());
    }
    bail!(
        "legacy registry credentials require a controlled upgrade; stop older clients and call migrate_registry_token for this same origin"
    )
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

/// A leftover registry session for `meetgerry.ai` or one of its subdomains.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RetiredRegistrySession {
    /// Normalized origin, including the root path.
    pub origin: String,
    /// The private `registry-session.json` file names this origin.
    pub legacy_file: bool,
    /// The encrypted vault has a record for this origin.
    pub vault_record: bool,
}

/// List leftover sessions for the retired registry. Token values are not returned.
///
/// # Errors
/// Reports an unreadable legacy file, an unavailable key service, or invalid stored state.
pub fn locate_retired_registry_sessions(root: &Path) -> Result<Vec<RetiredRegistrySession>> {
    locate(&open(root)?, root)
}

/// Delete leftover retired-registry sessions locally. Nothing is sent on the network.
///
/// A legacy file for any other origin is left in place. Retired tokens are not copied
/// into another origin. An unsafe or unreadable `registry-session.json` is unlinked
/// without following it and without parsing it into a credential.
///
/// # Errors
/// Reports an unavailable key service or a failed delete.
pub fn discard_retired_registry_sessions(root: &Path) -> Result<Vec<RetiredRegistrySession>> {
    discard(&open(root)?, root)
}

/// Unlink an unsafe or unreadable legacy `registry-session.json`.
///
/// Returns whether a directory entry was removed. A missing path returns `false`.
/// A private, well-formed legacy file is left in place and returns `false`.
/// Nothing is sent on the network. A symlink or other unsafe entry is unlinked
/// without being followed or read. On Unix the unlink is relative to a handle on
/// the package root opened before the checks, so a parent directory swapped
/// afterwards cannot redirect it. A private file is read only to tell a
/// well-formed session from one that does not parse.
///
/// # Errors
/// Reports a directory or unlink failure. A well-formed file is not an error.
pub fn force_discard_legacy_registry_session(root: &Path) -> Result<bool> {
    let directory = PackageRoot::open(root)?;
    match classify_legacy(root)? {
        LegacyRead::Missing => Ok(false),
        LegacyRead::Unusable => directory.unlink_legacy(),
        LegacyRead::Bytes(bytes) => {
            if serde_json::from_slice::<LegacySession>(&bytes).is_err() {
                directory.unlink_legacy()
            } else {
                Ok(false)
            }
        }
    }
}

/// Message a host can show when a retired session is still stored.
///
/// `None` means sign-in to another origin is not affected by a retired session.
///
/// # Errors
/// Reports the same storage failures as [`locate_retired_registry_sessions`].
pub fn retired_registry_session_notice(root: &Path) -> Result<Option<String>> {
    let found = locate_retired_registry_sessions(root)?;
    Ok((!found.is_empty()).then(super::retired_registry_message))
}

fn locate(vault: &SecretVault, root: &Path) -> Result<Vec<RetiredRegistrySession>> {
    let transaction = vault.lock_blocking(&CancellationToken::new())?;
    let mut found = Vec::new();
    for stored in transaction.identities(PRODUCT, PURPOSE) {
        if origin_is_retired(&stored)? {
            found.push(RetiredRegistrySession {
                origin: stored,
                legacy_file: false,
                vault_record: true,
            });
        }
    }
    drop(transaction);
    if let Some(legacy) = read_legacy_session(root)?
        && origin_is_retired(&legacy.origin)?
    {
        let origin = super::normalize_origin(&legacy.origin)?.to_string();
        if let Some(existing) = found.iter_mut().find(|session| session.origin == origin) {
            existing.legacy_file = true;
        } else {
            found.push(RetiredRegistrySession {
                origin,
                legacy_file: true,
                vault_record: false,
            });
        }
    }
    found.sort_by(|left, right| left.origin.cmp(&right.origin));
    Ok(found)
}

fn discard(vault: &SecretVault, root: &Path) -> Result<Vec<RetiredRegistrySession>> {
    let transaction = vault.lock_blocking(&CancellationToken::new())?;
    let mut found = Vec::new();
    for stored in transaction.identities(PRODUCT, PURPOSE) {
        if origin_is_retired(&stored)? {
            found.push(RetiredRegistrySession {
                origin: stored,
                legacy_file: false,
                vault_record: true,
            });
        }
    }
    drop(transaction);
    let directory = PackageRoot::open(root)?;
    let legacy_action = match classify_legacy(root)? {
        LegacyRead::Missing => LegacyDiscard::Leave,
        LegacyRead::Unusable => LegacyDiscard::Unlink,
        LegacyRead::Bytes(bytes) => match serde_json::from_slice::<LegacySession>(&bytes) {
            Err(_) => LegacyDiscard::Unlink,
            Ok(legacy) => {
                if origin_is_retired(&legacy.origin)? {
                    let origin = super::normalize_origin(&legacy.origin)?.to_string();
                    if let Some(existing) =
                        found.iter_mut().find(|session| session.origin == origin)
                    {
                        existing.legacy_file = true;
                    } else {
                        found.push(RetiredRegistrySession {
                            origin,
                            legacy_file: true,
                            vault_record: false,
                        });
                    }
                    LegacyDiscard::RemoveWellFormed
                } else {
                    LegacyDiscard::Leave
                }
            }
        },
    };
    if found.is_empty() && legacy_action != LegacyDiscard::Unlink {
        return Ok(found);
    }
    if found.iter().any(|session| session.vault_record) {
        let mut transaction = vault.lock_blocking(&CancellationToken::new())?;
        for session in &found {
            if session.vault_record {
                let id = identity(&session.origin)?;
                transaction.delete(&id);
                transaction.mark_migrated(id)?;
            }
        }
        transaction.commit()?;
    }
    match legacy_action {
        LegacyDiscard::Unlink => {
            directory.unlink_legacy()?;
        }
        LegacyDiscard::RemoveWellFormed => remove_legacy(&directory)?,
        LegacyDiscard::Leave => {}
    }
    Ok(found)
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
    let id = identity(origin)?;
    let mut transaction = vault.lock_blocking(&CancellationToken::new())?;
    let directory = PackageRoot::open(root)?;
    let Some(legacy) = read_legacy_session(root)? else {
        return Ok(false);
    };
    if origin_is_retired(&legacy.origin)? {
        bail!("{}", super::retired_registry_message());
    }
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
    remove_legacy(&directory).context("registry vault committed; legacy cleanup failed, retry migration with older clients stopped")?;
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

enum LegacyRead {
    Missing,
    Bytes(Zeroizing<Vec<u8>>),
    Unusable,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LegacyDiscard {
    Leave,
    Unlink,
    RemoveWellFormed,
}

fn unusable_legacy_message(root: &Path) -> String {
    format!(
        "legacy registry session at {} is unsafe or unreadable and was not sent. Discard it with `{}`",
        root.join(LEGACY).display(),
        super::DISCARD_RETIRED_REGISTRY_COMMAND
    )
}

fn read_legacy_session(root: &Path) -> Result<Option<LegacySession>> {
    match classify_legacy(root)? {
        LegacyRead::Missing => Ok(None),
        LegacyRead::Unusable => bail!("{}", unusable_legacy_message(root)),
        LegacyRead::Bytes(bytes) => match serde_json::from_slice(&bytes) {
            Ok(session) => Ok(Some(session)),
            Err(_) => bail!("{}", unusable_legacy_message(root)),
        },
    }
}

/// The package root that legacy deletes are relative to. Callers open it before
/// their safety checks. On Unix it is a directory handle opened without following
/// the final component and checked against the path, and deletes use `unlinkat`
/// on it, so a parent directory swapped after the checks cannot redirect a delete
/// outside the package root. On Windows deletes go through `rightkit_fs` by path;
/// a parent swapped between the checks and the delete is not detected there.
struct PackageRoot {
    #[cfg(unix)]
    handle: rustix::fd::OwnedFd,
    #[cfg(not(unix))]
    path: std::path::PathBuf,
}

impl PackageRoot {
    #[cfg(unix)]
    fn open(root: &Path) -> Result<Self> {
        use rustix::fs::{Mode, OFlags};
        crate::manager::private_directory(root)?;
        let handle = rustix::fs::open(
            root,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .context("cannot open package root")?;
        let opened = rustix::fs::fstat(&handle)?;
        let listed = rustix::fs::lstat(root)?;
        ensure!(
            opened.st_dev == listed.st_dev && opened.st_ino == listed.st_ino,
            "package root changed while it was opened"
        );
        Ok(Self { handle })
    }

    #[cfg(not(unix))]
    fn open(root: &Path) -> Result<Self> {
        crate::manager::private_directory(root)?;
        Ok(Self {
            path: root.to_path_buf(),
        })
    }

    /// Remove the legacy directory entry itself, without following it. File bytes are
    /// not read here. Returns `false` when the entry is absent.
    #[cfg(unix)]
    fn unlink_legacy(&self) -> Result<bool> {
        use rustix::fs::AtFlags;
        match rustix::fs::statat(&self.handle, LEGACY, AtFlags::SYMLINK_NOFOLLOW) {
            Err(rustix::io::Errno::NOENT) => return Ok(false),
            Err(error) => return Err(error).context("cannot inspect legacy registry session"),
            Ok(_) => {}
        }
        rustix::fs::unlinkat(&self.handle, LEGACY, AtFlags::empty())
            .context("cannot discard legacy registry session")?;
        rustix::fs::fsync(&self.handle).context("cannot sync package root")?;
        Ok(true)
    }

    #[cfg(not(unix))]
    fn unlink_legacy(&self) -> Result<bool> {
        let path = self.path.join(LEGACY);
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error).context("cannot inspect legacy registry session"),
            Ok(_) => {
                fs::remove_file(&path).context("cannot discard legacy registry session")?;
                Ok(true)
            }
        }
    }
}

#[cfg(unix)]
fn legacy_metadata_is_private(metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    metadata.is_file()
        && metadata.nlink() == 1
        && metadata.mode() & 0o7777 == 0o600
        && metadata.uid() == rustix::process::geteuid().as_raw()
}

#[cfg(unix)]
fn classify_legacy(root: &Path) -> Result<LegacyRead> {
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    crate::manager::private_directory(root)?;
    let path = root.join(LEGACY);
    let listed = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(LegacyRead::Missing);
        }
        Err(error) => return Err(error).context("cannot inspect legacy registry session"),
    };
    // Safety failures stop before any read, including a symlink, loose mode, or extra link.
    if !legacy_metadata_is_private(&listed) || listed.len() > u64::try_from(MAX_LEGACY)? {
        return Ok(LegacyRead::Unusable);
    }
    #[cfg(target_os = "macos")]
    if rightkit_fs::private::verify_owner_only(&path).is_err() {
        return Ok(LegacyRead::Unusable);
    }
    let file = match fs::OpenOptions::new()
        .read(true)
        .custom_flags(
            (rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK)
                .bits()
                .cast_signed(),
        )
        .open(&path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(LegacyRead::Missing);
        }
        Err(_) => return Ok(LegacyRead::Unusable),
    };
    let metadata = file.metadata()?;
    if !legacy_metadata_is_private(&metadata) || metadata.len() > u64::try_from(MAX_LEGACY)? {
        return Ok(LegacyRead::Unusable);
    }
    let bytes = read_bounded(file)?;
    let current = fs::symlink_metadata(&path)?;
    #[cfg(target_os = "macos")]
    if rightkit_fs::private::verify_owner_only(&path).is_err() {
        return Ok(LegacyRead::Unusable);
    }
    ensure!(
        current.dev() == metadata.dev() && current.ino() == metadata.ino(),
        "legacy registry session changed during read"
    );
    Ok(LegacyRead::Bytes(bytes))
}

#[cfg(windows)]
fn classify_legacy(root: &Path) -> Result<LegacyRead> {
    let directory = rightkit_fs::StateDir::open(root)?;
    if !directory.exists(LEGACY)? {
        return Ok(LegacyRead::Missing);
    }
    let Ok(file) = directory.open_read(
        LEGACY,
        &rightkit_fs::handle::ReadOptions::new()
            .require_single_link(true)
            .share_read_only(true),
    ) else {
        return Ok(LegacyRead::Unusable);
    };
    if rightkit_fs::private::verify_owner_only(&root.join(LEGACY)).is_err() {
        return Ok(LegacyRead::Unusable);
    }
    let metadata = file.metadata()?;
    if metadata.len() > u64::try_from(MAX_LEGACY)? {
        return Ok(LegacyRead::Unusable);
    }
    Ok(LegacyRead::Bytes(read_bounded(file)?))
}

fn read_bounded(file: fs::File) -> Result<Zeroizing<Vec<u8>>> {
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
fn remove_legacy(directory: &PackageRoot) -> Result<()> {
    ensure!(
        directory.unlink_legacy()?,
        "legacy registry session disappeared during cleanup"
    );
    Ok(())
}

#[cfg(windows)]
fn remove_legacy(directory: &PackageRoot) -> Result<()> {
    ensure!(
        rightkit_fs::StateDir::open(&directory.path)?.remove(LEGACY)?,
        "legacy registry session disappeared during cleanup"
    );
    Ok(())
}

#[cfg(test)]
mod tests;
