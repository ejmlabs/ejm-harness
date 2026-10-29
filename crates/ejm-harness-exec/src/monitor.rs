//! One lazy process sampler shared by the governor's active commands.
//!
//! Sampling is reactive. A child that escapes and is reparented between samples
//! cannot be reliably attributed without stronger OS containment.

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::{Arc, Mutex},
    time::Duration,
};

use tokio_util::sync::CancellationToken;

use crate::{
    Reservation,
    governor::lock,
    processes::{ProcessSample, Sampler, bounded_sampling_failure},
};

#[cfg(target_os = "macos")]
use crate::processes::ProcessSeed;

pub(crate) const SAMPLE_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Default)]
pub(crate) struct ProcessMonitor {
    state: Mutex<State>,
    #[cfg(test)]
    violation_witness: Mutex<Option<Vec<u32>>>,
}

#[derive(Default)]
struct State {
    running: bool,
    next_id: u64,
    jobs: BTreeMap<u64, Job>,
}

struct Job {
    root: u32,
    root_start: Option<u64>,
    tracked: HashMap<u32, u64>,
    reservation: Reservation,
    cancel: CancellationToken,
    usage: Arc<Mutex<ObservedUsage>>,
}

#[derive(Default, Clone)]
pub(crate) struct ObservedUsage {
    pub peak_memory_bytes: u64,
    pub peak_processes: u32,
    pub violation: Option<&'static str>,
    pub sampling_failure: Option<String>,
}

impl ObservedUsage {
    pub fn check_violation(&self) -> anyhow::Result<()> {
        if let Some(message) = self.violation {
            if let Some(detail) = &self.sampling_failure {
                return Err(anyhow::Error::msg(detail.clone()).context(message));
            }
            return Err(anyhow::Error::msg(message));
        }
        Ok(())
    }

    pub fn sampling_error(&self) -> Option<String> {
        self.sampling_failure
            .as_ref()
            .map(|detail| format!("process sampler failed: {detail}"))
    }
}

pub(crate) struct MonitorLease {
    monitor: Arc<ProcessMonitor>,
    id: u64,
    usage: Arc<Mutex<ObservedUsage>>,
}

impl ProcessMonitor {
    pub fn register(
        self: &Arc<Self>,
        root: u32,
        reservation: Reservation,
        cancel: CancellationToken,
    ) -> MonitorLease {
        let usage = Arc::new(Mutex::new(ObservedUsage::default()));
        let mut state = lock(&self.state);
        let id = state.next_id;
        state.next_id = state.next_id.wrapping_add(1);
        state.jobs.insert(
            id,
            Job {
                root,
                root_start: None,
                tracked: HashMap::new(),
                reservation,
                cancel,
                usage: usage.clone(),
            },
        );
        if !state.running {
            state.running = true;
            tokio::spawn(self.clone().sample());
        }
        MonitorLease {
            monitor: self.clone(),
            id,
            usage,
        }
    }

    async fn sample(self: Arc<Self>) {
        let mut sampler = Sampler::default();
        loop {
            {
                let mut state = lock(&self.state);
                if state.jobs.is_empty() {
                    state.running = false;
                    return;
                }
            }
            let owner = self.clone();
            // Native process enumeration may block. One shared job performs it,
            // independent of the number of managed or waiting agent sessions.
            let result = tokio::task::spawn_blocking(move || {
                #[cfg(target_os = "macos")]
                let (jobs, seeds) = owner.seeds();
                #[cfg(target_os = "macos")]
                let sample = sampler.sample_owned(&seeds)?;
                #[cfg(not(target_os = "macos"))]
                let sample = ProcessSample {
                    current: sampler.sample_all()?,
                    ..ProcessSample::default()
                };
                #[cfg(target_os = "macos")]
                owner.update(&sample, Some(&jobs));
                #[cfg(not(target_os = "macos"))]
                owner.update(&sample, None);
                Ok::<_, std::io::Error>(sampler)
            })
            .await;
            let result = result
                .map_err(|error| {
                    (
                        "blocking_join",
                        if error.is_cancelled() {
                            "sampler task cancelled"
                        } else {
                            "sampler task panicked"
                        }
                        .into(),
                    )
                })
                .and_then(|result| {
                    result.map_err(|error| ("topology_enumeration", error.to_string()))
                });
            sampler = match result {
                Ok(value) => value,
                Err((failure_stage, cause)) => {
                    let mut state = lock(&self.state);
                    for job in state.jobs.values() {
                        let mut usage = lock(&job.usage);
                        usage.violation = Some("process sampler failed");
                        usage.sampling_failure.get_or_insert_with(|| {
                            bounded_sampling_failure(format!(
                                "stage={failure_stage} pid={} cause={cause}",
                                job.root
                            ))
                        });
                        job.cancel.cancel();
                    }
                    state.running = false;
                    return;
                }
            };
            {
                let mut state = lock(&self.state);
                if state.jobs.is_empty() {
                    state.running = false;
                    return;
                }
            }
            tokio::time::sleep(SAMPLE_INTERVAL).await;
        }
    }

    #[cfg(target_os = "macos")]
    fn seeds(&self) -> (Vec<u64>, Vec<ProcessSeed>) {
        let state = lock(&self.state);
        let jobs = state.jobs.keys().copied().collect();
        let seeds = state
            .jobs
            .values()
            .flat_map(|job| {
                std::iter::once(ProcessSeed {
                    pid: job.root,
                    start: job.root_start,
                })
                .chain(job.tracked.iter().map(|(&pid, &start)| ProcessSeed {
                    pid,
                    start: Some(start),
                }))
            })
            .collect();
        (jobs, seeds)
    }

    fn update(&self, sample: &ProcessSample, sampled_jobs: Option<&[u64]>) {
        let current = &sample.current;
        // Build this once per native sample. Repeated full-host scans for each
        // job and generation of descendants grow with jobs * host processes.
        let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
        for (&pid, process) in current {
            if !process.zombie
                && let Some(parent) = process.parent
                && !sample.replaced.contains(&parent)
            {
                children.entry(parent).or_default().push(pid);
            }
        }
        for (&pid, process) in sample
            .uncertain
            .iter()
            .chain(&sample.exiting)
            .chain(&sample.exited)
        {
            if let Some(parent) = process.parent
                && !sample.replaced.contains(&parent)
            {
                children.entry(parent).or_default().push(pid);
            }
        }
        let mut state = lock(&self.state);
        for (&id, job) in &mut state.jobs {
            if sampled_jobs.is_some_and(|jobs| jobs.binary_search(&id).is_err()) {
                continue;
            }
            job.update(sample, &children);
            #[cfg(test)]
            if lock(&job.usage).violation
                == Some("observed command tree process count exceeded its reservation")
                && let Some(witness) = lock(&self.violation_witness).as_mut()
                && witness.is_empty()
            {
                *witness = job.tracked.keys().copied().collect();
            }
        }
    }
}

impl Job {
    fn update(&mut self, sample: &ProcessSample, children: &HashMap<u32, Vec<u32>>) {
        let (descendants, mut uncertain) = self.descendants(sample, children);
        let mut sampling_failure = if uncertain {
            sample.sampling_failures.get(&self.root)
        } else {
            None
        };
        let mut memory = 0_u64;
        let mut memory_complete = true;
        let mut processes = 0_u32;
        for pid in descendants {
            if let Some(process) = sample.current.get(&pid)
                && !process.zombie
            {
                self.tracked.insert(pid, process.start);
                memory = memory.saturating_add(process.memory);
                processes = processes.saturating_add(1);
            } else if let Some(process) = sample.exiting.get(&pid) {
                if let Some(start) = process.start {
                    self.tracked.insert(pid, start);
                } else {
                    uncertain = true;
                }
                // Native exit progress does not prove execution or RSS ended.
                // Keep process occupancy and the previous complete memory peak.
                processes = processes.saturating_add(1);
                memory_complete = false;
            } else if let Some(process) = sample.uncertain.get(&pid) {
                if let Some(start) = process.start {
                    self.tracked.insert(pid, start);
                }
                uncertain = true;
                sampling_failure = sampling_failure.or_else(|| sample.sampling_failures.get(&pid));
            }
        }
        let mut usage = lock(&self.usage);
        if uncertain {
            usage.violation.get_or_insert("process sampler failed");
            if usage.sampling_failure.is_none() {
                usage.sampling_failure = sampling_failure.cloned().map(bounded_sampling_failure);
            }
            self.cancel.cancel();
            return;
        }
        if memory_complete {
            usage.peak_memory_bytes = usage.peak_memory_bytes.max(memory);
        }
        usage.peak_processes = usage.peak_processes.max(processes);
        // Readable components remain a valid lower bound even when another
        // owned process has unavailable RSS during native exit progress.
        let violation = if memory > self.reservation.memory_bytes {
            Some("observed command tree memory exceeded its reservation")
        } else if processes > self.reservation.processes {
            Some("observed command tree process count exceeded its reservation")
        } else {
            None
        };
        if let Some(message) = violation {
            usage.violation.get_or_insert(message);
            self.cancel.cancel();
        }
    }

    fn descendants(
        &mut self,
        sample: &ProcessSample,
        children: &HashMap<u32, Vec<u32>>,
    ) -> (HashSet<u32>, bool) {
        let mut descendants: HashSet<u32> = self
            .tracked
            .iter()
            .filter_map(|(&pid, &start)| {
                sample
                    .exited
                    .get(&pid)
                    .filter(|process| process.start == Some(start))
                    .map(|_| pid)
            })
            .collect();
        // Keep identities for observed children even if their parent exits.
        self.tracked.retain(|pid, start| {
            sample.current.get(pid).map_or_else(
                || {
                    sample
                        .uncertain
                        .get(pid)
                        .is_some_and(|process| process.start.is_none_or(|value| value == *start))
                        || sample
                            .exiting
                            .get(pid)
                            .is_some_and(|process| process.start == Some(*start))
                },
                |process| process.start == *start && !process.zombie,
            )
        });
        if let Some(root) = sample.current.get(&self.root)
            && self.root_start.is_none_or(|start| root.start == start)
            && !root.zombie
        {
            self.root_start.get_or_insert(root.start);
            self.tracked.entry(self.root).or_insert(root.start);
        }
        let uncertain_root = sample.uncertain.get(&self.root).filter(|process| {
            match (self.root_start, process.start) {
                (Some(expected), Some(observed)) => expected == observed,
                _ => true,
            }
        });
        let exiting_root = sample.exiting.get(&self.root).filter(|process| {
            process
                .start
                .is_some_and(|start| self.root_start.is_none_or(|expected| expected == start))
        });
        if let Some(root) = uncertain_root.or(exiting_root)
            && let Some(start) = root.start
        {
            self.root_start.get_or_insert(start);
            self.tracked.entry(self.root).or_insert(start);
        }
        if let Some(root) = sample.exited.get(&self.root)
            && let Some(start) = root.start
            && self.root_start.is_none_or(|expected| expected == start)
        {
            self.root_start.get_or_insert(start);
            descendants.insert(self.root);
        }
        descendants.extend(self.tracked.keys().copied());
        let mut pending: Vec<u32> = descendants.iter().copied().collect();
        while let Some(parent) = pending.pop() {
            if let Some(children) = children.get(&parent) {
                for &child in children {
                    if descendants.insert(child) {
                        pending.push(child);
                    }
                }
            }
        }
        (descendants, uncertain_root.is_some())
    }
}

impl MonitorLease {
    pub fn usage(&self) -> ObservedUsage {
        lock(&self.usage).clone()
    }

    pub async fn kill_known_descendants(&self) {
        let tracked = lock(&self.monitor.state)
            .jobs
            .get(&self.id)
            .map(|job| {
                job.tracked
                    .iter()
                    .filter(|(pid, _)| **pid != job.root)
                    .map(|(pid, start)| (*pid, *start))
                    .collect::<HashMap<_, _>>()
            })
            .unwrap_or_default();
        if tracked.is_empty() {
            return;
        }
        let _ = tokio::task::spawn_blocking(move || {
            Sampler::default().kill_known(&tracked);
        })
        .await;
    }
}

impl Drop for MonitorLease {
    fn drop(&mut self) {
        lock(&self.monitor.state).jobs.remove(&self.id);
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;
    use crate::processes::{ProcessInfo, ProcessMetadata};

    fn fixture(jobs: u32, process_count: u32) -> (ProcessMonitor, ProcessSample) {
        let mut state = State::default();
        let mut processes = HashMap::new();
        for pid in 1..=process_count {
            processes.insert(
                pid,
                ProcessInfo {
                    parent: (pid % 8 != 1).then(|| pid - 1),
                    start: u64::from(pid),
                    memory: 1,
                    zombie: false,
                },
            );
        }
        for index in 0..jobs {
            state.jobs.insert(
                u64::from(index),
                Job {
                    root: index * 8 + 1,
                    root_start: None,
                    tracked: HashMap::new(),
                    reservation: Reservation {
                        memory_bytes: 100,
                        processes: 10,
                        ..Reservation::default()
                    },
                    cancel: CancellationToken::new(),
                    usage: Arc::new(Mutex::new(ObservedUsage::default())),
                },
            );
        }
        (
            ProcessMonitor {
                state: Mutex::new(state),
                ..ProcessMonitor::default()
            },
            ProcessSample {
                current: processes,
                ..ProcessSample::default()
            },
        )
    }

    #[tokio::test]
    async fn observed_process_budget_abuse_is_stopped_and_released() -> anyhow::Result<()> {
        use anyhow::Context;
        use sysinfo::{Pid, ProcessRefreshKind, ProcessStatus, ProcessesToUpdate, System};

        let directory = tempfile::tempdir()?;
        let governor = Arc::new(crate::Governor::new(crate::ResourceLimits::default())?);
        *lock(&governor.monitor.violation_witness) = Some(Vec::new());
        let executor = crate::Executor::new(governor.clone(), directory.path().join("artifacts"))?;
        let current = std::env::current_exe()?;
        let program = current
            .parent()
            .and_then(std::path::Path::parent)
            .context("test executable is outside Cargo's target directory")?
            .join(format!(
                "ejm-harness-exec-fixture{}",
                std::env::consts::EXE_SUFFIX
            ));
        let mut request = crate::CommandRequest {
            program: program.to_string_lossy().into_owned(),
            cwd: directory.path().to_owned(),
            timeout_ms: 5_000,
            ..crate::CommandRequest::default()
        };
        request
            .env
            .insert("EJM_HARNESS_EXEC_FIXTURE".into(), "many-descendants".into());
        request.reservation.processes = 5;
        request.reservation.memory_bytes = 256 * 1024 * 1024;
        let error = executor
            .run(request, CancellationToken::new())
            .await
            .err()
            .context("process budget violation did not stop the fixture")?;
        assert!(error.to_string().contains("process count"), "{error:#}");
        let pids = lock(&governor.monitor.violation_witness)
            .clone()
            .context("process budget witness was not installed")?;
        assert_eq!(
            pids.len(),
            6,
            "root and all five children must be observed"
        );
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let mut system = System::new();
                let ids: Vec<Pid> = pids.iter().copied().map(Pid::from_u32).collect();
                system.refresh_processes_specifics(
                    ProcessesToUpdate::Some(&ids),
                    true,
                    ProcessRefreshKind::nothing(),
                );
                if governor.snapshot() == crate::GovernorSnapshot::default()
                    && ids.iter().all(|pid| {
                        system
                            .process(*pid)
                            .is_none_or(|process| process.status() == ProcessStatus::Zombie)
                    })
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .context("all command resources were not cleaned up")?;
        Ok(())
    }

    #[test]
    fn observed_children_survive_reparenting_and_reused_pids_are_not_owned() {
        let (monitor, mut processes) = fixture(1, 8);
        monitor.update(&processes, None);
        assert_eq!(lock(&lock(&monitor.state).jobs[&0].usage).peak_processes, 8);
        processes.current.remove(&1);
        if let Some(child) = processes.current.get_mut(&2) {
            child.parent = None;
        }
        monitor.update(&processes, None);
        assert_eq!(lock(&monitor.state).jobs[&0].tracked.len(), 7);
        for process in processes.current.values_mut() {
            process.start += 1;
        }
        monitor.update(&processes, None);
        assert!(lock(&monitor.state).jobs[&0].tracked.is_empty());
    }

    #[test]
    fn uncertain_descendants_retain_identity_and_cancel_only_their_owner() {
        let (monitor, mut sample) = fixture(2, 16);
        monitor.update(&sample, None);
        let before = lock(&lock(&monitor.state).jobs[&0].usage).clone();
        sample.current.remove(&2);
        sample.uncertain.insert(
            2,
            ProcessMetadata {
                parent: Some(1),
                start: Some(2),
            },
        );
        sample.uncertain.insert(
            17,
            ProcessMetadata {
                parent: Some(2),
                start: Some(17),
            },
        );
        sample.sampling_failures.insert(
            2,
            "stage=owned_rss_retry pid=2 cause=inspection unavailable".into(),
        );
        monitor.update(&sample, None);
        let state = lock(&monitor.state);
        let affected = &state.jobs[&0];
        let peer = &state.jobs[&1];
        assert!(affected.cancel.is_cancelled());
        assert!(!peer.cancel.is_cancelled());
        assert_eq!(affected.tracked.get(&2), Some(&2));
        assert_eq!(affected.tracked.get(&17), Some(&17));
        let usage = lock(&affected.usage);
        assert_eq!(usage.peak_memory_bytes, before.peak_memory_bytes);
        assert_eq!(usage.peak_processes, before.peak_processes);
        assert!(usage.violation.is_some());
        assert_eq!(
            usage.sampling_failure.as_deref(),
            Some("stage=owned_rss_retry pid=2 cause=inspection unavailable")
        );
        assert!(lock(&peer.usage).violation.is_none());
        assert!(lock(&peer.usage).sampling_failure.is_none());
    }

    #[test]
    fn exiting_ancestry_retains_occupancy_and_the_last_complete_memory_peak()
    -> Result<(), &'static str> {
        let (monitor, mut sample) = fixture(1, 2);
        monitor.update(&sample, None);
        sample.current.remove(&1);
        sample.exiting.insert(
            1,
            ProcessMetadata {
                parent: None,
                start: Some(1),
            },
        );
        sample.current.insert(
            3,
            ProcessInfo {
                parent: Some(1),
                start: 3,
                memory: 4,
                zombie: false,
            },
        );
        monitor.update(&sample, None);
        {
            let state = lock(&monitor.state);
            let job = &state.jobs[&0];
            assert_eq!(job.root_start, Some(1));
            assert_eq!(job.tracked, HashMap::from([(1, 1), (2, 2), (3, 3)]));
            assert!(!job.cancel.is_cancelled());
            let usage = lock(&job.usage);
            assert_eq!(usage.peak_memory_bytes, 2);
            assert_eq!(usage.peak_processes, 3);
            assert!(usage.violation.is_none());
        }

        sample
            .current
            .get_mut(&3)
            .ok_or("fixture child PID 3 is missing before the lower-bound violation")?
            .memory = 101;
        monitor.update(&sample, None);
        let state = lock(&monitor.state);
        let job = &state.jobs[&0];
        assert!(job.cancel.is_cancelled());
        let usage = lock(&job.usage);
        assert_eq!(usage.peak_memory_bytes, 2);
        assert!(usage.violation.is_some());
        assert!(usage.sampling_failure.is_none());
        Ok(())
    }

    #[test]
    fn exiting_processes_still_enforce_the_full_process_limit() -> Result<(), &'static str> {
        let (monitor, mut sample) = fixture(1, 2);
        monitor.update(&sample, None);
        lock(&monitor.state)
            .jobs
            .get_mut(&0)
            .ok_or("fixture job 0 is missing before its process limit changes")?
            .reservation
            .processes = 2;
        sample.current.remove(&2);
        sample.exiting.insert(
            2,
            ProcessMetadata {
                parent: Some(1),
                start: Some(2),
            },
        );
        sample.current.insert(
            3,
            ProcessInfo {
                parent: Some(2),
                start: 3,
                memory: 1,
                zombie: false,
            },
        );
        monitor.update(&sample, None);
        let state = lock(&monitor.state);
        let job = &state.jobs[&0];
        assert_eq!(job.tracked.get(&2), Some(&2));
        assert_eq!(job.tracked.get(&3), Some(&3));
        assert!(job.cancel.is_cancelled());
        let usage = lock(&job.usage);
        assert_eq!(usage.peak_memory_bytes, 2);
        assert_eq!(usage.peak_processes, 3);
        assert!(usage.violation.is_some());
        assert!(usage.sampling_failure.is_none());
        Ok(())
    }

    #[test]
    fn in_flight_scoped_sample_does_not_bind_a_new_job_to_an_old_root() -> Result<(), &'static str>
    {
        let (monitor, mut sample) = fixture(2, 2);
        monitor.update(&sample, Some(&[0]));
        lock(&monitor.state)
            .jobs
            .get_mut(&1)
            .ok_or("fixture job 1 is missing before the in-flight sample")?
            .root = 1;
        monitor.update(&sample, Some(&[0]));
        assert_eq!(lock(&monitor.state).jobs[&1].root_start, None);
        sample
            .current
            .get_mut(&1)
            .ok_or("fixture root PID 1 is missing before birth replacement")?
            .start = 100;
        sample.current.remove(&2);
        monitor.update(&sample, Some(&[1]));
        assert_eq!(lock(&monitor.state).jobs[&1].root_start, Some(100));
        Ok(())
    }

    #[test]
    fn exited_ancestry_keeps_newly_discovered_descendants_owned() {
        let (monitor, mut sample) = fixture(1, 1);
        sample.current.remove(&1);
        sample.exited.insert(
            1,
            ProcessMetadata {
                parent: None,
                start: Some(1),
            },
        );
        sample.current.insert(
            2,
            ProcessInfo {
                parent: Some(1),
                start: 2,
                memory: 10,
                zombie: false,
            },
        );
        monitor.update(&sample, None);
        assert_eq!(lock(&monitor.state).jobs[&0].tracked.get(&2), Some(&2));
        sample.current.remove(&2);
        sample.exited.remove(&1);
        sample.exited.insert(
            2,
            ProcessMetadata {
                parent: Some(1),
                start: Some(2),
            },
        );
        sample.current.insert(
            3,
            ProcessInfo {
                parent: Some(2),
                start: 3,
                memory: 11,
                zombie: false,
            },
        );
        monitor.update(&sample, None);
        let state = lock(&monitor.state);
        let job = &state.jobs[&0];
        assert_eq!(job.tracked, HashMap::from([(3, 3)]));
        assert!(!job.cancel.is_cancelled());
        let usage = lock(&job.usage);
        assert_eq!(usage.peak_memory_bytes, 11);
        assert_eq!(usage.peak_processes, 1);
        assert!(usage.violation.is_none());
    }

    #[test]
    fn replaced_pid_cannot_adopt_a_new_child_but_retained_orphans_stay_owned()
    -> Result<(), &'static str> {
        let (monitor, mut sample) = fixture(1, 0);
        lock(&monitor.state)
            .jobs
            .get_mut(&0)
            .ok_or("fixture job 0 is missing before the replacement sample")?
            .root = 100;
        sample.current.extend([
            (
                100,
                ProcessInfo {
                    parent: None,
                    start: 10,
                    memory: 1,
                    zombie: false,
                },
            ),
            (
                50,
                ProcessInfo {
                    parent: Some(100),
                    start: 12,
                    memory: 1,
                    zombie: false,
                },
            ),
        ]);
        monitor.update(&sample, None);
        assert_eq!(lock(&monitor.state).jobs[&0].tracked.len(), 2);

        sample.current.remove(&100);
        sample.replaced.insert(100);
        sample
            .current
            .get_mut(&50)
            .ok_or("fixture child PID 50 is missing before reparenting")?
            .parent = None;
        sample.current.insert(
            101,
            ProcessInfo {
                parent: Some(100),
                start: 21,
                memory: 1,
                zombie: false,
            },
        );
        monitor.update(&sample, None);
        let state = lock(&monitor.state);
        let job = &state.jobs[&0];
        assert_eq!(job.tracked, HashMap::from([(50, 12)]));
        assert!(!job.cancel.is_cancelled());
        assert!(lock(&job.usage).violation.is_none());
        Ok(())
    }

    #[test]
    fn sampler_traversal_cost() {
        for jobs in [4, 32, 128] {
            let (monitor, processes) = fixture(jobs, 4096);
            let mut micros = Vec::new();
            for _ in 0..25 {
                for job in lock(&monitor.state).jobs.values_mut() {
                    job.tracked.clear();
                    job.root_start = None;
                }
                let start = Instant::now();
                monitor.update(&processes, None);
                micros.push(start.elapsed().as_micros());
            }
            micros.sort_unstable();
            println!(
                "sampler jobs={jobs} host_processes=4096 samples=25 p50_us={} p95_us={} max_us={}",
                micros[12], micros[23], micros[24]
            );
        }
    }
}
