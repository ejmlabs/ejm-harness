use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

use crate::{SecretString, StoredAuth};

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

    pub(super) fn validate(&self) -> Result<()> {
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

#[derive(Clone, Default)]
pub(super) struct VaultState {
    pub(super) provider: Option<StoredAuth>,
    pub(super) secrets: BTreeMap<SecretId, SecretString>,
    pub(super) markers: BTreeSet<SecretId>,
    pub(super) migration: Migration,
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Migration {
    #[default]
    None,
    Prepared,
    Complete,
}
