//! Cgroup v2 constraint adapter over maintained procfs record parsers.
//!
//! Only visible ancestry is reported. A delegated or hidden root is explicitly
//! incomplete and cannot establish an independent host budget.

use std::{
    fs::File,
    io::{self, Read},
    os::unix::fs::MetadataExt,
    path::{Component, Path},
};

use procfs_core::{FromRead, ProcessCGroups, process::MountInfos};

use super::{
    HostObservations, Observation, ObservationAvailability, ObservationSource,
    ResourceDomainObservation,
};

#[cfg(target_os = "linux")]
const MAX_PROC_BYTES: usize = 256 * 1024;
const MAX_VALUE_BYTES: usize = 16 * 1024;
const MAX_ANCESTORS: usize = 64;

#[cfg(target_os = "linux")]
pub(super) fn apply(observations: &mut HostObservations) {
    let result = (|| {
        let membership = read_bounded(Path::new("/proc/self/cgroup"), MAX_VALUE_BYTES)?;
        let mounts = read_bounded(Path::new("/proc/self/mountinfo"), MAX_PROC_BYTES)?;
        let domains = discover(&membership, &mounts)?;
        if membership != read_bounded(Path::new("/proc/self/cgroup"), MAX_VALUE_BYTES)? {
            return Err(io::Error::other("resource domain changed during sampling"));
        }
        Ok(domains)
    })();
    match result {
        Ok((domains, complete)) => apply_domains(observations, domains, complete),
        Err(_) => observations.domain_visibility = ObservationAvailability::Unavailable,
    }
}

pub(super) fn apply_domains(
    observations: &mut HostObservations,
    domains: Vec<ResourceDomainObservation>,
    complete: bool,
) {
    for domain in &domains {
        tighten(&mut observations.cpu_millis, domain.cpu_millis);
        tighten(&mut observations.memory_bytes, domain.memory_bytes);
        tighten(
            &mut observations.available_memory_bytes,
            domain.available_memory_bytes,
        );
    }
    observations.domains.extend(domains);
    observations.domain_visibility = if complete {
        // A `/` mount root can itself be namespaced. Visible ancestry does not
        // prove that every host ancestor is accessible to this process.
        ObservationAvailability::Estimated
    } else {
        ObservationAvailability::Unsupported
    };
}

fn tighten<T: Copy + Ord>(observation: &mut Observation<T>, limit: Option<T>) {
    if let Some(limit) = limit {
        observation.value = Some(
            observation
                .value
                .map_or(limit, |current| current.min(limit)),
        );
        observation.source = ObservationSource::LinuxCgroupV2;
        observation.availability = ObservationAvailability::Measured;
    }
}

fn discover(
    membership: &[u8],
    mounts: &[u8],
) -> io::Result<(Vec<ResourceDomainObservation>, bool)> {
    let groups = ProcessCGroups::from_read(membership).map_err(io::Error::other)?;
    let mounts = MountInfos::from_read(mounts).map_err(io::Error::other)?;
    let group = groups
        .0
        .iter()
        .find(|group| group.hierarchy == 0 && group.controllers.is_empty())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::Unsupported,
                "cgroup v1 has no supported adapter",
            )
        })?;
    let group_path = checked_path(&group.pathname)?;
    let mount = mounts
        .0
        .iter()
        .filter(|mount| mount.fs_type == "cgroup2")
        .filter(|mount| group_path.starts_with(&mount.root))
        .max_by_key(|mount| mount.root.len())
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "cgroup v2 mount is not visible"))?;
    let root = checked_path(&mount.root)?;
    let relative = group_path.strip_prefix(root).map_err(io::Error::other)?;
    let current = mount.mount_point.join(relative);
    let mut paths = Vec::new();
    for ancestor in current.ancestors() {
        if !ancestor.starts_with(&mount.mount_point) {
            break;
        }
        if paths.len() >= MAX_ANCESTORS {
            return Err(io::Error::other(
                "cgroup ancestry exceeds observation bound",
            ));
        }
        paths.push(ancestor.to_path_buf());
    }
    if paths.last().is_none_or(|path| path != &mount.mount_point) {
        return Err(io::Error::other("cgroup mount ancestry is incomplete"));
    }
    let ids = paths
        .iter()
        .map(|path| {
            let metadata = std::fs::metadata(path)?;
            Ok(format!("cgroup-v2:{}:{}", metadata.dev(), metadata.ino()))
        })
        .collect::<io::Result<Vec<_>>>()?;
    let mut domains = Vec::with_capacity(paths.len());
    for (index, path) in paths.iter().enumerate() {
        let memory_bytes = scalar(path, "memory.max")?;
        let memory_current = scalar(path, "memory.current")?;
        let max_processes = scalar(path, "pids.max")?;
        let process_current = scalar(path, "pids.current")?;
        let cpu_quota = cpu_quota(path)?;
        let cpuset = cpu_set(path)?;
        domains.push(ResourceDomainObservation {
            id: ids[index].clone(),
            parent: ids
                .get(index + 1)
                .cloned()
                .or_else(|| (mount.root == "/").then(|| "host".into())),
            cpu_millis: match (cpu_quota, cpuset) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            },
            memory_bytes,
            available_memory_bytes: memory_bytes
                .zip(memory_current)
                .map(|(limit, used)| limit.saturating_sub(used)),
            max_processes,
            available_processes: max_processes
                .zip(process_current)
                .map(|(limit, used)| limit.saturating_sub(used)),
        });
    }
    Ok((domains, mount.root == "/"))
}

fn checked_path(path: &str) -> io::Result<&Path> {
    let path = Path::new(path);
    if !path.is_absolute()
        || path.as_os_str().len() > 4_096
        || path.components().any(|part| {
            matches!(
                part,
                Component::ParentDir | Component::CurDir | Component::Prefix(_)
            )
        })
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid cgroup path",
        ));
    }
    Ok(path)
}

fn read_bounded(path: &Path, maximum: usize) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::with_capacity(maximum.min(1_024));
    File::open(path)?
        .take(u64::try_from(maximum).unwrap_or(u64::MAX).saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() > maximum {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "capacity observation exceeds read bound",
        ));
    }
    Ok(bytes)
}

fn value(path: &Path, name: &str) -> io::Result<Option<String>> {
    match read_bounded(&path.join(name), MAX_VALUE_BYTES) {
        Ok(bytes) => String::from_utf8(bytes).map(Some).map_err(io::Error::other),
        // A missing controller is not an unlimited controller. Domain visibility
        // is retained, but its unsupported dimension stays absent.
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn scalar(path: &Path, name: &str) -> io::Result<Option<u64>> {
    let Some(value) = value(path, name)? else {
        return Ok(None);
    };
    if value.trim() == "max" {
        Ok(None)
    } else {
        value.trim().parse().map(Some).map_err(io::Error::other)
    }
}

fn cpu_quota(path: &Path) -> io::Result<Option<u32>> {
    let Some(value) = value(path, "cpu.max")? else {
        return Ok(None);
    };
    let mut fields = value.split_whitespace();
    let quota = fields
        .next()
        .ok_or_else(|| io::Error::other("missing CPU quota"))?;
    let period = fields
        .next()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .ok_or_else(|| io::Error::other("invalid CPU period"))?;
    if fields.next().is_some() {
        return Err(io::Error::other("extra CPU quota fields"));
    }
    if quota == "max" {
        return Ok(None);
    }
    let quota = quota.parse::<u64>().map_err(io::Error::other)?;
    let millis = u128::from(quota).saturating_mul(1_000) / u128::from(period);
    Ok(Some(u32::try_from(millis).unwrap_or(u32::MAX)))
}

fn cpu_set(path: &Path) -> io::Result<Option<u32>> {
    let Some(value) = value(path, "cpuset.cpus.effective")? else {
        return Ok(None);
    };
    if value.trim().is_empty() {
        return Ok(None);
    }
    let mut count = 0_u32;
    let mut previous = None;
    for range in value.trim().split(',') {
        let mut values = range.split('-');
        let first = values
            .next()
            .and_then(|value| value.parse::<u32>().ok())
            .ok_or_else(|| io::Error::other("invalid CPU set"))?;
        let last = values
            .next()
            .map(str::parse::<u32>)
            .transpose()
            .map_err(io::Error::other)?
            .unwrap_or(first);
        if values.next().is_some() || last < first || previous.is_some_and(|value| first <= value) {
            return Err(io::Error::other("invalid or overlapping CPU set"));
        }
        count = count.saturating_add(last.saturating_sub(first).saturating_add(1));
        previous = Some(last);
    }
    Ok(Some(count.saturating_mul(1_000)))
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "Constraint fixtures must fail at their assertion site."
)]
mod tests {
    use super::*;

    fn fixture(root: &Path, path: &str) -> (Vec<u8>, Vec<u8>) {
        (
            format!("0::{path}\n").into_bytes(),
            format!("30 20 0:28 / {} rw - cgroup2 cgroup rw\n", root.display()).into_bytes(),
        )
    }

    #[test]
    fn nested_limits_tighten_each_dimension_without_losing_ancestor_identity() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let child = root.join("parent/child");
        std::fs::create_dir_all(&child).unwrap();
        std::fs::write(root.join("memory.max"), "1000\n").unwrap();
        std::fs::write(root.join("memory.current"), "250\n").unwrap();
        std::fs::write(root.join("parent/cpu.max"), "50000 100000\n").unwrap();
        std::fs::write(child.join("memory.max"), "900\n").unwrap();
        std::fs::write(child.join("memory.current"), "800\n").unwrap();
        std::fs::write(child.join("pids.max"), "20\n").unwrap();
        std::fs::write(child.join("pids.current"), "3\n").unwrap();
        std::fs::write(child.join("cpuset.cpus.effective"), "0-2,4\n").unwrap();
        let (membership, mounts) = fixture(root, "/parent/child");
        let (domains, complete) = discover(&membership, &mounts).unwrap();
        assert!(complete);
        assert_eq!(domains.len(), 3);
        assert_eq!(domains[0].memory_bytes, Some(900));
        assert_eq!(domains[0].available_memory_bytes, Some(100));
        assert_eq!(domains[0].available_processes, Some(17));
        assert_eq!(domains[0].cpu_millis, Some(4_000));
        assert_eq!(domains[1].cpu_millis, Some(500));
        assert_eq!(domains[0].parent.as_ref(), Some(&domains[1].id));
        assert_eq!(domains[2].parent.as_deref(), Some("host"));
        let mut observations = super::super::HostSampler::default().sample();
        apply_domains(&mut observations, domains, true);
        assert_eq!(observations.cpu_millis.value, Some(500));
        assert_eq!(observations.memory_bytes.value, Some(900));
        assert_eq!(observations.available_memory_bytes.value, Some(100));
    }

    #[test]
    fn unsafe_or_incomplete_scopes_and_malformed_values_never_mean_unlimited() {
        let temp = tempfile::tempdir().unwrap();
        let (membership, mounts) = fixture(temp.path(), "/../escape");
        assert!(discover(&membership, &mounts).is_err());
        assert!(discover(b"5:cpu:/legacy\n", &mounts).is_err());
        let mounts = format!(
            "30 20 0:28 /delegated {} rw - cgroup2 cgroup rw\n",
            temp.path().display()
        );
        let (_, complete) = discover(b"0::/delegated\n", mounts.as_bytes()).unwrap();
        assert!(!complete);
        std::fs::write(temp.path().join("cpu.max"), "1000 0\n").unwrap();
        assert!(cpu_quota(temp.path()).is_err());
        std::fs::write(temp.path().join("cpu.max"), "max 100000\n").unwrap();
        assert_eq!(cpu_quota(temp.path()).unwrap(), None);
        std::fs::write(temp.path().join("cpuset.cpus.effective"), "0-3,2-5\n").unwrap();
        assert!(cpu_set(temp.path()).is_err());
        std::fs::write(
            temp.path().join("memory.max"),
            vec![b'0'; MAX_VALUE_BYTES + 1],
        )
        .unwrap();
        assert!(scalar(temp.path(), "memory.max").is_err());
    }
}
