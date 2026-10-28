//! The `search` tool: a bounded regular expression scan over gitignore-aware text files.
use std::{fmt::Write as _, io::Read, path::Path};

use anyhow::{Context, Result, ensure};
use cap_std::fs::Dir;
use ejm_harness::ToolOutput;
use regex::{Regex, RegexBuilder};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use super::{
    BoundedLines,
    path::{protected_path, relative_path, resolve_read_path},
};

#[cfg(test)]
mod tests;

const MAX_SEARCH_BYTES: usize = 8 * 1024 * 1024;
const MAX_SEARCH_DEPTH: usize = 32;
const MAX_SEARCH_ENTRIES: usize = 20_000;
const MAX_SEARCH_FILE_BYTES: usize = 256 * 1024;
const MAX_MATCH_CHARS: usize = 240;

/// Counters and hits for one scan, including why it stopped early.
#[derive(Default)]
struct Scan {
    hits: BoundedLines,
    bytes_read: usize,
    visited: usize,
    skipped: usize,
    limit_reached: Option<&'static str>,
}

pub(super) fn search(
    root: &Dir,
    cwd: &Path,
    arguments: &Value,
    cancel: &CancellationToken,
) -> Result<ToolOutput> {
    let path = resolve_read_path(root, &relative_path(arguments, Some("."))?)?;
    let regex = compile_pattern(arguments)?;
    let limit = arguments["limit"].as_u64().unwrap_or(50).clamp(1, 100) as usize;
    // The ambient walk only discovers names. Every file read is re-opened through the capability.
    let start = cwd.join(&path);
    let resolved = std::fs::canonicalize(&start)?;
    ensure!(resolved.starts_with(cwd), "search path escapes workspace");
    let walker = ignore::WalkBuilder::new(start)
        .follow_links(false)
        .max_depth(Some(MAX_SEARCH_DEPTH))
        .build();
    let mut scan = Scan::default();
    for entry in walker {
        ensure!(!cancel.is_cancelled(), "cancelled");
        scan.visited += 1;
        if scan.visited > MAX_SEARCH_ENTRIES {
            scan.limit_reached = Some("20000-entry scan limit");
            break;
        }
        if scan.bytes_read >= MAX_SEARCH_BYTES {
            scan.limit_reached = Some("8 MiB scan limit");
            break;
        }
        let Ok(entry) = entry else {
            scan.skipped += 1;
            continue;
        };
        let relative = entry.path().strip_prefix(cwd)?;
        if protected_path(relative) {
            continue;
        }
        if entry.file_type().is_some_and(|t| t.is_dir()) && entry.depth() == MAX_SEARCH_DEPTH {
            scan.limit_reached.get_or_insert("32-level depth limit");
        }
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let text = read_scannable(root, relative, &mut scan.bytes_read)?;
        if scan.bytes_read == MAX_SEARCH_BYTES {
            scan.limit_reached = Some("8 MiB scan limit");
        }
        let Some(text) = text else {
            scan.skipped += 1;
            continue;
        };
        if let Some(reason) = scan.collect_hits(&text, relative, &regex, limit) {
            scan.limit_reached = Some(reason);
            break;
        }
    }
    scan.into_output()
}

fn compile_pattern(arguments: &Value) -> Result<Regex> {
    let pattern = arguments["pattern"]
        .as_str()
        .context("pattern is required")?;
    ensure!(pattern.len() <= 1024, "pattern exceeds limit");
    RegexBuilder::new(pattern)
        .size_limit(1024 * 1024)
        .build()
        .context("invalid regular expression")
}

/// Reads one candidate through the workspace capability. `None` means skip it: it is protected
/// after resolution, unreadable, over [`MAX_SEARCH_FILE_BYTES`], or not UTF-8.
fn read_scannable(root: &Dir, relative: &Path, bytes_read: &mut usize) -> Result<Option<String>> {
    let Ok(resolved) = resolve_read_path(root, relative) else {
        return Ok(None);
    };
    let Ok(file) = root.open(resolved) else {
        return Ok(None);
    };
    if file.metadata()?.len() > MAX_SEARCH_FILE_BYTES as u64 {
        return Ok(None);
    }
    let mut bytes = vec![];
    file.take((MAX_SEARCH_FILE_BYTES + 1).min(MAX_SEARCH_BYTES - *bytes_read) as u64)
        .read_to_end(&mut bytes)?;
    *bytes_read += bytes.len();
    Ok(String::from_utf8(bytes).ok())
}

impl Scan {
    /// Records matching lines of `text`. Returns the reason when a limit ends the scan.
    fn collect_hits(
        &mut self,
        text: &str,
        relative: &Path,
        regex: &Regex,
        limit: usize,
    ) -> Option<&'static str> {
        for (index, line) in text.lines().enumerate().filter(|(_, l)| regex.is_match(l)) {
            let line: String = line.chars().take(MAX_MATCH_CHARS).collect();
            if !self
                .hits
                .push(format!("{}:{}:{line}", relative.display(), index + 1))
            {
                return Some("14 KiB output limit");
            }
            if self.hits.lines.len() == limit {
                return Some("match limit");
            }
        }
        None
    }

    fn into_output(self) -> Result<ToolOutput> {
        let Self {
            hits,
            bytes_read,
            visited,
            skipped,
            limit_reached,
        } = self;
        let matches = hits.lines;
        let mut content = if !matches.is_empty() {
            matches.join("\n")
        } else if limit_reached.is_some() || skipped > 0 {
            "No matches in the scanned text.".into()
        } else {
            "No matches.".into()
        };
        if let Some(reason) = limit_reached {
            write!(
                content,
                "\n\n[Search incomplete: {reason} reached. Narrow path or pattern.]"
            )?;
        }
        if skipped > 0 {
            write!(
                content,
                "\n\n[Skipped {skipped} entries: unreadable, over 256 KiB, or non-UTF-8. Results cover only scanned text; use read_file for a known text file.]"
            )?;
        }
        Ok(ToolOutput {
            content,
            data: json!({"matches":matches.len(),"bytes_read":bytes_read,"visited":visited,"skipped":skipped,"truncated":limit_reached.is_some()}),
            ..Default::default()
        })
    }
}
