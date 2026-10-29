use std::process::ExitStatus;
#[cfg(windows)]
use std::time::Duration;

use anyhow::{Context, Result};
#[cfg(unix)]
use rustix::process::{Pid, Signal, kill_process_group};
use tokio::process::{Child, Command};

pub(super) struct ProcessGroup {
    #[cfg(unix)]
    pid: Option<u32>,
    #[cfg(windows)]
    native: processkit::ProcessGroup,
}
impl ProcessGroup {
    pub(super) fn spawn(mut command: Command, captured: bool) -> Result<(Child, Self)> {
        command.kill_on_drop(true);
        #[cfg(unix)]
        if captured {
            command.process_group(0);
        }
        #[cfg(windows)]
        let native = processkit::ProcessGroup::new()?;
        #[cfg(windows)]
        let child = native.spawn(command)?;
        #[cfg(not(windows))]
        let child = command.spawn()?;
        let group = Self {
            #[cfg(unix)]
            pid: if captured { child.id() } else { None },
            #[cfg(windows)]
            native,
        };
        #[cfg(not(unix))]
        let _ = captured;
        Ok((child, group))
    }

    fn kill(&self) -> Result<()> {
        #[cfg(unix)]
        if let Some(pid) = self
            .pid
            .and_then(|pid| i32::try_from(pid).ok())
            .and_then(Pid::from_raw)
        {
            match kill_process_group(pid, Signal::KILL) {
                Ok(()) | Err(rustix::io::Errno::SRCH) => {}
                Err(error) => return Err(error.into()),
            }
        }
        #[cfg(windows)]
        self.native.kill_all()?;
        Ok(())
    }

    pub(super) async fn reap(&mut self, child: &mut Child) -> Result<ExitStatus> {
        let group = self.kill();
        let root = child.start_kill();
        #[cfg(windows)]
        loop {
            if self
                .native
                .members()
                .is_ok_and(|members| members.is_empty())
            {
                break;
            }
            let _ = self.native.kill_all();
            let _ = child.start_kill();
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        let status = child.wait().await;
        group.context("terminate Claude Code auth process group")?;
        root.context("terminate Claude Code auth child")?;
        let status = status.context("reap Claude Code auth")?;
        #[cfg(unix)]
        {
            // Cleanup consumed this identity. Drop must not signal a reused group.
            self.pid = None;
        }
        Ok(status)
    }
}
impl Drop for ProcessGroup {
    fn drop(&mut self) {
        let _ = self.kill();
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::ProcessGroup;

    #[tokio::test]
    async fn successful_auth_reap_retires_the_unix_group_identity() -> anyhow::Result<()> {
        let mut command = tokio::process::Command::new(std::env::current_exe()?);
        command
            .arg("--list")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        let (mut child, mut group) = ProcessGroup::spawn(command, true)?;
        assert!(group.pid.is_some(), "captured child must own its group");
        tokio::time::timeout(std::time::Duration::from_secs(3), group.reap(&mut child)).await??;
        assert!(
            child.id().is_none(),
            "successful cleanup must reap its child"
        );
        assert!(group.pid.is_none(), "completed ownership must be retired");
        Ok(())
    }
}
