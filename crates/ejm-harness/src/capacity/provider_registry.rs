//! Coarse provider grants. Prepaid rate charges outlive owners and host boots.

use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

use crate::{
    ProviderCapacity, ProviderObservation, ProviderRate, ProviderRateWindow, ProviderRetry,
};

const MAX_DOMAINS: i64 = 256;
const QUANTUM_MS: u64 = 2000;
const MAX_BLOCK: u64 = 4096;
const CREDIT_TIME_MS: u64 = 2500;
const REENTRY_STEP_MS: u64 = 500;

#[derive(Debug)]
pub(super) struct ContractError(&'static str);

impl std::fmt::Display for ContractError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.0)
    }
}

impl std::error::Error for ContractError {}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct Window {
    ends: u64,
    limit: u64,
    charged: u64,
    period: Option<u64>,
    charges: VecDeque<(u64, u64)>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct Domain {
    epoch: u64,
    request: Option<Window>,
    token: Option<Window>,
    request_hint: Option<Window>,
    token_hint: Option<Window>,
    cooldown: u64,
    blocked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Participant {
    policy: ProviderCapacity,
    hard: usize,
    desired: usize,
    slots: usize,
    served: u64,
    selected_until: u64,
    requests_left: u64,
    tokens_left: u64,
    expires: u64,
    epoch: u64,
    waiting: usize,
    tokens_needed: u64,
}

pub(super) struct Demand {
    pub owner: String,
    pub domain: String,
    pub policy: ProviderCapacity,
    pub hard: usize,
    pub desired: usize,
    pub active: usize,
    pub waiting: usize,
    pub tokens_needed: u64,
    pub largest_request: u64,
    pub requests_left: u64,
    pub tokens_left: u64,
    pub token_debt: u64,
    pub expires: u64,
    pub epoch: u64,
    pub clear_cooldown: bool,
    pub feedback_lost: bool,
    pub feedback: Vec<(u64, ProviderObservation)>,
}

pub(super) struct Grant {
    pub domain: String,
    pub revision: i64,
    pub target: usize,
    pub request_credit: Option<u64>,
    pub token_credit: Option<u64>,
    pub request_reserve: Option<u64>,
    pub token_reserve: Option<u64>,
    pub expires: u64,
    pub epoch: u64,
    pub cooldown: u64,
    pub blocked: bool,
}

pub(super) fn initialize(connection: &Connection) -> Result<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS provider_domains(
          id TEXT PRIMARY KEY CHECK(length(id)<=128),
          expires INTEGER NOT NULL, payload TEXT NOT NULL CHECK(length(payload)<=8192));
         CREATE TABLE IF NOT EXISTS provider_owners(
          owner TEXT PRIMARY KEY CHECK(length(owner)<=128), domain TEXT NOT NULL,
          revision INTEGER NOT NULL, payload TEXT NOT NULL CHECK(length(payload)<=2048));
         CREATE INDEX IF NOT EXISTS provider_owners_domain ON provider_owners(domain,owner);",
    )?;
    Ok(())
}

/// Allocate before installing increases. The caller installs a lower local
/// target and reports fresh usage to `release_slots` before releasing a charge.
pub(super) fn allocate(connection: &mut Connection, demand: &Demand, now: u64) -> Result<Grant> {
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    ensure!(
        tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM owners WHERE id=?1 AND state='active')",
            [&demand.owner],
            |row| row.get::<_, bool>(0)
        )?,
        "provider owner is not active in host coordination"
    );
    tx.execute(
        "DELETE FROM provider_owners WHERE NOT EXISTS(SELECT 1 FROM owners WHERE id=owner)",
        [],
    )?;
    tx.execute("DELETE FROM provider_domains WHERE expires<=?1 AND NOT EXISTS(SELECT 1 FROM provider_owners WHERE domain=provider_domains.id)", [i64::try_from(now)?])?;
    let (mut current, revision) = register_participant(&tx, demand)?;
    let previous_slots = current.slots;
    let mut participants = participants(&tx, &demand.domain)?;
    let mut domain = load_domain(&tx, &demand.domain)?;
    if demand.clear_cooldown {
        domain.cooldown = 0;
        domain.blocked = false;
    }
    domain.blocked |= demand.feedback_lost;
    update_windows(&mut domain, &participants, demand, now)?;
    // Excess validated usage must be charged without waiting for another call.
    charge(&mut domain.token, demand.token_debt, now);
    charge(&mut domain.token_hint, demand.token_debt, now);
    let target = if domain.blocked || domain.cooldown > now {
        0
    } else {
        slot_target(&mut participants, &demand.owner, now, domain.cooldown)
    };
    current.slots = previous_slots.max(target);
    if target > 0 && previous_slots == 0 {
        current.served = now;
        current.selected_until = now.saturating_add(QUANTUM_MS);
    }
    let (request_credit, token_credit) =
        allocate_rates(&mut domain, &participants, demand, target, now);
    let request_reserve = domain.request_hint.as_ref().map(|_| {
        unused(&participants, domain.epoch, now, |participant| {
            participant.requests_left
        })
        .saturating_add(request_credit.unwrap_or(0))
    });
    let token_reserve = domain.token_hint.as_ref().map(|_| {
        unused(&participants, domain.epoch, now, |participant| {
            participant.tokens_left
        })
        .saturating_add(token_credit.unwrap_or(0))
    });
    current.epoch = domain.epoch;
    let mut expires = expiry(&domain, now).min(now.saturating_add(86_400_000));
    if demand.epoch == domain.epoch
        && demand.expires > now
        && (demand.requests_left > 0 || demand.tokens_left > 0)
    {
        // A renewal cannot extend the last usable time of previously charged
        // credits. New credits share the older, conservative expiry.
        expires = expires.min(demand.expires);
    }
    if demand.epoch != domain.epoch || demand.expires <= now {
        current.requests_left = 0;
        current.tokens_left = 0;
    }
    current.requests_left = current
        .requests_left
        .saturating_add(request_credit.unwrap_or(0));
    current.tokens_left = current
        .tokens_left
        .saturating_add(token_credit.unwrap_or(0));
    current.expires = expires;
    tx.execute(
        "UPDATE provider_owners SET payload=?2 WHERE owner=?1 AND revision=?3",
        params![demand.owner, serde_json::to_string(&current)?, revision],
    )?;
    store_domain(&tx, &demand.domain, &domain)?;
    tx.commit()?;
    Ok(Grant {
        domain: demand.domain.clone(),
        revision,
        target,
        request_credit,
        token_credit,
        request_reserve,
        token_reserve,
        expires,
        epoch: domain.epoch,
        cooldown: domain.cooldown,
        blocked: domain.blocked,
    })
}

fn unused(
    participants: &[(String, Participant)],
    epoch: u64,
    now: u64,
    credits: impl Fn(&Participant) -> u64,
) -> u64 {
    participants
        .iter()
        .filter(|(_, participant)| participant.epoch == epoch && participant.expires > now)
        .map(|(_, participant)| credits(participant))
        .fold(0, u64::saturating_add)
}

fn register_participant(connection: &Connection, demand: &Demand) -> Result<(Participant, i64)> {
    let old: Option<(String, i64, String)> = connection
        .query_row(
            "SELECT domain,revision,payload FROM provider_owners WHERE owner=?1",
            [&demand.owner],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let old_participant = old
        .as_ref()
        .map(|(_, _, payload)| serde_json::from_str::<Participant>(payload))
        .transpose()?;
    let moving = old
        .as_ref()
        .is_some_and(|(domain, _, _)| domain != &demand.domain);
    ensure!(
        !moving || demand.active == 0,
        "cannot change a provider quota domain while streams remain active"
    );
    let revision = old
        .as_ref()
        .map_or(0, |(_, revision, _)| *revision)
        .checked_add(1)
        .context("provider grant revision exhausted")?;
    let retained_slots = if moving {
        0
    } else {
        old_participant
            .as_ref()
            .map_or(0, |participant| participant.slots)
    };
    let current = Participant {
        policy: demand.policy.clone(),
        hard: demand.hard,
        desired: demand.desired,
        slots: retained_slots,
        served: old_participant
            .as_ref()
            .map_or(0, |participant| participant.served),
        selected_until: old_participant
            .as_ref()
            .map_or(0, |participant| participant.selected_until),
        requests_left: demand.requests_left,
        tokens_left: demand.tokens_left,
        expires: demand.expires,
        epoch: demand.epoch,
        waiting: demand.waiting,
        tokens_needed: demand.tokens_needed,
    };
    connection.execute("INSERT INTO provider_owners VALUES(?1,?2,?3,?4) ON CONFLICT(owner) DO UPDATE SET domain=excluded.domain,revision=excluded.revision,payload=excluded.payload", params![demand.owner, demand.domain, revision, serde_json::to_string(&current)?])?;
    Ok((current, revision))
}

fn participants(connection: &Connection, domain: &str) -> Result<Vec<(String, Participant)>> {
    let mut statement = connection.prepare("SELECT p.owner,p.payload,o.state FROM provider_owners p JOIN owners o ON o.id=p.owner WHERE p.domain=?1")?;
    statement
        .query_map([domain], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .map(|row| {
            let (owner, payload, state) = row?;
            let mut participant: Participant = serde_json::from_str(&payload)?;
            if state != "active" {
                participant.desired = 0;
            }
            Ok((owner, participant))
        })
        .collect()
}

pub(super) fn release_slots(
    connection: &mut Connection,
    owner: &str,
    grant: &Grant,
    active: usize,
) -> Result<()> {
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let payload: String = tx
        .query_row(
            "SELECT payload FROM provider_owners WHERE owner=?1 AND domain=?2 AND revision=?3",
            params![owner, grant.domain, grant.revision],
            |row| row.get(0),
        )
        .context("stale provider grant release")?;
    let mut participant: Participant = serde_json::from_str(&payload)?;
    ensure!(
        active <= participant.slots,
        "provider usage exceeds its retained slot charge"
    );
    participant.slots = active.max(grant.target).min(participant.slots);
    let changed = tx.execute(
        "UPDATE provider_owners SET payload=?2 WHERE owner=?1 AND domain=?3 AND revision=?4",
        params![
            owner,
            serde_json::to_string(&participant)?,
            grant.domain,
            grant.revision
        ],
    )?;
    ensure!(changed == 1, "stale provider grant release");
    tx.commit()?;
    Ok(())
}

fn load_domain(connection: &Connection, domain: &str) -> Result<Domain> {
    let payload: Option<String> = connection
        .query_row(
            "SELECT payload FROM provider_domains WHERE id=?1",
            [domain],
            |row| row.get(0),
        )
        .optional()?;
    payload.map_or_else(
        || Ok(Domain::default()),
        |payload| serde_json::from_str(&payload).map_err(Into::into),
    )
}

fn store_domain(connection: &Connection, id: &str, domain: &Domain) -> Result<()> {
    let exists: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM provider_domains WHERE id=?1)",
        [id],
        |row| row.get(0),
    )?;
    ensure!(
        exists
            || connection.query_row("SELECT COUNT(*) FROM provider_domains", [], |row| row
                .get::<_, i64>(0))?
                < MAX_DOMAINS,
        ContractError("shared provider domain bound reached; unexpired charges must remain intact")
    );
    let expires = if domain.blocked {
        i64::MAX
    } else {
        i64::try_from(expiry_max(domain).max(domain.cooldown))?
    };
    connection.execute("INSERT INTO provider_domains VALUES(?1,?2,?3) ON CONFLICT(id) DO UPDATE SET expires=excluded.expires,payload=excluded.payload", params![id, expires, serde_json::to_string(domain)?])?;
    Ok(())
}

fn slot_target(
    participants: &mut [(String, Participant)],
    owner: &str,
    now: u64,
    cooldown: u64,
) -> usize {
    let configured = participants
        .iter()
        .filter_map(|(_, participant)| participant.policy.max_in_flight)
        .min()
        .unwrap_or_else(|| {
            participants
                .iter()
                .map(|(_, participant)| participant.desired)
                .sum()
        });
    let limit = if cooldown == 0 {
        configured
    } else {
        configured.min(
            usize::try_from(now.saturating_sub(cooldown) / REENTRY_STEP_MS)
                .unwrap_or(usize::MAX)
                .saturating_add(1),
        )
    };
    participants.sort_unstable_by_key(|(id, participant)| {
        (
            participant.slots == 0 || participant.selected_until <= now,
            participant.served,
            id.clone(),
        )
    });
    let count = participants
        .iter()
        .filter(|(_, participant)| participant.desired > 0)
        .count();
    if count == 0 {
        return 0;
    }
    let share = limit / count;
    let mut unassigned = limit % count;
    let mut targets = Vec::with_capacity(participants.len());
    for (_, participant) in participants.iter() {
        if participant.desired == 0 {
            targets.push(0);
            continue;
        }
        let extra = usize::from(unassigned > 0 && participant.desired > 0);
        unassigned -= extra;
        let target = participant.desired.min(share + extra).min(participant.hard);
        unassigned += (share + extra).saturating_sub(target);
        targets.push(target);
    }
    for ((_, participant), target) in participants.iter().zip(&mut targets) {
        let extra = unassigned.min(participant.desired.saturating_sub(*target));
        *target += extra;
        unassigned -= extra;
    }
    let desired = participants
        .iter()
        .zip(targets)
        .find_map(|((id, _), target)| (id == owner).then_some(target))
        .unwrap_or(0);
    let charged: usize = participants
        .iter()
        .filter(|(id, _)| id != owner)
        .map(|(_, participant)| participant.slots)
        .sum();
    desired.min(limit.saturating_sub(charged))
}

fn update_windows(
    domain: &mut Domain,
    participants: &[(String, Participant)],
    demand: &Demand,
    now: u64,
) -> Result<()> {
    for window in [
        &mut domain.request,
        &mut domain.token,
        &mut domain.request_hint,
        &mut domain.token_hint,
    ] {
        if let Some(window) = window.as_mut()
            && window.period.is_some()
        {
            window.charges.retain(|(until, _)| *until > now);
            window.charged = window
                .charges
                .iter()
                .map(|(_, amount)| *amount)
                .fold(0, u64::saturating_add);
        }
        if window.as_ref().is_some_and(|window| window.ends <= now) {
            *window = None;
            domain.epoch = domain
                .epoch
                .checked_add(1)
                .context("provider rate epoch exhausted")?;
        }
    }
    let request = merged_rate(participants, |policy| policy.requests)?;
    let token = merged_rate(participants, |policy| policy.tokens)?;
    install_fixed(&mut domain.request, request, now)?;
    install_fixed(&mut domain.token, token, now)?;
    let requests_left = participants
        .iter()
        .filter(|(_, participant)| participant.expires > now)
        .map(|(_, participant)| participant.requests_left)
        .fold(0_u64, u64::saturating_add);
    let tokens_left = participants
        .iter()
        .filter(|(_, participant)| participant.expires > now)
        .map(|(_, participant)| participant.tokens_left)
        .fold(0_u64, u64::saturating_add);
    for (at, observation) in &demand.feedback {
        match observation {
            ProviderObservation::Limits { requests, tokens } => {
                if let Some(window) = requests {
                    install_hint(&mut domain.request_hint, *window, requests_left, now);
                }
                if let Some(window) = tokens {
                    install_hint(&mut domain.token_hint, *window, tokens_left, now);
                }
                if requests.is_some_and(|window| window.remaining < requests_left)
                    || tokens.is_some_and(|window| window.remaining < tokens_left)
                {
                    domain.epoch = domain
                        .epoch
                        .checked_add(1)
                        .context("provider rate epoch exhausted")?;
                }
            }
            ProviderObservation::Throttled { retry } => match retry {
                ProviderRetry::Delay { millis } => {
                    domain.cooldown = domain.cooldown.max(at.saturating_add(*millis));
                }
                ProviderRetry::BeyondHorizon {
                    retry_at_ms: Some(until),
                } => domain.cooldown = domain.cooldown.max(*until),
                ProviderRetry::BeyondHorizon { retry_at_ms: None } => domain.blocked = true,
                ProviderRetry::Unknown => {
                    domain.cooldown = domain.cooldown.max(at.saturating_add(10_000));
                }
            },
            ProviderObservation::Usage(_) => {}
        }
    }
    Ok(())
}

fn merged_rate(
    participants: &[(String, Participant)],
    rate: impl Fn(&ProviderCapacity) -> Option<ProviderRate>,
) -> Result<Option<ProviderRate>> {
    let mut result: Option<ProviderRate> = None;
    for (_, participant) in participants {
        if let Some(value) = rate(&participant.policy) {
            if let Some(old) = &mut result {
                ensure!(
                    old.window_ms == value.window_ms,
                    ContractError("provider quota domain has incompatible explicit rate windows")
                );
                old.limit = old.limit.min(value.limit);
            } else {
                result = Some(value);
            }
        }
    }
    Ok(result)
}

fn install_fixed(window: &mut Option<Window>, rate: Option<ProviderRate>, now: u64) -> Result<()> {
    if let Some(rate) = rate {
        if let Some(old) = window {
            ensure!(
                old.period == Some(rate.window_ms),
                ContractError("unexpired provider charges use a different explicit rate window")
            );
            old.limit = old.limit.min(rate.limit);
        } else {
            let ends = now
                .checked_add(rate.window_ms)
                .and_then(|until| until.checked_add(CREDIT_TIME_MS))
                .context("provider window exceeds clock range")?;
            *window = Some(Window {
                ends,
                limit: rate.limit,
                charged: 0,
                period: Some(rate.window_ms),
                charges: VecDeque::new(),
            });
        }
    }
    Ok(())
}

fn install_hint(window: &mut Option<Window>, hint: ProviderRateWindow, unused: u64, now: u64) {
    if !valid_hint(hint, now) {
        return;
    }
    let remaining = hint.remaining.saturating_sub(unused);
    if let Some(old) = window {
        old.ends = old.ends.max(hint.reset_at_ms);
        old.limit = old.limit.min(old.charged.saturating_add(remaining));
    } else {
        *window = Some(Window {
            ends: hint.reset_at_ms,
            limit: remaining,
            charged: 0,
            period: None,
            charges: VecDeque::new(),
        });
    }
}

pub(super) fn valid_hint(hint: ProviderRateWindow, now: u64) -> bool {
    hint.limit > 0
        && hint.remaining <= hint.limit
        && hint.reset_at_ms > now
        && hint.reset_at_ms.saturating_sub(now) <= 86_400_000
}

fn allocate_rates(
    domain: &mut Domain,
    participants: &[(String, Participant)],
    demand: &Demand,
    target: usize,
    now: u64,
) -> (Option<u64>, Option<u64>) {
    let requests = available(domain.request.as_ref(), domain.request_hint.as_ref());
    let tokens = available(domain.token.as_ref(), domain.token_hint.as_ref());
    if domain.blocked || domain.cooldown > now || target == 0 || demand.waiting == 0 {
        return (requests.map(|_| 0), tokens.map(|_| 0));
    }
    let matching = demand.epoch == domain.epoch && demand.expires > now;
    let requests_left = if matching { demand.requests_left } else { 0 };
    let tokens_left = if matching { demand.tokens_left } else { 0 };
    let request_peers = u64::try_from(
        participants
            .iter()
            .filter(|(_, participant)| {
                let remaining = if participant.epoch == domain.epoch && participant.expires > now {
                    participant.requests_left
                } else {
                    0
                };
                u64::try_from(participant.waiting).unwrap_or(u64::MAX) > remaining
            })
            .count(),
    )
    .unwrap_or(u64::MAX)
    .max(1);
    let token_peers = u64::try_from(
        participants
            .iter()
            .filter(|(_, participant)| {
                let remaining = if participant.epoch == domain.epoch && participant.expires > now {
                    participant.tokens_left
                } else {
                    0
                };
                participant.tokens_needed > remaining
            })
            .count(),
    )
    .unwrap_or(u64::MAX)
    .max(1);
    let block = u64::try_from(demand.waiting)
        .unwrap_or(MAX_BLOCK)
        .max(prepay(
            domain.request.as_ref(),
            domain.request_hint.as_ref(),
            now,
        ))
        .min(MAX_BLOCK);
    let request_credit = requests.map(|available| {
        block
            .saturating_sub(requests_left)
            .min(available.div_ceil(request_peers))
    });
    let token_block = demand.tokens_needed.max(
        prepay(domain.token.as_ref(), domain.token_hint.as_ref(), now)
            .min(demand.largest_request.saturating_mul(MAX_BLOCK)),
    );
    let token_credit = tokens.map(|available| {
        token_block.saturating_sub(tokens_left).min(
            available
                .div_ceil(token_peers)
                .max(demand.largest_request)
                .min(available),
        )
    });
    charge(&mut domain.request, request_credit.unwrap_or(0), now);
    charge(&mut domain.request_hint, request_credit.unwrap_or(0), now);
    charge(&mut domain.token, token_credit.unwrap_or(0), now);
    charge(&mut domain.token_hint, token_credit.unwrap_or(0), now);
    (request_credit, token_credit)
}

fn prepay(one: Option<&Window>, two: Option<&Window>, now: u64) -> u64 {
    [one, two]
        .into_iter()
        .flatten()
        .map(|window| {
            let (limit, period) = window.period.map_or_else(
                || {
                    (
                        window.limit.saturating_sub(window.charged),
                        window.ends.saturating_sub(now),
                    )
                },
                |period| (window.limit, period),
            );
            limit.saturating_mul(CREDIT_TIME_MS).div_ceil(period.max(1))
        })
        .min()
        .unwrap_or(0)
}

fn charge(window: &mut Option<Window>, amount: u64, now: u64) {
    if let Some(window) = window
        && amount > 0
    {
        window.charged = window.charged.saturating_add(amount);
        if let Some(period) = window.period {
            let width = period.div_ceil(32).max(1);
            let expires = now
                .saturating_add(CREDIT_TIME_MS)
                .div_ceil(width)
                .saturating_mul(width)
                .saturating_add(period);
            if let Some((_, charged)) = window
                .charges
                .iter_mut()
                .find(|(until, _)| *until == expires)
            {
                *charged = charged.saturating_add(amount);
            } else if window.charges.len() < 64 {
                window.charges.push_back((expires, amount));
            } else if let Some((until, charged)) = window.charges.back_mut() {
                *until = (*until).max(expires);
                *charged = charged.saturating_add(amount);
            }
            window.ends = window.ends.max(expires);
        }
    }
}

fn available(one: Option<&Window>, two: Option<&Window>) -> Option<u64> {
    [one, two]
        .into_iter()
        .flatten()
        .map(|window| window.limit.saturating_sub(window.charged))
        .min()
}

fn expiry(domain: &Domain, now: u64) -> u64 {
    [
        &domain.request,
        &domain.token,
        &domain.request_hint,
        &domain.token_hint,
    ]
    .into_iter()
    .flatten()
    .map(|window| {
        if window.period.is_some() {
            now.saturating_add(CREDIT_TIME_MS)
        } else {
            window.ends
        }
    })
    .min()
    .unwrap_or(u64::MAX)
}

fn expiry_max(domain: &Domain) -> u64 {
    [
        &domain.request,
        &domain.token,
        &domain.request_hint,
        &domain.token_hint,
    ]
    .into_iter()
    .flatten()
    .map(|window| window.ends)
    .max()
    .unwrap_or(0)
}

#[cfg(test)]
#[path = "provider_registry_tests.rs"]
mod tests;
