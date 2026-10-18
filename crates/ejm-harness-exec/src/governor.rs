use std::{
    collections::{HashSet, VecDeque},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
};

use anyhow::{Context, Result, bail, ensure};
use tokio::sync::{Notify, oneshot};
use tokio_util::sync::CancellationToken;

use crate::{GovernorSnapshot, Reservation, ResourceLimits, monitor::ProcessMonitor};

/// Metadata allowance keeps even very small retained artifacts bounded.
pub(crate) const ARTIFACT_OVERHEAD: u64 = 4096;

pub struct Governor {
    pub(crate) limits: ResourceLimits,
    state: Mutex<State>,
    idle: Notify,
    artifacts: Mutex<ArtifactState>,
    pub(crate) monitor: Arc<ProcessMonitor>,
}

#[derive(Default)]
struct State {
    snapshot: GovernorSnapshot,
    sessions: VecDeque<SessionQueue>,
    next_ticket: u64,
}

struct SessionQueue {
    id: String,
    waiters: VecDeque<Waiter>,
}

struct Waiter {
    ticket: u64,
    reservation: Reservation,
    sender: oneshot::Sender<Lease>,
}

#[derive(Default)]
struct ArtifactState {
    used: u64,
    roots: HashSet<PathBuf>,
}

/// A lease releases its exact reservation on drop, including cancelled futures.
pub struct Lease {
    governor: Arc<Governor>,
    reservation: Reservation,
}

impl Governor {
    /// Create a shared governor from validated aggregate budgets.
    ///
    /// # Errors
    /// Returns an error for zero required budgets or limits above supported bounds.
    pub fn new(limits: ResourceLimits) -> Result<Self> {
        ensure!(limits.max_jobs > 0, "max_jobs must be positive");
        ensure!(
            limits.max_heavy_jobs <= limits.max_jobs,
            "max_heavy_jobs cannot exceed max_jobs"
        );
        ensure!(limits.cpu_millis > 0, "cpu_millis must be positive");
        ensure!(limits.memory_bytes > 0, "memory_bytes must be positive");
        ensure!(limits.max_processes > 0, "max_processes must be positive");
        ensure!(limits.max_queue > 0, "max_queue must be positive");
        ensure!(limits.max_queue <= 65_536, "max_queue exceeds 65536");
        ensure!(
            limits.max_output_bytes > 0,
            "max_output_bytes must be positive"
        );
        ensure!(
            limits.max_artifact_bytes > 0,
            "max_artifact_bytes must be positive"
        );
        ensure!(limits.max_job_ms > 0, "max_job_ms must be positive");
        // This also keeps Instant arithmetic and retained frame allocations bounded.
        ensure!(
            limits.max_job_ms <= 86_400_000,
            "max_job_ms exceeds one day"
        );
        Ok(Self {
            limits,
            state: Mutex::new(State::default()),
            idle: Notify::new(),
            artifacts: Mutex::new(ArtifactState::default()),
            monitor: Arc::new(ProcessMonitor::default()),
        })
    }

    #[must_use]
    pub fn snapshot(&self) -> GovernorSnapshot {
        lock(&self.state).snapshot.clone()
    }

    /// The immutable aggregate limits used for every admission decision.
    #[must_use]
    pub fn limits(&self) -> &ResourceLimits {
        &self.limits
    }

    /// Wait for all admission waiters and leases to leave. Hosts must stop
    /// submitting new work and cancel their jobs before draining on shutdown.
    pub async fn wait_idle(&self) {
        loop {
            let notified = self.idle.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            {
                let state = lock(&self.state);
                if state.snapshot.running == 0 && state.snapshot.queued == 0 {
                    return;
                }
            }
            notified.await;
        }
    }

    /// Wait for a reservation using FIFO session rotation.
    ///
    /// # Errors
    /// Returns an error for an invalid reservation, a full queue, cancellation,
    /// or a closed admission dispatcher.
    pub async fn acquire(
        self: &Arc<Self>,
        session_id: &str,
        reservation: Reservation,
        cancel: &CancellationToken,
    ) -> Result<Lease> {
        ensure!(!session_id.is_empty(), "session_id must not be empty");
        ensure!(session_id.len() <= 256, "session_id exceeds 256 bytes");
        self.validate_reservation(&reservation)?;
        ensure!(!cancel.is_cancelled(), "admission cancelled");
        let (sender, receiver) = oneshot::channel();
        let ticket;
        {
            let mut state = lock(&self.state);
            ensure!(
                state.snapshot.queued < self.limits.max_queue,
                "admission queue is full"
            );
            ticket = state.next_ticket;
            state.next_ticket = ticket
                .checked_add(1)
                .context("admission ticket exhausted")?;
            let waiter = Waiter {
                ticket,
                reservation,
                sender,
            };
            if let Some(session) = state
                .sessions
                .iter_mut()
                .find(|session| session.id == session_id)
            {
                session.waiters.push_back(waiter);
            } else {
                state.sessions.push_back(SessionQueue {
                    id: session_id.to_owned(),
                    waiters: VecDeque::from([waiter]),
                });
            }
            state.snapshot.queued += 1;
        }
        let _queued = QueuedTicket {
            governor: self.clone(),
            ticket,
        };
        self.dispatch();
        tokio::select! {
            biased;
            () = cancel.cancelled() => bail!("admission cancelled"),
            result = receiver => result.context("admission dispatcher closed"),
        }
    }

    fn validate_reservation(&self, reservation: &Reservation) -> Result<()> {
        ensure!(
            reservation.cpu_millis <= self.limits.cpu_millis,
            "CPU reservation exceeds aggregate limit"
        );
        ensure!(
            reservation.memory_bytes <= self.limits.memory_bytes,
            "memory reservation exceeds aggregate limit"
        );
        ensure!(
            reservation.processes <= self.limits.max_processes,
            "process reservation exceeds aggregate limit"
        );
        ensure!(
            reservation.output_bytes <= self.limits.max_output_bytes,
            "output reservation exceeds aggregate limit"
        );
        ensure!(
            !reservation.heavy || self.limits.max_heavy_jobs > 0,
            "heavy jobs are disabled"
        );
        Ok(())
    }

    // Round robin between sessions, FIFO within each session. The oldest session
    // head cannot be bypassed by an endless stream of smaller reservations.
    fn dispatch(self: &Arc<Self>) {
        let mut grants = Vec::new();
        {
            let mut state = lock(&self.state);
            while let Some(mut session) = state.sessions.pop_front() {
                let Some(waiter) = session.waiters.pop_front() else {
                    continue;
                };
                if !self.fits(&state.snapshot, &waiter.reservation) {
                    session.waiters.push_front(waiter);
                    state.sessions.push_front(session);
                    break;
                }
                if !session.waiters.is_empty() {
                    state.sessions.push_back(session);
                }
                state.snapshot.queued -= 1;
                add_reservation(&mut state.snapshot, &waiter.reservation);
                grants.push((waiter.sender, waiter.reservation));
            }
        }
        // Sending or dropping a lease may re-enter the governor. Never do it
        // under the state lock, even when the waiting receiver was dropped.
        for (sender, reservation) in grants {
            let _ = sender.send(Lease {
                governor: self.clone(),
                reservation,
            });
        }
    }

    fn fits(&self, snapshot: &GovernorSnapshot, reservation: &Reservation) -> bool {
        snapshot.running < self.limits.max_jobs
            && (!reservation.heavy || snapshot.heavy_running < self.limits.max_heavy_jobs)
            && reservation.cpu_millis <= self.limits.cpu_millis - snapshot.reserved_cpu_millis
            && reservation.memory_bytes <= self.limits.memory_bytes - snapshot.reserved_memory_bytes
            && reservation.processes <= self.limits.max_processes - snapshot.reserved_processes
            && reservation.output_bytes
                <= self.limits.max_output_bytes - snapshot.reserved_output_bytes
    }

    fn cancel_ticket(self: &Arc<Self>, ticket: u64) {
        let mut removed = None;
        {
            let mut state = lock(&self.state);
            for session in &mut state.sessions {
                if let Some(index) = session
                    .waiters
                    .iter()
                    .position(|waiter| waiter.ticket == ticket)
                {
                    removed = session.waiters.remove(index);
                    break;
                }
            }
            if removed.is_some() {
                state.snapshot.queued -= 1;
                state.sessions.retain(|session| !session.waiters.is_empty());
            }
        }
        if removed.is_some() {
            drop(removed);
            self.dispatch();
            self.idle.notify_waiters();
        }
    }

    pub(crate) fn register_artifact_root(&self, root: &Path) -> Result<()> {
        let mut state = lock(&self.artifacts);
        if state.roots.contains(root) {
            return Ok(());
        }
        let mut bytes = 0_u64;
        for entry in std::fs::read_dir(root).context("read executor artifact directory")? {
            let metadata = entry?.metadata()?;
            if metadata.is_file() {
                bytes = bytes
                    .checked_add(metadata.len())
                    .and_then(|value| value.checked_add(ARTIFACT_OVERHEAD))
                    .context("artifact usage overflow")?;
            }
        }
        let used = state
            .used
            .checked_add(bytes)
            .context("artifact usage overflow")?;
        ensure!(
            used <= self.limits.max_artifact_bytes,
            "existing artifacts exceed shared quota"
        );
        state.used = used;
        state.roots.insert(root.to_path_buf());
        Ok(())
    }

    pub(crate) fn claim_artifact_bytes(&self, requested: u64) -> u64 {
        let mut state = lock(&self.artifacts);
        let granted = requested.min(self.limits.max_artifact_bytes - state.used);
        state.used += granted;
        granted
    }

    pub(crate) fn release_artifact_bytes(&self, bytes: u64) {
        let mut state = lock(&self.artifacts);
        state.used = state.used.saturating_sub(bytes);
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        {
            let mut state = lock(&self.governor.state);
            let snapshot = &mut state.snapshot;
            snapshot.running -= 1;
            snapshot.heavy_running -= usize::from(self.reservation.heavy);
            snapshot.reserved_cpu_millis -= self.reservation.cpu_millis;
            snapshot.reserved_memory_bytes -= self.reservation.memory_bytes;
            snapshot.reserved_processes -= self.reservation.processes;
            snapshot.reserved_output_bytes -= self.reservation.output_bytes;
        }
        self.governor.dispatch();
        self.governor.idle.notify_waiters();
    }
}

struct QueuedTicket {
    governor: Arc<Governor>,
    ticket: u64,
}

impl Drop for QueuedTicket {
    fn drop(&mut self) {
        self.governor.cancel_ticket(self.ticket);
    }
}

fn add_reservation(snapshot: &mut GovernorSnapshot, reservation: &Reservation) {
    snapshot.running += 1;
    snapshot.heavy_running += usize::from(reservation.heavy);
    snapshot.reserved_cpu_millis += reservation.cpu_millis;
    snapshot.reserved_memory_bytes += reservation.memory_bytes;
    snapshot.reserved_processes += reservation.processes;
    snapshot.reserved_output_bytes += reservation.output_bytes;
}

pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
