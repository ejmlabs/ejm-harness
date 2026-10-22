//! Typed native observations without reading process arguments or environments.

use std::collections::HashSet;

use libproc::{
    bsd_info::BSDInfo,
    proc_pid::pidinfo,
    processes::{ProcFilter, pids_by_type},
    task_info::TaskAllInfo,
};
use rustix::process::{Pid, Signal, kill_process, test_kill_process};
use sysinfo::ProcessStatus;

use super::{
    HashMap, ProcessInfo, ProcessMetadata, ProcessSample, ProcessSeed, bounded_sampling_failure, io,
};

const MICROS_PER_SECOND: u64 = 1_000_000;
// XNU's PROC_PIDTBSDINFO includes unreaped zombies only for a nonzero argument.
const BSDINFO_INCLUDE_ZOMBIES: u64 = 1;
// Darwin's proc_info.h defines PROC_FLAG_INEXIT as exit progress, not completion.
const BSDINFO_FLAG_INEXIT: u32 = 4;

#[derive(Default)]
pub(crate) struct Sampler {}

#[derive(Clone, Copy)]
struct Topology {
    parent: Option<u32>,
    start: u64,
    status: u32,
    zombie: bool,
    in_exit: bool,
}

enum IdentityObservation {
    Readable(ProcessInfo),
    Uncertain(io::Result<Topology>),
    Exiting,
    Exited,
    Replaced,
}

pub(crate) fn supported() -> bool {
    true
}

impl Sampler {
    #[allow(
        clippy::unused_self,
        reason = "Match the stateful sampler interface on other platforms."
    )]
    pub fn observe(&mut self, pid: u32) -> io::Result<ProcessInfo> {
        // A single native call pairs the process identity with its resident memory.
        let info = pidinfo::<TaskAllInfo>(native_pid(pid)?, 0).map_err(io::Error::other)?;
        let topology = Topology::from_bsd(pid, &info.pbsd)?;
        Ok(ProcessInfo {
            parent: topology.parent,
            start: topology.start,
            memory: info.ptinfo.pti_resident_size,
            zombie: topology.zombie,
        })
    }

    #[allow(
        dead_code,
        reason = "Retain the complete sampler interface for native diagnostics and tests."
    )]
    pub fn sample_all(&mut self) -> io::Result<HashMap<u32, ProcessInfo>> {
        let pids = pids_by_type(ProcFilter::All)?;
        let mut processes = HashMap::with_capacity(pids.len());
        for pid in pids {
            // Processes may exit or become unreadable during enumeration.
            if let Ok(process) = self.observe(pid) {
                processes.insert(pid, process);
            }
        }
        Ok(processes)
    }

    pub fn sample_owned(&mut self, seeds: &[ProcessSeed]) -> io::Result<ProcessSample> {
        let mut topology = Self::topology()?;
        let mut cached = HashMap::new();
        let mut unavailable = HashSet::new();
        let mut exited = HashSet::new();
        let mut sample = ProcessSample::default();
        // A root may start after PID enumeration, or its BSD observation may
        // fail. Probe each missing seed once without treating failure as exit.
        for seed in seeds {
            if topology.contains_key(&seed.pid)
                || unavailable.contains(&seed.pid)
                || exited.contains(&seed.pid)
            {
                continue;
            }
            match self.observe(seed.pid) {
                Ok(process) => {
                    topology.insert(
                        seed.pid,
                        Topology {
                            parent: process.parent,
                            start: process.start,
                            // Cached paired RSS supplies this entry's final observation.
                            status: 0,
                            zombie: process.zombie,
                            in_exit: false,
                        },
                    );
                    cached.insert(seed.pid, process);
                }
                Err(error) => match observe_topology(seed.pid) {
                    Ok(process) => {
                        topology.insert(seed.pid, process);
                    }
                    Err(_) if process_absent(seed.pid) => {
                        exited.insert(seed.pid);
                    }
                    Err(topology_error) => {
                        sample.sampling_failures.insert(
                            seed.pid,
                            bounded_sampling_failure(format!(
                                "stage=missing_seed; pid={}; expected_birth={:?}; task_all={error}; bsd={topology_error}; absence=unconfirmed",
                                seed.pid, seed.start,
                            )),
                        );
                        unavailable.insert(seed.pid);
                    }
                },
            }
        }
        for seed in seeds {
            if unavailable.contains(&seed.pid) {
                sample
                    .uncertain
                    .entry(seed.pid)
                    .and_modify(|process| {
                        if process.start != seed.start {
                            process.start = None;
                        }
                    })
                    .or_insert(ProcessMetadata {
                        parent: None,
                        start: seed.start,
                    });
            }
        }
        for pid in select_owned(&topology, seeds) {
            let observed = cached.remove(&pid).map_or_else(|| self.observe(pid), Ok);
            record_observation(&mut sample, pid, topology[&pid], &observed);
        }
        Ok(sample)
    }

    fn topology() -> io::Result<HashMap<u32, Topology>> {
        let pids = pids_by_type(ProcFilter::All)?;
        let mut topology = HashMap::with_capacity(pids.len());
        for pid in pids {
            if let Ok(process) = observe_topology(pid) {
                topology.insert(pid, process);
            }
        }
        Ok(topology)
    }

    pub fn kill_known(&mut self, tracked: &HashMap<u32, u64>) {
        for (&pid, &start) in tracked {
            if let Ok(process) = self.observe(pid)
                && process.start == start
                && !process.zombie
                && let Ok(pid) = i32::try_from(pid)
                && let Some(pid) = Pid::from_raw(pid)
            {
                // The identity check reduces PID reuse risk but is not atomic with kill.
                let _ = kill_process(pid, Signal::KILL);
            }
        }
    }
}

fn native_pid(pid: u32) -> io::Result<i32> {
    i32::try_from(pid)
        .ok()
        .filter(|pid| *pid > 0)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid process ID"))
}

fn observe_topology(pid: u32) -> io::Result<Topology> {
    let birth =
        pidinfo::<BSDInfo>(native_pid(pid)?, BSDINFO_INCLUDE_ZOMBIES).map_err(io::Error::other)?;
    Topology::from_bsd(pid, &birth)
}

fn process_absent(pid: u32) -> bool {
    native_pid(pid)
        .ok()
        .and_then(Pid::from_raw)
        .is_some_and(|pid| test_kill_process(pid) == Err(rustix::io::Errno::SRCH))
}

fn identity_observation(
    start: u64,
    observed: &io::Result<ProcessInfo>,
    observe_topology: impl FnOnce() -> io::Result<Topology>,
    process_absent: impl FnOnce() -> bool,
) -> IdentityObservation {
    match observed {
        Ok(process) if process.start != start => return IdentityObservation::Replaced,
        Ok(process) if process.zombie => return IdentityObservation::Exited,
        Ok(process) => return IdentityObservation::Readable(*process),
        Err(_) => {}
    }
    match observe_topology() {
        Ok(process) if process.start != start => IdentityObservation::Replaced,
        Ok(process) if process.zombie => IdentityObservation::Exited,
        Ok(process) if process.in_exit => IdentityObservation::Exiting,
        Err(_) if process_absent() => IdentityObservation::Exited,
        result => IdentityObservation::Uncertain(result),
    }
}

fn failed_observation_note(
    observed: &io::Result<ProcessInfo>,
    topology: &io::Result<Topology>,
) -> String {
    let error = observed
        .as_ref()
        .err()
        .map_or_else(|| "none".to_owned(), ToString::to_string);
    match topology {
        Ok(process) => format!(
            "task_all={error}; bsd=matching_identity; status={}; in_exit={}; absence=not_checked",
            process.status, process.in_exit,
        ),
        Err(topology_error) => {
            format!("task_all={error}; bsd={topology_error}; absence=unconfirmed")
        }
    }
}

impl Topology {
    fn from_bsd(pid: u32, birth: &BSDInfo) -> io::Result<Self> {
        if birth.pbi_pid != pid
            || birth.pbi_start_tvsec == 0
            || birth.pbi_start_tvusec >= MICROS_PER_SECOND
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "process identity unavailable",
            ));
        }
        let start = birth
            .pbi_start_tvsec
            .checked_mul(MICROS_PER_SECOND)
            .and_then(|seconds| seconds.checked_add(birth.pbi_start_tvusec))
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "invalid process birth time")
            })?;
        Ok(Self {
            parent: (birth.pbi_ppid != 0).then_some(birth.pbi_ppid),
            start,
            status: birth.pbi_status,
            zombie: ProcessStatus::from(birth.pbi_status) == ProcessStatus::Zombie,
            in_exit: birth.pbi_flags & BSDINFO_FLAG_INEXIT != 0,
        })
    }
}

fn select_owned(topology: &HashMap<u32, Topology>, seeds: &[ProcessSeed]) -> HashSet<u32> {
    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    for (&pid, process) in topology {
        if let Some(parent) = process.parent {
            children.entry(parent).or_default().push(pid);
        }
    }
    let mut pending: Vec<_> = seeds
        .iter()
        .filter(|seed| {
            topology
                .get(&seed.pid)
                .is_some_and(|process| seed.start.is_none_or(|start| process.start == start))
        })
        .map(|seed| seed.pid)
        .collect();
    let mut selected = HashSet::new();
    while let Some(pid) = pending.pop() {
        if selected.insert(pid)
            && let Some(children) = children.get(&pid)
        {
            pending.extend(children);
        }
    }
    selected
}

fn record_observation(
    sample: &mut ProcessSample,
    pid: u32,
    topology: Topology,
    observed: &io::Result<ProcessInfo>,
) {
    record_observation_with(
        sample,
        pid,
        topology,
        observed,
        || Sampler::default().observe(pid),
        || observe_topology(pid),
        || process_absent(pid),
    );
}

fn record_observation_with(
    sample: &mut ProcessSample,
    pid: u32,
    topology: Topology,
    observed: &io::Result<ProcessInfo>,
    retry: impl FnOnce() -> io::Result<ProcessInfo>,
    mut observe_topology: impl FnMut() -> io::Result<Topology>,
    mut process_absent: impl FnMut() -> bool,
) {
    let mut observation = identity_observation(
        topology.start,
        observed,
        &mut observe_topology,
        &mut process_absent,
    );
    if let IdentityObservation::Uncertain(first_topology) = observation {
        // A failed RSS read can race an exit or a transient inspection failure.
        // Retry once, then require fresh identity evidence or retain uncertainty.
        let retried = retry();
        observation = match identity_observation(
            topology.start,
            &retried,
            observe_topology,
            process_absent,
        ) {
            IdentityObservation::Uncertain(retried_topology) => {
                sample.sampling_failures.insert(
                    pid,
                    bounded_sampling_failure(format!(
                        "stage=owned_rss_retry; pid={pid}; expected_birth={}; first=[{}]; retry=[{}]",
                        topology.start,
                        failed_observation_note(observed, &first_topology),
                        failed_observation_note(&retried, &retried_topology),
                    )),
                );
                IdentityObservation::Uncertain(retried_topology)
            }
            observation => observation,
        };
    }
    let destination = match observation {
        IdentityObservation::Readable(mut process) => {
            // Retain ancestry witnessed during topology selection if the same
            // process was reparented before its memory observation.
            process.parent = topology.parent;
            sample.current.insert(pid, process);
            return;
        }
        IdentityObservation::Uncertain(_) => &mut sample.uncertain,
        IdentityObservation::Exiting => &mut sample.exiting,
        IdentityObservation::Exited => &mut sample.exited,
        IdentityObservation::Replaced => {
            sample.replaced.insert(pid);
            return;
        }
    };
    destination.insert(
        pid,
        ProcessMetadata {
            parent: topology.parent,
            start: Some(topology.start),
        },
    );
}

#[cfg(test)]
mod tests {
    use std::{os::unix::process::ExitStatusExt, process::Stdio, time::Duration};

    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        process::Command,
        time::timeout,
    };

    use super::*;

    #[test]
    fn current_process_has_native_rss_parent_and_precise_birth_identity() -> io::Result<()> {
        let pid = std::process::id();
        let mut sampler = Sampler::default();
        let process = sampler.observe(pid)?;
        let native = pidinfo::<TaskAllInfo>(i32::try_from(pid).map_err(io::Error::other)?, 0)
            .map_err(io::Error::other)?;
        assert_eq!(
            process.parent,
            rustix::process::getppid()
                .map(Pid::as_raw_pid)
                .and_then(|pid| u32::try_from(pid).ok())
        );
        assert_eq!(
            process.start,
            native.pbsd.pbi_start_tvsec * MICROS_PER_SECOND + native.pbsd.pbi_start_tvusec
        );
        // The optional recovery identity also requires a readable boot identity.
        if let Some(public_identity) = crate::observations::process_identity(pid) {
            assert_eq!(
                public_identity.start_time,
                process.start / MICROS_PER_SECOND
            );
        }
        assert!(process.memory > 0);
        assert!(!process.zombie);
        let all = sampler.sample_all()?;
        let enumerated = all.get(&pid).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "current process was not enumerated",
            )
        })?;
        assert_eq!(enumerated.start, process.start);
        assert_eq!(enumerated.parent, process.parent);
        assert!(enumerated.memory > 0);
        assert!(sampler.observe(0).is_err());
        assert!(sampler.observe(u32::MAX).is_err());
        Ok(())
    }

    #[tokio::test]
    async fn native_selection_and_cleanup_require_the_observed_birth_identity() -> io::Result<()> {
        let mut child = Command::new("/bin/cat")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;
        let pid = child
            .id()
            .ok_or_else(|| io::Error::other("child has no PID"))?;
        let mut sampler = Sampler::default();
        let observed = sampler.observe(pid)?;
        assert_eq!(observed.parent, Some(std::process::id()));
        let wrong_start = observed
            .start
            .checked_add(1)
            .ok_or_else(|| io::Error::other("child birth time overflow"))?;
        let selected = sampler.sample_owned(&[ProcessSeed {
            pid,
            start: Some(observed.start),
        }])?;
        assert_eq!(selected.current.len(), 1);
        assert_eq!(selected.current[&pid].start, observed.start);
        assert!(selected.current[&pid].memory > 0);
        assert!(selected.uncertain.is_empty());
        let reused = sampler.sample_owned(&[ProcessSeed {
            pid,
            start: Some(wrong_start),
        }])?;
        assert!(reused.current.is_empty());
        assert!(reused.uncertain.is_empty());
        sampler.kill_known(&HashMap::from([(pid, wrong_start)]));
        let mut input = child
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("missing child stdin"))?;
        let mut output = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("missing child stdout"))?;
        timeout(Duration::from_secs(5), input.write_all(b"x"))
            .await
            .map_err(io::Error::other)??;
        let mut echoed = [0];
        timeout(Duration::from_secs(5), output.read_exact(&mut echoed))
            .await
            .map_err(io::Error::other)??;
        assert_eq!(echoed, *b"x");
        assert!(child.try_wait()?.is_none());
        sampler.kill_known(&HashMap::from([(pid, observed.start)]));
        let status = timeout(Duration::from_secs(5), child.wait())
            .await
            .map_err(io::Error::other)??;
        assert_eq!(status.signal(), Some(Signal::KILL.as_raw()));
        Ok(())
    }

    #[test]
    fn full_topology_selection_retains_orphans_and_ignores_reused_seeds() -> Result<(), &'static str>
    {
        let mut topology: HashMap<_, _> = [
            (1, None),
            (2, Some(1)),
            (3, Some(2)),
            (4, None),
            (5, Some(4)),
            (6, Some(5)),
        ]
        .into_iter()
        .map(|(pid, parent)| {
            (
                pid,
                Topology {
                    parent,
                    start: u64::from(pid) * 10,
                    status: 0,
                    zombie: false,
                    in_exit: false,
                },
            )
        })
        .collect();
        let selected = select_owned(
            &topology,
            &[
                ProcessSeed {
                    pid: 1,
                    start: Some(10),
                },
                ProcessSeed {
                    pid: 5,
                    start: Some(50),
                },
            ],
        );
        assert_eq!(selected, HashSet::from([1, 2, 3, 5, 6]));
        assert!(
            select_owned(
                &topology,
                &[ProcessSeed {
                    pid: 1,
                    start: Some(11),
                }]
            )
            .is_empty()
        );
        assert_eq!(
            select_owned(
                &topology,
                &[
                    ProcessSeed {
                        pid: 1,
                        start: Some(11),
                    },
                    ProcessSeed {
                        pid: 1,
                        start: None,
                    },
                ]
            ),
            HashSet::from([1, 2, 3])
        );
        topology
            .get_mut(&1)
            .ok_or("fixture root PID 1 is missing before zombie transition")?
            .zombie = true;
        topology
            .get_mut(&2)
            .ok_or("fixture child PID 2 is missing before zombie transition")?
            .zombie = true;
        assert_eq!(
            select_owned(
                &topology,
                &[ProcessSeed {
                    pid: 1,
                    start: Some(10),
                }]
            ),
            HashSet::from([1, 2, 3])
        );
        Ok(())
    }

    #[test]
    fn mixed_topology_does_not_turn_pid_replacement_into_exit_ancestry() {
        let topology = HashMap::from([
            (
                100,
                Topology {
                    parent: None,
                    start: 10,
                    status: 0,
                    zombie: false,
                    in_exit: false,
                },
            ),
            (
                101,
                Topology {
                    parent: Some(100),
                    start: 21,
                    status: 0,
                    zombie: false,
                    in_exit: false,
                },
            ),
        ]);
        let mut sample = ProcessSample::default();
        for pid in select_owned(
            &topology,
            &[ProcessSeed {
                pid: 100,
                start: Some(10),
            }],
        ) {
            record_observation(
                &mut sample,
                pid,
                topology[&pid],
                &Ok(ProcessInfo {
                    parent: topology[&pid].parent,
                    start: if pid == 100 { 20 } else { 21 },
                    memory: 10,
                    zombie: false,
                }),
            );
        }
        assert!(sample.replaced.contains(&100));
        assert!(sample.exited.is_empty());
        assert!(sample.uncertain.is_empty());
        assert!(!sample.current.contains_key(&100));
        assert_eq!(sample.current[&101].start, 21);
    }

    #[test]
    fn final_memory_revalidates_identity_and_preserves_observed_ancestry() -> io::Result<()> {
        let pid = std::process::id();
        let observed = Sampler::default().observe(pid)?;
        let topology = Topology {
            parent: observed.parent,
            start: observed.start,
            status: 0,
            zombie: false,
            in_exit: false,
        };
        let mut reused = ProcessSample::default();
        record_observation(
            &mut reused,
            pid,
            topology,
            &Ok(ProcessInfo {
                start: observed.start + 1,
                ..observed
            }),
        );
        assert!(reused.current.is_empty());
        assert!(reused.uncertain.is_empty());
        assert!(reused.exited.is_empty());
        assert!(reused.replaced.contains(&pid));
        assert!(matches!(
            identity_observation(
                observed.start + 1,
                &unreadable(),
                || observe_topology(pid),
                || process_absent(pid),
            ),
            IdentityObservation::Replaced
        ));
        let mut zombie = ProcessSample::default();
        record_observation(
            &mut zombie,
            pid,
            topology,
            &Ok(ProcessInfo {
                zombie: true,
                ..observed
            }),
        );
        assert!(zombie.current.is_empty());
        assert!(zombie.uncertain.is_empty());
        assert_eq!(zombie.exited[&pid].start, Some(observed.start));
        let mut reparented = ProcessSample::default();
        record_observation(
            &mut reparented,
            pid,
            topology,
            &Ok(ProcessInfo {
                parent: None,
                ..observed
            }),
        );
        assert_eq!(reparented.current[&pid].parent, observed.parent);
        assert_eq!(reparented.current[&pid].memory, observed.memory);
        assert!(reparented.uncertain.is_empty());
        assert!(reparented.exited.is_empty());
        Ok(())
    }

    fn unreadable<T>() -> io::Result<T> {
        Err(io::Error::from(io::ErrorKind::PermissionDenied))
    }

    fn scripted_sample(
        topology: Topology,
        memory_reads: Vec<io::Result<ProcessInfo>>,
        topology_reads: Vec<io::Result<Topology>>,
        absence_reads: Vec<bool>,
    ) -> io::Result<ProcessSample> {
        let expected_reads = [
            memory_reads.len(),
            topology_reads.len(),
            absence_reads.len(),
        ];
        let mut memory_reads = memory_reads.into_iter();
        let mut topology_reads = topology_reads.into_iter();
        let mut absence_reads = absence_reads.into_iter();
        let observed = memory_reads
            .next()
            .ok_or_else(|| io::Error::other("missing initial process read"))?;
        let (mut memory_count, mut topology_count, mut absence_count) = (1, 0, 0);
        let mut sample = ProcessSample::default();
        record_observation_with(
            &mut sample,
            7,
            topology,
            &observed,
            || {
                memory_count += 1;
                memory_reads.next().unwrap_or_else(unreadable)
            },
            || {
                topology_count += 1;
                topology_reads.next().unwrap_or_else(unreadable)
            },
            || {
                absence_count += 1;
                absence_reads.next().unwrap_or(false)
            },
        );
        assert_eq!(
            [memory_count, topology_count, absence_count],
            expected_reads
        );
        Ok(sample)
    }

    fn assert_scripted_sample(
        case: &str,
        topology: Topology,
        memory: u64,
        sample: &ProcessSample,
        expected: [usize; 5],
    ) {
        assert_eq!(
            [
                sample.current.len(),
                sample.uncertain.len(),
                sample.exiting.len(),
                sample.exited.len(),
                sample.replaced.len(),
            ],
            expected,
            "{case}"
        );
        if let Some(process) = sample.current.get(&7) {
            assert_eq!(process.start, topology.start, "{case}");
            assert_eq!(process.parent, topology.parent, "{case}");
            assert_eq!(process.memory, memory, "{case}");
            assert!(!process.zombie, "{case}");
        }
        for process in sample
            .uncertain
            .values()
            .chain(sample.exiting.values())
            .chain(sample.exited.values())
        {
            assert_eq!(process.start, Some(topology.start), "{case}");
            assert_eq!(process.parent, topology.parent, "{case}");
        }
        assert_eq!(sample.sampling_failures.len(), expected[1], "{case}");
        for (&pid, detail) in &sample.sampling_failures {
            assert!(sample.uncertain.contains_key(&pid), "{case}");
            assert!(!detail.is_empty() && detail.len() <= 512, "{case}");
        }
    }

    #[test]
    fn failed_memory_read_retries_once_without_losing_accounting() -> io::Result<()> {
        let topology = Topology {
            parent: Some(3),
            start: 100,
            status: 0,
            zombie: false,
            in_exit: false,
        };
        let observed = ProcessInfo {
            parent: None,
            start: topology.start,
            memory: 321,
            zombie: false,
        };
        let cases = [
            (
                "transient RSS failure",
                vec![unreadable(), Ok(observed)],
                vec![Ok(topology)],
                vec![],
                [1, 0, 0, 0, 0],
            ),
            (
                "persistent RSS failure",
                vec![unreadable(), unreadable()],
                vec![Ok(topology), Ok(topology)],
                vec![],
                [0, 1, 0, 0, 0],
            ),
            (
                "persistent RSS and BSD failures",
                vec![unreadable(), unreadable()],
                vec![unreadable(), unreadable()],
                vec![false, false],
                [0, 1, 0, 0, 0],
            ),
            (
                "initial RSS failure with fresh exit progress",
                vec![unreadable()],
                vec![Ok(Topology {
                    parent: None,
                    in_exit: true,
                    ..topology
                })],
                vec![],
                [0, 0, 1, 0, 0],
            ),
            (
                "exit progress after RSS retry",
                vec![unreadable(), unreadable()],
                vec![
                    Ok(topology),
                    Ok(Topology {
                        parent: None,
                        in_exit: true,
                        ..topology
                    }),
                ],
                vec![],
                [0, 0, 1, 0, 0],
            ),
        ];
        for (case, memory, births, absence, expected) in cases {
            let sample = scripted_sample(topology, memory, births, absence)?;
            assert_scripted_sample(case, topology, observed.memory, &sample, expected);
        }
        Ok(())
    }

    #[test]
    fn retried_memory_read_revalidates_replacement_and_exit() -> io::Result<()> {
        let topology = Topology {
            parent: Some(3),
            start: 100,
            status: 0,
            zombie: false,
            in_exit: false,
        };
        let observed = ProcessInfo {
            parent: None,
            start: topology.start,
            memory: 321,
            zombie: false,
        };
        let cases = [
            (
                "replacement in paired RSS read",
                vec![
                    unreadable(),
                    Ok(ProcessInfo {
                        start: topology.start + 1,
                        ..observed
                    }),
                ],
                vec![Ok(topology)],
                vec![],
                [0, 0, 0, 0, 1],
            ),
            (
                "zombie in paired RSS read",
                vec![
                    unreadable(),
                    Ok(ProcessInfo {
                        zombie: true,
                        ..observed
                    }),
                ],
                vec![Ok(topology)],
                vec![],
                [0, 0, 0, 1, 0],
            ),
            (
                "replacement wins over retried exit progress",
                vec![unreadable(), unreadable()],
                vec![
                    Ok(topology),
                    Ok(Topology {
                        start: topology.start + 1,
                        in_exit: true,
                        ..topology
                    }),
                ],
                vec![],
                [0, 0, 0, 0, 1],
            ),
            (
                "zombie after failed retry",
                vec![unreadable(), unreadable()],
                vec![
                    Ok(topology),
                    Ok(Topology {
                        zombie: true,
                        in_exit: true,
                        ..topology
                    }),
                ],
                vec![],
                [0, 0, 0, 1, 0],
            ),
            (
                "ESRCH after failed retry",
                vec![unreadable(), unreadable()],
                vec![Ok(topology), unreadable()],
                vec![true],
                [0, 0, 0, 1, 0],
            ),
        ];
        for (case, memory, births, absence, expected) in cases {
            let sample = scripted_sample(topology, memory, births, absence)?;
            assert_scripted_sample(case, topology, observed.memory, &sample, expected);
        }
        Ok(())
    }

    #[test]
    fn selected_exit_progress_requires_fresh_evidence_after_rss_failure() -> io::Result<()> {
        let topology = Topology {
            parent: Some(3),
            start: 100,
            status: 0,
            zombie: false,
            in_exit: true,
        };
        let observed = ProcessInfo {
            parent: None,
            start: topology.start,
            memory: 321,
            zombie: false,
        };
        let readable = scripted_sample(topology, vec![Ok(observed)], vec![], vec![])?;
        assert_scripted_sample(
            "readable RSS remains authoritative",
            topology,
            observed.memory,
            &readable,
            [1, 0, 0, 0, 0],
        );
        let fresh = Topology {
            in_exit: false,
            ..topology
        };
        let unknown = scripted_sample(
            topology,
            vec![unreadable(), unreadable()],
            vec![Ok(fresh), Ok(fresh)],
            vec![],
        )?;
        assert_scripted_sample(
            "selection flags do not substitute for fresh exit progress",
            topology,
            observed.memory,
            &unknown,
            [0, 1, 0, 0, 0],
        );
        Ok(())
    }

    #[tokio::test]
    async fn retained_normal_exit_does_not_become_an_unreadable_process() -> io::Result<()> {
        let mut child = Command::new("/bin/cat")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .kill_on_drop(true)
            .spawn()?;
        let pid = child
            .id()
            .ok_or_else(|| io::Error::other("child has no PID"))?;
        let mut sampler = Sampler::default();
        let observed = sampler.observe(pid)?;
        assert!(!observed.zombie);
        assert_eq!(observed.parent, Some(std::process::id()));
        drop(child.stdin.take());
        let zombie = timeout(Duration::from_secs(5), async {
            loop {
                if let Ok(process) = observe_topology(pid)
                    && process.zombie
                {
                    break process;
                }
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .map_err(io::Error::other)?;
        assert_eq!(zombie.start, observed.start);
        assert_eq!(zombie.parent, observed.parent);
        let seeds = [ProcessSeed {
            pid,
            start: Some(observed.start),
        }];
        let sample = sampler.sample_owned(&seeds)?;
        assert!(sample.current.is_empty());
        assert!(sample.uncertain.is_empty());
        assert!(sample.exiting.is_empty());
        assert!(sample.sampling_failures.is_empty());
        assert!(sample.replaced.is_empty());
        assert_eq!(sample.exited[&pid].start, Some(observed.start));
        assert_eq!(sample.exited[&pid].parent, observed.parent);
        let status = timeout(Duration::from_secs(5), child.wait())
            .await
            .map_err(io::Error::other)??;
        assert!(status.success());
        let sample = sampler.sample_owned(&seeds)?;
        assert!(sample.current.is_empty());
        assert!(sample.uncertain.is_empty());
        assert!(sample.exiting.is_empty());
        assert!(sample.sampling_failures.is_empty());
        Ok(())
    }
}
