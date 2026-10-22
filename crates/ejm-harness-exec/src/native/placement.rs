use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Private placement evidence is compared exactly, without persisting paths.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Placement {
    affinity: Vec<u32>,
    namespaces: Vec<Option<(u64, u64)>>,
}

pub(super) fn supported() -> Result<()> {
    anyhow::ensure!(
        !cfg!(windows),
        "shared native owner placement is unsupported on Windows: enclosing Job and affinity identity cannot be established; use local execution"
    );
    Ok(())
}

impl Placement {
    pub fn observe() -> Result<Self> {
        supported()?;
        #[cfg(target_os = "linux")]
        {
            linux()
        }
        #[cfg(not(target_os = "linux"))]
        {
            Ok(Self {
                affinity: Vec::new(),
                namespaces: Vec::new(),
            })
        }
    }
}

#[cfg(target_os = "linux")]
fn linux() -> Result<Placement> {
    use std::os::unix::fs::MetadataExt;

    use anyhow::{Context, ensure};
    use rustix::thread::{CpuSet, sched_getaffinity};

    let cpus = sched_getaffinity(None).context("read native caller affinity")?;
    let affinity = (0..CpuSet::MAX_CPU)
        .filter(|cpu| cpus.is_set(*cpu))
        .map(u32::try_from)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    ensure!(!affinity.is_empty(), "native caller affinity is empty");
    let mut namespaces = Vec::with_capacity(10);
    for name in [
        "cgroup",
        "ipc",
        "mnt",
        "net",
        "pid",
        "pid_for_children",
        "user",
        "uts",
        "time",
        "time_for_children",
    ] {
        let path = format!("/proc/thread-self/ns/{name}");
        match std::fs::metadata(&path) {
            Ok(metadata) => namespaces.push(Some((metadata.dev(), metadata.ino()))),
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound && name.starts_with("time") =>
            {
                namespaces.push(None);
            }
            Err(error) => return Err(error).context("read native caller namespace identity"),
        }
    }
    // HostSampler's cgroup ancestry describes the process leader. A differently
    // placed thread must not borrow that evidence for an owner launch.
    ensure!(
        membership("/proc/thread-self/cgroup")? == membership("/proc/self/cgroup")?,
        "native thread cgroup differs from process observations"
    );
    Ok(Placement {
        affinity,
        namespaces,
    })
}

#[cfg(target_os = "linux")]
fn membership(path: &str) -> Result<Vec<u8>> {
    use std::io::Read;

    const MAX_BYTES: u64 = 16 * 1024;
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)?;
    anyhow::ensure!(
        bytes.len() as u64 <= MAX_BYTES,
        "native cgroup membership exceeds bound"
    );
    Ok(bytes)
}
