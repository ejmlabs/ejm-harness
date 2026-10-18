use std::collections::HashSet;

use anyhow::{Result, ensure};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::MAX_MANIFEST_BYTES;

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

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Exports {
    pub tools: Vec<ToolExport>,
    pub hooks: Vec<HookExport>,
    pub providers: Vec<NamedExport>,
    pub commands: Vec<CommandExport>,
    pub themes: Vec<String>,
    pub widgets: Vec<NamedExport>,
    pub skills: Vec<String>,
    pub prompts: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ToolExport {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub effect: ToolEffect,
    pub version: String,
    pub handler: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ToolEffect {
    Read,
    Write,
    External,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NamedExport {
    pub name: String,
    pub handler: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CommandExport {
    pub name: String,
    pub description: String,
    pub handler: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HookExport {
    pub event: HookEvent,
    pub handler: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HookEvent {
    RunStart,
    BeforeModel,
    BeforeTool,
    AfterTool,
    RunEnd,
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
        ensure!(
            serde_json::to_vec(self)?.len() <= MAX_MANIFEST_BYTES,
            "package manifest exceeds 16 KiB"
        );
        let exports = &self.exports;
        let total = exports.tools.len()
            + exports.hooks.len()
            + exports.providers.len()
            + exports.commands.len()
            + exports.widgets.len()
            + exports.themes.len()
            + exports.skills.len()
            + exports.prompts.len();
        ensure!(total <= 64, "package exceeds 64 exports");
        let has_code = !exports.tools.is_empty()
            || !exports.hooks.is_empty()
            || !exports.providers.is_empty()
            || !exports.commands.is_empty()
            || !exports.widgets.is_empty();
        ensure!(
            !has_code || self.runtime.is_some(),
            "code exports require a runtime"
        );
        if let Some(runtime) = &self.runtime {
            validate_relative_path(&runtime.entry)?;
        }
        let mut names = HashSet::new();
        for tool in &exports.tools {
            validate_export(&tool.name, &tool.handler, &mut names, "tool")?;
            ensure!(
                !tool.description.is_empty() && tool.description.len() <= 1024,
                "invalid tool description"
            );
            ensure!(
                tool.input_schema.is_object(),
                "tool input_schema must be an object"
            );
            semver::Version::parse(&tool.version)?;
        }
        for (kind, named) in [
            ("provider", &exports.providers),
            ("widget", &exports.widgets),
        ] {
            for export in named {
                validate_export(&export.name, &export.handler, &mut names, kind)?;
            }
        }
        for command in &exports.commands {
            validate_export(&command.name, &command.handler, &mut names, "command")?;
            ensure!(
                !command.description.is_empty() && command.description.len() <= 256,
                "invalid command description"
            );
        }
        let mut hooks = HashSet::new();
        for hook in &exports.hooks {
            validate_identifier(&hook.handler)?;
            ensure!(hooks.insert(hook.event), "duplicate hook event");
        }
        for path in exports
            .themes
            .iter()
            .chain(&exports.skills)
            .chain(&exports.prompts)
        {
            validate_relative_path(path)?;
        }
        Ok(())
    }
}

/// Validate the registry's ASCII publisher and package namespace.
///
/// # Errors
/// Requires a scoped name such as `@publisher/package` with bounded lowercase components.
pub fn validate_name(name: &str) -> Result<()> {
    let Some((publisher, package)) = name
        .strip_prefix('@')
        .and_then(|value| value.split_once('/'))
    else {
        anyhow::bail!("package name must be @publisher/package");
    };
    for component in [publisher, package] {
        ensure!(
            !component.is_empty() && component.len() <= 48,
            "invalid package name length"
        );
        ensure!(
            component.as_bytes()[0].is_ascii_lowercase(),
            "package name must start with a letter"
        );
        ensure!(
            component
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'),
            "package names use lowercase letters, digits, and hyphens"
        );
        ensure!(
            !component.ends_with('-'),
            "package name cannot end in a hyphen"
        );
    }
    Ok(())
}

/// Validate a portable archive path before creating any files.
///
/// # Errors
/// Rejects absolute paths, traversal, hidden components, backslashes, and Windows aliases.
pub fn validate_relative_path(path: &str) -> Result<()> {
    ensure!(
        !path.is_empty() && path.len() <= 240 && !path.contains(['\\', ':', '\0']),
        "invalid package path"
    );
    for part in path.split('/') {
        ensure!(
            !part.is_empty()
                && part != "."
                && part != ".."
                && !part.starts_with('.')
                && !part.ends_with(['.', ' ']),
            "unsafe package path"
        );
        ensure!(
            part.chars()
                .all(|character| character.is_ascii_alphanumeric() || "-_ .@+".contains(character)),
            "package path contains unsupported characters"
        );
        let stem = part
            .split('.')
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase();
        ensure!(
            !matches!(
                stem.as_str(),
                "CON"
                    | "PRN"
                    | "AUX"
                    | "NUL"
                    | "COM1"
                    | "COM2"
                    | "COM3"
                    | "COM4"
                    | "COM5"
                    | "COM6"
                    | "COM7"
                    | "COM8"
                    | "COM9"
                    | "LPT1"
                    | "LPT2"
                    | "LPT3"
                    | "LPT4"
                    | "LPT5"
                    | "LPT6"
                    | "LPT7"
                    | "LPT8"
                    | "LPT9"
            ),
            "reserved package path"
        );
    }
    Ok(())
}

fn validate_export(
    name: &str,
    handler: &str,
    names: &mut HashSet<String>,
    kind: &str,
) -> Result<()> {
    validate_identifier(name)?;
    validate_identifier(handler)?;
    ensure!(
        names.insert(format!("{kind}:{name}")),
        "duplicate package export"
    );
    Ok(())
}

fn validate_identifier(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.len() <= 64
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')),
        "invalid export identifier"
    );
    Ok(())
}
