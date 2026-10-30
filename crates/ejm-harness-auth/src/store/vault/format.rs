//! The encrypted envelope and its JSON plaintext. Nothing here touches files or locks.

use anyhow::{Result, ensure};
use aws_lc_rs::aead::{AES_256_GCM, Aad, Nonce, RandomizedNonceKey};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use super::{
    MAX_ENCRYPTED, MAX_SECRET, MAX_SECRETS,
    state::{Migration, SecretId, VaultState},
};
use crate::store::wire::{self, BoundedJson, WireAuth, WireSecret};

const MAGIC: &[u8; 8] = b"EJMVLT01";
const MAX_PLAIN: usize = 4 * 1024 * 1024;

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

fn additional_data(identity: &str) -> Vec<u8> {
    let mut aad = MAGIC.to_vec();
    aad.extend_from_slice(identity.as_bytes());
    aad
}

/// Encrypt serialized state into the `vault.bin` envelope: magic, nonce, ciphertext and tag.
pub(super) fn seal(mut payload: Zeroizing<Vec<u8>>, key: &[u8], identity: &str) -> Result<Vec<u8>> {
    let algorithm = RandomizedNonceKey::new(&AES_256_GCM, key)
        .map_err(|_| anyhow::anyhow!("invalid vault key"))?;
    let aad = additional_data(identity);
    let nonce = algorithm
        .seal_in_place_append_tag(Aad::from(&aad), &mut *payload)
        .map_err(|_| anyhow::anyhow!("cannot encrypt credential vault"))?;
    let mut file = Vec::with_capacity(MAGIC.len() + nonce.as_ref().len() + payload.len());
    file.extend_from_slice(MAGIC);
    file.extend_from_slice(nonce.as_ref());
    file.extend_from_slice(&payload);
    Ok(file)
}

pub(super) fn decrypt(encoded: &[u8], key: &[u8], identity: &str) -> Result<VaultState> {
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

pub(super) fn encode(state: &VaultState) -> Result<Zeroizing<Vec<u8>>> {
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
    let mut writer = BoundedJson::new(MAX_PLAIN, "credential vault exceeds 4 MiB");
    serde_json::to_writer(&mut writer, &wire)
        .map_err(|_| anyhow::anyhow!("credential vault exceeds supported JSON bounds"))?;
    Ok(writer.into_bytes())
}

pub(super) fn decode(bytes: &[u8]) -> Result<VaultState> {
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
