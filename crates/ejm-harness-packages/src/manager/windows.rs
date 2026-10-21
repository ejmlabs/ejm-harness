use std::{fs::File, io::Read, path::Path};

use anyhow::{Context, Result, ensure};
use rightkit_fs::{FileKey, StateDir, handle::ReadOptions};

const LOCK: &str = "mutation.lock";
const RECORDS: &str = "packages.lock.json";

#[derive(Debug)]
pub(super) struct NativeStore {
    directory: StateDir,
    objects: StateDir,
    lock_anchor: File,
}

fn read_options() -> ReadOptions {
    ReadOptions::new()
        .require_single_link(true)
        .share_read_only(true)
}

pub(super) fn private_directory(path: &Path) -> Result<StateDir> {
    let directory = StateDir::create(&std::path::absolute(path)?)
        .context("open protected package directory")?;
    protect(&directory)?;
    Ok(directory)
}

fn protect(directory: &StateDir) -> Result<()> {
    rightkit_fs::private::verify_owner_only(directory.path())
        .context("package directory must have an owner-only Windows ACL")?;
    // The staging tree uses native file APIs under this pinned private root.
    // Its inherited DACL must protect files from the instant they are created.
    rightkit_fs::private::restrict_owner_only(directory.path())?;
    ensure!(
        rightkit_fs::private::is_owner_only_strict(directory.path())?,
        "package directory requires an inheritable owner-only Windows ACL"
    );
    Ok(())
}

impl NativeStore {
    pub(super) fn open(path: &Path) -> Result<Self> {
        let directory = private_directory(path)?;
        let objects = directory.child("objects")?;
        protect(&objects)?;
        if !directory.exists(LOCK)? {
            match directory.write_new(LOCK, &[]) {
                Ok(()) => {}
                Err(_) if directory.exists(LOCK)? => {}
                Err(error) => return Err(error).context("create package mutation lock"),
            }
        }
        let lock_anchor = directory.open_read(LOCK, &read_options())?;
        rightkit_fs::private::verify_owner_only(&directory.path().join(LOCK))?;
        Ok(Self {
            directory,
            objects,
            lock_anchor,
        })
    }

    pub(super) fn path(&self) -> &Path {
        self.directory.path()
    }

    fn validate(&self) -> Result<()> {
        rightkit_fs::private::verify_owner_only(self.directory.path())?;
        rightkit_fs::private::verify_owner_only(self.objects.path())?;
        rightkit_fs::private::verify_owner_only(&self.directory.path().join(LOCK))?;
        let lock = self.directory.open_read(LOCK, &read_options())?;
        ensure!(
            FileKey::of_file(&lock, self.path())?
                == FileKey::of_file(&self.lock_anchor, self.path())?,
            "package mutation lock was replaced"
        );
        Ok(())
    }

    pub(super) fn lock(&self) -> Result<File> {
        self.validate()?;
        let lock = self.directory.open_read(LOCK, &read_options())?;
        lock.try_lock()
            .context("another package operation is running")?;
        Ok(lock)
    }

    pub(super) fn read_records(&self, max: usize) -> Result<Option<Vec<u8>>> {
        self.validate()?;
        if !self.directory.exists(RECORDS)? {
            return Ok(None);
        }
        let file = self.directory.open_read(RECORDS, &read_options())?;
        rightkit_fs::private::verify_owner_only(&self.path().join(RECORDS))?;
        ensure!(
            file.metadata()?.len() <= u64::try_from(max)?,
            "package lockfile exceeds limit"
        );
        let mut bytes = Vec::new();
        file.take(u64::try_from(max)? + 1).read_to_end(&mut bytes)?;
        ensure!(bytes.len() <= max, "package lockfile exceeds limit");
        Ok(Some(bytes))
    }

    pub(super) fn write_records(&self, bytes: &[u8]) -> Result<()> {
        self.validate()?;
        self.directory.write(RECORDS, bytes)?;
        self.validate()
    }

    pub(super) fn pin_snapshot(&self, digest: &str) -> Result<(StateDir, StateDir)> {
        self.validate()?;
        let snapshot = StateDir::open(&self.objects.path().join(digest))?;
        rightkit_fs::private::verify_owner_only(snapshot.path())?;
        let content = StateDir::open(&snapshot.path().join("content"))?;
        rightkit_fs::private::verify_owner_only(content.path())?;
        Ok((snapshot, content))
    }
}
