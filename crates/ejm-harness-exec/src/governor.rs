use std::{
    collections::{HashSet, VecDeque},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail, ensure};
use tokio::sync::{Notify, oneshot};
use tokio_util::sync::CancellationToken;

use crate::{
    AdmissionPolicy, GovernorSnapshot, OperatingSnapshot, OperatingTarget, Reservation,
    ResourceLimits, capacity::OperatingState, monitor::ProcessMonitor,
};

/// Metadata allowance keeps even very small retained artifacts bounded.
pub(crate) const ARTIFACT_OVERHEAD: u64 = 4096;
const MAX_BYPASS: u8 = 8;
const MAX_BYPASS_AGE: Duration = Duration::from_secs(1);

pub struct Governor {
    pub(crate) limits: ResourceLimits,
    state: Mutex<State>,
    idle: Notify,
    artifacts: Mutex<ArtifactState>,
    pub(crate) monitor: Arc<ProcessMonitor>,
    account_output_memory: bool,
}

struct State {
    snapshot: GovernorSnapshot,
    sessions: VecDeque<SessionQueue>,
    next_ticket: u64,
    resident_cpu: u32,
    resident_memory: u64,
    resident_processes: u32,
    operating: OperatingState,
    admission_activity: u64,
    admission_revision: u64,
    admission_revision_exhausted: bool,
}

struct SessionQueue {
    id: String,
    waiters: VecDeque<Waiter>,
}

struct Waiter {
    ticket: u64,
    reservation: Reservation,
    destination: WaiterDestination,
    job: bool,
    queued_at: Instant,
    bypasses: u8,
}

enum WaiterDestination {
    Lease(oneshot::Sender<Lease>),
    Staged(Arc<Notify>),
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
    job: bool,
}

/// A bounded queued request that holds no resource lease before activation.
///
/// Dropping this handle removes its original queue entry. Preparation retries
/// preserve the entry's session order, age and bounded bypass count.
pub struct QueuedAdmission {
    queued: QueuedTicket,
    reservation: Reservation,
    ready: Arc<Notify>,
    activated: bool,
}

/// A prospective turn, fenced to one ticket, grant and recovery revision.
#[derive(Clone, Copy)]
pub struct AdmissionEligibility {
    ticket: u64,
    generation: u64,
    revision: u64,
}

impl Governor {
    /// Create a shared governor from validated aggregate budgets.
    ///
    /// # Errors
    /// Returns an error for zero required budgets or limits above supported bounds.
    pub fn new(limits: ResourceLimits) -> Result<Self> {
        Self::build(limits, AdmissionPolicy::Fixed, false)
    }

    /// Create a governor for explicit capacity composition.
    ///
    /// Retained output is charged to the same memory account as tool/context
    /// reservations. Callers must install their charged shared grant before
    /// exposing the governor to participating work. This constructor performs no
    /// host I/O and does not itself join a capacity registry.
    ///
    /// # Errors
    /// Returns the same hard-limit validation errors as [`Self::new`].
    pub fn new_with_policy(limits: ResourceLimits, policy: AdmissionPolicy) -> Result<Self> {
        Self::build(limits, policy, true)
    }

    fn build(
        limits: ResourceLimits,
        policy: AdmissionPolicy,
        account_output_memory: bool,
    ) -> Result<Self> {
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
        let target = OperatingTarget::from_limits(&limits);
        Ok(Self {
            limits,
            state: Mutex::new(State {
                snapshot: GovernorSnapshot::default(),
                sessions: VecDeque::new(),
                next_ticket: 0,
                resident_cpu: 0,
                resident_memory: 0,
                resident_processes: 0,
                admission_activity: 0,
                admission_revision: 0,
                admission_revision_exhausted: false,
                operating: OperatingState {
                    policy,
                    generation: 0,
                    target,
                    contact_deadline: None,
                    memory_leases: 0,
                },
            }),
            idle: Notify::new(),
            artifacts: Mutex::new(ArtifactState::default()),
            monitor: Arc::new(ProcessMonitor::default()),
            account_output_memory,
        })
    }

    #[must_use]
    pub fn snapshot(&self) -> GovernorSnapshot {
        lock(&self.state).snapshot.clone()
    }

    /// The resident floor plus the oldest queued request needed for progress.
    ///
    /// This bounded cached projection separates useful work from hard ceilings.
    /// When no request waits, it describes current use. It performs no host I/O.
    #[must_use]
    pub fn admission_demand(&self) -> OperatingTarget {
        let state = lock(&self.state);
        let Some(head) = state
            .sessions
            .front()
            .and_then(|session| session.waiters.front())
        else {
            let used = &state.snapshot;
            return OperatingTarget {
                max_jobs: used.running,
                max_heavy_jobs: used.heavy_running,
                cpu_millis: used.reserved_cpu_millis,
                memory_bytes: used.reserved_memory_bytes,
                max_processes: used.reserved_processes,
                max_output_bytes: used.reserved_output_bytes,
            };
        };
        OperatingTarget {
            max_jobs: usize::from(head.job),
            max_heavy_jobs: usize::from(head.reservation.heavy),
            cpu_millis: state
                .resident_cpu
                .saturating_add(head.reservation.cpu_millis),
            memory_bytes: state
                .resident_memory
                .saturating_add(head.reservation.memory_bytes),
            max_processes: state
                .resident_processes
                .saturating_add(head.reservation.processes),
            max_output_bytes: head.reservation.output_bytes,
        }
    }

    /// Cached resident reservations, without transient jobs or host I/O.
    #[must_use]
    pub fn admission_floor(&self) -> OperatingTarget {
        let state = lock(&self.state);
        OperatingTarget {
            cpu_millis: state.resident_cpu,
            memory_bytes: state.resident_memory,
            max_processes: state.resident_processes,
            ..OperatingTarget::default()
        }
    }

    /// Cached monotonic count of actual job grants, including staged activation.
    ///
    /// Resident leases, prospective eligibility and rejected activation do not
    /// advance this counter. It saturates rather than wrapping and performs no
    /// host or registry I/O.
    #[must_use]
    pub fn admission_activity(&self) -> u64 {
        lock(&self.state).admission_activity
    }

    /// The immutable aggregate limits used for every admission decision.
    #[must_use]
    pub fn limits(&self) -> &ResourceLimits {
        &self.limits
    }

    /// Inspect the cached target and drain debt without host or registry I/O.
    #[must_use]
    pub fn operating_snapshot(&self) -> OperatingSnapshot {
        let state = lock(&self.state);
        let operating = &state.operating;
        let now = Instant::now();
        OperatingSnapshot {
            policy: operating.policy,
            generation: operating.generation,
            target: operating.target,
            debt: operating.target.debt(&state.snapshot),
            memory_leases: operating.memory_leases,
            contact_valid: operating.contact_valid(now),
            contact_remaining: operating
                .contact_deadline
                .map(|deadline| deadline.saturating_duration_since(now)),
        }
    }

    /// Stop future admission before releasing a host lifetime witness.
    ///
    /// This only restricts the current target. It preserves the generation and
    /// outstanding leases, and cannot grant capacity even at generation exhaustion.
    pub fn park_admission(&self) {
        {
            let mut state = lock(&self.state);
            state.operating.target = OperatingTarget::default();
            state.operating.contact_deadline = Some(Instant::now());
            if let Some(next) = state.admission_revision.checked_add(1) {
                state.admission_revision = next;
            } else {
                state.admission_revision_exhausted = true;
            }
        }
        self.idle.notify_waiters();
    }

    /// Read the cached fence for recovery sampling without host I/O.
    ///
    /// Capture this before sampling and use [`Self::install_target_for_revision`]
    /// so a concurrent park cannot be undone by a stale sample or grant.
    #[must_use]
    pub fn admission_revision(&self) -> u64 {
        lock(&self.state).admission_revision
    }

    /// Atomically install a charged grant or a smaller draining target.
    ///
    /// The core must charge an increase before calling this method, and retain
    /// the previous shared charge until a reduction has drained. Expiry stops
    /// future admissions; it never releases existing leases. Every update,
    /// including renewal, needs a strictly newer local generation.
    ///
    /// # Errors
    /// Rejects stale generations, targets above immutable limits, inconsistent
    /// heavy-job counts, and deadlines that have already expired.
    pub fn install_target(
        self: &Arc<Self>,
        generation: u64,
        target: OperatingTarget,
        contact_deadline: Option<Instant>,
    ) -> Result<()> {
        self.install_charged_target(None, generation, target, contact_deadline)
    }

    /// Install only if admission has not been parked since observation began.
    ///
    /// Shared charging and generation rules match [`Self::install_target`].
    ///
    /// # Errors
    /// Also rejects a changed recovery revision or exhausted revision counter.
    pub fn install_target_for_revision(
        self: &Arc<Self>,
        expected_revision: u64,
        generation: u64,
        target: OperatingTarget,
        contact_deadline: Option<Instant>,
    ) -> Result<()> {
        self.install_charged_target(
            Some(expected_revision),
            generation,
            target,
            contact_deadline,
        )
    }

    fn install_charged_target(
        self: &Arc<Self>,
        expected_revision: Option<u64>,
        generation: u64,
        target: OperatingTarget,
        contact_deadline: Option<Instant>,
    ) -> Result<()> {
        ensure!(
            target.max_jobs <= self.limits.max_jobs,
            "job target exceeds hard limit"
        );
        ensure!(
            target.max_heavy_jobs <= target.max_jobs
                && target.max_heavy_jobs <= self.limits.max_heavy_jobs,
            "heavy-job target exceeds hard limit or job target"
        );
        ensure!(
            target.cpu_millis <= self.limits.cpu_millis,
            "CPU target exceeds hard limit"
        );
        ensure!(
            target.memory_bytes <= self.limits.memory_bytes,
            "memory target exceeds hard limit"
        );
        ensure!(
            target.max_processes <= self.limits.max_processes,
            "process target exceeds hard limit"
        );
        ensure!(
            target.max_output_bytes <= self.limits.max_output_bytes,
            "output target exceeds hard limit"
        );
        {
            let mut state = lock(&self.state);
            ensure!(
                !state.admission_revision_exhausted
                    && expected_revision
                        .is_none_or(|revision| revision == state.admission_revision),
                "admission recovery revision changed or exhausted"
            );
            ensure!(
                generation > state.operating.generation,
                "stale admission generation"
            );
            ensure!(
                contact_deadline.is_none_or(|deadline| deadline > Instant::now()),
                "admission contact deadline expired"
            );
            state.operating.generation = generation;
            state.operating.target = target;
            state.operating.contact_deadline = contact_deadline;
        }
        self.dispatch();
        self.idle.notify_waiters();
        Ok(())
    }

    /// Wait until outstanding reservations fit an installed target.
    ///
    /// # Errors
    /// Returns an error on cancellation or if another generation replaces the
    /// requested target. A successful wait does not itself release shared credit.
    pub async fn wait_drained(&self, generation: u64, cancel: &CancellationToken) -> Result<()> {
        loop {
            let notified = self.idle.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            {
                let state = lock(&self.state);
                ensure!(
                    state.operating.generation == generation,
                    "admission generation changed while draining"
                );
                if state.operating.target.debt(&state.snapshot) == OperatingTarget::default() {
                    return Ok(());
                }
            }
            tokio::select! {
                biased;
                () = cancel.cancelled() => bail!("admission drain cancelled"),
                () = notified => {},
            }
        }
    }

    /// Wait for all admission waiters and leases to leave. Hosts must stop
    /// submitting new work and cancel their jobs before draining on shutdown.
    pub async fn wait_idle(&self) {
        self.wait_idle_kind(true).await;
    }

    /// Wait for execution jobs and queue removals while retaining resident
    /// context/package memory. Drop those resident leases before final
    /// [`Self::wait_idle`] and release of the shared process grant.
    pub async fn wait_jobs_idle(&self) {
        self.wait_idle_kind(false).await;
    }

    async fn wait_idle_kind(&self, include_memory: bool) {
        loop {
            let notified = self.idle.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            {
                let state = lock(&self.state);
                if state.snapshot.running == 0
                    && state.snapshot.queued == 0
                    && (!include_memory || state.operating.memory_leases == 0)
                {
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
        self.acquire_kind(session_id, reservation, cancel, true)
            .await
    }

    /// Queue native preparation without reserving execution resources.
    ///
    /// Demand becomes visible immediately through the existing queue snapshot
    /// and resident-floor projection. Callers wait for eligibility, establish
    /// native readiness, then atomically activate this same ticket. Drop unused
    /// readiness before waiting again after failed activation.
    ///
    /// # Errors
    /// Returns identity, reservation, queue-bound or ticket-exhaustion errors.
    pub fn stage(
        self: &Arc<Self>,
        session_id: &str,
        reservation: Reservation,
    ) -> Result<QueuedAdmission> {
        let reservation = self.normalize_reservation(session_id, reservation)?;
        let ready = Arc::new(Notify::new());
        let ticket = self.enqueue(
            session_id,
            reservation.clone(),
            WaiterDestination::Staged(ready.clone()),
            true,
        )?;
        let queued = QueuedAdmission {
            queued: QueuedTicket {
                governor: self.clone(),
                ticket,
            },
            reservation,
            ready,
            activated: false,
        };
        self.dispatch();
        Ok(queued)
    }

    /// Reserve context, model-buffer or idle-host memory without a tool slot.
    ///
    /// # Errors
    /// Returns the same queue, validation and cancellation errors as
    /// [`Self::acquire`]. A zero memory reservation is rejected.
    pub async fn acquire_memory(
        self: &Arc<Self>,
        session_id: &str,
        memory_bytes: u64,
        cancel: &CancellationToken,
    ) -> Result<Lease> {
        self.acquire_resident(session_id, memory_bytes, 0, cancel)
            .await
    }

    /// Reserve resident memory and processes without an active command slot.
    ///
    /// Keep this lease until the resident process has been reaped. It carries
    /// no CPU or output reservation and uses the same memory/process account
    /// as command leases.
    ///
    /// # Errors
    /// Returns the queue, validation and cancellation errors from
    /// [`Self::acquire`]. A zero memory reservation is rejected.
    pub async fn acquire_resident(
        self: &Arc<Self>,
        session_id: &str,
        memory_bytes: u64,
        processes: u32,
        cancel: &CancellationToken,
    ) -> Result<Lease> {
        self.acquire_resident_envelope(session_id, 0, memory_bytes, processes, cancel)
            .await
    }

    /// Reserve a resident scope's full CPU, memory and task envelope without
    /// consuming an active job slot. Retain it through verified scope cleanup.
    /// OCI CPU quotas and PID limits count the whole interpreter generation,
    /// including background threads between callbacks.
    ///
    /// # Errors
    /// Returns the queue, validation and cancellation errors from
    /// [`Self::acquire`]. A zero memory reservation is rejected.
    pub async fn acquire_resident_envelope(
        self: &Arc<Self>,
        session_id: &str,
        cpu_millis: u32,
        memory_bytes: u64,
        processes: u32,
        cancel: &CancellationToken,
    ) -> Result<Lease> {
        ensure!(memory_bytes > 0, "memory reservation must be positive");
        self.acquire_kind(
            session_id,
            Reservation {
                memory_bytes,
                cpu_millis,
                processes,
                output_bytes: 0,
                heavy: false,
            },
            cancel,
            false,
        )
        .await
    }

    /// Reserve startup memory immediately without passing queued work.
    ///
    /// `None` means the owner must wait or report unavailable capacity. This
    /// method performs no registry I/O and never creates a fallback budget.
    ///
    /// # Errors
    /// Rejects an empty/oversized session identity, zero bytes, or an estimate
    /// exceeding the immutable memory ceiling.
    pub fn try_acquire_memory(
        self: &Arc<Self>,
        session_id: &str,
        memory_bytes: u64,
    ) -> Result<Option<Lease>> {
        self.try_acquire_resident(session_id, memory_bytes, 0)
    }

    /// Reserve resident memory and processes immediately without bypassing
    /// queued work. Keep the lease through native cleanup.
    ///
    /// # Errors
    /// Returns identity and hard-limit validation errors as
    /// [`Self::try_acquire_memory`].
    pub fn try_acquire_resident(
        self: &Arc<Self>,
        session_id: &str,
        memory_bytes: u64,
        processes: u32,
    ) -> Result<Option<Lease>> {
        self.try_acquire_resident_envelope(session_id, 0, memory_bytes, processes)
    }

    /// Reserve a resident CPU, memory and task envelope immediately, without
    /// bypassing queued work or consuming an active job slot.
    ///
    /// # Errors
    /// Returns the validation errors from [`Self::try_acquire_resident`].
    pub fn try_acquire_resident_envelope(
        self: &Arc<Self>,
        session_id: &str,
        cpu_millis: u32,
        memory_bytes: u64,
        processes: u32,
    ) -> Result<Option<Lease>> {
        ensure!(!session_id.is_empty(), "session_id must not be empty");
        ensure!(session_id.len() <= 256, "session_id exceeds 256 bytes");
        ensure!(memory_bytes > 0, "memory reservation must be positive");
        let reservation = Reservation {
            cpu_millis,
            memory_bytes,
            processes,
            output_bytes: 0,
            heavy: false,
        };
        self.validate_reservation(&reservation)?;
        {
            let mut state = lock(&self.state);
            if state.snapshot.queued != 0
                || !state.operating.contact_valid(Instant::now())
                || !fits(&state.snapshot, state.operating.target, &reservation, false)
            {
                return Ok(None);
            }
            add_reservation(&mut state.snapshot, &reservation, false);
            state.operating.memory_leases += 1;
            state.resident_cpu += reservation.cpu_millis;
            state.resident_memory += reservation.memory_bytes;
            state.resident_processes += reservation.processes;
        }
        Ok(Some(Lease {
            governor: self.clone(),
            reservation,
            job: false,
        }))
    }

    async fn acquire_kind(
        self: &Arc<Self>,
        session_id: &str,
        reservation: Reservation,
        cancel: &CancellationToken,
        job: bool,
    ) -> Result<Lease> {
        let reservation = self.normalize_reservation(session_id, reservation)?;
        ensure!(!cancel.is_cancelled(), "admission cancelled");
        let (sender, receiver) = oneshot::channel();
        let ticket = self.enqueue(
            session_id,
            reservation,
            WaiterDestination::Lease(sender),
            job,
        )?;
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

    fn normalize_reservation(
        &self,
        session_id: &str,
        mut reservation: Reservation,
    ) -> Result<Reservation> {
        ensure!(!session_id.is_empty(), "session_id must not be empty");
        ensure!(session_id.len() <= 256, "session_id exceeds 256 bytes");
        self.validate_reservation(&reservation)?;
        if self.account_output_memory {
            reservation.memory_bytes = reservation
                .memory_bytes
                .checked_add(
                    u64::try_from(reservation.output_bytes)
                        .context("output memory reservation overflow")?,
                )
                .context("memory reservation overflow")?;
            ensure!(
                reservation.memory_bytes <= self.limits.memory_bytes,
                "combined memory and output reservation exceeds aggregate limit"
            );
        }
        Ok(reservation)
    }

    fn enqueue(
        &self,
        session_id: &str,
        reservation: Reservation,
        destination: WaiterDestination,
        job: bool,
    ) -> Result<u64> {
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
                destination,
                job,
                queued_at: Instant::now(),
                bypasses: 0,
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
        Ok(ticket)
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

    fn dispatch(self: &Arc<Self>) {
        let mut grants = Vec::new();
        let mut preparing = None;
        {
            let mut state = lock(&self.state);
            while let Some(index) = Self::next_fitting(&state) {
                if let Some(WaiterDestination::Staged(ready)) = state
                    .sessions
                    .get(index)
                    .and_then(|session| session.waiters.front())
                    .map(|waiter| &waiter.destination)
                {
                    preparing = Some(ready.clone());
                    break;
                }
                let Some(waiter) = take_waiter(&mut state, index) else {
                    break;
                };
                add_reservation(&mut state.snapshot, &waiter.reservation, waiter.job);
                if waiter.job {
                    state.admission_activity = state.admission_activity.saturating_add(1);
                }
                state.operating.memory_leases += usize::from(!waiter.job);
                if !waiter.job {
                    state.resident_cpu += waiter.reservation.cpu_millis;
                    state.resident_memory += waiter.reservation.memory_bytes;
                    state.resident_processes += waiter.reservation.processes;
                }
                grants.push((waiter.destination, waiter.reservation, waiter.job));
            }
        }
        // Sending or dropping a lease may re-enter the governor. Never do it
        // under the state lock, even when the waiting receiver was dropped.
        for (destination, reservation, job) in grants {
            if let WaiterDestination::Lease(sender) = destination {
                let _ = sender.send(Lease {
                    governor: self.clone(),
                    reservation,
                    job,
                });
            }
        }
        if let Some(ready) = preparing {
            ready.notify_one();
        }
    }

    fn activate(
        self: &Arc<Self>,
        ticket: u64,
        eligibility: AdmissionEligibility,
    ) -> Result<Option<Lease>> {
        let reservation = {
            let mut state = lock(&self.state);
            if eligibility.ticket != ticket
                || eligibility.generation != state.operating.generation
                || eligibility.revision != state.admission_revision
                || state.admission_revision_exhausted
            {
                return Ok(None);
            }
            let Some(index) = Self::next_fitting(&state) else {
                return Ok(None);
            };
            let Some(head) = state.sessions[index].waiters.front() else {
                return Ok(None);
            };
            if head.ticket != ticket {
                return Ok(None);
            }
            ensure!(
                matches!(head.destination, WaiterDestination::Staged(_)),
                "admission ticket is not staged"
            );
            let waiter = take_waiter(&mut state, index).context("staged admission disappeared")?;
            add_reservation(&mut state.snapshot, &waiter.reservation, true);
            state.admission_activity = state.admission_activity.saturating_add(1);
            waiter.reservation
        };
        let lease = Lease {
            governor: self.clone(),
            reservation,
            job: true,
        };
        self.dispatch();
        self.idle.notify_waiters();
        Ok(Some(lease))
    }

    fn next_fitting(state: &State) -> Option<usize> {
        let now = Instant::now();
        if !state.operating.contact_valid(now) {
            return None;
        }
        for (index, session) in state.sessions.iter().enumerate() {
            let head = session.waiters.front()?;
            if fits(
                &state.snapshot,
                state.operating.target,
                &head.reservation,
                head.job,
            ) {
                return Some(index);
            }
            if state.operating.policy == AdmissionPolicy::Fixed
                || head.bypasses >= MAX_BYPASS
                || now.saturating_duration_since(head.queued_at) >= MAX_BYPASS_AGE
            {
                return None;
            }
        }
        None
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

impl QueuedAdmission {
    /// The exact queued envelope, including retained-output memory when enabled.
    #[must_use]
    pub fn reservation(&self) -> &Reservation {
        &self.reservation
    }

    /// Wait for this original ticket to be prospectively eligible.
    ///
    /// No lease or native keepalive may be retained solely for this wait.
    ///
    /// # Errors
    /// Returns an error on cancellation or after this ticket was activated.
    pub async fn wait_eligible(&self, cancel: &CancellationToken) -> Result<AdmissionEligibility> {
        ensure!(!self.activated, "staged admission is already active");
        loop {
            ensure!(!cancel.is_cancelled(), "admission cancelled");
            let notified = self.ready.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            {
                let state = lock(&self.queued.governor.state);
                if let Some(index) = Governor::next_fitting(&state)
                    && state.sessions[index]
                        .waiters
                        .front()
                        .is_some_and(|head| head.ticket == self.queued.ticket)
                    && !state.admission_revision_exhausted
                {
                    return Ok(AdmissionEligibility {
                        ticket: self.queued.ticket,
                        generation: state.operating.generation,
                        revision: state.admission_revision,
                    });
                }
            }
            tokio::select! {
                biased;
                () = cancel.cancelled() => bail!("admission cancelled"),
                () = notified => {},
            }
        }
    }

    /// Atomically activate this ticket under its unchanged eligibility fence.
    ///
    /// `None` retains the original queue entry and charges nothing. Release any
    /// unused native readiness before calling [`Self::wait_eligible`] again.
    ///
    /// # Errors
    /// Returns an error after activation or if queue invariants no longer hold.
    pub fn try_activate(&mut self, eligibility: AdmissionEligibility) -> Result<Option<Lease>> {
        ensure!(!self.activated, "staged admission is already active");
        let lease = self
            .queued
            .governor
            .activate(self.queued.ticket, eligibility)?;
        self.activated = lease.is_some();
        Ok(lease)
    }
}

impl Lease {
    /// Transfer part of a job envelope into a resident lease without re-admission.
    ///
    /// Aggregate reservations and the active job slot remain unchanged. The
    /// returned pair is `(resident, remaining_job)`, each with its own lifetime.
    ///
    /// # Errors
    /// Rejects a non-job lease, zero resident memory or a share above the lease's
    /// execution memory and process reservation. Retained-output memory remains
    /// with the job until output cleanup.
    pub fn split_resident(mut self, memory_bytes: u64, processes: u32) -> Result<(Self, Self)> {
        ensure!(self.job, "resident transfer requires an active job lease");
        ensure!(memory_bytes > 0, "resident transfer must include memory");
        let output_memory = if self.governor.account_output_memory {
            u64::try_from(self.reservation.output_bytes)
                .context("output memory reservation overflow")?
        } else {
            0
        };
        ensure!(
            memory_bytes <= self.reservation.memory_bytes.saturating_sub(output_memory)
                && processes <= self.reservation.processes,
            "resident transfer exceeds the admitted envelope"
        );
        let resident = Reservation {
            cpu_millis: 0,
            memory_bytes,
            processes,
            output_bytes: 0,
            heavy: false,
        };
        {
            let mut state = lock(&self.governor.state);
            state.operating.memory_leases += 1;
            state.resident_memory += memory_bytes;
            state.resident_processes += processes;
            self.reservation.memory_bytes -= memory_bytes;
            self.reservation.processes -= processes;
        }
        Ok((
            Self {
                governor: self.governor.clone(),
                reservation: resident,
                job: false,
            },
            self,
        ))
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        {
            let mut state = lock(&self.governor.state);
            state.operating.memory_leases -= usize::from(!self.job);
            if !self.job {
                state.resident_cpu -= self.reservation.cpu_millis;
                state.resident_memory -= self.reservation.memory_bytes;
                state.resident_processes -= self.reservation.processes;
            }
            let snapshot = &mut state.snapshot;
            snapshot.running -= usize::from(self.job);
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

fn add_reservation(snapshot: &mut GovernorSnapshot, reservation: &Reservation, job: bool) {
    snapshot.running += usize::from(job);
    snapshot.heavy_running += usize::from(reservation.heavy);
    snapshot.reserved_cpu_millis += reservation.cpu_millis;
    snapshot.reserved_memory_bytes += reservation.memory_bytes;
    snapshot.reserved_processes += reservation.processes;
    snapshot.reserved_output_bytes += reservation.output_bytes;
}

fn take_waiter(state: &mut State, index: usize) -> Option<Waiter> {
    for session in state.sessions.iter_mut().take(index) {
        if let Some(head) = session.waiters.front_mut() {
            head.bypasses = head.bypasses.saturating_add(1);
        }
    }
    let mut session = state.sessions.remove(index)?;
    let waiter = session.waiters.pop_front()?;
    if !session.waiters.is_empty() {
        state.sessions.push_back(session);
    }
    state.snapshot.queued -= 1;
    Some(waiter)
}

fn fits(
    snapshot: &GovernorSnapshot,
    target: OperatingTarget,
    reservation: &Reservation,
    job: bool,
) -> bool {
    (!job || snapshot.running < target.max_jobs)
        && (!reservation.heavy || snapshot.heavy_running < target.max_heavy_jobs)
        && reservation.cpu_millis
            <= target
                .cpu_millis
                .saturating_sub(snapshot.reserved_cpu_millis)
        && reservation.memory_bytes
            <= target
                .memory_bytes
                .saturating_sub(snapshot.reserved_memory_bytes)
        && reservation.processes
            <= target
                .max_processes
                .saturating_sub(snapshot.reserved_processes)
        && reservation.output_bytes
            <= target
                .max_output_bytes
                .saturating_sub(snapshot.reserved_output_bytes)
}

pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod recovery_tests {
    #![allow(
        clippy::unwrap_used,
        reason = "Recovery boundary assertions fail at their call site."
    )]

    use super::*;

    #[test]
    fn exhausted_recovery_revision_cannot_restore_admission() {
        let governor = Arc::new(Governor::new(ResourceLimits::default()).unwrap());
        lock(&governor.state).admission_revision = u64::MAX;
        governor.park_admission();
        let target = OperatingTarget::from_limits(governor.limits());
        assert!(governor.install_target(1, target, None).is_err());
        assert!(
            governor
                .install_target_for_revision(u64::MAX, 1, target, None)
                .is_err()
        );
        assert!(!governor.operating_snapshot().contact_valid);
    }
}
