use std::{path::PathBuf, time::Duration};

use anyhow::{Result, ensure};

/// Fixed bounds for two lazy language hosts, never one process per agent.
#[derive(Clone, Debug)]
pub struct RuntimeConfig {
    pub node: PathBuf,
    pub python: PathBuf,
    pub max_pending: usize,
    pub max_frame_bytes: usize,
    pub max_buffer_bytes: usize,
    pub call_timeout: Duration,
    pub cleanup_timeout: Duration,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            node: PathBuf::from("node"),
            python: PathBuf::from("python3"),
            max_pending: 64,
            max_frame_bytes: 1024 * 1024,
            max_buffer_bytes: 4 * 1024 * 1024,
            call_timeout: Duration::from_secs(120),
            cleanup_timeout: Duration::from_secs(5),
        }
    }
}

impl RuntimeConfig {
    pub(super) fn validate(&self) -> Result<()> {
        ensure!(
            !self.node.as_os_str().is_empty(),
            "Node executable is required"
        );
        ensure!(
            !self.python.as_os_str().is_empty(),
            "Python executable is required"
        );
        ensure!(
            (1..=64).contains(&self.max_pending),
            "runtime pending calls must be 1..64"
        );
        ensure!(
            (1024..=1024 * 1024).contains(&self.max_frame_bytes),
            "runtime frame limit must be 1 KiB..1 MiB"
        );
        ensure!(
            self.max_buffer_bytes >= self.max_frame_bytes
                && self.max_buffer_bytes <= 16 * 1024 * 1024,
            "runtime outbound buffer must fit a frame and stay within 16 MiB"
        );
        ensure!(
            !self.call_timeout.is_zero() && self.call_timeout <= Duration::from_hours(24),
            "runtime call timeout must be positive and at most one day"
        );
        ensure!(
            !self.cleanup_timeout.is_zero() && self.cleanup_timeout <= Duration::from_secs(30),
            "runtime cleanup timeout must be positive and at most 30 seconds"
        );
        Ok(())
    }
}
