//! Read-only process observations and best-effort identity-checked cleanup.

use std::{collections::HashMap, io};

// Linux exposes a short stat record, but still bound reads from procfs.
#[cfg(any(target_os = "linux", test))]
const MAX_STAT_BYTES: usize = 16 * 1024;

#[derive(Debug)]
pub(crate) struct ProcessInfo {
    pub parent: Option<u32>,
    pub start: u64,
    pub memory: u64,
    pub zombie: bool,
}

#[cfg(target_os = "linux")]
pub(crate) use linux::{Sampler, supported};
#[cfg(not(target_os = "linux"))]
pub(crate) use native::{Sampler, supported};

#[cfg(target_os = "linux")]
mod linux {
    use std::{fs::File, io::Read};

    use rustix::process::{Pid, Signal, kill_process};

    use super::{HashMap, MAX_STAT_BYTES, ProcessInfo, io};

    pub(crate) struct Sampler {
        page_bytes: u64,
    }

    pub(crate) fn supported() -> bool {
        true
    }

    impl Default for Sampler {
        fn default() -> Self {
            Self {
                page_bytes: u64::try_from(rustix::param::page_size()).unwrap_or(0),
            }
        }
    }

    impl Sampler {
        pub fn sample_all(&mut self) -> io::Result<HashMap<u32, ProcessInfo>> {
            if self.page_bytes == 0 {
                return Err(io::Error::other("cannot read the system page size"));
            }
            let mut processes = HashMap::new();
            for entry in std::fs::read_dir("/proc")? {
                let entry = entry?;
                let Some(pid) = entry
                    .file_name()
                    .to_str()
                    .and_then(|name| name.parse::<u32>().ok())
                else {
                    continue;
                };
                // Processes may exit or become unreadable during enumeration.
                if let Ok(process) = self.read(pid) {
                    processes.insert(pid, process);
                }
            }
            Ok(processes)
        }

        fn read(&self, pid: u32) -> io::Result<ProcessInfo> {
            let file = File::open(format!("/proc/{pid}/stat"))?;
            let mut bytes = Vec::with_capacity(512);
            file.take((MAX_STAT_BYTES + 1) as u64)
                .read_to_end(&mut bytes)?;
            if bytes.len() > MAX_STAT_BYTES {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "stat too large"));
            }
            let (observed_pid, process) = super::parse_linux_stat(&bytes, self.page_bytes)
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "invalid process stat")
                })?;
            if observed_pid != pid {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "process ID changed",
                ));
            }
            Ok(process)
        }

        pub fn kill_known(&mut self, tracked: &HashMap<u32, u64>) {
            for (&pid, &start) in tracked {
                if let Ok(process) = self.read(pid)
                    && process.start == start
                    && !process.zombie
                    && let Ok(pid) = i32::try_from(pid)
                    && let Some(pid) = Pid::from_raw(pid)
                {
                    // The check reduces PID reuse risk but is not atomic with kill.
                    let _ = kill_process(pid, Signal::KILL);
                }
            }
        }
    }
}

#[cfg(not(target_os = "linux"))]
mod native {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessStatus, ProcessesToUpdate, System};

    use super::{HashMap, ProcessInfo, io};

    pub(crate) struct Sampler {
        system: System,
    }

    pub(crate) fn supported() -> bool {
        sysinfo::IS_SUPPORTED_SYSTEM
    }

    impl Default for Sampler {
        fn default() -> Self {
            Self {
                system: System::new(),
            }
        }
    }

    impl Sampler {
        #[allow(
            clippy::unnecessary_wraps,
            reason = "Match the fallible procfs sampler interface on Linux."
        )]
        pub fn sample_all(&mut self) -> io::Result<HashMap<u32, ProcessInfo>> {
            self.system.refresh_processes_specifics(
                ProcessesToUpdate::All,
                true,
                ProcessRefreshKind::nothing().with_memory(),
            );
            Ok(self
                .system
                .processes()
                .iter()
                .map(|(pid, process)| {
                    (
                        pid.as_u32(),
                        ProcessInfo {
                            parent: process.parent().map(Pid::as_u32),
                            start: process.start_time(),
                            memory: process.memory(),
                            zombie: process.status() == ProcessStatus::Zombie,
                        },
                    )
                })
                .collect())
        }

        pub fn kill_known(&mut self, tracked: &HashMap<u32, u64>) {
            let pids: Vec<_> = tracked.keys().copied().map(Pid::from_u32).collect();
            self.system.refresh_processes_specifics(
                ProcessesToUpdate::Some(&pids),
                true,
                ProcessRefreshKind::nothing(),
            );
            for (pid, start) in tracked {
                if let Some(process) = self.system.process(Pid::from_u32(*pid))
                    && process.start_time() == *start
                    && process.status() != ProcessStatus::Zombie
                {
                    process.kill();
                }
            }
        }
    }
}

// Keep the parser available to fixtures on other hosts.
#[cfg(any(target_os = "linux", test))]
fn parse_linux_stat(bytes: &[u8], page_bytes: u64) -> Option<(u32, ProcessInfo)> {
    use procfs_core::{FromRead, process::Stat};

    if bytes.len() > MAX_STAT_BYTES {
        return None;
    }
    let stat = Stat::from_read(bytes).ok()?;
    let pid = u32::try_from(stat.pid).ok()?;
    let parent = u32::try_from(stat.ppid).ok()?;
    Some((
        pid,
        ProcessInfo {
            parent: (parent != 0).then_some(parent),
            start: stat.starttime,
            memory: stat.rss.saturating_mul(page_bytes),
            zombie: matches!(stat.state, 'Z' | 'X' | 'x'),
        },
    ))
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "Parser fixtures assert the presence of valid records."
)]
mod tests {
    use super::{MAX_STAT_BYTES, parse_linux_stat};

    fn stat(pid: i64, comm: &[u8], state: &str, parent: i64, start: u64, rss: i64) -> Vec<u8> {
        let mut bytes = format!("{pid} (").into_bytes();
        bytes.extend_from_slice(comm);
        // Supply every required field, from state (3) through cnswap (37).
        let mut fields = vec!["0".to_owned(); 35];
        fields[0] = state.to_owned();
        fields[1] = parent.to_string();
        fields[19] = start.to_string();
        fields[21] = rss.to_string();
        bytes.extend_from_slice(format!(") {}\n", fields.join(" ")).as_bytes());
        bytes
    }

    #[test]
    fn linux_stat_handles_comm_bytes_and_preserves_start_ticks() {
        let bytes = stat(42, b"job (worker)\n\xff)", "S", 12, 123_456_789, 32);
        let (pid, process) = parse_linux_stat(&bytes, 4096).unwrap();
        assert_eq!(pid, 42);
        assert_eq!(process.parent, Some(12));
        assert_eq!(process.start, 123_456_789);
        assert_eq!(process.memory, 32 * 4096);
        assert!(!process.zombie);
    }

    #[test]
    fn linux_stat_identifies_dead_states_and_kernel_parent() {
        for state in ["Z", "X", "x"] {
            let bytes = stat(42, b"exited", state, 0, 1, 0);
            let (_, process) = parse_linux_stat(&bytes, 16_384).unwrap();
            assert_eq!(process.parent, None);
            assert_eq!(process.memory, 0);
            assert!(process.zombie);
        }
    }

    #[test]
    fn linux_stat_rejects_negative_rss() {
        assert!(parse_linux_stat(&stat(42, b"job", "S", 1, 2, -1), 4096).is_none());
    }

    #[test]
    fn linux_stat_saturates_resident_byte_count() {
        let bytes = stat(42, b"job", "S", 1, 2, i64::MAX);
        let (_, process) = parse_linux_stat(&bytes, u64::MAX).unwrap();
        assert_eq!(process.memory, u64::MAX);
    }

    #[test]
    fn linux_stat_rejects_negative_or_overflowing_identifiers() {
        let overflow = i64::from(i32::MAX) + 1;
        for (pid, parent) in [(-1, 1), (overflow, 1), (42, -1), (42, overflow)] {
            assert!(parse_linux_stat(&stat(pid, b"job", "S", parent, 2, 3), 4096).is_none());
        }
    }

    #[test]
    fn linux_stat_rejects_oversized_records() {
        let mut bytes = stat(42, b"job", "S", 1, 2, 3);
        bytes.resize(MAX_STAT_BYTES + 1, b' ');
        assert!(parse_linux_stat(&bytes, 4096).is_none());
    }

    #[test]
    fn linux_stat_rejects_malformed_or_truncated_records() {
        for bytes in [
            b"42 (job) S 1".as_slice(),
            b"42 (job S 1".as_slice(),
            b"not-a-pid (job) S 1".as_slice(),
            b"42 (job)S 1".as_slice(),
        ] {
            assert!(parse_linux_stat(bytes, 4096).is_none());
        }
        let mut bytes = stat(42, b"job", "S", 1, 2, 3);
        bytes.pop();
        bytes.push(b'x');
        assert!(parse_linux_stat(&bytes, 4096).is_none());
    }
}
