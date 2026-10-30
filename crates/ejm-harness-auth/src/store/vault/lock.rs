//! Completing a held lock: key setup, bootstrap, and decryption come before any
//! credential transaction is exposed. Each step checks cancellation and the deadline.

use anyhow::{Context, Result, bail, ensure};
use aws_lc_rs::rand::fill;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use tokio::sync::OwnedMutexGuard;
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

use super::{
    BOOTSTRAP, BOOTSTRAP_FILE, Backend, LEGACY_STATE_FILE, MAX_ENCRYPTED, VAULT_FILE,
    format::decrypt, key_store::VaultKeyStore, state::VaultState, transaction::VaultTransaction,
};
use crate::{
    SecretString,
    store::{files::RawTransaction, wire},
};

pub(super) enum Prepared {
    File(RawTransaction),
    Memory(OwnedMutexGuard<VaultState>),
}

pub(super) fn check_lock_wait(cancel: &CancellationToken, timed_out: bool) -> Result<()> {
    ensure!(!cancel.is_cancelled(), "credential store lock cancelled");
    ensure!(!timed_out, "credential store lock timed out");
    Ok(())
}

pub(super) fn finish_lock(
    backend: Backend,
    prepared: Prepared,
    check_wait: impl Fn() -> Result<()>,
) -> Result<VaultTransaction> {
    check_wait()?;
    match (backend, prepared) {
        (Backend::Memory(_), Prepared::Memory(guard)) => Ok(VaultTransaction::memory(guard)),
        (Backend::File { keys, identity, .. }, Prepared::File(raw)) => {
            open_file_transaction(raw, keys.as_ref(), identity, &check_wait)
        }
        _ => bail!("invalid credential transaction backend"),
    }
}

fn open_file_transaction(
    raw: RawTransaction,
    keys: &dyn VaultKeyStore,
    identity: String,
    check_wait: &impl Fn() -> Result<()>,
) -> Result<VaultTransaction> {
    let encoded = raw.read(VAULT_FILE, MAX_ENCRYPTED)?;
    let bootstrap = raw.read(BOOTSTRAP_FILE, BOOTSTRAP.len())?;
    ensure!(
        bootstrap.as_deref().is_none_or(|bytes| bytes == BOOTSTRAP),
        "invalid credential bootstrap marker; preserve the vault and key"
    );
    let existing = read_existing_key(keys, &identity, check_wait)?;
    ensure!(
        encoded.is_none() || existing.is_some(),
        "existing vault key is missing; preserve the vault and recover the original OS key record"
    );
    ensure!(
        encoded.is_some() || existing.is_none() || bootstrap.is_some(),
        "existing credential vault data is missing; preserve the OS key and recover the original encrypted vault"
    );
    let new_vault = encoded.is_none();
    let legacy = raw.read(LEGACY_STATE_FILE, wire::MAX_STORE_BYTES)?;
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
        raw.write(BOOTSTRAP_FILE, BOOTSTRAP, BOOTSTRAP.len())?;
    }
    let secret = if let Some(secret) = existing {
        secret
    } else {
        create_key(keys, &identity, check_wait)?
    };
    let (key, state) = unseal(&secret, encoded, &identity)?;
    let mut transaction = VaultTransaction::file(raw, identity, key, state);
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
            .storage()?
            .remove(BOOTSTRAP_FILE, BOOTSTRAP.len())?;
    }
    check_wait()?;
    Ok(transaction)
}

fn read_existing_key(
    keys: &dyn VaultKeyStore,
    identity: &str,
    check_wait: &impl Fn() -> Result<()>,
) -> Result<Option<SecretString>> {
    check_wait()?;
    let existing = keys.get(identity);
    check_wait()?;
    existing.map_err(|_| {
        anyhow::anyhow!(
            "vault key service unavailable; unlock macOS Keychain or Windows Credential Manager, or start an unlocked Linux Secret Service in the user's D-Bus session"
        )
    })
}

fn create_key(
    keys: &dyn VaultKeyStore,
    identity: &str,
    check_wait: &impl Fn() -> Result<()>,
) -> Result<SecretString> {
    let mut bytes = Zeroizing::new([0u8; 32]);
    fill(bytes.as_mut()).map_err(|_| anyhow::anyhow!("cannot generate vault key"))?;
    let secret = SecretString::new(STANDARD.encode(bytes.as_ref()));
    check_wait()?;
    let saved = keys.set(identity, &secret);
    check_wait()?;
    saved.map_err(|_| anyhow::anyhow!("cannot save vault key in OS credential service"))?;
    check_wait()?;
    let confirmed = keys.get(identity);
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
    Ok(secret)
}

/// Decode the key record, then decrypt existing state or start empty.
fn unseal(
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
