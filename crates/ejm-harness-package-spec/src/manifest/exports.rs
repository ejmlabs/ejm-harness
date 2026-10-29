use std::collections::HashSet;

use anyhow::{Result, ensure};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::names::{validate_identifier, validate_relative_path};

const MAX_EXPORTS: usize = 64;

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

impl Exports {
    /// Bound the export count and require a runtime for any code export.
    pub(super) fn validate_shape(&self, has_runtime: bool) -> Result<()> {
        let total = self.tools.len()
            + self.hooks.len()
            + self.providers.len()
            + self.commands.len()
            + self.widgets.len()
            + self.themes.len()
            + self.skills.len()
            + self.prompts.len();
        ensure!(total <= MAX_EXPORTS, "package exceeds 64 exports");
        let has_code = !self.tools.is_empty()
            || !self.hooks.is_empty()
            || !self.providers.is_empty()
            || !self.commands.is_empty()
            || !self.widgets.is_empty();
        ensure!(has_runtime || !has_code, "code exports require a runtime");
        Ok(())
    }

    /// Validate each export's identifiers, descriptions, uniqueness, and asset paths.
    pub(super) fn validate_entries(&self) -> Result<()> {
        let mut names = HashSet::new();
        for tool in &self.tools {
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
        for (kind, named) in [("provider", &self.providers), ("widget", &self.widgets)] {
            for export in named {
                validate_export(&export.name, &export.handler, &mut names, kind)?;
            }
        }
        for command in &self.commands {
            validate_export(&command.name, &command.handler, &mut names, "command")?;
            ensure!(
                !command.description.is_empty() && command.description.len() <= 256,
                "invalid command description"
            );
        }
        let mut hooks = HashSet::new();
        for hook in &self.hooks {
            validate_identifier(&hook.handler)?;
            ensure!(hooks.insert(hook.event), "duplicate hook event");
        }
        for path in self.themes.iter().chain(&self.skills).chain(&self.prompts) {
            validate_relative_path(path)?;
        }
        Ok(())
    }
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
