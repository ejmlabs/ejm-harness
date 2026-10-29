use std::path::{Path, PathBuf};

use anyhow::Result;
use uuid::Uuid;

/// Commits the shared helper allowance before the executor starts its process.
///
/// Core implements this port with its existing host registry. It must retain
/// the charge independently of the creating client until the stable lifetime
/// witness is released. Dropping a request or permit never authorizes a refund.
pub trait NativeOwnerAdmission: Send + Sync {
    /// Select the current compatible execution domain before job admission.
    ///
    /// Returning `None` preserves the handle's configured domain. The executor
    /// coalesces this bounded blocking probe across cloned handles.
    ///
    /// # Errors
    /// Returns an error if the current domain cannot be safely established.
    fn domain_key(&self) -> Result<Option<String>> {
        Ok(None)
    }

    /// Commit this exclusively owned generation, replacing stale same-lock state.
    ///
    /// # Errors
    /// Returns an error if the shared commitment cannot be established within
    /// the implementation's bounded wait or its protected state is unavailable.
    fn admit(&self, request: &NativeOwnerRequest) -> Result<NativeOwnerPermit>;
}

/// Evidence of exclusive pre-spawn ownership, constructed only by the executor.
///
/// The executor holds the stable lifetime lock while this request is admitted.
/// Successful exclusive acquisition permits the registry to retire a stale
/// generation for this same lock before committing the new generation.
pub struct NativeOwnerRequest {
    root: PathBuf,
    domain: String,
    generation: Uuid,
    lifetime: PathBuf,
}

impl NativeOwnerRequest {
    pub(super) fn new(root: PathBuf, domain: String, generation: Uuid, lifetime: PathBuf) -> Self {
        Self {
            root,
            domain,
            generation,
            lifetime,
        }
    }

    #[must_use]
    pub fn state_root(&self) -> &Path {
        &self.root
    }

    #[must_use]
    pub fn domain_key(&self) -> &str {
        &self.domain
    }

    #[must_use]
    pub const fn generation(&self) -> Uuid {
        self.generation
    }

    #[must_use]
    pub fn lifetime_path(&self) -> &Path {
        &self.lifetime
    }

    /// Acknowledge the generation after its shared commitment has been saved.
    #[must_use]
    pub const fn permit(&self) -> NativeOwnerPermit {
        NativeOwnerPermit {
            generation: self.generation,
        }
    }
}

/// Acknowledgement of a coarse shared commitment, without client-owned refunds.
pub struct NativeOwnerPermit {
    pub(super) generation: Uuid,
}

#[cfg(unix)]
pub(super) fn adopt(root: &Path, domain: &str) -> Result<std::fs::File> {
    use std::{fs::TryLockError, os::fd::AsFd, os::unix::fs::MetadataExt};

    use anyhow::{Context, bail, ensure};

    use super::wire;

    let stdin = std::io::stdin();
    let ownership = std::fs::File::from(stdin.as_fd().try_clone_to_owned()?);
    let directory = wire::directory(root, domain)?;
    let stable = wire::stable_lock(&directory, "owner.lock")?;
    let inherited = ownership.metadata()?;
    let expected = stable.metadata()?;
    ensure!(
        inherited.dev() == expected.dev()
            && inherited.ino() == expected.ino()
            && inherited.nlink() == 1
            && inherited.is_file(),
        "native helper did not inherit its stable lifetime witness"
    );
    // A separate open description must be excluded, while the inherited
    // description retains the lock acquired by the launcher before spawn.
    match stable.try_lock() {
        Err(TryLockError::WouldBlock) => {}
        Ok(()) => bail!("native helper inherited an unlocked lifetime witness"),
        Err(TryLockError::Error(error)) => return Err(error.into()),
    }
    ownership
        .try_lock()
        .context("adopt inherited native lifetime ownership")?;
    for descriptor in [stdin.as_fd(), ownership.as_fd()] {
        let flags = rustix::io::fcntl_getfd(descriptor)?;
        rustix::io::fcntl_setfd(descriptor, flags | rustix::io::FdFlags::CLOEXEC)?;
    }
    Ok(ownership)
}

#[cfg(not(unix))]
pub(super) fn adopt(_root: &Path, _domain: &str) -> Result<std::fs::File> {
    anyhow::bail!("shared native lifetime inheritance is unsupported on this platform")
}
