//! The `read_file` tool and the bounded, hashed file load the edit tools share.
use std::{fmt::Write as _, io::Read, path::Path};

use anyhow::{Context, Result, ensure};
use cap_std::fs::Dir;
use ejm_harness::ToolOutput;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::{
    MAX_RETRIEVAL_BYTES,
    path::{relative_path, resolve_read_path},
};

pub(super) const MAX_FILE_BYTES: u64 = 1024 * 1024;

pub(super) fn read_file(root: &Dir, arguments: &Value) -> Result<ToolOutput> {
    let path = relative_path(arguments, None)?;
    let bytes = read_bounded(root, &resolve_read_path(root, &path)?)?;
    let text = std::str::from_utf8(&bytes).context("file is not UTF-8 text")?;
    let offset = usize::try_from(arguments["offset"].as_u64().unwrap_or(1).max(1))
        .context("line offset exceeds platform range")?;
    let limit = arguments["limit"].as_u64().unwrap_or(200).min(400) as usize;
    let mut content = String::new();
    let mut emitted = 0;
    let total = text.lines().count();
    for (index, line) in text.lines().enumerate().skip(offset - 1).take(limit) {
        let rendered = format!("{}: {}\n", index + 1, line);
        if content.len() + rendered.len() > MAX_RETRIEVAL_BYTES {
            break;
        }
        content.push_str(&rendered);
        emitted += 1;
    }
    let digest = sha256_hex(&bytes);
    let truncated = offset.saturating_sub(1).saturating_add(emitted) < total;
    let more = if truncated {
        " | more lines available"
    } else {
        ""
    };
    write!(content, "\nsha256: {digest} | {total} lines{more}")?;
    Ok(ToolOutput {
        content,
        data: json!({"path":path,"sha256":digest,"lines":total,"offset":offset,"returned":emitted,"truncated":truncated}),
        ..Default::default()
    })
}

/// Reads a regular file of at most [`MAX_FILE_BYTES`] through the workspace capability.
pub(super) fn read_bounded(root: &Dir, path: &Path) -> Result<Vec<u8>> {
    let file = root
        .open(path)
        .with_context(|| format!("cannot open {}", path.display()))?;
    let metadata = file.metadata()?;
    ensure!(metadata.is_file(), "path is not a regular file");
    ensure!(
        metadata.len() <= MAX_FILE_BYTES,
        "file exceeds 1 MiB native-tool limit; use a scoped shell command if granted"
    );
    let mut bytes = vec![];
    file.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= MAX_FILE_BYTES,
        "file grew beyond the native-tool limit"
    );
    Ok(bytes)
}

pub(super) fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
