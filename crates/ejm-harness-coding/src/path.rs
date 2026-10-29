//! Path safety: every tool path stays inside the workspace and away from protected metadata.
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, ensure};
use cap_std::fs::Dir;
use serde_json::Value;

#[cfg(test)]
mod tests;

/// Reads the `path` argument, falling back to `default`, and rejects anything that is not a
/// plain workspace-relative path.
pub(super) fn relative_path(arguments: &Value, default: Option<&str>) -> Result<PathBuf> {
    let text = arguments["path"]
        .as_str()
        .or(default)
        .context("path is required")?;
    ensure!(text.len() <= 4096 && !text.contains('\0'), "invalid path");
    let path = PathBuf::from(text);
    ensure!(
        !path.is_absolute()
            && !path.components().any(|c| matches!(
                c,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )),
        "path must stay inside the workspace"
    );
    Ok(path)
}

/// Matches `.git` and `.ejm-harness` in any component, case-insensitively.
pub(super) fn protected_path(path: &Path) -> bool {
    path.components().any(|component| {
        let name = component.as_os_str().to_string_lossy();
        name.eq_ignore_ascii_case(".git") || name.eq_ignore_ascii_case(".ejm-harness")
    })
}

/// Canonicalizes a path for reading through the workspace capability. Symlinks that lead into
/// protected metadata are rejected after resolution.
pub(super) fn resolve_read_path(root: &Dir, path: &Path) -> Result<PathBuf> {
    ensure!(
        !protected_path(path),
        "tool cannot read repository metadata or harness state"
    );
    let resolved = root.canonicalize(path)?;
    ensure!(
        !protected_path(&resolved),
        "resolved path is protected metadata"
    );
    Ok(resolved)
}

/// Checks the resolved parent and refuses any symbolic link along the path being written.
pub(super) fn validate_edit_path(root: &Dir, path: &Path) -> Result<()> {
    ensure!(
        !protected_path(path),
        "tool cannot edit repository metadata or harness state"
    );
    let parent = root.canonicalize(
        path.parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or(Path::new(".")),
    )?;
    ensure!(
        !protected_path(&parent),
        "resolved parent is protected metadata"
    );
    let mut prefix = PathBuf::new();
    for component in path.components() {
        prefix.push(component);
        match root.symlink_metadata(&prefix) {
            Ok(metadata) => ensure!(
                !metadata.file_type().is_symlink(),
                "write paths cannot contain symbolic links"
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && prefix == path => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}
