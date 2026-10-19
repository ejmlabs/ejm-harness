//! Native child ownership shared by command, package, and host supervisors.
//!
//! Unix uses a separate process group. Windows assigns a suspended child to a
//! kill-on-close Job Object before its code runs. This is process lifecycle
//! control, not a filesystem or network sandbox.

use std::{io, process::ExitStatus};

#[cfg(unix)]
use rustix::process::{Pid, Signal, kill_process_group};
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command};

/// A child and its native process group, retained by one supervisor.
///
/// Dropping this value initiates termination but cannot wait for cleanup. Keep
/// it, its pipe tasks, and any host leases alive until [`Self::kill`] completes.
/// Windows cleanup confirms that the Job Object is empty. Unix group signals
/// are best effort; the executor also checks its sampled descendants.
pub struct OwnedChild {
    child: Child,
    #[cfg(unix)]
    group: Option<Pid>,
    #[cfg(windows)]
    group: processkit::ProcessGroup,
}

/// Spawn a command in a group owned by the calling supervisor.
///
/// Stdio, cwd, arguments, and environment come from `command`. On Windows,
/// processkit sets the creation flags needed for suspended Job assignment;
/// caller-supplied Windows creation flags are not preserved. Use a fresh
/// command for each child.
///
/// # Errors
/// Returns an error if spawn or native group creation/assignment fails. A child
/// that cannot join its Windows Job is terminated before it can run user code.
pub fn spawn_owned(mut command: Command) -> io::Result<OwnedChild> {
    command.kill_on_drop(true);
    #[cfg(unix)]
    {
        command.process_group(0);
        let child = command.spawn()?;
        let group = child
            .id()
            .and_then(|pid| i32::try_from(pid).ok())
            .and_then(Pid::from_raw);
        Ok(OwnedChild { child, group })
    }
    #[cfg(windows)]
    {
        let group = processkit::ProcessGroup::new().map_err(io::Error::other)?;
        let child = group.spawn(command).map_err(io::Error::other)?;
        Ok(OwnedChild { child, group })
    }
    #[cfg(not(any(unix, windows)))]
    Ok(OwnedChild {
        child: command.spawn()?,
    })
}

impl OwnedChild {
    #[must_use]
    pub fn id(&self) -> Option<u32> {
        self.child.id()
    }

    pub fn take_stdin(&mut self) -> Option<ChildStdin> {
        self.child.stdin.take()
    }

    pub fn take_stdout(&mut self) -> Option<ChildStdout> {
        self.child.stdout.take()
    }

    pub fn take_stderr(&mut self) -> Option<ChildStderr> {
        self.child.stderr.take()
    }

    /// Wait for the direct child. Descendants may still be running.
    ///
    /// # Errors
    /// Returns the operating system's child wait error.
    pub async fn wait(&mut self) -> io::Result<ExitStatus> {
        self.child.wait().await
    }

    /// Inspect the direct child without waiting for descendants.
    ///
    /// # Errors
    /// Returns the operating system's child wait error.
    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        self.child.try_wait()
    }

    /// Initiate group termination. Call [`Self::kill`] to finish cleanup.
    ///
    /// # Errors
    /// Returns an error if the native group or direct child cannot be signalled.
    pub fn start_kill(&mut self) -> io::Result<()> {
        #[cfg(unix)]
        let group = self.signal(Signal::KILL).map_err(|error| {
            io::Error::new(error.kind(), format!("terminate process group: {error}"))
        });
        #[cfg(windows)]
        let group = self.group.kill_all().map_err(io::Error::other);
        #[cfg(not(any(unix, windows)))]
        let group: io::Result<()> = Ok(());
        let child = self.child.start_kill().map_err(|error| {
            io::Error::new(error.kind(), format!("terminate direct child: {error}"))
        });
        group.and(child)
    }

    /// Terminate the group, reap the direct child, and finish native cleanup.
    ///
    /// Windows retains the Job Object until a successful membership query says
    /// it is empty. Failed termination or membership queries keep cleanup
    /// pending and are retried, so a supervisor cannot release host leases while
    /// the tree may still run. Keep the Tokio runtime alive while this waits.
    ///
    /// # Errors
    /// Returns a termination or wait error after native cleanup has completed.
    pub async fn kill(&mut self) -> io::Result<ExitStatus> {
        let termination = self.start_kill();
        #[cfg(windows)]
        loop {
            if self.group.members().is_ok_and(|members| members.is_empty()) {
                break;
            }
            // A failed query is not evidence of an empty Job. Retain ownership
            // and repeat the kill while Windows completes process teardown.
            let _ = self.group.kill_all();
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
        let status = self.child.wait().await;
        #[cfg(target_os = "macos")]
        let termination = {
            let mut result = termination;
            // Darwin can report EPERM while an already terminated group is
            // disappearing. Reconcile that race after the root was reaped;
            // retain a real permission failure after this bounded retry.
            if status.is_ok() {
                for _ in 0..5 {
                    if !result
                        .as_ref()
                        .is_err_and(|error| error.kind() == io::ErrorKind::PermissionDenied)
                    {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                    result = self.signal(Signal::KILL);
                }
            }
            result
        };
        #[cfg(unix)]
        if termination.is_ok() && status.is_ok() {
            // Cleanup consumed this group identity. A later Drop must not
            // signal an unrelated group after the OS has reused the number.
            self.group = None;
        }
        termination?;
        status
    }

    pub(crate) fn terminate(&mut self) -> io::Result<()> {
        #[cfg(unix)]
        {
            self.signal(Signal::TERM)
        }
        #[cfg(not(unix))]
        {
            self.start_kill()
        }
    }

    #[cfg(unix)]
    fn signal(&self, signal: Signal) -> io::Result<()> {
        if let Some(group) = self.group {
            match kill_process_group(group, signal) {
                Ok(()) | Err(rustix::io::Errno::SRCH) => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }
}

impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.start_kill();
    }
}
