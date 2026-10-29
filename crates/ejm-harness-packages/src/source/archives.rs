use std::{
    io::{Cursor, Read, Write},
    path::Path,
};

use anyhow::{Context, Result, ensure};
use ejm_harness_package_spec::{
    ArchiveFile, MANIFEST_FILE, MAX_ARCHIVE_BYTES, MAX_ARCHIVE_FILES, MAX_EXPANDED_BYTES,
    validate_archive, validate_relative_path,
};
use flate2::{Compression, read::MultiGzDecoder, write::GzEncoder};

pub(super) fn tar(
    bytes: &[u8],
    prefix: Option<&str>,
    exclude_build_files: bool,
) -> Result<Vec<u8>> {
    let mut expanded = Vec::new();
    MultiGzDecoder::new(bytes)
        .take(u64::try_from(MAX_EXPANDED_BYTES)? + 1)
        .read_to_end(&mut expanded)?;
    ensure!(
        expanded.len() <= MAX_EXPANDED_BYTES,
        "source archive exceeds 32 MiB expanded"
    );
    let mut archive = tar::Archive::new(expanded.as_slice());
    let mut root = prefix.map(str::to_owned);
    let mut files = Vec::new();
    for (count, entry) in archive.entries()?.enumerate() {
        ensure!(
            count < MAX_ARCHIVE_FILES,
            "source archive exceeds 1024 entries"
        );
        let mut entry = entry?;
        let kind = entry.header().entry_type();
        ensure!(
            kind.is_file() || kind.is_dir(),
            "source archive contains links or special files"
        );
        let path = std::str::from_utf8(&entry.path_bytes())?
            .trim_end_matches('/')
            .to_owned();
        let (first, relative) = path.split_once('/').unwrap_or((&path, ""));
        ensure!(
            !first.is_empty() && !matches!(first, "." | ".."),
            "invalid source archive root"
        );
        let expected = root.get_or_insert_with(|| first.to_owned());
        ensure!(
            first == expected,
            "source archive must have one matching root"
        );
        if relative.is_empty() {
            ensure!(kind.is_dir(), "source archive file has no relative path");
            continue;
        }
        if exclude_build_files && excluded(relative) {
            continue;
        }
        validate_relative_path(relative)?;
        if kind.is_dir() {
            continue;
        }
        let mut content = Vec::new();
        entry.read_to_end(&mut content)?;
        files.push(ArchiveFile {
            path: relative.to_owned(),
            bytes: content,
        });
    }
    encode(files)
}

pub(super) fn wheel(bytes: &[u8]) -> Result<Vec<u8>> {
    let files = wheel_files(bytes)?;
    let root = wheel_root(&files)?;
    let files = files
        .into_iter()
        .filter_map(|file| {
            let path = file.path.strip_prefix(&root)?.to_owned();
            Some(ArchiveFile {
                path,
                bytes: file.bytes,
            })
        })
        .collect();
    encode(files)
}

fn wheel_files(bytes: &[u8]) -> Result<Vec<ArchiveFile>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
    ensure!(
        archive.len() <= MAX_ARCHIVE_FILES,
        "wheel exceeds 1024 entries"
    );
    let mut total = 0_usize;
    let mut files = Vec::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let path = entry.name()?.trim_end_matches('/').to_owned();
        validate_relative_path(&path)?;
        ensure!(
            !entry.is_symlink()
                && entry
                    .unix_mode()
                    .is_none_or(|mode| matches!(mode & 0o170_000, 0 | 0o100_000 | 0o40_000)),
            "wheel contains links or special files"
        );
        ensure!(
            !is_startup_or_relocation_path(&path),
            "wheel startup hooks and relocation layouts are unsupported; bundle locally"
        );
        if entry.is_dir() {
            continue;
        }
        let remaining = MAX_EXPANDED_BYTES.saturating_sub(total);
        ensure!(
            entry.size() <= u64::try_from(remaining)?,
            "wheel exceeds 32 MiB expanded"
        );
        let mut content = Vec::new();
        entry
            .by_ref()
            .take(u64::try_from(remaining)? + 1)
            .read_to_end(&mut content)?;
        ensure!(
            u64::try_from(content.len())? == entry.size(),
            "wheel entry size changed"
        );
        total = total.saturating_add(content.len());
        ensure!(total <= MAX_EXPANDED_BYTES, "wheel exceeds 32 MiB expanded");
        files.push(ArchiveFile {
            path,
            bytes: content,
        });
    }
    Ok(files)
}

/// `.pth` files run at interpreter startup and `.data` folders relocate files outside the package.
fn is_startup_or_relocation_path(path: &str) -> bool {
    let has_extension = |name: &str, wanted: &str| {
        Path::new(name)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case(wanted))
    };
    has_extension(path, "pth") || path.split('/').any(|part| has_extension(part, "data"))
}

fn wheel_root(files: &[ArchiveFile]) -> Result<String> {
    let mut manifests = files
        .iter()
        .filter(|file| file.path.rsplit('/').next() == Some(MANIFEST_FILE));
    let manifest = manifests.next().context("wheel has no package manifest")?;
    ensure!(
        manifests.next().is_none(),
        "wheel must contain exactly one package manifest"
    );
    ensure!(
        manifest.path.split('/').count() <= 8,
        "wheel package manifest exceeds eight directory levels"
    );
    Ok(manifest
        .path
        .strip_suffix(MANIFEST_FILE)
        .context("wheel has no package root")?
        .to_owned())
}

fn excluded(path: &str) -> bool {
    path.split('/').any(|part| {
        (part.starts_with('.') && !matches!(part, "." | ".."))
            || matches!(part, "target" | "node_modules" | "__pycache__")
    })
}

fn encode(files: Vec<ArchiveFile>) -> Result<Vec<u8>> {
    let mut builder = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::fast()));
    for file in files {
        let mut header = tar::Header::new_gnu();
        header.set_size(u64::try_from(file.bytes.len())?);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append_data(&mut header, file.path, file.bytes.as_slice())?;
    }
    let mut gzip = builder.into_inner()?;
    gzip.flush()?;
    let bytes = gzip.finish()?;
    ensure!(
        bytes.len() <= MAX_ARCHIVE_BYTES,
        "normalized archive exceeds 8 MiB"
    );
    validate_archive(&bytes)?;
    Ok(bytes)
}
