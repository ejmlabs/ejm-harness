use std::{
    fs::{File, TryLockError},
    io::{ErrorKind, Read},
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context, Result, ensure};
use rightkit_fs::{FileKey, StateDir, handle::ReadOptions};
use zeroize::Zeroizing;

#[derive(Clone)]
pub(crate) struct PrivateDirectory(Arc<Location>);

struct Location {
    directory: StateDir,
    path: PathBuf,
    // Read-only sharing denies rename/delete and keeps this lock identity pinned.
    lock_anchor: File,
}

pub(crate) struct RawTransaction {
    location: Arc<Location>,
    _lock: File,
}

fn read_options() -> ReadOptions {
    ReadOptions::new()
        .require_single_link(true)
        .share_read_only(true)
}

impl PrivateDirectory {
    pub(crate) fn open(path: &Path) -> Result<Self> {
        ensure!(
            path.file_name().is_some(),
            "credential directory must have a final component"
        );
        let absolute = std::path::absolute(path).context("resolve credential directory path")?;
        let mut ancestor = absolute
            .parent()
            .context("credential directory has no parent")?;
        // Resolve existing parents, then let StateDir create missing components
        // privately without following a raced reparse point.
        let resolved = loop {
            match ancestor.canonicalize() {
                Ok(parent) => break parent.join(absolute.strip_prefix(ancestor)?),
                Err(error) if error.kind() == ErrorKind::NotFound => {
                    ancestor = ancestor
                        .parent()
                        .context("credential directory has no existing parent")?;
                }
                Err(error) => return Err(error).context("resolve credential directory parent"),
            }
        };
        let directory =
            StateDir::create(&resolved).context("open protected native credential directory")?;
        rightkit_fs::private::verify_owner_only(directory.path())
            .context("credential directory must have an owner-only Windows ACL")?;
        if !directory.exists(".lock")? {
            match directory.write_new(".lock", &[]) {
                Ok(()) => {}
                Err(error) if directory.exists(".lock")? => {
                    let _ = error;
                }
                Err(error) => return Err(error).context("create credential lock"),
            }
        }
        let lock_anchor = directory.open_read(".lock", &read_options())?;
        rightkit_fs::private::verify_owner_only(&directory.path().join(".lock"))?;
        let path = rightkit_fs::handle::final_path(&lock_anchor)?
            .parent()
            .context("credential lock has no parent directory")?
            .to_path_buf();
        Ok(Self(Arc::new(Location {
            directory,
            path,
            lock_anchor,
        })))
    }

    pub(crate) fn path(&self) -> &Path {
        &self.0.path
    }

    pub(crate) fn try_lock(&self) -> Result<Option<RawTransaction>> {
        self.0.validate()?;
        let lock = self.0.directory.open_read(".lock", &read_options())?;
        ensure!(
            FileKey::of_file(&lock, self.path())?
                == FileKey::of_file(&self.0.lock_anchor, self.path())?,
            "credential lock was replaced"
        );
        match lock.try_lock() {
            Ok(()) => Ok(Some(RawTransaction {
                location: Arc::clone(&self.0),
                _lock: lock,
            })),
            Err(TryLockError::WouldBlock) => Ok(None),
            Err(TryLockError::Error(error)) => Err(error).context("acquire credential store lock"),
        }
    }
}

impl Location {
    fn validate(&self) -> Result<()> {
        // StateDir pins the directory without delete sharing and refuses reparse traversal.
        rightkit_fs::private::verify_owner_only(self.directory.path())?;
        let lock = self.directory.open_read(".lock", &read_options())?;
        ensure!(
            FileKey::of_file(&lock, self.directory.path())?
                == FileKey::of_file(&self.lock_anchor, self.directory.path())?,
            "credential lock was replaced"
        );
        rightkit_fs::private::verify_owner_only(&self.directory.path().join(".lock"))?;
        Ok(())
    }
}

impl RawTransaction {
    pub(crate) fn remove(&self, name: &str, max: usize) -> Result<()> {
        // Use the same protected handle and entry validation as credential reads.
        ensure!(
            self.read(name, max)?.is_some(),
            "credential bootstrap marker is missing"
        );
        ensure!(
            self.location.directory.remove(name)?,
            "credential bootstrap marker disappeared during completion"
        );
        self.location.validate()
    }

    pub(crate) fn read(&self, name: &str, max: usize) -> Result<Option<Zeroizing<Vec<u8>>>> {
        self.location.validate()?;
        if !self.location.directory.exists(name)? {
            return Ok(None);
        }
        let mut file = self.location.directory.open_read(name, &read_options())?;
        rightkit_fs::private::verify_owner_only(&self.location.directory.path().join(name))?;
        ensure!(
            file.metadata()?.len() <= u64::try_from(max)?,
            "credential file exceeds size limit"
        );
        let mut bytes = Zeroizing::new(Vec::new());
        (&mut file)
            .take(u64::try_from(max + 1)?)
            .read_to_end(&mut bytes)?;
        ensure!(bytes.len() <= max, "credential file exceeds size limit");
        self.location.validate()?;
        Ok(Some(bytes))
    }

    pub(crate) fn write(&self, name: &str, bytes: &[u8], max: usize) -> Result<()> {
        ensure!(bytes.len() <= max, "credential file exceeds size limit");
        self.location.validate()?;
        if self.location.directory.exists(name)? {
            // Validate the current destination before replacing its directory entry.
            let file = self.location.directory.open_read(name, &read_options())?;
            rightkit_fs::private::verify_owner_only(&self.location.directory.path().join(name))?;
            ensure!(
                file.metadata()?.len() <= u64::try_from(max)?,
                "credential file exceeds size limit"
            );
        }
        // The maintained adapter creates an owner-only temporary file and publishes
        // relative to the pinned directory with native write-through semantics.
        self.location
            .directory
            .write(name, bytes)
            .context("durably replace encrypted credential state")?;
        self.location.validate()
    }
}
