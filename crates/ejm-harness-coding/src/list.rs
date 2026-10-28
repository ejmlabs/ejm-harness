//! The `list_files` tool: one directory, bounded in entries and output bytes.
use std::{fmt::Write as _, path::Path};

use anyhow::Result;
use cap_std::fs::Dir;
use ejm_harness::ToolOutput;
use serde_json::{Value, json};

use super::{
    BoundedLines,
    path::{protected_path, relative_path, resolve_read_path},
};

const MAX_LIST_ENTRIES: usize = 200;

pub(super) fn list_files(root: &Dir, arguments: &Value) -> Result<ToolOutput> {
    let path = relative_path(arguments, Some("."))?;
    let directory = root.open_dir(resolve_read_path(root, &path)?)?;
    let mut entries = BoundedLines::default();
    let mut limit_reached = None;
    for entry in directory.entries()? {
        if entries.lines.len() >= MAX_LIST_ENTRIES {
            limit_reached = Some("200-entry limit");
            break;
        }
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if protected_path(Path::new(&name)) {
            continue;
        }
        let suffix = if entry.file_type()?.is_dir() { "/" } else { "" };
        if !entries.push(format!("{name}{suffix}")) {
            limit_reached = Some("14 KiB output limit");
            break;
        }
    }
    entries.lines.sort();
    let mut content = entries.lines.join("\n");
    if let Some(reason) = limit_reached {
        write!(
            content,
            "\n\n[Listing incomplete: {reason} reached. Narrow path to inspect a subdirectory.]"
        )?;
    }
    Ok(ToolOutput {
        content,
        data: json!({"path":path,"truncated":limit_reached.is_some()}),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Context;
    use cap_std::ambient_authority;

    use crate::MAX_RETRIEVAL_BYTES;

    #[test]
    fn directory_listing_limit_is_visible_with_bounded_entries() -> Result<()> {
        let temp = tempfile::tempdir()?;
        for index in 0..201 {
            std::fs::write(temp.path().join(format!("item-{index:03}")), "")?;
        }
        let root = Dir::open_ambient_dir(temp.path(), ambient_authority())?;
        let output = list_files(&root, &json!({}))?;

        assert_eq!(output.data["truncated"], true);
        assert_eq!(
            output
                .content
                .lines()
                .filter(|line| line.starts_with("item-"))
                .count(),
            200
        );
        assert!(
            output
                .content
                .contains("Listing incomplete: 200-entry limit")
        );
        assert!(output.content.contains("Narrow path"));
        Ok(())
    }

    #[test]
    fn directory_listing_output_limit_preserves_the_notice_and_whole_names() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let suffix = "界".repeat(60);
        for index in 0..200 {
            std::fs::write(temp.path().join(format!("item-{index:03}-{suffix}")), "")?;
        }
        let root = Dir::open_ambient_dir(temp.path(), ambient_authority())?;
        let output = list_files(&root, &json!({}))?;
        let (listing, notice) = output
            .content
            .split_once("\n\n")
            .context("missing listing notice")?;

        assert_eq!(output.data["truncated"], true);
        assert!(listing.len() <= MAX_RETRIEVAL_BYTES);
        assert!((1..200).contains(&listing.lines().count()));
        assert!(
            listing
                .lines()
                .all(|line| line.starts_with("item-") && line.ends_with('界'))
        );
        assert!(notice.contains("Listing incomplete: 14 KiB output limit"));
        assert!(output.content.len() <= 16 * 1024);
        Ok(())
    }
}
