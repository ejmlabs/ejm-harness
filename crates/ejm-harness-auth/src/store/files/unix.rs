use std::{
    fs::{self, DirBuilder, File, Metadata, OpenOptions, Permissions, TryLockError},
    io::{self, Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context, Result, ensure};
use zeroize::Zeroizing;

#[derive(Clone)]
pub(crate) struct PrivateDirectory(Arc<Location>);

struct Location {
    path: PathBuf,
    directory: File,
    lock_anchor: File,
    owner: u32,
}

pub(crate) struct RawTransaction {
    location: Arc<Location>,
    _lock: File,
}

impl PrivateDirectory {
    pub(crate) fn open(path: &Path) -> Result<Self> {
        let owner = rustix::process::geteuid().as_raw();
        match fs::symlink_metadata(path) {
            Ok(metadata) => check_directory(&metadata, owner)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                DirBuilder::new()
                    .recursive(true)
                    .mode(0o700)
                    .create(path)
                    .context("create private credential directory")?;
            }
            Err(error) => return Err(error).context("inspect credential directory"),
        }
        let before = fs::symlink_metadata(path)?;
        check_directory(&before, owner)?;
        #[cfg(target_os = "macos")]
        verify_acl(path)?;
        let directory = OpenOptions::new()
            .read(true)
            .custom_flags(
                (rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::DIRECTORY)
                    .bits()
                    .cast_signed(),
            )
            .open(path)
            .context("open credential directory")?;
        let opened = directory.metadata()?;
        check_directory(&opened, owner)?;
        ensure!(
            same_inode(&before, &opened),
            "credential directory changed during open"
        );
        let after = fs::symlink_metadata(path)?;
        check_directory(&after, owner)?;
        ensure!(
            same_inode(&after, &opened),
            "credential directory changed during open"
        );
        // Resolve parent aliases such as /tmp, never a link at the store itself.
        #[cfg(not(target_os = "macos"))]
        let path = fs::canonicalize(path).context("resolve credential directory")?;
        #[cfg(target_os = "macos")]
        let path = rightkit_fs::handle::final_path(&directory)
            .context("resolve native credential directory")?;
        let current = fs::symlink_metadata(&path)?;
        check_directory(&current, owner)?;
        #[cfg(target_os = "macos")]
        verify_acl(&path)?;
        ensure!(
            same_inode(&current, &opened),
            "credential directory changed during open"
        );
        let lock_path = path.join(".lock");
        match OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&lock_path)
        {
            Ok(file) => {
                check_file(&file.metadata()?, owner)?;
                file.sync_all().context("synchronize new credential lock")?;
                directory
                    .sync_all()
                    .context("synchronize credential directory")?;
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error).context("create credential lock"),
        }
        let lock_anchor = open_private_file(&lock_path, owner, true)?;
        let location = Arc::new(Location {
            path,
            directory,
            lock_anchor,
            owner,
        });
        location.validate()?;
        location.check_entry("state.json", super::super::wire::MAX_STORE_BYTES)?;
        location.check_entry("vault.bin", 8 * 1024 * 1024)?;
        Ok(Self(location))
    }

    pub(crate) fn path(&self) -> &Path {
        &self.0.path
    }

    pub(crate) fn try_lock(&self) -> Result<Option<RawTransaction>> {
        self.0.validate()?;
        // Independent open descriptions are necessary for interprocess exclusivity.
        let lock = open_private_file(&self.0.path.join(".lock"), self.0.owner, true)?;
        ensure!(
            same_inode(&lock.metadata()?, &self.0.lock_anchor.metadata()?),
            "credential lock was replaced"
        );
        match lock.try_lock() {
            Ok(()) => {
                self.0.validate()?;
                Ok(Some(RawTransaction {
                    location: Arc::clone(&self.0),
                    _lock: lock,
                }))
            }
            Err(TryLockError::WouldBlock) => Ok(None),
            Err(TryLockError::Error(error)) => Err(error).context("acquire credential store lock"),
        }
    }
}

impl Location {
    fn validate(&self) -> Result<()> {
        let current = fs::symlink_metadata(&self.path).context("inspect credential directory")?;
        check_directory(&current, self.owner)?;
        #[cfg(target_os = "macos")]
        verify_acl(&self.path)?;
        let opened = self.directory.metadata()?;
        check_directory(&opened, self.owner)?;
        ensure!(
            same_inode(&current, &opened),
            "credential directory was replaced"
        );
        let lock = fs::symlink_metadata(self.path.join(".lock"))?;
        check_file(&lock, self.owner)?;
        #[cfg(target_os = "macos")]
        verify_acl(&self.path.join(".lock"))?;
        let anchor = self.lock_anchor.metadata()?;
        check_file(&anchor, self.owner)?;
        ensure!(same_inode(&lock, &anchor), "credential lock was replaced");
        Ok(())
    }

    fn check_entry(&self, name: &str, max: usize) -> Result<()> {
        match fs::symlink_metadata(self.path.join(name)) {
            Ok(metadata) => {
                check_file(&metadata, self.owner)?;
                #[cfg(target_os = "macos")]
                verify_acl(&self.path.join(name))?;
                ensure!(
                    metadata.len() <= u64::try_from(max)?,
                    "credential file exceeds size limit"
                );
                Ok(())
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error).context("inspect credential file"),
        }
    }
}

impl RawTransaction {
    pub(crate) fn remove(&self, name: &str, max: usize) -> Result<()> {
        self.location.validate()?;
        self.location.check_entry(name, max)?;
        fs::remove_file(self.location.path.join(name))
            .context("retire credential bootstrap marker")?;
        self.location
            .directory
            .sync_all()
            .context("synchronize credential bootstrap completion")?;
        self.location.validate()
    }

    pub(crate) fn read(&self, name: &str, max: usize) -> Result<Option<Zeroizing<Vec<u8>>>> {
        self.location.validate()?;
        self.location.check_entry(name, max)?;
        let path = self.location.path.join(name);
        if !path.try_exists().context("inspect credential file")? {
            return Ok(None);
        }
        let mut file = open_private_file(&path, self.location.owner, false)?;
        let mut bytes = Zeroizing::new(Vec::new());
        (&mut file)
            .take(u64::try_from(max + 1)?)
            .read_to_end(&mut bytes)
            .context("read credential file")?;
        ensure!(bytes.len() <= max, "credential file exceeds size limit");
        let current = fs::symlink_metadata(path)?;
        check_file(&current, self.location.owner)?;
        ensure!(
            same_inode(&current, &file.metadata()?),
            "credential file changed during read"
        );
        self.location.validate()?;
        Ok(Some(bytes))
    }

    pub(crate) fn write(&self, name: &str, bytes: &[u8], max: usize) -> Result<()> {
        ensure!(bytes.len() <= max, "credential file exceeds size limit");
        self.location.validate()?;
        self.location.check_entry(name, max)?;
        let mut temporary = tempfile::Builder::new()
            .prefix(".vault-")
            .tempfile_in(&self.location.path)
            .context("create private credential replacement")?;
        temporary
            .as_file()
            .set_permissions(Permissions::from_mode(0o600))?;
        check_file(&temporary.as_file().metadata()?, self.location.owner)?;
        #[cfg(target_os = "macos")]
        verify_acl(temporary.path())?;
        temporary
            .write_all(bytes)
            .context("write credential replacement")?;
        temporary
            .as_file()
            .sync_all()
            .context("synchronize credential replacement")?;
        self.location
            .directory
            .sync_all()
            .context("synchronize credential replacement directory")?;
        self.location.validate()?;
        self.location.check_entry(name, max)?;
        temporary
            .persist(self.location.path.join(name))
            .map_err(|_| anyhow::anyhow!("atomically replace credential state"))?;
        self.location
            .directory
            .sync_all()
            .context("synchronize saved credential state")?;
        self.location.validate()?;
        self.location.check_entry(name, max)
    }
}

fn check_directory(metadata: &Metadata, owner: u32) -> Result<()> {
    ensure!(
        metadata.is_dir(),
        "credential directory must be a real directory"
    );
    ensure!(
        metadata.uid() == owner,
        "credential directory has a different owner"
    );
    ensure!(
        metadata.mode() & 0o7777 == 0o700,
        "credential directory must have mode 0700"
    );
    Ok(())
}

fn check_file(metadata: &Metadata, owner: u32) -> Result<()> {
    ensure!(
        metadata.is_file(),
        "credential file must be regular without symlinks"
    );
    ensure!(
        metadata.uid() == owner,
        "credential file has a different owner"
    );
    ensure!(
        metadata.mode() & 0o7777 == 0o600,
        "credential file must have mode 0600"
    );
    ensure!(
        metadata.nlink() == 1,
        "credential file must not have hard links"
    );
    Ok(())
}

fn same_inode(left: &Metadata, right: &Metadata) -> bool {
    left.dev() == right.dev() && left.ino() == right.ino()
}

#[cfg(target_os = "macos")]
fn verify_acl(path: &Path) -> Result<()> {
    // macOS allow ACLs can grant access despite private mode bits. Linux ACLs
    // are constrained by the mode's group mask, already checked above.
    rightkit_fs::private::verify_owner_only(path)?;
    Ok(())
}

fn open_private_file(path: &Path, owner: u32, write: bool) -> Result<File> {
    let before = fs::symlink_metadata(path)?;
    check_file(&before, owner)?;
    #[cfg(target_os = "macos")]
    verify_acl(path)?;
    let file = OpenOptions::new()
        .read(true)
        .write(write)
        .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits().cast_signed())
        .open(path)
        .context("open private credential file")?;
    let opened = file.metadata()?;
    check_file(&opened, owner)?;
    ensure!(
        same_inode(&before, &opened),
        "credential file changed during open"
    );
    let after = fs::symlink_metadata(path)?;
    check_file(&after, owner)?;
    #[cfg(target_os = "macos")]
    verify_acl(path)?;
    ensure!(
        same_inode(&after, &opened),
        "credential file changed during open"
    );
    Ok(file)
}
