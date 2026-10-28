use anyhow::{Result, ensure};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::MAX_MANIFEST_BYTES;

mod exports;
mod names;

pub use exports::{
    CommandExport, Exports, HookEvent, HookExport, NamedExport, ToolEffect, ToolExport,
};
pub use names::{validate_name, validate_relative_path};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema_version: u32,
    pub name: String,
    pub version: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime: Option<Runtime>,
    #[serde(default)]
    pub exports: Exports,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Runtime {
    pub kind: RuntimeKind,
    pub entry: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeKind {
    Node,
    Python,
}

impl Manifest {
    /// Parse and validate the bounded manifest, rejecting unknown or duplicate fields.
    ///
    /// # Errors
    /// Rejects unsupported schemas, names, versions, exports, paths, or oversized input.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        ensure!(
            bytes.len() <= MAX_MANIFEST_BYTES,
            "package manifest exceeds 16 KiB"
        );
        let manifest: Self = serde_json::from_slice(bytes)?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Validate the same contract used by local hosts and the public registry.
    ///
    /// # Errors
    /// Rejects unsupported schemas, names, versions, exports, paths, or oversized metadata.
    pub fn validate(&self) -> Result<()> {
        self.validate_identity()?;
        ensure!(
            serde_json::to_vec(self)?.len() <= MAX_MANIFEST_BYTES,
            "package manifest exceeds 16 KiB"
        );
        self.exports.validate_shape(self.runtime.is_some())?;
        if let Some(runtime) = &self.runtime {
            validate_relative_path(&runtime.entry)?;
        }
        self.exports.validate_entries()
    }

    fn validate_identity(&self) -> Result<()> {
        ensure!(self.schema_version == 1, "unsupported package schema");
        validate_name(&self.name)?;
        let parsed = semver::Version::parse(&self.version)?;
        ensure!(
            parsed.to_string() == self.version && parsed.build.is_empty(),
            "use canonical semver without build metadata"
        );
        ensure!(
            !self.description.is_empty()
                && self.description.len() <= 512
                && !self.description.chars().any(char::is_control),
            "invalid package description"
        );
        Ok(())
    }
}
