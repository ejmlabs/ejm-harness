use crate::{SecretString, StoredAuth, StoredCredentials, StoredRegistration};
use anyhow::{Result, ensure};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{
    collections::HashSet,
    io::{self, Write},
};
use zeroize::Zeroizing;
pub(crate) const MAX_STORE_BYTES: usize = 1024 * 1024;
const MAX_REGISTRATIONS: usize = 64;

/// Zeroizing JSON output buffer that refuses to grow past `limit`.
pub(crate) struct BoundedJson {
    bytes: Zeroizing<Vec<u8>>,
    limit: usize,
    message: &'static str,
}

impl BoundedJson {
    pub(crate) fn new(limit: usize, message: &'static str) -> Self {
        Self {
            bytes: Zeroizing::new(Vec::new()),
            limit,
            message,
        }
    }

    pub(crate) fn into_bytes(self) -> Zeroizing<Vec<u8>> {
        self.bytes
    }
}

impl Write for BoundedJson {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
            return Err(io::Error::new(io::ErrorKind::FileTooLarge, self.message));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(crate) fn validate_state(state: &StoredAuth) -> Result<()> {
    ensure!(
        !state.host_id.is_empty(),
        "credential host identifier is empty"
    );
    ensure!(
        state.registrations.len() <= MAX_REGISTRATIONS,
        "credential state exceeds 64 registrations"
    );
    let mut identifiers = HashSet::new();
    let mut bytes = state.host_id.len();
    for registration in &state.registrations {
        ensure!(
            identity_fields(registration)
                .iter()
                .all(|value| !value.is_empty()),
            "credential registration has an empty identity field"
        );
        ensure!(
            identifiers.insert(&registration.id),
            "duplicate credential registration identifier"
        );
        bytes = bytes.saturating_add(registration_bytes(registration)?);
        ensure!(bytes <= MAX_STORE_BYTES, "credential state exceeds 1 MiB");
    }
    ensure!(bytes <= MAX_STORE_BYTES, "credential state exceeds 1 MiB");
    Ok(())
}

fn identity_fields(registration: &StoredRegistration) -> [&String; 4] {
    [
        &registration.id,
        &registration.issuer,
        &registration.subject,
        &registration.client_id,
    ]
}

/// Validate the registration's credentials and return the bytes it adds to the state.
fn registration_bytes(registration: &StoredRegistration) -> Result<usize> {
    let mut bytes = 0usize;
    for value in identity_fields(registration) {
        bytes = bytes.saturating_add(value.len());
    }
    bytes = bytes.saturating_add(registration.email.as_ref().map_or(0, String::len));
    if let Some(credentials) = &registration.credentials {
        bytes = bytes.saturating_add(credentials_bytes(credentials)?);
    }
    Ok(bytes)
}

fn credentials_bytes(credentials: &StoredCredentials) -> Result<usize> {
    ensure!(
        !credentials.id_token.expose_secret().is_empty(),
        "credential identity token is empty"
    );
    let mut bytes = 0usize;
    for token in [
        credentials.access_token.as_ref(),
        credentials.refresh_token.as_ref(),
        Some(&credentials.id_token),
    ]
    .into_iter()
    .flatten()
    {
        ensure!(
            !token.expose_secret().is_empty(),
            "credential token is empty"
        );
        bytes = bytes.saturating_add(token.expose_secret().len());
    }
    ensure!(
        credentials.scopes.len() <= 256,
        "credential state has too many scopes"
    );
    for scope in &credentials.scopes {
        ensure!(!scope.is_empty(), "credential scope is empty");
        bytes = bytes.saturating_add(scope.len());
    }
    Ok(bytes)
}

// Only private wire adapters can serialize a secret. Both owned token copies and
// complete JSON buffers are zeroed on drop, including parse and write failures.
pub(crate) struct WireSecret(pub(crate) SecretString);

impl Serialize for WireSecret {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(self.0.expose_secret())
    }
}

impl<'de> Deserialize<'de> for WireSecret {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        String::deserialize(deserializer).map(|value| Self(SecretString::new(value)))
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WireAuth {
    version: u8,
    host_id: String,
    registrations: Vec<WireRegistration>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireRegistration {
    id: String,
    issuer: String,
    subject: String,
    client_id: String,
    email: Option<String>,
    credentials: Option<WireCredentials>,
    refresh_pending: bool,
    #[serde(default)]
    authorization_revision: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireCredentials {
    access_token: Option<WireSecret>,
    refresh_token: Option<WireSecret>,
    id_token: WireSecret,
    scopes: Vec<String>,
    expires_at: Option<u64>,
    earliest_refresh_at: Option<u64>,
}

impl WireAuth {
    pub(crate) fn from_state(state: &StoredAuth) -> Self {
        Self {
            version: 1,
            host_id: state.host_id.clone(),
            registrations: state
                .registrations
                .iter()
                .map(|registration| WireRegistration {
                    id: registration.id.clone(),
                    issuer: registration.issuer.clone(),
                    subject: registration.subject.clone(),
                    client_id: registration.client_id.clone(),
                    email: registration.email.clone(),
                    credentials: registration.credentials.as_ref().map(|credentials| {
                        WireCredentials {
                            access_token: credentials.access_token.clone().map(WireSecret),
                            refresh_token: credentials.refresh_token.clone().map(WireSecret),
                            id_token: WireSecret(credentials.id_token.clone()),
                            scopes: credentials.scopes.clone(),
                            expires_at: credentials.expires_at,
                            earliest_refresh_at: credentials.earliest_refresh_at,
                        }
                    }),
                    refresh_pending: registration.refresh_pending,
                    authorization_revision: registration.authorization_revision,
                })
                .collect(),
        }
    }

    pub(crate) fn into_state(self) -> Result<StoredAuth> {
        ensure!(self.version == 1, "unsupported credential state version");
        let state = StoredAuth {
            host_id: self.host_id,
            registrations: self
                .registrations
                .into_iter()
                .map(|registration| StoredRegistration {
                    id: registration.id,
                    issuer: registration.issuer,
                    subject: registration.subject,
                    client_id: registration.client_id,
                    email: registration.email,
                    credentials: registration
                        .credentials
                        .map(|credentials| StoredCredentials {
                            access_token: credentials.access_token.map(|token| token.0),
                            refresh_token: credentials.refresh_token.map(|token| token.0),
                            id_token: credentials.id_token.0,
                            scopes: credentials.scopes,
                            expires_at: credentials.expires_at,
                            earliest_refresh_at: credentials.earliest_refresh_at,
                        }),
                    refresh_pending: registration.refresh_pending,
                    authorization_revision: registration.authorization_revision,
                })
                .collect(),
        };
        validate_state(&state)?;
        Ok(state)
    }
}

pub(crate) fn encode(state: &StoredAuth) -> Result<Zeroizing<Vec<u8>>> {
    validate_state(state)?;
    let mut out = BoundedJson::new(MAX_STORE_BYTES, "credential state exceeds 1 MiB");
    serde_json::to_writer(&mut out, &WireAuth::from_state(state))
        .map_err(|_| anyhow::anyhow!("credential state exceeds supported JSON size"))?;
    Ok(out.into_bytes())
}
pub(crate) fn decode(bytes: &[u8]) -> Result<StoredAuth> {
    ensure!(
        bytes.len() <= MAX_STORE_BYTES,
        "credential state exceeds 1 MiB"
    );
    let wire: WireAuth = serde_json::from_slice(bytes)
        .map_err(|_| anyhow::anyhow!("credential state contains invalid JSON"))?;
    wire.into_state()
}
pub(crate) fn is_barrier(bytes: &[u8]) -> bool {
    serde_json::from_slice::<WireAuth>(bytes)
        .is_ok_and(|wire| wire.version == 2 && wire.registrations.is_empty())
}
pub(crate) fn barrier(host_id: &str) -> Result<Vec<u8>> {
    Ok(serde_json::to_vec(&WireAuth {
        version: 2,
        host_id: host_id.into(),
        registrations: Vec::new(),
    })?)
}
