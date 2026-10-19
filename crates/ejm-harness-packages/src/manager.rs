use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, ensure};
use ejm_harness_package_spec::{
    MANIFEST_FILE, MAX_ARCHIVE_BYTES, MAX_ARCHIVE_FILES, MAX_EXPANDED_BYTES, Manifest,
    archive_sha256, read_archive, validate_name, validate_relative_path,
};
use flate2::{Compression, write::GzEncoder};
use serde::{Deserialize, Serialize};
use tempfile::{NamedTempFile, TempDir};
use walkdir::WalkDir;

use crate::{InstalledPackage, PackageGrants};

const MAX_RECORDS: usize = 64;
const MAX_LOCK_BYTES: usize = 256 * 1024;
const MAX_DIRECTORY_DEPTH: usize = 32;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageRecord {
    pub name: String,
    pub version: String,
    pub sha256: String,
    pub source: String,
    pub enabled: bool,
    pub grants: PackageGrants,
}

/// User-owned installed snapshots. Discovery never executes project files.
#[derive(Clone, Debug)]
pub struct PackageManager {
    root: PathBuf,
}

impl PackageManager {
    /// Create a private package store. All package writes happen in temporary directories.
    ///
    /// # Errors
    /// Rejects inaccessible or symbolic-link storage roots.
    pub fn new(root: impl AsRef<Path>) -> Result<Self> {
        private_directory(root.as_ref())?;
        let root = root.as_ref().canonicalize()?;
        private_directory(&root.join("objects"))?;
        Ok(Self { root })
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Read the bounded lockfile, including exact archive hashes and explicit grants.
    ///
    /// # Errors
    /// Rejects invalid, oversized, duplicate, or tampered records.
    pub fn list(&self) -> Result<Vec<PackageRecord>> {
        let path = self.root.join("packages.lock.json");
        if !path.exists() {
            return Ok(Vec::new());
        }
        let bytes = read_bounded(&path, MAX_LOCK_BYTES)?;
        let records: Vec<PackageRecord> = serde_json::from_slice(&bytes)?;
        ensure!(
            records.len() <= MAX_RECORDS,
            "package store exceeds 64 packages"
        );
        let mut names = std::collections::HashSet::new();
        for record in &records {
            validate_name(&record.name)?;
            semver::Version::parse(&record.version)?;
            ensure!(
                record.sha256.len() == 64
                    && record
                        .sha256
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
                "invalid package digest"
            );
            ensure!(
                names.insert(&record.name),
                "duplicate installed package name"
            );
        }
        Ok(records)
    }

    /// Install a validated immutable snapshot, initially disabled. No package code runs.
    ///
    /// # Errors
    /// Rejects invalid archives, store capacity, concurrent mutation, or failed atomic writes.
    pub fn install(&self, bytes: &[u8], source: &str) -> Result<PackageRecord> {
        self.install_checked(bytes, source, &|| Ok(()))
    }

    /// Install like [`Self::install`], calling `check` between steps so a caller can stop early.
    /// The final registration is not interruptible once `check` has passed.
    pub(crate) fn install_checked(
        &self,
        bytes: &[u8],
        source: &str,
        check: &dyn Fn() -> Result<()>,
    ) -> Result<PackageRecord> {
        ensure!(
            source.len() <= 1024 && !source.chars().any(char::is_control),
            "invalid package source"
        );
        check()?;
        let package = read_archive(bytes)?;
        let digest = archive_sha256(bytes);
        check()?;
        let _lock = self.lock()?;
        let mut records = self.list()?;
        let prior = records
            .iter()
            .position(|record| record.name == package.manifest.name);
        ensure!(
            prior.is_some() || records.len() < MAX_RECORDS,
            "package store is full"
        );
        let target = self.root.join("objects").join(&digest);
        if !target.exists() {
            let staging = TempDir::new_in(self.root.join("objects"))?;
            fs::create_dir(staging.path().join("content"))?;
            for file in package.files {
                check()?;
                let path = staging.path().join("content").join(&file.path);
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)?;
                }
                let mut output = OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(&path)?;
                output.write_all(&file.bytes)?;
                output.sync_all()?;
            }
            fs::write(staging.path().join("archive.tgz"), bytes)?;
            fs::rename(staging.path(), &target).context("commit immutable package snapshot")?;
        }
        let record = PackageRecord {
            name: package.manifest.name,
            version: package.manifest.version,
            sha256: digest,
            source: source.into(),
            enabled: false,
            grants: PackageGrants::default(),
        };
        if let Some(index) = prior {
            records[index] = record.clone();
        } else {
            records.push(record.clone());
        }
        check()?;
        self.write_records(&records)?;
        Ok(record)
    }

    /// Grant named exports for an exact installed version after the host obtains trust.
    ///
    /// # Errors
    /// Rejects unknown package names, undeclared exports, or failed lockfile writes.
    pub fn enable(&self, name: &str, grants: PackageGrants) -> Result<PackageRecord> {
        let _lock = self.lock()?;
        let mut records = self.list()?;
        let record = records
            .iter_mut()
            .find(|record| record.name == name)
            .context("package is not installed")?;
        let package = self.open(record)?;
        grants.validate(&package.manifest)?;
        record.enabled = true;
        record.grants = grants;
        let result = record.clone();
        self.write_records(&records)?;
        Ok(result)
    }

    /// Disable a package for future hosts and reloads.
    ///
    /// # Errors
    /// Rejects an unknown name or failed lockfile write.
    pub fn disable(&self, name: &str) -> Result<()> {
        let _lock = self.lock()?;
        let mut records = self.list()?;
        let record = records
            .iter_mut()
            .find(|record| record.name == name)
            .context("package is not installed")?;
        record.enabled = false;
        self.write_records(&records)
    }

    /// Remove a registration. Active hosts retain their snapshot until shutdown or reload.
    ///
    /// # Errors
    /// Rejects an unknown name or failed lockfile write.
    pub fn remove(&self, name: &str) -> Result<()> {
        let _lock = self.lock()?;
        let mut records = self.list()?;
        let count = records.len();
        records.retain(|record| record.name != name);
        ensure!(records.len() != count, "package is not installed");
        self.write_records(&records)
    }

    /// Discover explicitly enabled packages, verifying content before any runtime starts.
    ///
    /// # Errors
    /// Rejects changed snapshots, invalid grants, or corrupt records.
    pub fn enabled(&self) -> Result<Vec<(InstalledPackage, PackageGrants)>> {
        self.list()?
            .iter()
            .filter(|record| record.enabled)
            .map(|record| {
                let package = self.open(record)?;
                record.grants.validate(&package.manifest)?;
                Ok((package, record.grants.clone()))
            })
            .collect()
    }

    /// Read and verify an installed snapshot without executing it.
    ///
    /// # Errors
    /// Rejects altered hashes, files, paths, or manifest identity.
    pub fn open(&self, record: &PackageRecord) -> Result<InstalledPackage> {
        ensure!(
            record.sha256.len() == 64 && record.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()),
            "invalid package digest"
        );
        let snapshot = self.root.join("objects").join(&record.sha256);
        let metadata = fs::symlink_metadata(&snapshot)?;
        ensure!(
            metadata.is_dir() && !metadata.file_type().is_symlink(),
            "package snapshot must be a directory, not a symlink"
        );
        let root = snapshot.join("content");
        let metadata = fs::symlink_metadata(&root)?;
        ensure!(
            metadata.is_dir() && !metadata.file_type().is_symlink(),
            "package content must be a directory, not a symlink"
        );
        let bytes = read_bounded(&snapshot.join("archive.tgz"), MAX_ARCHIVE_BYTES)?;
        ensure!(
            archive_sha256(&bytes) == record.sha256,
            "installed archive hash changed"
        );
        let archive = read_archive(&bytes)?;
        ensure!(
            archive.manifest.name == record.name && archive.manifest.version == record.version,
            "installed package identity changed"
        );
        let expected_paths: HashSet<_> = archive
            .files
            .iter()
            .flat_map(|file| {
                Path::new(&file.path)
                    .ancestors()
                    .take_while(|path| !path.as_os_str().is_empty())
            })
            .collect();
        for entry in WalkDir::new(&root).min_depth(1) {
            let entry = entry?;
            ensure!(
                entry.file_type().is_file() || entry.file_type().is_dir(),
                "installed package contains a link or special file"
            );
            let path = entry.path().strip_prefix(&root)?;
            ensure!(
                expected_paths.contains(path),
                "unexpected installed package path: {}",
                path.display()
            );
        }
        let canonical_root = root.canonicalize()?;
        for file in archive.files {
            let path = root.join(&file.path);
            ensure!(
                path.canonicalize()?.starts_with(&canonical_root),
                "installed file escaped package root"
            );
            ensure!(
                read_bounded(&path, MAX_EXPANDED_BYTES)? == file.bytes,
                "installed package content changed: {}",
                file.path
            );
        }
        Ok(InstalledPackage {
            manifest: archive.manifest,
            root,
        })
    }

    fn lock(&self) -> Result<File> {
        let path = self.root.join("mutation.lock");
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)?;
        file.try_lock()
            .context("another package operation is running")?;
        Ok(file)
    }

    fn write_records(&self, records: &[PackageRecord]) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(records)?;
        ensure!(
            bytes.len() <= MAX_LOCK_BYTES,
            "package lockfile exceeds 256 KiB"
        );
        let mut temporary = NamedTempFile::new_in(&self.root)?;
        temporary.write_all(&bytes)?;
        temporary.as_file().sync_all()?;
        temporary.persist(self.root.join("packages.lock.json"))?;
        Ok(())
    }
}

/// Build a deterministic package snapshot with maintained traversal, tar, and gzip libraries.
///
/// # Errors
/// Rejects unsafe paths, links, excessive entries, depth or sizes, and invalid manifests.
pub fn pack_directory(root: &Path) -> Result<Vec<u8>> {
    pack_directory_checked(root, &|| Ok(()))
}

/// Pack like [`pack_directory`], calling `check` for each entry and file so a caller can stop early.
pub(crate) fn pack_directory_checked(
    root: &Path,
    check: &dyn Fn() -> Result<()>,
) -> Result<Vec<u8>> {
    let root = root.canonicalize()?;
    Manifest::from_bytes(&read_bounded_checked(
        &root.join(MANIFEST_FILE),
        ejm_harness_package_spec::MAX_MANIFEST_BYTES,
        check,
    )?)?;
    let mut files = Vec::new();
    let mut count = 0;
    let mut total = 0_u64;
    let mut entries = WalkDir::new(&root)
        .min_depth(1)
        .max_depth(MAX_DIRECTORY_DEPTH + 1)
        .into_iter();
    while let Some(entry) = entries.next() {
        check()?;
        let entry = entry?;
        count += 1;
        ensure!(count <= MAX_ARCHIVE_FILES, "package exceeds 1024 entries");
        if entry.file_name().to_string_lossy().starts_with('.')
            || matches!(
                entry.file_name().to_str(),
                Some("target" | "node_modules" | "__pycache__")
            )
        {
            if entry.file_type().is_dir() {
                entries.skip_current_dir();
            }
            continue;
        }
        ensure!(
            entry.depth() <= MAX_DIRECTORY_DEPTH,
            "package directory depth exceeds 32 levels"
        );
        ensure!(
            !entry.file_type().is_symlink(),
            "package contains symbolic link"
        );
        if entry.file_type().is_dir() {
            continue;
        }
        ensure!(
            entry.file_type().is_file(),
            "package contains a special file"
        );
        let path = entry
            .path()
            .strip_prefix(&root)?
            .to_string_lossy()
            .replace('\\', "/");
        validate_relative_path(&path)?;
        let metadata = entry.metadata()?;
        total = total.saturating_add(metadata.len());
        ensure!(
            total <= u64::try_from(MAX_EXPANDED_BYTES)?,
            "package exceeds 32 MiB"
        );
        files.push((path, entry.path().to_path_buf(), metadata.len()));
    }
    files.sort_unstable_by(|left, right| left.0.cmp(&right.0));
    let gzip = GzEncoder::new(Vec::new(), Compression::fast());
    let mut builder = tar::Builder::new(gzip);
    for (path, source, size) in files {
        check()?;
        let mut header = tar::Header::new_gnu();
        header.set_size(size);
        header.set_mode(0o644);
        header.set_mtime(0);
        header.set_cksum();
        builder.append_data(&mut header, path, File::open(source)?)?;
    }
    let bytes = builder.into_inner()?.finish()?;
    read_archive(&bytes)?;
    Ok(bytes)
}

/// Read a regular file without allowing a hostile size hint or stream to grow memory.
///
/// # Errors
/// Rejects links, nonregular files, inaccessible paths, or the byte limit.
pub fn read_bounded(path: &Path, max: usize) -> Result<Vec<u8>> {
    read_bounded_checked(path, max, &|| Ok(()))
}

/// Read like [`read_bounded`], calling `check` before each chunk so a caller can stop early.
pub(crate) fn read_bounded_checked(
    path: &Path,
    max: usize,
    check: &dyn Fn() -> Result<()>,
) -> Result<Vec<u8>> {
    check()?;
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink(),
        "expected a regular package file"
    );
    ensure!(
        metadata.len() <= u64::try_from(max)?,
        "package file exceeds limit"
    );
    let mut bytes = Vec::new();
    let mut reader = File::open(path)?.take(u64::try_from(max)? + 1);
    loop {
        check()?;
        if reader.by_ref().take(64 * 1024).read_to_end(&mut bytes)? == 0 {
            break;
        }
    }
    ensure!(bytes.len() <= max, "package file exceeds limit");
    Ok(bytes)
}

#[cfg(windows)]
pub(crate) fn private_directory(path: &Path) -> Result<()> {
    let directory = rightkit_fs::StateDir::create(&std::path::absolute(path)?)
        .context("open protected package directory")?;
    rightkit_fs::private::verify_owner_only(directory.path())
        .context("package directory must have an owner-only Windows ACL")?;
    rightkit_fs::private::restrict_owner_only(directory.path())?;
    ensure!(
        rightkit_fs::private::is_owner_only_strict(directory.path())?,
        "package directory requires an inheritable owner-only Windows ACL"
    );
    Ok(())
}

#[cfg(target_os = "macos")]
pub(crate) fn private_directory(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(_) => rightkit_fs::private::verify_owner_only(path)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error).context("inspect private package directory"),
    }
    rightkit_fs::private::ensure_private_dir(path)?;
    rightkit_fs::private::verify_owner_only(path)
        .context("package store must have an owner-only ACL")
}

#[cfg(not(any(windows, target_os = "macos")))]
pub(crate) fn private_directory(path: &Path) -> Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)?;
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_dir() && !metadata.file_type().is_symlink(),
        "package store cannot be a symlink"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        ensure!(
            metadata.uid() == rustix::process::getuid().as_raw(),
            "package store has another owner"
        );
        ensure!(
            metadata.permissions().mode().trailing_zeros() >= 6,
            "package store must be private (chmod 700)"
        );
    }
    Ok(())
}
