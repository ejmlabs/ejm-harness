use std::{collections::HashSet, io::Read};

use anyhow::{Context, Result, ensure};
use flate2::read::MultiGzDecoder;
use sha2::{Digest, Sha256};

use crate::{
    MANIFEST_FILE, MAX_ARCHIVE_BYTES, MAX_ARCHIVE_FILES, MAX_EXPANDED_BYTES, MAX_MANIFEST_BYTES,
    Manifest, validate_relative_path,
};

#[derive(Debug)]
pub struct ArchiveFile {
    pub path: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug)]
pub struct PackageArchive {
    pub manifest: Manifest,
    pub files: Vec<ArchiveFile>,
}

/// Compute the archive's immutable content address with the maintained SHA-256 implementation.
#[must_use]
pub fn archive_sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Validate a compressed package without executing it or touching the filesystem.
///
/// # Errors
/// Rejects invalid manifests, decompression bombs, links, traversal, duplicates, and missing assets.
pub fn validate_archive(bytes: &[u8]) -> Result<Manifest> {
    Ok(read_archive(bytes)?.manifest)
}

/// Read bounded regular files from a package. Archive permission bits are never applied.
///
/// # Errors
/// Rejects invalid manifests, decompression bombs, links, traversal, duplicates, and missing assets.
pub fn read_archive(bytes: &[u8]) -> Result<PackageArchive> {
    ensure!(
        bytes.len() <= MAX_ARCHIVE_BYTES,
        "package archive exceeds 8 MiB"
    );
    let decoded = MultiGzDecoder::new(bytes).take(u64::try_from(MAX_EXPANDED_BYTES)? + 1);
    let mut archive = tar::Archive::new(decoded);
    let mut paths = HashSet::new();
    let mut files = Vec::new();
    for entry in archive.entries()? {
        let mut entry = entry?;
        ensure!(
            paths.len() < MAX_ARCHIVE_FILES,
            "package exceeds 1024 entries"
        );
        let path = std::str::from_utf8(&entry.path_bytes())?
            .trim_end_matches('/')
            .to_owned();
        validate_relative_path(&path)?;
        ensure!(
            paths.insert(path.to_ascii_lowercase()),
            "duplicate or case-conflicting archive path"
        );
        let kind = entry.header().entry_type();
        ensure!(
            kind.is_file() || kind.is_dir(),
            "only regular files and directories are allowed"
        );
        if kind.is_dir() {
            continue;
        }
        let size = usize::try_from(entry.size())?;
        ensure!(
            size <= MAX_EXPANDED_BYTES,
            "package file exceeds expanded limit"
        );
        ensure!(
            path != MANIFEST_FILE || size <= MAX_MANIFEST_BYTES,
            "package manifest exceeds 16 KiB"
        );
        let mut content = Vec::with_capacity(size);
        entry.read_to_end(&mut content)?;
        ensure!(content.len() == size, "truncated package entry");
        files.push(ArchiveFile {
            path,
            bytes: content,
        });
    }
    let mut decoded = archive.into_inner();
    std::io::copy(&mut decoded, &mut std::io::sink())?;
    ensure!(decoded.limit() > 0, "expanded package exceeds 32 MiB");
    let regular_paths: HashSet<_> = files
        .iter()
        .map(|file| file.path.to_ascii_lowercase())
        .collect();
    for file in &files {
        let lowered = file.path.to_ascii_lowercase();
        for (index, _) in lowered.match_indices('/') {
            ensure!(
                !regular_paths.contains(&lowered[..index]),
                "archive file is also a directory"
            );
        }
    }
    let manifest_bytes = &files
        .iter()
        .find(|file| file.path == MANIFEST_FILE)
        .context("archive has no root ejm.package.json")?
        .bytes;
    let manifest = Manifest::from_bytes(manifest_bytes)?;
    let mut assets: Vec<&str> = manifest
        .exports
        .themes
        .iter()
        .chain(&manifest.exports.skills)
        .chain(&manifest.exports.prompts)
        .map(String::as_str)
        .collect();
    if let Some(runtime) = &manifest.runtime {
        assets.push(&runtime.entry);
    }
    for asset in assets {
        ensure!(
            files.iter().any(|file| file.path == asset),
            "declared asset is absent: {asset}"
        );
    }
    Ok(PackageArchive { manifest, files })
}
