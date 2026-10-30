use std::{
    collections::{BTreeSet, HashSet},
    path::PathBuf,
};

use anyhow::{Result, ensure};
use ejm_harness_package_spec::{HookEvent, Manifest, RuntimeKind};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A validated installation. Loading it requires a separate, explicit host grant.
#[derive(Clone, Debug)]
pub struct InstalledPackage {
    pub manifest: Manifest,
    pub root: PathBuf,
}

impl InstalledPackage {
    pub(super) fn kind(&self) -> Option<RuntimeKind> {
        self.manifest.runtime.as_ref().map(|runtime| runtime.kind)
    }
}

/// Host-owned registration allowlists. They do not sandbox trusted native code.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct PackageGrants {
    pub tools: Vec<String>,
    pub hooks: Vec<HookEvent>,
    pub providers: Vec<String>,
    pub commands: Vec<String>,
    pub widgets: Vec<String>,
}

impl PackageGrants {
    pub(crate) fn validate(&self, manifest: &Manifest) -> Result<()> {
        let exports = &manifest.exports;
        validate_named(&self.tools, exports.tools.iter().map(|export| &export.name))?;
        validate_named(
            &self.providers,
            exports.providers.iter().map(|export| &export.name),
        )?;
        validate_named(
            &self.commands,
            exports.commands.iter().map(|export| &export.name),
        )?;
        validate_named(
            &self.widgets,
            exports.widgets.iter().map(|export| &export.name),
        )?;
        let mut hooks = HashSet::new();
        for event in &self.hooks {
            ensure!(
                exports.hooks.iter().any(|hook| hook.event == *event),
                "hook grant is not declared"
            );
            ensure!(hooks.insert(event), "duplicate hook grant");
        }
        Ok(())
    }

    /// Handler names of every granted export, sorted and deduplicated.
    pub(super) fn handlers(&self, manifest: &Manifest) -> Vec<String> {
        let exports = &manifest.exports;
        let mut handlers = BTreeSet::new();
        handlers.extend(
            exports
                .tools
                .iter()
                .filter(|export| self.tools.contains(&export.name))
                .map(|export| &export.handler),
        );
        handlers.extend(
            exports
                .hooks
                .iter()
                .filter(|export| self.hooks.contains(&export.event))
                .map(|export| &export.handler),
        );
        handlers.extend(
            exports
                .providers
                .iter()
                .filter(|export| self.providers.contains(&export.name))
                .map(|export| &export.handler),
        );
        handlers.extend(
            exports
                .commands
                .iter()
                .filter(|export| self.commands.contains(&export.name))
                .map(|export| &export.handler),
        );
        handlers.extend(
            exports
                .widgets
                .iter()
                .filter(|export| self.widgets.contains(&export.name))
                .map(|export| &export.handler),
        );
        handlers.into_iter().cloned().collect()
    }
}

fn validate_named<'a>(grants: &[String], declared: impl Iterator<Item = &'a String>) -> Result<()> {
    let declared: HashSet<_> = declared.collect();
    let mut unique = HashSet::new();
    for grant in grants {
        ensure!(
            declared.contains(grant),
            "grant is not declared in package manifest: {grant}"
        );
        ensure!(unique.insert(grant), "duplicate package grant");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn manifest() -> Result<Manifest> {
        let tool = |name: &str| {
            json!({
                "name": name, "description": "Fixture", "input_schema": {"type": "object"},
                "effect": "read", "version": "1.0.0", "handler": "shared"
            })
        };
        Manifest::from_bytes(&serde_json::to_vec(&json!({
            "schema_version": 1, "name": "@example/grants", "version": "1.0.0",
            "description": "Grant fixture",
            "runtime": {"kind": "node", "entry": "index.mjs"},
            "exports": {
                "tools": [tool("alpha"), tool("beta")],
                "hooks": [{"event": "run_start", "handler": "start"}],
                "commands": [{"name": "gamma", "description": "Fixture", "handler": "cmd"}]
            }
        }))?)
    }

    fn rejection(result: Result<()>) -> String {
        result
            .err()
            .map_or_else(|| "accepted".into(), |error| error.to_string())
    }

    #[test]
    fn handlers_are_sorted_and_deduplicated() -> Result<()> {
        let grants = PackageGrants {
            tools: vec!["alpha".into(), "beta".into()],
            hooks: vec![HookEvent::RunStart],
            commands: vec!["gamma".into()],
            ..PackageGrants::default()
        };
        assert_eq!(grants.handlers(&manifest()?), ["cmd", "shared", "start"]);
        Ok(())
    }

    #[test]
    fn grants_must_be_declared_and_unique() -> Result<()> {
        let manifest = manifest()?;
        let tools = |names: &[&str]| PackageGrants {
            tools: names.iter().map(|name| (*name).into()).collect(),
            ..PackageGrants::default()
        };
        assert_eq!(
            rejection(tools(&["alpha", "beta"]).validate(&manifest)),
            "accepted"
        );
        assert_eq!(
            rejection(tools(&["missing"]).validate(&manifest)),
            "grant is not declared in package manifest: missing"
        );
        assert_eq!(
            rejection(tools(&["alpha", "alpha"]).validate(&manifest)),
            "duplicate package grant"
        );
        let hooks = |events: Vec<HookEvent>| PackageGrants {
            hooks: events,
            ..PackageGrants::default()
        };
        assert_eq!(
            rejection(hooks(vec![HookEvent::RunEnd]).validate(&manifest)),
            "hook grant is not declared"
        );
        assert_eq!(
            rejection(hooks(vec![HookEvent::RunStart; 2]).validate(&manifest)),
            "duplicate hook grant"
        );
        Ok(())
    }
}
