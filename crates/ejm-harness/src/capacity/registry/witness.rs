//! Bounded owner lifetime locks. Slot files are stable and never unlinked.

use std::{collections::BTreeSet, fs::File, sync::Arc};

use anyhow::{Result, ensure};
use rusqlite::{Connection, params};

use super::{MAX_PARTICIPANTS, PrivateFiles, file_key, path_key};

pub(super) fn create_schema(connection: &Connection) -> Result<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS owner_witnesses(
         owner TEXT PRIMARY KEY, slot INTEGER NOT NULL UNIQUE CHECK(slot BETWEEN 0 AND 255),
         file_key TEXT NOT NULL CHECK(length(file_key) BETWEEN 1 AND 128));",
    )?;
    Ok(())
}

pub(super) struct OwnerWitness {
    slot: u16,
    file: Arc<File>,
}

impl OwnerWitness {
    pub(super) fn acquire(connection: &Connection, files: &PrivateFiles) -> Result<Self> {
        let mut query = connection.prepare("SELECT slot FROM owner_witnesses LIMIT 257")?;
        let occupied = query
            .query_map([], |row| row.get::<_, u16>(0))?
            .collect::<rusqlite::Result<BTreeSet<_>>>()?;
        ensure!(
            occupied.len() <= MAX_PARTICIPANTS,
            "owner witness bound exceeded"
        );
        for slot in (0..u16::try_from(MAX_PARTICIPANTS)?).filter(|slot| !occupied.contains(slot)) {
            let file = open(files, slot, true)?;
            match file.try_lock() {
                Ok(()) => {
                    return Ok(Self {
                        slot,
                        file: Arc::new(file),
                    });
                }
                Err(std::fs::TryLockError::WouldBlock) => {}
                Err(std::fs::TryLockError::Error(error)) => return Err(error.into()),
            }
        }
        anyhow::bail!("shared capacity lifetime slots are occupied; existing owners must drain")
    }

    pub(super) fn retained(&self) -> Arc<File> {
        self.file.clone()
    }

    pub(super) fn record(
        &self,
        connection: &Connection,
        owner: &str,
        files: &PrivateFiles,
    ) -> Result<()> {
        self.verify(files)?;
        connection.execute(
            "INSERT INTO owner_witnesses(owner,slot,file_key) VALUES(?1,?2,?3)",
            params![owner, self.slot, key(&self.file, files, self.slot)?],
        )?;
        Ok(())
    }

    pub(super) fn verify(&self, files: &PrivateFiles) -> Result<()> {
        let path = files.database.with_file_name(name(self.slot));
        let metadata = self.file.metadata()?;
        ensure!(
            metadata.is_file() && metadata.len() == 0,
            "capacity lifetime witness must remain an empty regular file"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            ensure!(
                metadata.nlink() == 1,
                "capacity lifetime witness must retain one link"
            );
            #[cfg(target_os = "linux")]
            ensure!(
                metadata.mode() & 0o077 == 0
                    && metadata.uid() == rustix::process::geteuid().as_raw(),
                "capacity lifetime witness must remain owner-only"
            );
        }
        #[cfg(any(target_os = "macos", windows))]
        rightkit_fs::private::verify_owner_only(&path)?;
        ensure!(
            path_key(&path)? == file_key(&self.file, &path)?,
            "capacity lifetime witness identity changed"
        );
        Ok(())
    }
}

pub(super) fn released(files: &PrivateFiles, slot: u16, expected: &str) -> Result<bool> {
    let file = open(files, slot, false)?;
    ensure!(
        key(&file, files, slot)? == expected,
        "capacity lifetime witness was replaced"
    );
    match file.try_lock() {
        Ok(()) => Ok(true),
        Err(std::fs::TryLockError::WouldBlock) => Ok(false),
        Err(std::fs::TryLockError::Error(error)) => Err(error.into()),
    }
}

fn name(slot: u16) -> String {
    format!("owner-{slot}.lock")
}

fn key(file: &File, files: &PrivateFiles, slot: u16) -> Result<String> {
    let path = files.database.with_file_name(name(slot));
    let (volume, index) = file_key(file, &path)?;
    ensure!(
        path_key(&path)? == (volume, index),
        "capacity lifetime witness identity changed"
    );
    Ok(format!("{volume}:{index}"))
}

fn open(files: &PrivateFiles, slot: u16, create: bool) -> Result<File> {
    ensure!(
        usize::from(slot) < MAX_PARTICIPANTS,
        "invalid capacity lifetime slot"
    );
    let name = name(slot);
    #[cfg(any(target_os = "macos", windows))]
    {
        let directory = &files.directory;
        if create && !directory.exists(&name)? {
            match directory.write_new(&name, &[]) {
                Ok(()) => {}
                Err(_) if directory.exists(&name)? => {}
                Err(error) => return Err(error.into()),
            }
        }
        let file = directory.open_read(
            &name,
            &rightkit_fs::handle::ReadOptions::new().require_single_link(true),
        )?;
        rightkit_fs::private::verify_owner_only(&files.database.with_file_name(&name))?;
        ensure!(
            file.metadata()?.len() == 0,
            "capacity lifetime witness must be empty"
        );
        Ok(file)
    }
    #[cfg(target_os = "linux")]
    {
        use rustix::fs::{Mode, OFlags};
        use std::os::unix::fs::MetadataExt;
        let mut flags = OFlags::RDWR | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK;
        if create {
            flags |= OFlags::CREATE;
        }
        let file = File::from(rustix::fs::openat(
            &files.directory,
            &*name,
            flags,
            Mode::RUSR | Mode::WUSR,
        )?);
        let metadata = file.metadata()?;
        ensure!(
            metadata.is_file() && metadata.nlink() == 1 && metadata.len() == 0,
            "capacity lifetime witness must be an empty regular file with one link"
        );
        ensure!(
            metadata.mode() & 0o077 == 0 && metadata.uid() == rustix::process::geteuid().as_raw(),
            "capacity lifetime witness must be owner-only"
        );
        Ok(file)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        let _ = (files, name, create);
        anyhow::bail!("native owner lifetime witnesses are unsupported")
    }
}
