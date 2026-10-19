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
        super::parse_origin(origin)?.as_str(),
    )
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
    let id = identity(origin)?;
    let mut transaction = vault.lock_blocking(&CancellationToken::new())?;
    ensure!(
        transaction.migrated(&id) || !legacy_exists(root)?,
        "legacy registry credentials require a controlled upgrade; stop older clients and run ejm-cli package migrate-credentials --old-clients-stopped"
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
    let id = identity(origin)?;
    let transaction = vault.lock_blocking(&CancellationToken::new())?;
    if let Some(token) = transaction.get(&id) {
        return Ok(token);
    }
    ensure!(
        transaction.migrated(&id) || !legacy_exists(root)?,
        "legacy registry credentials require a controlled upgrade; stop older clients and run ejm-cli package migrate-credentials --old-clients-stopped"
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
    let id = identity(origin)?;
    let mut transaction = vault.lock_blocking(&CancellationToken::new())?;
    let Some(source) = read_legacy(root)? else {
        return Ok(false);
    };
    let legacy: LegacySession = serde_json::from_slice(&source)
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
    remove_legacy(root).context("registry vault committed; legacy cleanup failed, retry migration with older clients stopped")?;
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
fn read_legacy(root: &Path) -> Result<Option<Zeroizing<Vec<u8>>>> {
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    crate::manager::private_directory(root)?;
    let path = root.join(LEGACY);
    #[cfg(target_os = "macos")]
    if path.try_exists()? {
        rightkit_fs::private::verify_owner_only(&path)?;
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
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => bail!("cannot open protected legacy registry session"),
    };
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file()
            && metadata.nlink() == 1
            && metadata.mode() & 0o7777 == 0o600
            && metadata.uid() == rustix::process::geteuid().as_raw(),
        "legacy registry session must be a private owner-only regular file"
    );
    let bytes = read_bounded(file)?;
    let current = fs::symlink_metadata(&path)?;
    #[cfg(target_os = "macos")]
    rightkit_fs::private::verify_owner_only(&path)?;
    ensure!(
        current.dev() == metadata.dev() && current.ino() == metadata.ino(),
        "legacy registry session changed during read"
    );
    Ok(Some(bytes))
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
    Ok(Some(read_bounded(file)?))
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
fn remove_legacy(root: &Path) -> Result<()> {
    fs::remove_file(root.join(LEGACY))?;
    fs::File::open(root)?.sync_all()?;
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
