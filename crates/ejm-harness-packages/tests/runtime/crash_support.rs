use std::{
    fs::{File, TryLockError},
    os::unix::fs::MetadataExt,
    path::PathBuf,
};

use ejm_harness_exec::{ProcessIdentity, process_identity};
use rustix::process::{Signal, kill_process, test_kill_process};

use super::*;

/// Captured roots only. This guard makes no claim about arbitrary descendants.
#[derive(Default)]
pub(super) struct Roots(Vec<ProcessIdentity>);

impl Roots {
    pub(super) fn capture(&mut self, pid: u32) -> Result<ProcessIdentity> {
        let identity = process_identity(pid).context("fixture root identity unavailable")?;
        self.0.push(identity.clone());
        Ok(identity)
    }

    pub(super) fn signal(identity: &ProcessIdentity, signal: Signal) -> Result<()> {
        anyhow::ensure!(
            process_identity(identity.pid).as_ref() == Some(identity),
            "fixture root identity changed before signal: {}",
            identity.pid
        );
        kill_process(native_pid(identity.pid)?, signal)?;
        Ok(())
    }

    pub(super) async fn wait_ended(identity: &ProcessIdentity) -> Result<()> {
        timeout(BOUND, async {
            loop {
                match test_kill_process(native_pid(identity.pid)?) {
                    Err(rustix::io::Errno::SRCH) => return Ok(()),
                    Err(error) => return Err(error.into()),
                    Ok(()) => {}
                }
                if let Some(current) = process_identity(identity.pid) {
                    if current != *identity {
                        return Ok(());
                    }
                } else {
                    let pid = Pid::from_u32(identity.pid);
                    let mut system = System::new();
                    system.refresh_processes_specifics(
                        ProcessesToUpdate::Some(&[pid]),
                        true,
                        ProcessRefreshKind::nothing(),
                    );
                    if system
                        .process(pid)
                        .is_some_and(|process| process.status() == ProcessStatus::Zombie)
                    {
                        return Ok(());
                    }
                }
                // Missing identity alone can mean an unreadable process.
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .context("recorded fixture root did not stop")?
    }

    pub(super) async fn wait_all_ended(&self) -> Result<()> {
        for identity in &self.0 {
            Self::wait_ended(identity).await?;
        }
        Ok(())
    }
}

impl Drop for Roots {
    fn drop(&mut self) {
        // Failure-only fallback for roots still matching their recorded token.
        // These signals do not satisfy any assertion in the test body. On
        // macOS the public birth token uses seconds, not an atomic PID handle.
        for identity in &self.0 {
            let _ = Self::signal(identity, Signal::CONT);
            let _ = Self::signal(identity, Signal::KILL);
        }
    }
}

fn native_pid(pid: u32) -> Result<rustix::process::Pid> {
    rustix::process::Pid::from_raw(i32::try_from(pid)?).context("positive fixture root PID")
}

pub(super) struct HelperWitness {
    file: File,
    path: PathBuf,
    key: (u64, u64),
}

impl HelperWitness {
    pub(super) fn capture(root: &Path) -> Result<Self> {
        let mut locks = Vec::new();
        for entry in std::fs::read_dir(root.join("native"))?.take(9) {
            let path = entry?.path().join("owner.lock");
            if path.is_file() {
                locks.push(path);
            }
        }
        anyhow::ensure!(locks.len() == 1, "expected one fixture helper witness");
        let path = locks.pop().unwrap();
        let file = File::open(&path)?;
        let metadata = file.metadata()?;
        let witness = Self {
            file,
            path,
            key: (metadata.dev(), metadata.ino()),
        };
        anyhow::ensure!(witness.held()?, "fixture helper witness was not held");
        Ok(witness)
    }

    pub(super) fn held(&self) -> Result<bool> {
        self.verify_identity()?;
        match self.file.try_lock() {
            Ok(()) => {
                self.file.unlock()?;
                Ok(false)
            }
            Err(TryLockError::WouldBlock) => Ok(true),
            Err(TryLockError::Error(error)) => Err(error.into()),
        }
    }

    pub(super) async fn wait_released(&self) -> Result<()> {
        timeout(BOUND, async {
            while self.held()? {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            Ok::<_, anyhow::Error>(())
        })
        .await
        .context("original helper witness remained held")??;
        Ok(())
    }

    pub(super) fn verify_identity(&self) -> Result<()> {
        let metadata = std::fs::symlink_metadata(&self.path)?;
        anyhow::ensure!(
            (metadata.dev(), metadata.ino()) == self.key,
            "original helper witness was removed or replaced"
        );
        Ok(())
    }
}

pub(super) async fn wait_helpers(
    capacity: &ejm_harness::CapacityContext,
    committed: usize,
    phase: &str,
) -> Result<ejm_harness::CapacitySnapshot> {
    let before = capacity.snapshot();
    let generation = before.operating.generation;
    let mut last = before.clone();
    // Successful renewals install a newer generation even for an unchanged
    // fixed target. Keep that fence rather than accepting a stale helper cache.
    capacity.notify_demand();
    timeout(BOUND, async {
        loop {
            last = capacity.snapshot();
            let helpers = last.native_helpers;
            if last.operating.generation > generation
                && last.operating.contact_valid
                && helpers.occupied == committed
                && helpers.committed == committed
                && helpers.pending == 0
                && helpers.memory_bytes == u64::try_from(committed).unwrap() * 32 * 1024 * 1024
                && (committed != 0 || helpers.processes == 0)
            {
                return last.clone();
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .with_context(|| {
        format!(
            "{phase}: helper commitments did not converge within {BOUND:?}; \
             expected committed={committed}, pending=0, occupied={committed}, \
             helper_memory={} bytes, generation>{generation}, valid_contact=true; \
             before={}; last={}",
            u64::try_from(committed).unwrap() * 32 * 1024 * 1024,
            snapshot_diagnostics(&before),
            snapshot_diagnostics(&last),
        )
    })
}

pub(super) fn snapshot_diagnostics(snapshot: &ejm_harness::CapacitySnapshot) -> String {
    const MAX_BYTES: usize = 16 * 1024;
    let mut text = format!("{snapshot:?}");
    if text.len() > MAX_BYTES {
        text.truncate(text.floor_char_boundary(MAX_BYTES));
        text.push_str(" [public snapshot truncated]");
    }
    text
}

pub(super) fn registry_key(root: &Path) -> Result<(u64, u64)> {
    // Metadata preserves SQLite's process-wide lock semantics. Opening and
    // closing a second database fd in this process would not be harmless.
    let metadata = std::fs::symlink_metadata(root.join("capacity.sqlite3"))?;
    Ok((metadata.dev(), metadata.ino()))
}
