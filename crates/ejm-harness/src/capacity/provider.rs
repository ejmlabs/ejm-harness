//! Cached provider admission. HTTP callbacks never access shared persistence.

use std::{
    collections::VecDeque,
    io::Write,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

use anyhow::{Context, Result, ensure};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use crate::{
    ModelRequest, ProviderCapacity, ProviderCapacityObserver, ProviderObservation,
    ProviderRateWindow, types::timestamp_ms,
};

use super::{
    CONTACT_TIME, lock,
    provider_registry::{self as ledger, Demand},
    registry::Registry,
};

const MAX_FEEDBACK: usize = 256;
const WARM_TIME: Duration = Duration::from_secs(2);
const MAX_WAIT: u64 = 15 * 60 * 1000;

#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ProviderSnapshot {
    pub domain: Option<String>,
    pub coordinated: bool,
    pub target: usize,
    pub in_flight: usize,
    pub waiting: usize,
    pub request_credits: Option<u64>,
    pub token_credits: Option<u64>,
    pub token_debt: u64,
    pub cooldown_until_ms: Option<u64>,
    pub blocked: bool,
    pub reason: Option<String>,
}

pub(super) struct ProviderGate {
    inner: Arc<Inner>,
}

struct Inner {
    owner: String,
    hard: usize,
    state: Mutex<State>,
    changed: Notify,
    wake: mpsc::SyncSender<()>,
    feedback: Mutex<VecDeque<Feedback>>,
    feedback_sequence: AtomicU64,
    fenced_sequence: AtomicU64,
    overflow: AtomicU64,
}

struct State {
    policy: ProviderCapacity,
    prepared_generation: Option<u64>,
    generation: u64,
    ticket: u64,
    target: usize,
    active: usize,
    pending: VecDeque<(u64, u64)>,
    last_active: Option<Instant>,
    requests: Option<u64>,
    tokens: Option<u64>,
    request_reserve: Option<u64>,
    token_reserve: Option<u64>,
    token_debt: u64,
    expires: u64,
    epoch: u64,
    contact: Option<Instant>,
    uses_ledger: bool,
    initialized: bool,
    feedback_sequence: u64,
    cooldown: u64,
    blocked: bool,
    clear_through: Option<u64>,
    fatal: Option<String>,
    reason: Option<String>,
    unsent: VecDeque<Feedback>,
}

struct Feedback {
    sequence: u64,
    generation: u64,
    at: u64,
    observation: ProviderObservation,
}

#[derive(Default)]
struct ActualUsage {
    finished: AtomicBool,
    known: AtomicBool,
    tokens: AtomicU64,
}

pub(crate) struct ProviderLease {
    inner: Arc<Inner>,
    generation: u64,
    epoch: u64,
    reserved: u64,
    usage: Arc<ActualUsage>,
}

pub(crate) struct PreparedProvider {
    inner: Arc<Inner>,
    policy: ProviderCapacity,
    generation: Option<u64>,
    committed: bool,
}

struct Observer {
    inner: Arc<Inner>,
    generation: u64,
    usage: Arc<ActualUsage>,
}

struct Waiter {
    inner: Arc<Inner>,
    ticket: u64,
}

impl ProviderGate {
    pub fn new(owner: String, hard: usize, wake: mpsc::SyncSender<()>) -> Self {
        Self {
            inner: Arc::new(Inner {
                owner,
                hard,
                wake,
                feedback: Mutex::new(VecDeque::new()),
                changed: Notify::new(),
                feedback_sequence: AtomicU64::new(0),
                fenced_sequence: AtomicU64::new(0),
                overflow: AtomicU64::new(0),
                state: Mutex::new(State {
                    policy: ProviderCapacity::default(),
                    prepared_generation: None,
                    generation: 1,
                    ticket: 0,
                    target: hard,
                    active: 0,
                    pending: VecDeque::new(),
                    last_active: None,
                    requests: None,
                    tokens: None,
                    request_reserve: None,
                    token_reserve: None,
                    token_debt: 0,
                    expires: u64::MAX,
                    epoch: 0,
                    contact: None,
                    uses_ledger: false,
                    initialized: false,
                    feedback_sequence: 0,
                    cooldown: 0,
                    blocked: false,
                    clear_through: None,
                    fatal: None,
                    reason: None,
                    unsent: VecDeque::new(),
                }),
            }),
        }
    }

    #[cfg(test)]
    pub fn configure(&self, policy: ProviderCapacity) -> Result<()> {
        self.prepare(policy)?.commit();
        Ok(())
    }

    pub(crate) fn prepare(&self, policy: ProviderCapacity) -> Result<PreparedProvider> {
        policy.validate()?;
        let mut state = lock(&self.inner.state);
        ensure!(
            state.prepared_generation.is_none(),
            "provider capacity configuration is already in progress"
        );
        let generation = if state.policy == policy {
            None
        } else {
            ensure!(
                state.active == 0 && state.pending.is_empty(),
                "provider capacity can change only while model streams and quota waits are idle"
            );
            ensure!(
                state.token_debt == 0
                    && state.unsent.is_empty()
                    && state.feedback_sequence
                        == self.inner.feedback_sequence.load(Ordering::Acquire)
                    && self.inner.overflow.load(Ordering::Acquire) == 0,
                "provider capacity has uncommitted feedback; wait for shared coordination before changing the quota domain"
            );
            Some(
                state
                    .generation
                    .checked_add(1)
                    .context("provider configuration generation exhausted")?,
            )
        };
        state.prepared_generation = Some(generation.unwrap_or(state.generation));
        Ok(PreparedProvider {
            inner: self.inner.clone(),
            policy,
            generation,
            committed: false,
        })
    }

    pub fn snapshot(&self) -> ProviderSnapshot {
        let state = lock(&self.inner.state);
        let valid = !state.uses_ledger || state.contact.is_some_and(|until| Instant::now() < until);
        ProviderSnapshot {
            domain: state
                .policy
                .domain
                .as_ref()
                .map(|domain| domain.id().to_owned()),
            coordinated: state.policy.domain.is_some() && valid,
            target: if valid { state.target } else { 0 },
            in_flight: state.active,
            waiting: state.pending.len(),
            request_credits: state.requests,
            token_credits: state.tokens,
            token_debt: state.token_debt,
            cooldown_until_ms: (state.cooldown > timestamp_ms()).then_some(state.cooldown),
            blocked: state.blocked || self.inner.overflow.load(Ordering::Acquire) != 0,
            reason: state.reason.clone(),
        }
    }

    pub fn request_clear_cooldown(&self) {
        let mut state = lock(&self.inner.state);
        state.clear_through = Some(self.inner.feedback_sequence.load(Ordering::Acquire));
        state.contact = None;
        drop(state);
        let _ = self.inner.wake.try_send(());
    }

    pub async fn acquire(
        &self,
        request: &ModelRequest,
        cancel: &CancellationToken,
    ) -> Result<ProviderLease> {
        let cost = request_cost(request)?;
        let ticket = self.enqueue(cost)?;
        let _waiter = Waiter {
            inner: self.inner.clone(),
            ticket,
        };
        let _ = self.inner.wake.try_send(());
        loop {
            let changed = self.inner.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if let Some(lease) = self.try_admit(ticket, cost, cancel.is_cancelled())? {
                self.inner.changed.notify_waiters();
                return Ok(lease);
            }
            tokio::select! {
                biased;
                () = cancel.cancelled() => anyhow::bail!("cancelled while waiting for provider quota"),
                () = &mut changed => {},
                () = tokio::time::sleep(Duration::from_secs(1)) => { let _ = self.inner.wake.try_send(()); }
            }
        }
    }

    fn enqueue(&self, cost: u64) -> Result<u64> {
        let mut state = lock(&self.inner.state);
        ensure!(
            state.pending.len() < self.inner.hard,
            "provider quota wait queue is full"
        );
        state.check_cost(cost)?;
        let ticket = state.ticket;
        state.ticket = state
            .ticket
            .checked_add(1)
            .context("provider queue ticket exhausted")?;
        state.pending.push_back((ticket, cost));
        Ok(ticket)
    }

    fn try_admit(&self, ticket: u64, cost: u64, cancelled: bool) -> Result<Option<ProviderLease>> {
        let mut state = lock(&self.inner.state);
        state.check_cost(cost)?;
        ensure!(
            state.fatal.is_none(),
            "{}",
            state.fatal.as_deref().unwrap_or_default()
        );
        ensure!(
            !state.blocked && self.inner.overflow.load(Ordering::Acquire) == 0,
            "provider quota is blocked; an explicit host cooldown reset is required"
        );
        let now = timestamp_ms();
        ensure!(
            state.cooldown.saturating_sub(now) <= MAX_WAIT,
            "provider cooldown exceeds the 15 minute local wait horizon; its actual reset remains in force"
        );
        let contact =
            !state.uses_ledger || state.contact.is_some_and(|until| Instant::now() < until);
        let sequence = self.inner.fenced_sequence.load(Ordering::Acquire);
        let credits_valid =
            state.expires > now || (state.requests.is_none() && state.tokens.is_none());
        if cancelled
            || state.prepared_generation.is_some()
            || !contact
            || !credits_valid
            || state.feedback_sequence < sequence
            || state.cooldown > now
            || state.active >= state.target
            || state
                .pending
                .front()
                .is_none_or(|(first, _)| *first != ticket)
            || state.requests.is_some_and(|remaining| remaining == 0)
            || state.tokens.is_some_and(|remaining| remaining < cost)
            || state.token_debt != 0
        {
            return Ok(None);
        }
        State::spend(&mut state.requests, 1);
        State::spend(&mut state.tokens, cost);
        State::spend(&mut state.request_reserve, 1);
        State::spend(&mut state.token_reserve, cost);
        state.pending.pop_front();
        state.active += 1;
        state.last_active = Some(Instant::now());
        Ok(Some(ProviderLease {
            inner: self.inner.clone(),
            generation: state.generation,
            epoch: state.epoch,
            reserved: cost,
            usage: Arc::new(ActualUsage::default()),
        }))
    }

    pub fn coordinate(&self, registry: &mut Registry) -> Result<()> {
        self.coordinate_connection(&mut registry.connection)
    }

    fn coordinate_connection(&self, connection: &mut rusqlite::Connection) -> Result<()> {
        let (sequence, feedback) = {
            let mut queued = lock(&self.inner.feedback);
            let sequence = self.inner.feedback_sequence.load(Ordering::Acquire);
            (sequence, queued.drain(..).collect::<Vec<_>>())
        };
        let now = timestamp_ms();
        let Some((demand, generation, initialize, clear)) = self.demand(sequence, feedback, now)
        else {
            return Ok(());
        };
        if initialize {
            ledger::initialize(connection)?;
        }
        let grant = match ledger::allocate(connection, &demand, now) {
            Ok(grant) => grant,
            Err(error) => {
                if let Some(error) = error.downcast_ref::<ledger::ContractError>() {
                    lock(&self.inner.state).fatal = Some(error.to_string());
                    self.inner.changed.notify_waiters();
                }
                return Err(error);
            }
        };
        let active = self.install(&demand, &grant, generation, sequence, clear, now)?;
        self.inner.changed.notify_waiters();
        ledger::release_slots(connection, &self.inner.owner, &grant, active)
    }

    fn demand(
        &self,
        sequence: u64,
        feedback: Vec<Feedback>,
        now: u64,
    ) -> Option<(Demand, u64, bool, Option<u64>)> {
        let mut state = lock(&self.inner.state);
        for event in feedback {
            if event.generation == state.generation {
                let sequence = event.sequence;
                if !queue_feedback(&mut state.unsent, event) {
                    self.inner.overflow.fetch_max(sequence, Ordering::Release);
                }
            }
        }
        state.uses_ledger |=
            !state.unsent.is_empty() || self.inner.overflow.load(Ordering::Acquire) != 0;
        if state.prepared_generation.is_some() {
            return None;
        }
        if !state.uses_ledger {
            // Usage-only observers and providers without quota hints preserve
            // the original local in-flight behavior.
            state.feedback_sequence = sequence;
            return None;
        }
        if state.expires <= now {
            state.requests = state.requests.map(|_| 0);
            state.tokens = state.tokens.map(|_| 0);
        }
        let active = state.active > 0
            || !state.pending.is_empty()
            || state.last_active.is_some_and(|at| at.elapsed() < WARM_TIME);
        let hard = self
            .inner
            .hard
            .min(state.policy.max_in_flight.unwrap_or(self.inner.hard));
        let clear = state.clear_through;
        let demand = Demand {
            owner: self.inner.owner.clone(),
            domain: domain_id(&self.inner.owner, &state.policy),
            policy: state.policy.clone(),
            hard,
            desired: if active { hard } else { 0 },
            active: state.active,
            waiting: state.pending.len(),
            tokens_needed: state
                .pending
                .iter()
                .map(|(_, cost)| *cost)
                .fold(0, u64::saturating_add),
            largest_request: state
                .pending
                .iter()
                .map(|(_, cost)| *cost)
                .max()
                .unwrap_or(0),
            requests_left: state.requests.unwrap_or(0),
            tokens_left: state.tokens.unwrap_or(0),
            token_debt: state.token_debt,
            expires: state.expires,
            epoch: state.epoch,
            clear_cooldown: clear.is_some(),
            feedback_lost: {
                let overflow = self.inner.overflow.load(Ordering::Acquire);
                overflow > 0 && clear.is_none_or(|through| overflow > through)
            },
            feedback: state
                .unsent
                .iter()
                .filter(|event| {
                    !clear.is_some_and(|through| {
                        event.sequence <= through
                            && matches!(event.observation, ProviderObservation::Throttled { .. })
                    })
                })
                .map(|event| (event.at, event.observation.clone()))
                .collect(),
        };
        Some((demand, state.generation, !state.initialized, clear))
    }

    fn install(
        &self,
        demand: &Demand,
        grant: &ledger::Grant,
        generation: u64,
        sequence: u64,
        clear: Option<u64>,
        now: u64,
    ) -> Result<usize> {
        let feedback = lock(&self.inner.feedback);
        let mut state = lock(&self.inner.state);
        ensure!(
            generation == state.generation,
            "provider configuration changed before a committed grant was installed"
        );
        state.initialized = true;
        if state.epoch != grant.epoch || state.expires <= now {
            state.requests = state.requests.map(|_| 0);
            state.tokens = state.tokens.map(|_| 0);
        }
        state.requests = grant
            .request_credit
            .map(|credit| state.requests.unwrap_or(0).saturating_add(credit));
        state.tokens = grant
            .token_credit
            .map(|credit| state.tokens.unwrap_or(0).saturating_add(credit));
        let matching = demand.epoch == grant.epoch && demand.expires > now;
        state.request_reserve = grant.request_reserve.map(|reserved| {
            reserved
                .saturating_sub(if matching { demand.requests_left } else { 0 })
                .saturating_add(
                    state
                        .requests
                        .unwrap_or(0)
                        .saturating_sub(grant.request_credit.unwrap_or(0)),
                )
        });
        state.token_reserve = grant.token_reserve.map(|reserved| {
            reserved
                .saturating_sub(if matching { demand.tokens_left } else { 0 })
                .saturating_add(
                    state
                        .tokens
                        .unwrap_or(0)
                        .saturating_sub(grant.token_credit.unwrap_or(0)),
                )
        });
        // Feedback received during the registry transaction must also fit the
        // newly installed pool. Otherwise a concurrent larger grant could
        // overtake a hint that was safe for the old, smaller pool.
        for event in feedback.iter() {
            if event.generation == state.generation && state.fences(&event.observation) {
                self.inner
                    .fenced_sequence
                    .fetch_max(event.sequence, Ordering::Release);
            }
        }
        state.token_debt = state.token_debt.saturating_sub(demand.token_debt);
        state.epoch = grant.epoch;
        state.expires = grant.expires;
        state.target = grant.target;
        state.contact = Some(Instant::now() + CONTACT_TIME);
        state.cooldown = grant.cooldown;
        if let Some(through) = clear {
            let overflow = self.inner.overflow.load(Ordering::Acquire);
            if overflow <= through {
                let _ = self.inner.overflow.compare_exchange(
                    overflow,
                    0,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                );
            }
            if state.clear_through == clear {
                state.clear_through = None;
            }
        }
        state.blocked = grant.blocked || self.inner.overflow.load(Ordering::Acquire) != 0;
        if state.clear_through.is_some() {
            state.contact = None;
        }
        state.feedback_sequence = sequence;
        state.unsent.clear();
        state.fatal = None;
        state.reason = if state.blocked {
            Some("provider cooldown requires an explicit host reset".into())
        } else if state.cooldown > now {
            Some("provider cooldown; existing failures are not retried".into())
        } else if state.pending.is_empty() {
            None
        } else {
            Some("sharing provider in-flight or prepaid rate capacity".into())
        };
        Ok(state.active)
    }

    pub fn failed(&self) {
        let mut state = lock(&self.inner.state);
        state.reason = state.fatal.clone().or_else(|| {
            Some(
                "provider coordination unavailable; cached admission expires within eight seconds"
                    .into(),
            )
        });
        drop(state);
        self.inner.changed.notify_waiters();
    }
}

impl PreparedProvider {
    pub(crate) fn commit(mut self) {
        let mut state = lock(&self.inner.state);
        if let Some(generation) = self.generation {
            state.generation = generation;
            state.uses_ledger |= self.policy.domain.is_some()
                || self.policy.requests.is_some()
                || self.policy.tokens.is_some();
            state.target = if state.uses_ledger {
                0
            } else {
                self.policy
                    .max_in_flight
                    .unwrap_or(self.inner.hard)
                    .min(self.inner.hard)
            };
            state.policy = std::mem::take(&mut self.policy);
            state.requests = None;
            state.tokens = None;
            state.request_reserve = None;
            state.token_reserve = None;
            state.expires = 0;
            state.contact = None;
            state.cooldown = 0;
            state.blocked = false;
            state.fatal = None;
            state.reason = None;
            state.clear_through = None;
        }
        state.prepared_generation = None;
        self.committed = true;
        drop(state);
        self.inner.changed.notify_waiters();
        let _ = self.inner.wake.try_send(());
    }
}

impl Drop for PreparedProvider {
    fn drop(&mut self) {
        if !self.committed {
            lock(&self.inner.state).prepared_generation = None;
            self.inner.changed.notify_waiters();
            let _ = self.inner.wake.try_send(());
        }
    }
}

impl State {
    fn check_cost(&self, cost: u64) -> Result<()> {
        if self.prepared_generation.is_none()
            && let Some(rate) = self.policy.tokens
        {
            ensure!(
                cost <= rate.limit,
                "prepared request token estimate exceeds its provider rate ceiling"
            );
        }
        Ok(())
    }

    fn spend(credits: &mut Option<u64>, cost: u64) {
        if let Some(credits) = credits {
            *credits = credits.saturating_sub(cost);
        }
    }

    fn fences(&self, observation: &ProviderObservation) -> bool {
        match observation {
            ProviderObservation::Limits { requests, tokens } => {
                requests.is_some_and(|hint| {
                    self.request_reserve
                        .is_none_or(|unused| hint.remaining < unused)
                }) || tokens.is_some_and(|hint| {
                    self.token_reserve
                        .is_none_or(|unused| hint.remaining < unused)
                })
            }
            ProviderObservation::Throttled { .. } => true,
            ProviderObservation::Usage(_) => false,
        }
    }
}

fn queue_feedback(queued: &mut VecDeque<Feedback>, event: Feedback) -> bool {
    if let ProviderObservation::Limits { requests, tokens } = &event.observation
        && let Some(existing) = queued.iter_mut().find(|existing| {
            existing.generation == event.generation
                && matches!(existing.observation, ProviderObservation::Limits { .. })
        })
        && let ProviderObservation::Limits {
            requests: old_requests,
            tokens: old_tokens,
        } = &mut existing.observation
    {
        merge_hint(old_requests, *requests);
        merge_hint(old_tokens, *tokens);
        existing.sequence = event.sequence;
        existing.at = event.at;
        return true;
    }
    if queued.len() == MAX_FEEDBACK {
        return false;
    }
    queued.push_back(event);
    true
}

fn merge_hint(previous: &mut Option<ProviderRateWindow>, next: Option<ProviderRateWindow>) {
    if let Some(next) = next {
        if let Some(previous) = previous {
            previous.limit = previous.limit.min(next.limit);
            previous.remaining = previous.remaining.min(next.remaining);
            previous.reset_at_ms = previous.reset_at_ms.max(next.reset_at_ms);
        } else {
            *previous = Some(next);
        }
    }
}

impl ProviderLease {
    pub(crate) fn observer(&self) -> Arc<dyn ProviderCapacityObserver> {
        Arc::new(Observer {
            inner: self.inner.clone(),
            generation: self.generation,
            usage: self.usage.clone(),
        })
    }
}

impl ProviderCapacityObserver for Observer {
    fn observe(&self, mut observation: ProviderObservation) {
        if self.usage.finished.load(Ordering::Acquire) {
            return;
        }
        if let ProviderObservation::Usage(usage) = &observation {
            if let Some(total) = usage.input_tokens.checked_add(usage.output_tokens)
                && usage
                    .cached_input_tokens
                    .is_none_or(|cached| cached <= usage.input_tokens)
                && usage
                    .reasoning_output_tokens
                    .is_none_or(|reasoning| reasoning <= usage.output_tokens)
            {
                self.usage.tokens.fetch_max(total, Ordering::Relaxed);
                self.usage.known.store(true, Ordering::Release);
            }
            return;
        }
        let now = timestamp_ms();
        if let ProviderObservation::Limits { requests, tokens } = &mut observation {
            *requests = requests.filter(|hint| ledger::valid_hint(*hint, now));
            *tokens = tokens.filter(|hint| ledger::valid_hint(*hint, now));
            if requests.is_none() && tokens.is_none() {
                return;
            }
        }
        let mut feedback = lock(&self.inner.feedback);
        if self.usage.finished.load(Ordering::Acquire) {
            return;
        }
        let sequence = self
            .inner
            .feedback_sequence
            .load(Ordering::Acquire)
            .saturating_add(1);
        if lock(&self.inner.state).fences(&observation) {
            self.inner
                .fenced_sequence
                .store(sequence, Ordering::Release);
        }
        if !queue_feedback(
            &mut feedback,
            Feedback {
                sequence,
                generation: self.generation,
                at: now,
                observation,
            },
        ) {
            self.inner.overflow.fetch_max(sequence, Ordering::Release);
        }
        self.inner
            .feedback_sequence
            .store(sequence, Ordering::Release);
        drop(feedback);
        let _ = self.inner.wake.try_send(());
    }
}

impl Drop for ProviderLease {
    fn drop(&mut self) {
        let _feedback = lock(&self.inner.feedback);
        self.usage.finished.store(true, Ordering::Release);
        let mut state = lock(&self.inner.state);
        state.active = state.active.saturating_sub(1);
        if state.generation == self.generation && self.usage.known.load(Ordering::Acquire) {
            let actual = self.usage.tokens.load(Ordering::Relaxed);
            if state.epoch == self.epoch
                && state.expires > timestamp_ms()
                && let Some(remaining) = &mut state.tokens
            {
                let unused = self.reserved.saturating_sub(actual);
                *remaining = remaining.saturating_add(unused);
                if let Some(reserved) = &mut state.token_reserve {
                    *reserved = reserved.saturating_add(unused);
                }
            }
            let debt = if state.tokens.is_some() {
                actual.saturating_sub(self.reserved)
            } else {
                0
            };
            state.token_debt = state.token_debt.saturating_add(debt);
            if debt > 0 {
                // Persist excess actual usage even without a subsequent waiter.
                state.contact = None;
            }
        }
        state.last_active = Some(Instant::now());
        drop(state);
        self.inner.changed.notify_waiters();
        let _ = self.inner.wake.try_send(());
    }
}

impl Drop for Waiter {
    fn drop(&mut self) {
        let mut state = lock(&self.inner.state);
        if let Some(index) = state
            .pending
            .iter()
            .position(|(ticket, _)| *ticket == self.ticket)
        {
            state.pending.remove(index);
        }
        drop(state);
        self.inner.changed.notify_waiters();
    }
}

fn domain_id(owner: &str, policy: &ProviderCapacity) -> String {
    policy
        .domain
        .as_ref()
        .map_or_else(|| format!("local-{owner}"), |domain| domain.id().to_owned())
}

fn request_cost(request: &ModelRequest) -> Result<u64> {
    struct Counter(u64);
    impl Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 = self
                .0
                .checked_add(u64::try_from(bytes.len()).map_err(std::io::Error::other)?)
                .ok_or_else(|| {
                    std::io::Error::other("request size exceeds provider admission range")
                })?;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut counter = Counter(0);
    serde_json::to_writer(&mut counter, request)?;
    counter
        .0
        .checked_add(u64::from(request.max_output_tokens))
        .and_then(|cost| cost.checked_add(256))
        .context("request token estimate exceeds provider admission range")
}

#[cfg(test)]
#[path = "provider_tests.rs"]
mod tests;
