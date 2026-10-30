//! The `write_file` and `patch_file` tools: SHA-256 preconditions and atomic replacement.
use std::{io::Write, path::Path};

use anyhow::{Context, Result, ensure};
use cap_std::fs::{Dir, File, OpenOptions};
use ejm_harness::ToolOutput;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use super::{
    path::{relative_path, validate_edit_path},
    read::{MAX_FILE_BYTES, read_bounded, sha256_hex},
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Edit {
    Write,
    Patch,
}

pub(super) fn edit(
    root: &Dir,
    arguments: &Value,
    mode: Edit,
    cancel: &CancellationToken,
) -> Result<ToolOutput> {
    let path = relative_path(arguments, None)?;
    validate_edit_path(root, &path)?;
    let current = read_existing(root, &path)?;
    let replaced_hash = verify_precondition(
        current.as_deref(),
        arguments["expected_hash"].as_str(),
        mode,
    )?;
    let content = replacement_content(current.as_deref(), arguments, mode)?;
    ensure!(
        content.len() as u64 <= MAX_FILE_BYTES,
        "replacement exceeds file limit"
    );
    ensure!(!cancel.is_cancelled(), "cancelled before write");
    match replaced_hash {
        Some(expected) => replace_file(root, &path, &content, expected, cancel)?,
        None => create_file(root, &path, &content)?,
    }
    let verb = match mode {
        Edit::Write => "Wrote",
        Edit::Patch => "Patched",
    };
    Ok(ToolOutput {
        content: format!("{verb} {} ({} bytes)", path.display(), content.len()),
        data: json!({"path":path,"sha256":sha256_hex(content.as_bytes()),"bytes":content.len()}),
        ..Default::default()
    })
}

/// The file's bytes, or `None` when it does not exist yet.
fn read_existing(root: &Dir, path: &Path) -> Result<Option<Vec<u8>>> {
    match read_bounded(root, path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error)
            if error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) =>
        {
            Ok(None)
        }
        Err(error) => Err(error),
    }
}

/// Never overwrite on a guess: an existing file needs the hash of its current bytes, and a
/// missing file needs none. Returns the verified hash when a file is being replaced.
fn verify_precondition<'a>(
    current: Option<&[u8]>,
    expected: Option<&'a str>,
    mode: Edit,
) -> Result<Option<&'a str>> {
    if let Some(bytes) = current {
        ensure!(
            expected == Some(sha256_hex(bytes).as_str()),
            "file changed or expected_hash is missing; read it before editing"
        );
        Ok(expected)
    } else {
        ensure!(
            expected.is_none() && mode == Edit::Write,
            "expected file is missing; read workspace state again"
        );
        Ok(None)
    }
}

fn replacement_content(current: Option<&[u8]>, arguments: &Value, mode: Edit) -> Result<String> {
    match mode {
        Edit::Write => Ok(arguments["content"]
            .as_str()
            .context("content is required")?
            .to_owned()),
        Edit::Patch => {
            let original = std::str::from_utf8(current.context("file does not exist")?)
                .context("file is not UTF-8")?;
            let old = arguments["old"].as_str().context("old is required")?;
            let new = arguments["new"].as_str().context("new is required")?;
            ensure!(
                !old.is_empty() && original.match_indices(old).count() == 1,
                "old text must match exactly once"
            );
            Ok(original.replacen(old, new, 1))
        }
    }
}

fn create_new(root: &Dir, path: &Path) -> std::io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    root.open_with(path, &options)
}

fn create_file(root: &Dir, path: &Path, content: &str) -> Result<()> {
    let mut file =
        create_new(root, path).context("file already exists or parent is unavailable")?;
    file.write_all(content.as_bytes())?;
    file.sync_all()?;
    Ok(())
}

/// Replaces `path` through a sibling temporary file, removing it if the replacement fails.
fn replace_file(
    root: &Dir,
    path: &Path,
    content: &str,
    expected: &str,
    cancel: &CancellationToken,
) -> Result<()> {
    let temp = path
        .parent()
        .unwrap_or(Path::new("."))
        .join(format!(".ejm-harness-edit-{}", uuid::Uuid::new_v4()));
    let result = stage_and_swap(root, path, &temp, content, expected, cancel);
    if result.is_err() {
        let _ = root.remove_file(&temp);
    }
    result
}

/// Writes `temp`, then renames it over `path` only if `path` still has the expected hash.
fn stage_and_swap(
    root: &Dir,
    path: &Path,
    temp: &Path,
    content: &str,
    expected: &str,
    cancel: &CancellationToken,
) -> Result<()> {
    let mut file = create_new(root, temp)?;
    file.set_permissions(root.metadata(path)?.permissions())?;
    file.write_all(content.as_bytes())?;
    file.sync_all()?;
    ensure!(!cancel.is_cancelled(), "cancelled before replacement");
    ensure!(
        sha256_hex(&read_bounded(root, path)?) == expected,
        "file changed during edit; replacement stopped"
    );
    root.rename(temp, root, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cap_std::ambient_authority;

    #[test]
    fn stale_edits_and_ambiguous_patch_fail() -> Result<()> {
        let temp = tempfile::tempdir()?;
        std::fs::write(temp.path().join("sample"), "one two one\n")?;
        let root = Dir::open_ambient_dir(temp.path(), ambient_authority())?;
        let digest = sha256_hex(b"one two one\n");
        let cancel = CancellationToken::new();
        assert!(
            edit(
                &root,
                &json!({"path":"sample","content":"wrong","expected_hash":"0".repeat(64)}),
                Edit::Write,
                &cancel
            )
            .is_err()
        );
        assert!(
            edit(
                &root,
                &json!({"path":"sample","old":"one","new":"three","expected_hash":digest}),
                Edit::Patch,
                &cancel
            )
            .is_err()
        );
        edit(
            &root,
            &json!({"path":"sample","old":"two","new":"three","expected_hash":digest}),
            Edit::Patch,
            &cancel,
        )?;
        assert_eq!(
            std::fs::read_to_string(temp.path().join("sample"))?,
            "one three one\n"
        );
        Ok(())
    }
}
