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
    processes::{ProcessInfo, Sampler},
};

pub(crate) const SAMPLE_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Default)]
pub(crate) struct ProcessMonitor {
    state: Mutex<State>,
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
                let processes = sampler.sample_all()?;
                owner.update(&processes);
                Ok::<_, std::io::Error>(sampler)
            })
            .await;
            if let Ok(Ok(value)) = result {
                sampler = value;
            } else {
                let mut state = lock(&self.state);
                for job in state.jobs.values() {
                    lock(&job.usage).violation = Some("process sampler failed");
                    job.cancel.cancel();
                }
                state.running = false;
                return;
            }
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

    fn update(&self, current: &HashMap<u32, ProcessInfo>) {
        let mut state = lock(&self.state);
        for job in state.jobs.values_mut() {
            // Keep identities for observed children even if their parent exits.
            job.tracked.retain(|pid, start| {
                current
                    .get(pid)
                    .is_some_and(|process| process.start == *start && !process.zombie)
            });
            if let Some(root) = current.get(&job.root)
                && job.root_start.is_none_or(|start| root.start == start)
                && !root.zombie
            {
                job.root_start.get_or_insert(root.start);
                job.tracked.entry(job.root).or_insert(root.start);
            }
            let mut descendants: HashSet<u32> = job.tracked.keys().copied().collect();
            loop {
                let before = descendants.len();
                for (pid, process) in current {
                    if !process.zombie
                        && process
                            .parent
                            .is_some_and(|parent| descendants.contains(&parent))
                    {
                        descendants.insert(*pid);
                    }
                }
                if descendants.len() == before {
                    break;
                }
            }
            let mut memory = 0_u64;
            let mut processes = 0_u32;
            for pid in descendants {
                if let Some(process) = current.get(&pid)
                    && !process.zombie
                {
                    job.tracked.insert(pid, process.start);
                    memory = memory.saturating_add(process.memory);
                    processes = processes.saturating_add(1);
                }
            }
            let mut usage = lock(&job.usage);
            usage.peak_memory_bytes = usage.peak_memory_bytes.max(memory);
            usage.peak_processes = usage.peak_processes.max(processes);
            let violation = if memory > job.reservation.memory_bytes {
                Some("observed command tree memory exceeded its reservation")
            } else if processes > job.reservation.processes {
                Some("observed command tree process count exceeded its reservation")
            } else {
                None
            };
            if let Some(message) = violation {
                usage.violation.get_or_insert(message);
                job.cancel.cancel();
            }
        }
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
