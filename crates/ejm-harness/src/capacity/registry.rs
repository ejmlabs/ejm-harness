//! Coarse, charged-before-install grants. Session journals stay independent.

mod bootstrap;
mod helpers;
mod witness;

#[cfg(test)]
mod faults;

use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use anyhow::{Context, Result, ensure};
use ejm_harness_exec::{
    Governor, GovernorSnapshot, HostObservations, HostSampler, ObservationAvailability,
    OperatingTarget, ResourceLimits, process_identity,
};
use rusqlite::{Connection, OpenFlags, params};
use serde::{Deserialize, Serialize};

use crate::types::timestamp_ms;

const PROTOCOL: u32 = 4;
const MAX_PARTICIPANTS: usize = 256;
const MAX_DOMAINS: usize = 64;
const MAX_RETAINED_DOMAINS: usize = MAX_DOMAINS * 2;
const MAX_DOMAIN_RECORDS: usize = 4096;
const CONTROL_WINDOW_MS: i64 = 500;
const CONTACT_TIME_MS: i64 = 8_000;
const BASELINE_MEMORY: u64 = 32 * 1024 * 1024;

// File creation may briefly open a database outside SQLite. Finish that close
// before another thread opens SQLite on the same inode and takes POSIX locks.
#[cfg(unix)]
static REGISTRY_OPEN: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct Budget {
    pub jobs: usize,
    heavy: usize,
    cpu: u32,
    memory: u64,
    processes: u32,
    output: usize,
    pub models: usize,
}

pub(super) struct Renewal {
    pub(super) granted: Budget,
    pub(super) planned: Budget,
    pub(super) plan_epoch: i64,
    pub(super) plan_refreshed: bool,
}

impl Budget {
    pub fn requested(hard: &ResourceLimits, models: usize, jobs: usize) -> Self {
        Self {
            jobs,
            heavy: hard.max_heavy_jobs.min(jobs),
            cpu: hard.cpu_millis,
            memory: hard.memory_bytes,
            processes: hard.max_processes,
            output: hard.max_output_bytes,
            models,
        }
    }

    fn operating(self) -> OperatingTarget {
        OperatingTarget {
            max_jobs: self.jobs,
            max_heavy_jobs: self.heavy,
            cpu_millis: self.cpu,
            memory_bytes: self.memory,
            max_processes: self.processes,
            max_output_bytes: self.output,
        }
    }

    pub fn target(self, hard: &ResourceLimits) -> OperatingTarget {
        OperatingTarget {
            max_jobs: self.jobs.min(hard.max_jobs),
            max_heavy_jobs: self.heavy.min(hard.max_heavy_jobs).min(self.jobs),
            cpu_millis: self.cpu.min(hard.cpu_millis),
            memory_bytes: self.memory.min(hard.memory_bytes),
            max_processes: self.processes.min(hard.max_processes),
            max_output_bytes: self.output.min(hard.max_output_bytes),
        }
    }

    fn demand(target: OperatingTarget) -> Self {
        Self {
            jobs: target.max_jobs,
            heavy: target.max_heavy_jobs,
            cpu: target.cpu_millis,
            memory: target.memory_bytes,
            processes: target.max_processes,
            output: target.max_output_bytes,
            models: 0,
        }
    }

    fn usage(used: &GovernorSnapshot, models: usize) -> Self {
        Self {
            jobs: used.running,
            heavy: used.heavy_running,
            cpu: used.reserved_cpu_millis,
            memory: used.reserved_memory_bytes,
            processes: used.reserved_processes,
            output: used.reserved_output_bytes,
            models,
        }
    }

    fn min(self, other: Self) -> Self {
        let jobs = self.jobs.min(other.jobs);
        Self {
            jobs,
            heavy: self.heavy.min(other.heavy).min(jobs),
            cpu: self.cpu.min(other.cpu),
            memory: self.memory.min(other.memory),
            processes: self.processes.min(other.processes),
            output: self.output.min(other.output),
            models: self.models.min(other.models),
        }
    }

    fn max(self, other: Self) -> Self {
        Self {
            jobs: self.jobs.max(other.jobs),
            heavy: self.heavy.max(other.heavy),
            cpu: self.cpu.max(other.cpu),
            memory: self.memory.max(other.memory),
            processes: self.processes.max(other.processes),
            output: self.output.max(other.output),
            models: self.models.max(other.models),
        }
    }

    fn add(self, other: Self) -> Self {
        Self {
            jobs: self.jobs.saturating_add(other.jobs),
            heavy: self.heavy.saturating_add(other.heavy),
            cpu: self.cpu.saturating_add(other.cpu),
            memory: self.memory.saturating_add(other.memory),
            processes: self.processes.saturating_add(other.processes),
            output: self.output.saturating_add(other.output),
            models: self.models.saturating_add(other.models),
        }
    }

    fn sub(self, other: Self) -> Self {
        Self {
            jobs: self.jobs.saturating_sub(other.jobs),
            heavy: self.heavy.saturating_sub(other.heavy),
            cpu: self.cpu.saturating_sub(other.cpu),
            memory: self.memory.saturating_sub(other.memory),
            processes: self.processes.saturating_sub(other.processes),
            output: self.output.saturating_sub(other.output),
            models: self.models.saturating_sub(other.models),
        }
    }

    fn fits(self, used: &GovernorSnapshot, models: usize) -> bool {
        Self::usage(used, models).min(self) == Self::usage(used, models)
    }

    fn native(self) -> Self {
        Self { models: 0, ..self }
    }
}

pub(super) struct Registry {
    pub(super) connection: Connection,
    files: PrivateFiles,
    owner: Option<String>,
    witness: Option<witness::OwnerWitness>,
    domains: Vec<String>,
    fenced: bool,
    reason: Option<String>,
    recovered: bool,
    pub(super) last_flush: u64,
}

impl Registry {
    pub fn open(root: &Path) -> Result<Self> {
        #[cfg(unix)]
        let _opening = REGISTRY_OPEN
            .lock()
            .map_err(|_| anyhow::anyhow!("capacity registry bootstrap lock is poisoned"))?;
        let files = PrivateFiles::open(root)?;
        let mut connection = Connection::open_with_flags(
            &files.database,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX
                | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )?;
        connection.busy_timeout(bootstrap::BUSY_BUDGET)?;
        files.verify()?;
        let version: u32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if version != PROTOCOL {
            // Version validation and initial schema creation hold the same lock.
            // A racing older client cannot replace a newly initialized protocol.
            let tx = connection
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .context("begin shared capacity schema validation")?;
            let version: u32 = tx.pragma_query_value(None, "user_version", |row| row.get(0))?;
            if matches!(version, 1..=3) {
                // Recover provably released older owners before requiring a drained
                // migration. Live or unknown owners roll this whole transaction back.
                witness::create_schema(&tx)?;
                helpers::create_schema(&tx)?;
                recover(&tx, &files, sql_time()?)?;
            }
            let drained_previous = matches!(version, 1..=3)
                && count(&tx, "SELECT COUNT(*) FROM owners")? == 0
                && count(&tx, "SELECT COUNT(*) FROM allocations")? == 0
                && table_empty_if_present(&tx, "owner_witnesses")?
                && helpers::empty_if_present(&tx)?;
            ensure!(
                version == 0 || version == PROTOCOL || drained_previous,
                "shared capacity registry protocol is incompatible; drain participants before migration"
            );
            if drained_previous {
                // A preopened older connection must also fail registration: it
                // cannot supply the required, nondefault helper protocol column.
                tx.execute("DROP TABLE owners", [])?;
                tx.execute("DROP TABLE native_helpers", [])?;
            }
            tx.execute_batch(
                "CREATE TABLE IF NOT EXISTS owners(
                  id TEXT PRIMARY KEY CHECK(length(id) BETWEEN 1 AND 128),
                  boot TEXT NOT NULL, pid INTEGER NOT NULL CHECK(pid>=0),
                  birth INTEGER NOT NULL CHECK(birth>=0), heartbeat INTEGER NOT NULL CHECK(heartbeat>=0),
                  revision INTEGER NOT NULL CHECK(revision>=0),
                  requested TEXT NOT NULL CHECK(length(requested)<=1024),
                  grant_json TEXT NOT NULL CHECK(length(grant_json)<=1024),
                  usage_json TEXT NOT NULL CHECK(length(usage_json)<=1024),
                  working_demand TEXT NOT NULL CHECK(length(working_demand)<=1024),
                  resident_floor TEXT NOT NULL CHECK(length(resident_floor)<=1024),
                  admission_activity INTEGER NOT NULL CHECK(admission_activity>=0),
                  native_domain TEXT NOT NULL CHECK(length(native_domain) BETWEEN 1 AND 128),
                  domain_ids TEXT NOT NULL CHECK(length(domain_ids)<=32768),
                  native_demand INTEGER NOT NULL CHECK(native_demand IN(0,1)),
                  queued_native INTEGER NOT NULL CHECK(queued_native IN(0,1)),
                  model_demand INTEGER NOT NULL CHECK(model_demand IN(0,1)),
                  warm_until INTEGER NOT NULL CHECK(warm_until>=0),
                  last_native INTEGER NOT NULL CHECK(last_native>=0),
                  last_model INTEGER NOT NULL CHECK(last_model>=0),
                  state TEXT NOT NULL CHECK(state IN('active','suspect','quarantined')),
                  unsafe_scope INTEGER NOT NULL CHECK(unsafe_scope IN(0,1)),
                  plan_json TEXT NOT NULL CHECK(length(plan_json)<=1024),
                  helper_protocol INTEGER NOT NULL CHECK(helper_protocol=4));
                 CREATE TABLE IF NOT EXISTS domains(id TEXT PRIMARY KEY, parent TEXT, budget TEXT NOT NULL);
                 CREATE TABLE IF NOT EXISTS allocations(
                  owner TEXT NOT NULL, domain TEXT NOT NULL, budget TEXT NOT NULL,
                  PRIMARY KEY(owner,domain));
                 CREATE INDEX IF NOT EXISTS allocations_domain ON allocations(domain,owner);
                 CREATE TABLE IF NOT EXISTS control(
                  id INTEGER PRIMARY KEY CHECK(id=1), at INTEGER NOT NULL, epoch INTEGER NOT NULL);
                 INSERT OR IGNORE INTO control VALUES(1,0,0);
                 CREATE TABLE IF NOT EXISTS profiles(
                  id TEXT PRIMARY KEY, updated INTEGER NOT NULL, samples INTEGER NOT NULL,
                  model_target INTEGER NOT NULL, job_target INTEGER NOT NULL);",
            )?;
            witness::create_schema(&tx)?;
            helpers::create_schema(&tx)?;
            tx.pragma_update(None, "user_version", PROTOCOL)?;
            tx.commit()
                .context("commit shared capacity schema validation")?;
        }
        bootstrap::enable_wal(&connection)?;
        connection.pragma_update(None, "synchronous", "FULL")?;
        files.verify()?;
        Ok(Self {
            connection,
            files,
            owner: None,
            witness: None,
            domains: Vec::new(),
            fenced: false,
            reason: None,
            recovered: false,
            last_flush: 0,
        })
    }

    pub fn reason(&self) -> Option<String> {
        self.reason.clone()
    }

    pub fn lifetime(&self) -> Result<Arc<File>> {
        self.witness
            .as_ref()
            .map(witness::OwnerWitness::retained)
            .context("capacity owner has no lifetime witness")
    }

    pub fn admit_native_helper(
        &mut self,
        owner: &str,
        request: &ejm_harness_exec::native::NativeOwnerRequest,
        observations: &HostObservations,
    ) -> Result<bool> {
        helpers::admit(self, owner, request, observations)
    }

    pub fn native_helpers(&self) -> Result<super::NativeHelperSnapshot> {
        helpers::snapshot(&self.connection)
    }

    #[cfg(test)]
    pub fn register(
        &mut self,
        owner: &str,
        requested: Budget,
        observations: &HostObservations,
    ) -> Result<Budget> {
        self.register_with_floor(owner, requested, OperatingTarget::default(), observations)
    }

    pub fn register_with_floor(
        &mut self,
        owner: &str,
        requested: Budget,
        floor: OperatingTarget,
        observations: &HostObservations,
    ) -> Result<Budget> {
        ensure!(
            self.owner.is_none(),
            "capacity registry already has a local owner"
        );
        ensure!(
            !owner.is_empty() && owner.len() <= 128,
            "invalid capacity owner nonce"
        );
        let domains = observed_domains(observations)?;
        self.files.verify()?;
        let identity = HostSampler::owner_identity();
        let boot = identity
            .as_ref()
            .map_or("unavailable", |identity| identity.boot_id.as_str());
        let birth = identity.as_ref().map_or(0, |identity| identity.start_time);
        let now = sql_time()?;
        let tx = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        self.recovered |= recover(&tx, &self.files, now)? > 0;
        ensure!(
            count(&tx, "SELECT COUNT(*) FROM owners")? < MAX_PARTICIPANTS,
            "shared capacity participant bound reached; existing owners must drain"
        );
        let witness = witness::OwnerWitness::acquire(&tx, &self.files)?;
        install_domains(&tx, &domains)?;
        let ids: Vec<_> = domains.iter().map(|domain| domain.id.clone()).collect();
        let zero = serde_json::to_string(&Budget::default())?;
        tx.execute(
            "INSERT INTO owners(id,boot,pid,birth,heartbeat,revision,requested,grant_json,usage_json,working_demand,
             resident_floor,admission_activity,native_domain,domain_ids,native_demand,queued_native,model_demand,warm_until,last_native,last_model,state,unsafe_scope,plan_json,helper_protocol)
             VALUES(?1,?2,?3,?4,?5,0,?6,?7,?7,?6,?11,0,?10,?8,1,0,1,?9,0,0,'active',0,?7,4)",
            params![owner, boot, std::process::id(), i64::try_from(birth)?, now,
                serde_json::to_string(&requested)?, zero, serde_json::to_string(&ids)?,
                now.checked_add(CONTACT_TIME_MS).context("capacity warm window overflow")?,
                super::execution_domain(observations),
                serde_json::to_string(&Budget::demand(floor))?],
        )?;
        witness.record(&tx, owner, &self.files)?;
        for domain in &ids {
            tx.execute(
                "INSERT INTO allocations VALUES(?1,?2,?3)",
                params![owner, domain, zero],
            )?;
        }
        // Registration is a bounded demand change. Refresh the shared plan once
        // so the new process can acquire its baseline before exposing work.
        tx.execute("UPDATE control SET at=0 WHERE id=1", [])?;
        tx.commit()?;
        self.owner = Some(owner.into());
        self.witness = Some(witness);
        self.domains = ids;
        let result = self
            .renew_with_floor(
                owner,
                requested,
                0,
                0,
                &GovernorSnapshot::default(),
                requested.operating(),
                floor,
                0,
                observations,
                || Ok(()),
                |_| Ok((GovernorSnapshot::default(), 0)),
            )
            .map(|renewal| renewal.granted);
        if result.is_err() {
            // No grant has been installed or exposed by register's private no-op
            // callback. This is a direct absence witness, unlike owner death.
            let cleanup = self.remove_uninstalled();
            if let Err(error) = cleanup {
                return result.context(format!("uninstalled owner cleanup also failed: {error}"));
            }
        }
        result
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "One serialized grant transition carries its usage witnesses and local callbacks."
    )]
    #[cfg(test)]
    pub fn renew(
        &mut self,
        owner: &str,
        requested: Budget,
        models: usize,
        waiting_models: usize,
        used: &GovernorSnapshot,
        demand: OperatingTarget,
        observations: &HostObservations,
        fence: impl FnOnce() -> Result<()>,
        install: impl FnOnce(Budget) -> Result<(GovernorSnapshot, usize)>,
    ) -> Result<Budget> {
        self.renew_with_floor(
            owner,
            requested,
            models,
            waiting_models,
            used,
            demand,
            OperatingTarget {
                memory_bytes: used.reserved_memory_bytes,
                max_processes: used.reserved_processes,
                ..OperatingTarget::default()
            },
            0,
            observations,
            fence,
            install,
        )
        .map(|renewal| renewal.granted)
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "One serialized grant transition carries its usage witnesses and local callbacks."
    )]
    pub fn renew_with_floor(
        &mut self,
        owner: &str,
        requested: Budget,
        models: usize,
        waiting_models: usize,
        used: &GovernorSnapshot,
        demand: OperatingTarget,
        floor: OperatingTarget,
        activity: u64,
        observations: &HostObservations,
        fence: impl FnOnce() -> Result<()>,
        install: impl FnOnce(Budget) -> Result<(GovernorSnapshot, usize)>,
    ) -> Result<Renewal> {
        ensure!(
            self.owner.as_deref() == Some(owner),
            "capacity owner nonce mismatch"
        );
        self.witness
            .as_ref()
            .context("capacity owner lifetime witness missing")?
            .verify(&self.files)?;
        let domains = self.observe_and_fence(owner, observations, fence)?;
        let ids: Vec<_> = domains.iter().map(|domain| domain.id.clone()).collect();
        let now = sql_time()?;
        let tx = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        self.recovered |= recover(&tx, &self.files, now)? > 0;
        let (revision, encoded, prior_activity): (i64, String, i64) = tx.query_row(
            "SELECT revision,grant_json,admission_activity FROM owners WHERE id=?1",
            [owner],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        ensure!(revision >= 0, "invalid shared grant revision");
        let activity = i64::try_from(activity).context("native service counter exhausted")?;
        ensure!(
            activity >= prior_activity,
            "native service counter moved backwards"
        );
        let old: Budget = serde_json::from_str(&encoded)?;
        let retained = allocation_domains(&tx, owner)?;
        let union: BTreeSet<_> = retained.iter().chain(&ids).collect();
        if union.len() > MAX_RETAINED_DOMAINS {
            tx.execute(
                "UPDATE owners SET unsafe_scope=1,state='quarantined' WHERE id=?1",
                [owner],
            )?;
            tx.commit()?;
            anyhow::bail!(
                "resource-domain transition exceeds its retained bound; drain this owner before retrying"
            );
        }
        install_domains(&tx, &domains)?;
        validate_local_identity(&tx, owner)?;
        // A moved owner's old charge follows it into every new ancestor, even
        // when an external move itself creates debt. Admission is already fenced.
        for domain in &ids {
            tx.execute(
                "INSERT OR IGNORE INTO allocations VALUES(?1,?2,?3)",
                params![owner, domain, serde_json::to_string(&old)?],
            )?;
        }
        tx.execute(
            "UPDATE owners SET heartbeat=?2,requested=?3,usage_json=?4,domain_ids=?5,
             native_demand=?6,model_demand=?7,working_demand=?8,resident_floor=?9,queued_native=?10,
             admission_activity=?11,native_domain=?12,state='active',unsafe_scope=0 WHERE id=?1",
            params![
                owner,
                now,
                serde_json::to_string(&requested)?,
                serde_json::to_string(&Budget::usage(used, models))?,
                serde_json::to_string(&ids)?,
                i64::from(used.running > 0 || used.queued > 0),
                i64::from(models > 0 || waiting_models > 0),
                serde_json::to_string(&Budget::demand(demand))?,
                serde_json::to_string(&Budget::demand(floor))?,
                i64::from(used.queued > 0),
                activity,
                super::execution_domain(observations)
            ],
        )?;
        let (epoch, plan_refreshed) = refresh_plans(&tx, now)?;
        let (candidate, planned, full_service) =
            grant_targets(&tx, owner, requested, Budget::demand(demand), &ids)?;
        let new_revision = charge_increase(&tx, owner, revision, old, candidate, &ids)?;
        #[cfg(test)]
        faults::hit(faults::Point::ChargeBeforeCommit)?;
        tx.commit()?;
        #[cfg(test)]
        faults::hit(faults::Point::ChargeCommitted)?;
        self.domains.clone_from(&ids);
        self.files.verify()?;
        // The recipient's increase is committed before the local target grows.
        // Any failure below strands charges conservatively until reconciliation.
        let grant = CommittedGrant {
            budget: candidate,
            revision: new_revision,
            epoch,
            full_service,
            native_activity: activity > prior_activity,
            retained,
            domains: ids,
        };
        let (fresh_used, fresh_models) = install(candidate)?;
        #[cfg(test)]
        faults::hit(faults::Point::TargetInstalled)?;
        self.fenced = false;
        self.complete_release(owner, &grant, &fresh_used, fresh_models)?;
        self.files.verify()?;
        self.refresh_reason()?;
        Ok(Renewal {
            granted: candidate,
            planned,
            plan_epoch: epoch,
            plan_refreshed,
        })
    }

    fn refresh_reason(&mut self) -> Result<()> {
        self.reason = shared_reason(&self.connection)?.or_else(|| {
            self.recovered.then(|| {
                "owner allowance retired after lifetime release; native cleanup remains unverified".into()
            })
        });
        Ok(())
    }

    fn observe_and_fence(
        &mut self,
        owner: &str,
        observations: &HostObservations,
        fence: impl FnOnce() -> Result<()>,
    ) -> Result<Vec<ObservedDomain>> {
        let observed = observed_domains(observations);
        let ids = observed.as_ref().ok().map(|domains| {
            domains
                .iter()
                .map(|domain| domain.id.clone())
                .collect::<Vec<_>>()
        });
        let moved = ids.as_ref().is_none_or(|ids| ids != &self.domains) || self.fenced;
        if moved {
            // Never enter SQLite while holding local admission locks.
            fence()?;
            self.fenced = true;
        }
        self.files.verify()?;
        if observed.is_err() {
            self.connection
                .execute("UPDATE owners SET unsafe_scope=1 WHERE id=?1", [owner])?;
        }
        observed
    }

    fn complete_release(
        &mut self,
        owner: &str,
        grant: &CommittedGrant,
        fresh_used: &GovernorSnapshot,
        fresh_models: usize,
    ) -> Result<()> {
        if grant.budget.fits(fresh_used, fresh_models) {
            let tx = self
                .connection
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            let changed = tx.execute(
                "UPDATE owners SET grant_json=?2,
                 last_native=CASE WHEN ?4 THEN ?6 ELSE last_native END,
                 last_model=CASE WHEN ?5 THEN ?6 ELSE last_model END WHERE id=?1 AND revision=?3",
                params![
                    owner,
                    serde_json::to_string(&grant.budget)?,
                    grant.revision,
                    grant.native_activity
                        || (fresh_used.queued == 0
                            && grant.budget.jobs > 0
                            && grant.full_service.min(grant.budget.native()) == grant.full_service),
                    grant.budget.models > 0,
                    grant.epoch
                ],
            )?;
            ensure!(changed == 1, "stale shared capacity release");
            for domain in &grant.domains {
                tx.execute(
                    "UPDATE allocations SET budget=?3 WHERE owner=?1 AND domain=?2",
                    params![owner, domain, serde_json::to_string(&grant.budget)?],
                )?;
            }
            // Moving the root does not prove its already-spawned descendants
            // moved too. Keep old domains until all native owners have drained.
            if fresh_used.running == 0 && fresh_used.reserved_processes == 0 {
                for domain in grant
                    .retained
                    .iter()
                    .filter(|domain| !grant.domains.contains(domain))
                {
                    tx.execute(
                        "DELETE FROM allocations WHERE owner=?1 AND domain=?2",
                        params![owner, domain],
                    )?;
                }
                prune_domains(&tx)?;
            }
            #[cfg(test)]
            faults::hit(faults::Point::ReleaseBeforeCommit)?;
            tx.commit()?;
            #[cfg(test)]
            faults::hit(faults::Point::ReleaseCommitted)?;
        }
        Ok(())
    }

    pub fn release_departed(&mut self, governor: &Arc<Governor>) -> Result<()> {
        if self.owner.is_none() {
            return Ok(());
        }
        let generation = governor
            .operating_snapshot()
            .generation
            .checked_add(1)
            .context("local admission generation exhausted")?;
        // Retained Governor handles cannot spend the refunded grant after exit.
        governor.install_target(generation, OperatingTarget::default(), None)?;
        let used = governor.snapshot();
        ensure!(
            used.running == 0
                && used.queued == 0
                && governor.operating_snapshot().memory_leases == 0,
            "shared capacity owner must drain before release"
        );
        self.files.verify()?;
        self.remove_uninstalled()
    }

    fn remove_uninstalled(&mut self) -> Result<()> {
        if let Some(owner) = &self.owner {
            let tx = self
                .connection
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            tx.execute("DELETE FROM allocations WHERE owner=?1", [owner])?;
            tx.execute("DELETE FROM owners WHERE id=?1", [owner])?;
            tx.execute("DELETE FROM owner_witnesses WHERE owner=?1", [owner])?;
            prune_domains(&tx)?;
            tx.commit()?;
            #[cfg(test)]
            faults::hit(faults::Point::RemovalCommitted)?;
            self.owner = None;
            self.witness = None;
        }
        self.files.verify()?;
        Ok(())
    }
}

fn charge_increase(
    connection: &Connection,
    owner: &str,
    revision: i64,
    old: Budget,
    candidate: Budget,
    domains: &[String],
) -> Result<i64> {
    let charged = serde_json::to_string(&old.max(candidate))?;
    let next_revision = revision
        .checked_add(1)
        .context("shared grant revision exhausted")?;
    let updated = connection.execute(
        "UPDATE owners SET revision=?2,grant_json=?3 WHERE id=?1 AND revision=?4",
        params![owner, next_revision, charged, revision],
    )?;
    ensure!(updated == 1, "stale shared capacity increase");
    for domain in domains {
        connection.execute(
            "UPDATE allocations SET budget=?3 WHERE owner=?1 AND domain=?2",
            params![owner, domain, charged],
        )?;
    }
    Ok(next_revision)
}

fn grant_targets(
    connection: &Connection,
    owner: &str,
    requested: Budget,
    demand: Budget,
    domains: &[String],
) -> Result<(Budget, Budget, Budget)> {
    let plan: String =
        connection.query_row("SELECT plan_json FROM owners WHERE id=?1", [owner], |row| {
            row.get(0)
        })?;
    let planned = serde_json::from_str(&plan)?;
    let mut candidate = requested.min(planned);
    let mut full_service = requested.min(demand).native();
    for domain in domains {
        let envelope = domain_budget(connection, domain)?;
        full_service = full_service.min(envelope.native());
        let others = charged_others(connection, domain, owner)?;
        candidate = candidate.min(envelope.sub(others));
    }
    Ok((candidate, planned, full_service))
}

struct CommittedGrant {
    budget: Budget,
    revision: i64,
    epoch: i64,
    full_service: Budget,
    native_activity: bool,
    retained: Vec<String>,
    domains: Vec<String>,
}

struct ObservedDomain {
    id: String,
    parent: Option<String>,
    budget: Budget,
}

fn observed_domains(observations: &HostObservations) -> Result<Vec<ObservedDomain>> {
    ensure!(
        observations.cpu_millis.is_fresh()
            && observations.memory_bytes.is_fresh()
            && observations.sampled_at.elapsed() <= Duration::from_secs(5),
        "resource observations are stale or unavailable"
    );
    ensure!(
        matches!(
            observations.domain_visibility,
            ObservationAvailability::Measured | ObservationAvailability::Estimated
        ),
        "resource ancestry is unavailable or unsupported"
    );
    ensure!(
        !observations.domains.is_empty() && observations.domains.len() <= MAX_DOMAINS,
        "resource ancestry is unavailable or exceeds its bound"
    );
    let mut parents = BTreeMap::new();
    for domain in &observations.domains {
        ensure!(
            !domain.id.is_empty() && domain.id.len() <= 256,
            "invalid resource-domain identity"
        );
        ensure!(
            parents
                .insert(domain.id.as_str(), domain.parent.as_deref())
                .is_none(),
            "duplicate resource-domain identity"
        );
    }
    ensure!(
        parents.get("host") == Some(&None),
        "resource ancestry requires the common host root"
    );
    for id in parents.keys() {
        let mut current = *id;
        let mut visited = BTreeSet::new();
        loop {
            ensure!(visited.insert(current), "cyclic resource ancestry");
            if let Some(parent) = parents
                .get(current)
                .context("incomplete resource ancestry")?
            {
                current = parent;
            } else {
                ensure!(
                    current == "host",
                    "independent resource roots are unsupported"
                );
                break;
            }
        }
    }
    observations
        .domains
        .iter()
        .map(|domain| {
            let raw_cpu = domain
                .cpu_millis
                .or(observations.cpu_millis.value)
                .context("domain CPU capacity unavailable")?;
            let raw_memory = domain
                .memory_bytes
                .or(observations.memory_bytes.value)
                .context("domain memory capacity unavailable")?;
            let (cpu, memory) = if domain.id == "host" {
                (
                    raw_cpu.saturating_sub(1_000).max(raw_cpu / 2),
                    raw_memory.saturating_sub((raw_memory / 8).max(256 * 1024 * 1024)),
                )
            } else {
                (raw_cpu, raw_memory)
            };
            Ok(ObservedDomain {
                id: domain.id.clone(),
                parent: domain.parent.clone(),
                budget: Budget {
                    jobs: usize::try_from(cpu.div_ceil(125))?.min(4096),
                    heavy: usize::try_from(cpu.div_ceil(1_000))?.min(256),
                    cpu,
                    memory,
                    processes: u32::try_from(domain.max_processes.unwrap_or(4096).min(4096))?,
                    output: usize::try_from((memory / 32).min(64 * 1024 * 1024))?,
                    models: 256,
                },
            })
        })
        .collect()
}

fn install_domains(connection: &Connection, domains: &[ObservedDomain]) -> Result<()> {
    let mut records = count(connection, "SELECT COUNT(*) FROM domains")?;
    for domain in domains {
        let exists: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM domains WHERE id=?1)",
            [&domain.id],
            |row| row.get(0),
        )?;
        if !exists {
            records += 1;
            ensure!(
                records <= MAX_DOMAIN_RECORDS,
                "shared resource-domain metadata bound reached; drain old transitions"
            );
        }
        connection.execute(
            "INSERT INTO domains VALUES(?1,?2,?3) ON CONFLICT(id) DO UPDATE SET parent=excluded.parent,budget=excluded.budget",
            params![domain.id, domain.parent, serde_json::to_string(&domain.budget)?],
        )?;
    }
    Ok(())
}

struct PlanOwner {
    id: String,
    requested: Budget,
    floor: Budget,
    demand: PlanDemand,
    native_domain: String,
    domains: Vec<String>,
    last_native: i64,
    last_model: i64,
    state: String,
    unsafe_scope: bool,
}

struct PlanDemand {
    working: Budget,
    native: bool,
    queued: bool,
    model: bool,
}

impl PlanOwner {
    fn baseline(&self) -> Budget {
        self.floor
            .max(Budget {
                memory: BASELINE_MEMORY,
                ..Budget::default()
            })
            .min(self.requested)
            .native()
    }

    fn minimum(&self, connection: &Connection) -> Result<Budget> {
        let mut wanted = self.requested.min(self.demand.working).native();
        if !self.demand.queued {
            // Warm-up advertises a hard ceiling before any real queue exists.
            for domain in &self.domains {
                wanted = wanted.min(domain_budget(connection, domain)?.native());
            }
        }
        Ok(wanted.max(self.baseline()))
    }
}

fn refresh_plans(connection: &Connection, now: i64) -> Result<(i64, bool)> {
    let (at, epoch): (i64, i64) =
        connection.query_row("SELECT at,epoch FROM control WHERE id=1", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?;
    ensure!(epoch >= 0, "invalid shared control generation");
    if now >= at && now.saturating_sub(at) < CONTROL_WINDOW_MS {
        return Ok((epoch, false));
    }
    let epoch = epoch
        .checked_add(1)
        .context("shared control generation exhausted")?;
    let owners = read_owners(connection, now)?;
    let mut remaining = read_domains(connection)?;
    // Only actual helper commitments are debited up front. Pending helpers
    // compete with useful staged turns in the same fair selection below.
    let pending_helpers = helpers::prepare_plans(connection, &mut remaining)?;
    let mut targets = vec![Budget::default(); owners.len()];
    if !owners.iter().any(|owner| owner.unsafe_scope) {
        for (index, owner) in owners
            .iter()
            .enumerate()
            .filter(|(_, owner)| owner.state == "active")
        {
            let base = owner.baseline();
            targets[index] = base;
            debit(&mut remaining, &owner.domains, base);
        }
        // Retained ancestors and suspect owners remain fully charged. Targets
        // are only proposals; committed charges also constrain each recipient.
        let mut statement = connection.prepare("SELECT owner,domain,budget FROM allocations")?;
        for entry in statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })? {
            let (id, domain, encoded) = entry?;
            let owner = owners
                .iter()
                .find(|owner| owner.id == id)
                .context("orphaned shared allocation")?;
            if owner.state != "active" || !owner.domains.contains(&domain) {
                let budget: Budget = serde_json::from_str(&encoded)?;
                debit(&mut remaining, &[domain], budget);
            }
        }
        plan_native_turns(
            connection,
            &owners,
            &pending_helpers,
            epoch,
            &mut remaining,
            &mut targets,
        )?;
        plan_models(&owners, &read_domains(connection)?, &mut targets);
    }
    for (owner, target) in owners.iter().zip(targets) {
        connection.execute(
            "UPDATE owners SET plan_json=?2 WHERE id=?1",
            params![owner.id, serde_json::to_string(&target)?],
        )?;
    }
    // One short write transaction claims every visible domain for this window.
    connection.execute(
        "UPDATE control SET at=?1,epoch=?2 WHERE id=1",
        params![now, epoch],
    )?;
    Ok((epoch, true))
}

fn plan_native_turns(
    connection: &Connection,
    owners: &[PlanOwner],
    pending: &[helpers::PendingHelper],
    epoch: i64,
    remaining: &mut BTreeMap<String, Budget>,
    targets: &mut [Budget],
) -> Result<()> {
    let mut order: Vec<_> = (0..owners.len())
        .filter(|index| owners[*index].state == "active" && owners[*index].demand.native)
        .collect();
    order.sort_by_key(|index| (owners[*index].last_native, owners[*index].id.as_str()));
    let mut served = Vec::new();
    let mut selected_helpers = BTreeSet::new();
    for (position, index) in order.into_iter().enumerate() {
        let owner = &owners[index];
        let helper = pending.iter().find(|helper| {
            owner.demand.queued
                && helper.domain_key == owner.native_domain
                && helper.domains.len() == owner.domains.len()
                && helper
                    .domains
                    .iter()
                    .all(|domain| owner.domains.contains(domain))
        });
        // Both reused helpers and new helpers protect the useful turn before
        // a younger pending helper can consume its room.
        let wanted = owner.minimum(connection)?;
        let desired = wanted.max(targets[index]);
        let extra = desired.sub(targets[index]);
        let new_helper = helper.filter(|helper| !selected_helpers.contains(&helper.generation));
        let helper_delta = if new_helper.is_some() {
            helpers::helper_budget()
        } else {
            Budget::default()
        };
        let needed = extra.add(helper_delta);
        if owner.domains.iter().all(|domain| {
            remaining
                .get(domain)
                .is_some_and(|budget| needed.min(*budget) == needed)
        }) {
            targets[index] = desired;
            debit(remaining, &owner.domains, needed);
            if let Some(helper) = new_helper {
                helpers::select(connection, helper, &owner.id, epoch, desired)?;
                selected_helpers.insert(helper.generation.clone());
            }
            served.push(index);
        } else if position == 0 {
            // Stop extending younger native work while an older fitting
            // working set waits for the existing reservations to drain.
            break;
        }
    }
    // Reserve useful minimum turns first, then let existing local queues
    // fill spare capacity beneath their independently tuned hard ceilings.
    for index in served {
        let owner = &owners[index];
        let mut desired = owner.requested.native();
        for domain in &owner.domains {
            desired = desired.min(remaining[domain].add(targets[index]));
        }
        let extra = desired.sub(targets[index]);
        targets[index] = targets[index].add(extra);
        debit(remaining, &owner.domains, extra);
    }
    Ok(())
}

fn plan_models(owners: &[PlanOwner], envelopes: &BTreeMap<String, Budget>, targets: &mut [Budget]) {
    for (index, owner) in owners.iter().enumerate() {
        targets[index].models = if owner.state == "active" && owner.demand.model {
            owner.requested.models
        } else {
            0
        };
    }
    for (domain, envelope) in envelopes {
        let mut order: Vec<_> = (0..owners.len())
            .filter(|index| {
                let owner = &owners[*index];
                owner.state == "active"
                    && owner.demand.model
                    && owner.requested.models > 0
                    && owner.domains.contains(domain)
            })
            .collect();
        order.sort_by_key(|index| (owners[*index].last_model, owners[*index].id.as_str()));
        let mut shares = vec![0_usize; owners.len()];
        let mut cursor = 0;
        for _ in 0..envelope.models {
            if order.is_empty() {
                break;
            }
            let index = order[cursor];
            shares[index] += 1;
            if shares[index] == owners[index].requested.models {
                order.remove(cursor);
            } else {
                cursor += 1;
            }
            if !order.is_empty() {
                cursor %= order.len();
            }
        }
        for (index, _) in owners
            .iter()
            .enumerate()
            .filter(|(_, owner)| owner.domains.contains(domain))
        {
            targets[index].models = targets[index].models.min(shares[index]);
        }
    }
}

fn debit(remaining: &mut BTreeMap<String, Budget>, domains: &[String], amount: Budget) {
    for domain in domains {
        if let Some(budget) = remaining.get_mut(domain) {
            *budget = budget.sub(amount);
        }
    }
}

fn read_owners(connection: &Connection, now: i64) -> Result<Vec<PlanOwner>> {
    let mut statement = connection.prepare(
        "SELECT id,requested,resident_floor,domain_ids,native_demand,model_demand,warm_until,last_native,last_model,state,unsafe_scope,working_demand,queued_native,native_domain FROM owners LIMIT 257")?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, bool>(4)?,
            row.get::<_, bool>(5)?,
            row.get::<_, i64>(6)?,
            row.get::<_, i64>(7)?,
            row.get::<_, i64>(8)?,
            row.get::<_, String>(9)?,
            row.get::<_, bool>(10)?,
            row.get::<_, String>(11)?,
            row.get::<_, bool>(12)?,
            row.get::<_, String>(13)?,
        ))
    })?;
    let mut owners = Vec::new();
    for row in rows {
        let (
            id,
            requested,
            floor,
            domains,
            native,
            model,
            warm_until,
            last_native,
            last_model,
            state,
            unsafe_scope,
            demand,
            queued_native,
            native_domain,
        ) = row?;
        let domains: Vec<String> = serde_json::from_str(&domains)?;
        ensure!(
            !domains.is_empty() && domains.len() <= MAX_DOMAINS,
            "invalid stored resource ancestry"
        );
        owners.push(PlanOwner {
            id,
            requested: serde_json::from_str(&requested)?,
            floor: serde_json::from_str(&floor)?,
            demand: PlanDemand {
                working: serde_json::from_str(&demand)?,
                native: native || now < warm_until,
                queued: queued_native,
                model: model || now < warm_until,
            },
            native_domain,
            domains,
            last_native,
            last_model,
            state,
            unsafe_scope,
        });
    }
    ensure!(
        owners.len() <= MAX_PARTICIPANTS,
        "shared capacity participant metadata exceeds its bound"
    );
    Ok(owners)
}

fn read_domains(connection: &Connection) -> Result<BTreeMap<String, Budget>> {
    let mut statement = connection.prepare("SELECT id,budget FROM domains LIMIT 4097")?;
    let mut domains = BTreeMap::new();
    for row in statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })? {
        let (id, encoded) = row?;
        domains.insert(id, serde_json::from_str(&encoded)?);
    }
    ensure!(
        domains.len() <= MAX_DOMAIN_RECORDS,
        "shared resource-domain metadata exceeds its bound"
    );
    Ok(domains)
}

fn allocation_domains(connection: &Connection, owner: &str) -> Result<Vec<String>> {
    let mut statement =
        connection.prepare("SELECT domain FROM allocations WHERE owner=?1 LIMIT 129")?;
    let domains = statement
        .query_map([owner], |row| row.get(0))?
        .collect::<rusqlite::Result<Vec<String>>>()?;
    ensure!(
        domains.len() <= MAX_RETAINED_DOMAINS,
        "retained resource ancestry exceeds its bound"
    );
    Ok(domains)
}

fn domain_budget(connection: &Connection, domain: &str) -> Result<Budget> {
    let encoded: String =
        connection.query_row("SELECT budget FROM domains WHERE id=?1", [domain], |row| {
            row.get(0)
        })?;
    Ok(serde_json::from_str(&encoded)?)
}

fn charged_others(connection: &Connection, domain: &str, owner: &str) -> Result<Budget> {
    let mut statement = connection
        .prepare("SELECT budget FROM allocations WHERE domain=?1 AND owner<>?2 LIMIT 257")?;
    let mut total = Budget::default();
    let mut count = 0;
    for budget in statement.query_map(params![domain, owner], |row| row.get::<_, String>(0))? {
        count += 1;
        ensure!(
            count <= MAX_PARTICIPANTS,
            "domain allocation metadata exceeds its bound"
        );
        total = total.add(serde_json::from_str(&budget?)?);
    }
    Ok(total.add(helpers::charged(connection, domain)?))
}

fn validate_local_identity(connection: &Connection, owner: &str) -> Result<()> {
    let (boot, pid, birth): (String, u32, i64) = connection.query_row(
        "SELECT boot,pid,birth FROM owners WHERE id=?1",
        [owner],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    ensure!(
        pid == std::process::id(),
        "capacity owner process identity changed"
    );
    if boot != "unavailable" {
        let identity = HostSampler::owner_identity()
            .context("capacity owner identity is unavailable; admission remains fenced")?;
        ensure!(
            identity.boot_id == boot && i64::try_from(identity.start_time)? == birth,
            "capacity owner birth or boot identity changed"
        );
    }
    Ok(())
}

fn recover(connection: &Connection, files: &PrivateFiles, now: i64) -> Result<usize> {
    recover_with_boot(
        connection,
        files,
        now,
        HostSampler::boot_identity().as_deref(),
    )
}

fn recover_with_boot(
    connection: &Connection,
    files: &PrivateFiles,
    now: i64,
    boot: Option<&str>,
) -> Result<usize> {
    let mut statement =
        connection.prepare("SELECT id,boot,pid,birth,heartbeat,slot,file_key FROM owners LEFT JOIN owner_witnesses ON owners.id=owner_witnesses.owner LIMIT 257")?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, u32>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, Option<u16>>(5)?,
                row.get::<_, Option<String>>(6)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    ensure!(
        rows.len() <= MAX_PARTICIPANTS,
        "owner recovery metadata exceeds its bound"
    );
    drop(statement);
    let mut retired = 0;
    for (owner, recorded_boot, pid, birth, heartbeat, slot, file_key) in rows {
        if boot.is_some_and(|current| recorded_boot != "unavailable" && recorded_boot != current) {
            // Native ownership cannot cross a proved host boot. Provider-window
            // records are deliberately separate and survive this reclamation.
            connection.execute("DELETE FROM allocations WHERE owner=?1", [&owner])?;
            connection.execute("DELETE FROM owners WHERE id=?1", [&owner])?;
            connection.execute("DELETE FROM owner_witnesses WHERE owner=?1", [&owner])?;
            retired += 1;
        } else if now < heartbeat || now.saturating_sub(heartbeat) > CONTACT_TIME_MS {
            // Lifetime release fences future allowance spending. It does not
            // prove that native descendants stopped or external effects ended.
            if let (Some(slot), Some(file_key)) = (slot, file_key.as_deref())
                && witness::released(files, slot, file_key)?
            {
                connection.execute("DELETE FROM allocations WHERE owner=?1", [&owner])?;
                connection.execute("DELETE FROM owners WHERE id=?1", [&owner])?;
                connection.execute("DELETE FROM owner_witnesses WHERE owner=?1", [&owner])?;
                retired += 1;
                continue;
            }
            let matching = slot.is_some()
                || process_identity(pid).is_some_and(|identity| {
                    identity.boot_id == recorded_boot
                        && i64::try_from(identity.start_time) == Ok(birth)
                });
            connection.execute(
                "UPDATE owners SET state=?2 WHERE id=?1",
                params![owner, if matching { "suspect" } else { "quarantined" }],
            )?;
        }
    }
    retired += helpers::recover(connection, files, boot)?;
    prune_domains(connection)?;
    Ok(retired)
}

fn prune_domains(connection: &Connection) -> Result<()> {
    connection.execute(
        "DELETE FROM domains WHERE id NOT IN(SELECT domain FROM allocations)
         AND id NOT IN(SELECT domain FROM native_helper_allocations)",
        [],
    )?;
    Ok(())
}

fn shared_reason(connection: &Connection) -> Result<Option<String>> {
    let uncertain = count(
        connection,
        "SELECT COUNT(*) FROM owners WHERE state<>'active' OR unsafe_scope=1",
    )?;
    let mut debt = 0;
    for (domain, budget) in read_domains(connection)? {
        let charged = charged_others(connection, &domain, "")?;
        if charged.min(budget) != charged {
            debt += 1;
        }
    }
    Ok(if uncertain > 0 {
        Some(format!(
            "{uncertain} suspect or quarantined host owner(s) retain their charges"
        ))
    } else if debt > 0 {
        Some(format!(
            "shared capacity is draining debt in {debt} resource domain(s)"
        ))
    } else {
        None
    })
}

fn count(connection: &Connection, sql: &str) -> Result<usize> {
    let value: i64 = connection.query_row(sql, [], |row| row.get(0))?;
    usize::try_from(value).context("invalid negative capacity metadata count")
}

fn table_empty_if_present(connection: &Connection, table: &str) -> Result<bool> {
    let exists: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name=?1)",
        [table],
        |row| row.get(0),
    )?;
    // Only static internal schema names reach this migration helper.
    Ok(!exists || count(connection, &format!("SELECT COUNT(*) FROM {table}"))? == 0)
}

fn sql_time() -> Result<i64> {
    i64::try_from(timestamp_ms()).context("capacity timestamp exceeds SQLite integer range")
}

struct PrivateFiles {
    database: PathBuf,
    #[cfg(unix)]
    database_key: (u64, u64),
    #[cfg(windows)]
    anchor: File,
    directory_key: (u64, u64),
    #[cfg(any(target_os = "macos", windows))]
    directory: rightkit_fs::StateDir,
    #[cfg(target_os = "linux")]
    directory: File,
}

impl PrivateFiles {
    fn open(root: &Path) -> Result<Self> {
        let root = std::path::absolute(root)?;
        #[cfg(any(target_os = "macos", windows))]
        {
            #[cfg(windows)]
            use rightkit_fs::handle::ReadOptions;
            ensure!(
                !rightkit_fs::is_network_filesystem(&root),
                "shared capacity requires a local filesystem with reliable SQLite locks"
            );
            let directory = rightkit_fs::StateDir::create(&root)?;
            rightkit_fs::private::verify_owner_only(directory.path())?;
            #[cfg(windows)]
            ensure!(
                rightkit_fs::private::is_owner_only_strict(directory.path())?,
                "capacity directory requires an inheritable owner-only ACL"
            );
            if !directory.exists("capacity.sqlite3")? {
                match directory.write_new("capacity.sqlite3", &[]) {
                    Ok(()) => {}
                    Err(_) if directory.exists("capacity.sqlite3")? => {}
                    Err(error) => return Err(error.into()),
                }
            }
            let database = directory.path().join("capacity.sqlite3");
            #[cfg(windows)]
            let anchor = directory.open_read(
                "capacity.sqlite3",
                &ReadOptions::new().require_single_link(true),
            )?;
            let files = Self {
                directory_key: path_key(&root)?,
                #[cfg(unix)]
                database_key: path_key(&database)?,
                database,
                #[cfg(windows)]
                anchor,
                directory,
            };
            files.verify()?;
            Ok(files)
        }
        #[cfg(target_os = "linux")]
        {
            use rustix::fs::{Mode, OFlags};
            let directory = open_linux_directory(&root, true)?;
            let filesystem = rustix::fs::fstatfs(&directory)?;
            ensure!(
                !matches!(
                    i128::from(filesystem.f_type),
                    0x6969 | 0x517B | 0xFF53_4D42 | 0xFE53_4D42 | 0x5346_414F | 0x00C3_6400
                ),
                "shared capacity requires a local filesystem with reliable SQLite locks"
            );
            match rustix::fs::openat(
                &directory,
                "capacity.sqlite3",
                OFlags::CREATE
                    | OFlags::EXCL
                    | OFlags::RDWR
                    | OFlags::NOFOLLOW
                    | OFlags::NONBLOCK
                    | OFlags::CLOEXEC,
                Mode::RUSR | Mode::WUSR,
            ) {
                Ok(created) => drop(created),
                Err(rustix::io::Errno::EXIST) => {}
                Err(error) => return Err(error.into()),
            }
            let database = root.join("capacity.sqlite3");
            let files = Self {
                database_key: path_key(&database)?,
                database,
                directory_key: file_key(&directory, &root)?,
                directory,
            };
            files.verify()?;
            Ok(files)
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
        anyhow::bail!("protected shared capacity state is unsupported on this platform")
    }

    fn verify(&self) -> Result<()> {
        let root = self
            .database
            .parent()
            .context("capacity directory missing")?;
        #[cfg(any(target_os = "macos", windows))]
        {
            let _verified = rightkit_fs::StateDir::open(root)?;
            ensure!(
                !rightkit_fs::is_network_filesystem(root),
                "capacity state moved to a remote filesystem"
            );
        }
        #[cfg(target_os = "linux")]
        {
            let current = open_linux_directory(root, false)?;
            ensure!(
                file_key(&current, root)? == file_key(&self.directory, root)?,
                "capacity directory identity changed"
            );
        }
        ensure!(
            path_key(root)? == self.directory_key,
            "capacity directory identity changed"
        );
        self.verify_database_identity()?;
        for (directory, path) in [
            (true, root.to_path_buf()),
            (false, self.database.clone()),
            (false, self.database.with_file_name("capacity.sqlite3-wal")),
            (false, self.database.with_file_name("capacity.sqlite3-shm")),
            (
                false,
                self.database.with_file_name("capacity.sqlite3-journal"),
            ),
        ] {
            let optional = path != self.database && path != root;
            let metadata = match std::fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(error) if optional && error.kind() == std::io::ErrorKind::NotFound => {
                    continue;
                }
                Err(error) => return Err(error.into()),
            };
            verify_entry(&path, directory, optional, &metadata)?;
        }
        ensure!(
            path_key(root)? == self.directory_key,
            "capacity directory identity changed"
        );
        self.verify_database_identity()?;
        Ok(())
    }

    fn verify_database_identity(&self) -> Result<()> {
        // SQLite owns the live Unix descriptor. Opening and closing a separate
        // descriptor would release its process-wide POSIX locks. Compare the
        // no-follow path identity captured before SQLite opens, then recheck it
        // after open and on each coordinated transition.
        #[cfg(unix)]
        let expected = self.database_key;
        #[cfg(windows)]
        let expected = file_key(&self.anchor, &self.database)?;
        ensure!(
            path_key(&self.database)? == expected,
            "capacity database identity changed"
        );
        Ok(())
    }
}

fn verify_entry(
    path: &Path,
    directory: bool,
    optional: bool,
    metadata: &std::fs::Metadata,
) -> Result<()> {
    let verified = (|| {
        verify_entry_metadata(directory, metadata)?;
        #[cfg(any(target_os = "macos", windows))]
        rightkit_fs::private::verify_owner_only(path)?;
        #[cfg(windows)]
        let handle = if directory {
            None
        } else {
            Some(rightkit_fs::handle::open_read_nofollow(
                path,
                &rightkit_fs::handle::ReadOptions::new().require_single_link(true),
            )?)
        };
        // SQLite can remove its last WAL or SHM while this check is in flight.
        // Inspect paths on Unix without opening or closing live SQLite files.
        let current = std::fs::symlink_metadata(path)?;
        verify_entry_metadata(directory, &current)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            ensure!(
                (metadata.dev(), metadata.ino()) == (current.dev(), current.ino()),
                "capacity entry identity changed during validation"
            );
        }
        #[cfg(windows)]
        if let Some(handle) = handle {
            ensure!(
                rightkit_fs::handle::link_count(&handle)? == 1
                    && file_key(&handle, path)? == path_key(path)?,
                "capacity entry identity or hard-link count changed"
            );
        }
        Ok(())
    })();
    if let Err(error) = &verified {
        let missing = error
            .downcast_ref::<std::io::Error>()
            .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound);
        #[cfg(any(target_os = "macos", windows))]
        let missing = missing
            || matches!(
                error.downcast_ref::<rightkit_fs::FsError>(),
                Some(rightkit_fs::FsError::Io { source, .. })
                    if source.kind() == std::io::ErrorKind::NotFound
            );
        if optional && missing {
            match std::fs::symlink_metadata(path) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
                Err(error) => return Err(error.into()),
                Ok(_) => {}
            }
        }
    }
    verified
}

fn verify_entry_metadata(directory: bool, metadata: &std::fs::Metadata) -> Result<()> {
    ensure!(
        !metadata.file_type().is_symlink()
            && if directory {
                metadata.is_dir()
            } else {
                metadata.is_file()
            },
        "capacity state must contain only its private directory and regular files"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        ensure!(
            metadata.mode().trailing_zeros() >= 6,
            "capacity state must be private"
        );
        if !directory {
            ensure!(
                metadata.nlink() == 1,
                "capacity state cannot have multiple hard links"
            );
        }
        #[cfg(target_os = "linux")]
        ensure!(
            metadata.uid() == rustix::process::geteuid().as_raw(),
            "capacity state belongs to another user"
        );
    }
    if !directory {
        ensure!(
            metadata.len() <= 64 * 1024 * 1024,
            "capacity state exceeds its file bound"
        );
    }
    Ok(())
}

#[cfg(unix)]
fn file_key(file: &File, _shown: &Path) -> Result<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    let metadata = file.metadata()?;
    Ok((metadata.dev(), metadata.ino()))
}

#[cfg(unix)]
fn path_key(path: &Path) -> Result<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    let metadata = std::fs::symlink_metadata(path)?;
    ensure!(
        !metadata.file_type().is_symlink(),
        "capacity state cannot follow symlinks"
    );
    Ok((metadata.dev(), metadata.ino()))
}

#[cfg(windows)]
fn file_key(file: &File, shown: &Path) -> Result<(u64, u64)> {
    let key = rightkit_fs::FileKey::of_file(file, shown)?;
    Ok((key.volume, key.index))
}

#[cfg(windows)]
fn path_key(path: &Path) -> Result<(u64, u64)> {
    let key = rightkit_fs::FileKey::of_path(path)?;
    Ok((key.volume, key.index))
}

#[cfg(target_os = "linux")]
fn open_linux_directory(path: &Path, create: bool) -> Result<File> {
    use rustix::fs::{Mode, OFlags};
    use std::path::Component;
    let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    let mut directory = File::from(rustix::fs::open("/", flags, Mode::empty())?);
    let components: Vec<_> = path.components().collect();
    for (index, component) in components.iter().enumerate() {
        match component {
            Component::RootDir => {}
            Component::Normal(name) => {
                let next = rustix::fs::openat(&directory, *name, flags, Mode::empty());
                let next = match next {
                    Ok(next) => next,
                    Err(rustix::io::Errno::NOENT) if create && index + 1 == components.len() => {
                        match rustix::fs::mkdirat(
                            &directory,
                            *name,
                            Mode::RUSR | Mode::WUSR | Mode::XUSR,
                        ) {
                            Ok(()) | Err(rustix::io::Errno::EXIST) => {}
                            Err(error) => return Err(error.into()),
                        }
                        rustix::fs::openat(&directory, *name, flags, Mode::empty())?
                    }
                    Err(error) => return Err(error.into()),
                };
                directory = File::from(next);
            }
            _ => anyhow::bail!(
                "capacity directory requires an absolute path without parent traversal"
            ),
        }
    }
    Ok(directory)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "Registry fixtures assert failures at the owned transition boundary."
)]
mod tests {
    use super::*;
    use crate::{ProviderCapacity, ProviderRate, QuotaDomain, capacity::provider_registry};
    use ejm_harness_exec::{
        AdmissionPolicy, Observation, ObservationSource, ResourceDomainObservation,
    };
    use tempfile::TempDir;
    use tokio_util::sync::CancellationToken;

    pub(super) fn observations(cpu: u32) -> HostObservations {
        let mut observed = HostSampler::default().sample();
        observed.cpu_millis = Observation {
            value: Some(cpu),
            source: ObservationSource::NativeSystem,
            availability: ObservationAvailability::Measured,
            age_ms: 0,
            max_age_ms: 5_000,
        };
        observed.memory_bytes = Observation {
            value: Some(4 * 1024 * 1024 * 1024),
            source: ObservationSource::NativeSystem,
            availability: ObservationAvailability::Measured,
            age_ms: 0,
            max_age_ms: 5_000,
        };
        observed.domains = vec![ResourceDomainObservation {
            id: "host".into(),
            parent: None,
            cpu_millis: Some(cpu),
            memory_bytes: observed.memory_bytes.value,
            available_memory_bytes: None,
            max_processes: Some(4096),
            available_processes: None,
        }];
        observed.domain_visibility = ObservationAvailability::Measured;
        observed
    }

    pub(super) fn root() -> (TempDir, PathBuf) {
        let parent = tempfile::tempdir().unwrap();
        let path = parent.path().canonicalize().unwrap().join("capacity");
        (parent, path)
    }

    pub(super) fn requested(cpu: u32) -> Budget {
        Budget::requested(
            &ResourceLimits {
                cpu_millis: cpu,
                ..ResourceLimits::default()
            },
            8,
            4,
        )
    }

    pub(super) fn quota_demand(owner: &str) -> provider_registry::Demand {
        let domain = QuotaDomain::new(
            "https://provider.invalid",
            "00000000-0000-0000-0000-000000000001",
            "recovery",
        )
        .unwrap();
        provider_registry::Demand {
            owner: owner.into(),
            domain: domain.id().into(),
            policy: ProviderCapacity {
                domain: Some(domain),
                max_in_flight: Some(1),
                requests: Some(ProviderRate {
                    limit: 1,
                    window_ms: 60_000,
                }),
                tokens: Some(ProviderRate {
                    limit: 100,
                    window_ms: 60_000,
                }),
            },
            hard: 1,
            desired: 1,
            active: 0,
            waiting: 1,
            tokens_needed: 100,
            largest_request: 100,
            requests_left: 0,
            tokens_left: 0,
            token_debt: 0,
            expires: 0,
            epoch: 0,
            clear_cooldown: false,
            feedback_lost: false,
            feedback: Vec::new(),
        }
    }

    pub(super) fn force_control(registry: &Registry) {
        registry
            .connection
            .execute("UPDATE control SET at=0", [])
            .unwrap();
    }

    pub(super) fn charge(registry: &Registry, owner: &str, domain: &str) -> Budget {
        let encoded: String = registry
            .connection
            .query_row(
                "SELECT budget FROM allocations WHERE owner=?1 AND domain=?2",
                params![owner, domain],
                |row| row.get(0),
            )
            .unwrap();
        serde_json::from_str(&encoded).unwrap()
    }

    fn renew(
        registry: &mut Registry,
        owner: &str,
        request: Budget,
        observed: &HostObservations,
    ) -> Budget {
        force_control(registry);
        registry
            .renew(
                owner,
                request,
                0,
                1,
                &GovernorSnapshot {
                    queued: 1,
                    ..GovernorSnapshot::default()
                },
                request.operating(),
                observed,
                || Ok(()),
                |_| Ok((GovernorSnapshot::default(), 0)),
            )
            .unwrap()
    }

    #[test]
    fn more_owners_than_native_slots_make_finite_progress() {
        let (_parent, root) = root();
        let observed = observations(2_000); // One useful native CPU slice after foreground headroom.
        let request = requested(1_000);
        let floor = OperatingTarget {
            memory_bytes: 96 * 1024 * 1024,
            ..OperatingTarget::default()
        };
        let mut owners = Vec::new();
        let mut memory_only = 0;
        for index in 0..8 {
            let id = format!("owner-{index}");
            let mut registry = Registry::open(&root).unwrap();
            let grant = registry
                .register_with_floor(&id, request, floor, &observed)
                .unwrap();
            assert!(grant.memory >= floor.memory_bytes);
            assert!(charge(&registry, &id, "host").memory >= floor.memory_bytes);
            memory_only += usize::from(grant.cpu == 0);
            let envelope = domain_budget(&registry.connection, "host").unwrap();
            let total = charged_others(&registry.connection, "host", "").unwrap();
            assert_eq!(total.min(envelope), total);
            owners.push((id, registry));
        }
        assert!(memory_only > 0, "the fixture must exhaust native CPU");
        let mut served = BTreeSet::new();
        for _ in 0..20 {
            for (id, registry) in &mut owners {
                let grant = renew(registry, id, request, &observed);
                if grant.cpu >= 1_000 && grant.jobs > 0 {
                    served.insert(id.clone());
                }
                let envelope = domain_budget(&registry.connection, "host").unwrap();
                let total = charged_others(&registry.connection, "host", "").unwrap();
                assert_eq!(
                    total.min(envelope),
                    total,
                    "a transfer overspent the shared envelope"
                );
            }
            if served.len() == owners.len() {
                break;
            }
        }
        assert_eq!(
            served.len(),
            owners.len(),
            "floor division must not strand every native owner"
        );
        assert!(
            owners
                .iter()
                .all(|(id, registry)| charge(registry, id, "host").memory >= BASELINE_MEMORY)
        );
    }

    #[test]
    fn native_and_model_service_are_independent() {
        let (_parent, root) = root();
        let observed = observations(2_000);
        let request = requested(1_000);
        let mut a = Registry::open(&root).unwrap();
        let mut b = Registry::open(&root).unwrap();
        a.register("a", request, &observed).unwrap();
        b.register("b", request, &observed).unwrap();
        let grant = renew(&mut b, "b", request, &observed);
        assert_eq!(grant.models, request.models);
        // b can have remote capacity while a still owns the only native CPU slice.
        assert_eq!(grant.cpu, 0);
    }

    #[test]
    fn cached_plan_preserves_generation_until_request_is_reconsidered() {
        let (_parent, root) = root();
        let observed = observations(2_000);
        let initial = Budget {
            models: 6,
            ..Budget::default()
        };
        let increased = Budget {
            models: 7,
            ..initial
        };
        let mut registry = Registry::open(&root).unwrap();
        registry.register("probe", initial, &observed).unwrap();
        let domains = registry.domains.clone();
        force_control(&registry);
        let tx = registry.connection.transaction().unwrap();
        tx.execute(
            "UPDATE owners SET model_demand=1,warm_until=0 WHERE id='probe'",
            [],
        )
        .unwrap();
        // Drive the planner's existing clock input, independent of test speed.
        let now = CONTROL_WINDOW_MS;
        let (epoch, refreshed) = refresh_plans(&tx, now).unwrap();
        assert!(refreshed);
        tx.execute(
            "UPDATE owners SET requested=?1 WHERE id='probe'",
            [serde_json::to_string(&increased).unwrap()],
        )
        .unwrap();
        for cached_at in [now, now + CONTROL_WINDOW_MS - 1] {
            assert_eq!(refresh_plans(&tx, cached_at).unwrap(), (epoch, false));
            let (granted, planned, _) =
                grant_targets(&tx, "probe", increased, Budget::default(), &domains).unwrap();
            assert_eq!((granted, planned), (initial, initial));
        }
        assert_eq!(
            refresh_plans(&tx, now + CONTROL_WINDOW_MS).unwrap(),
            (epoch + 1, true)
        );
        let (granted, planned, _) =
            grant_targets(&tx, "probe", increased, Budget::default(), &domains).unwrap();
        assert_eq!((granted, planned), (increased, increased));
    }

    #[test]
    fn resident_floors_do_not_force_full_hard_memory_turns() {
        let (_parent, root) = root();
        let observed = observations(5_000);
        let request = Budget {
            memory: 3 * 1024 * 1024 * 1024,
            ..requested(1_000)
        };
        let used = GovernorSnapshot {
            queued: 1,
            reserved_memory_bytes: 512 * 1024 * 1024,
            reserved_processes: 1,
            ..GovernorSnapshot::default()
        };
        let demand = OperatingTarget {
            max_jobs: 1,
            cpu_millis: 250,
            memory_bytes: 576 * 1024 * 1024,
            max_processes: 5,
            max_output_bytes: 64 * 1024,
            ..OperatingTarget::default()
        };
        let mut a = Registry::open(&root).unwrap();
        let mut b = Registry::open(&root).unwrap();
        a.register("a", request, &observed).unwrap();
        b.register("b", request, &observed).unwrap();
        for _ in 0..3 {
            for (registry, owner) in [(&mut a, "a"), (&mut b, "b")] {
                force_control(registry);
                registry
                    .renew(
                        owner,
                        request,
                        0,
                        0,
                        &used,
                        demand,
                        &observed,
                        || Ok(()),
                        |_| Ok((used.clone(), 0)),
                    )
                    .unwrap();
            }
        }
        for owner in ["a", "b"] {
            let grant = charge(&a, owner, "host");
            assert!(grant.memory >= demand.memory_bytes);
            assert!(grant.cpu >= demand.cpu_millis);
            assert!(grant.jobs >= 1);
        }
    }

    #[test]
    fn donor_usage_above_target_retains_its_charge_until_drain() {
        let (_parent, root) = root();
        let observed = observations(2_000);
        let request = requested(1_000);
        let mut a = Registry::open(&root).unwrap();
        let mut b = Registry::open(&root).unwrap();
        a.register("a", request, &observed).unwrap();
        b.register("b", request, &observed).unwrap();
        let held = GovernorSnapshot {
            running: 1,
            reserved_cpu_millis: 1_000,
            reserved_memory_bytes: 64 * 1024 * 1024,
            reserved_processes: 1,
            ..GovernorSnapshot::default()
        };
        force_control(&a);
        let target = a
            .renew(
                "a",
                request,
                0,
                0,
                &held,
                request.operating(),
                &observed,
                || Ok(()),
                |_| Ok((held.clone(), 0)),
            )
            .unwrap();
        assert_eq!(target.cpu, 0);
        assert_eq!(charge(&a, "a", "host").cpu, 1_000);
        assert_eq!(renew(&mut b, "b", request, &observed).cpu, 0);
        renew(
            &mut a,
            "a",
            Budget {
                cpu: 0,
                jobs: 0,
                heavy: 0,
                ..request
            },
            &observed,
        );
        assert_eq!(charge(&a, "a", "host").cpu, 0);
        assert_eq!(renew(&mut b, "b", request, &observed).cpu, 1_000);
    }

    #[test]
    fn failed_install_and_stale_release_leave_committed_credits_charged() {
        let (_parent, root) = root();
        let observed = observations(5_000);
        let mut registry = Registry::open(&root).unwrap();
        registry
            .register("owner", requested(500), &observed)
            .unwrap();
        force_control(&registry);
        assert!(
            registry
                .renew(
                    "owner",
                    requested(2_000),
                    0,
                    1,
                    &GovernorSnapshot {
                        queued: 1,
                        ..GovernorSnapshot::default()
                    },
                    requested(2_000).operating(),
                    &observed,
                    || Ok(()),
                    |_| anyhow::bail!("lost install reply")
                )
                .is_err()
        );
        assert_eq!(charge(&registry, "owner", "host").cpu, 2_000);
        let other = Connection::open(root.join("capacity.sqlite3")).unwrap();
        force_control(&registry);
        assert!(
            registry
                .renew(
                    "owner",
                    Budget {
                        cpu: 0,
                        jobs: 0,
                        heavy: 0,
                        ..requested(2_000)
                    },
                    0,
                    0,
                    &GovernorSnapshot::default(),
                    requested(2_000).operating(),
                    &observed,
                    || Ok(()),
                    |_| {
                        other.execute(
                            "UPDATE owners SET revision=revision+1 WHERE id='owner'",
                            [],
                        )?;
                        Ok((GovernorSnapshot::default(), 0))
                    }
                )
                .is_err()
        );
        assert_eq!(charge(&registry, "owner", "host").cpu, 2_000);
    }

    #[test]
    fn move_fences_before_charging_and_retains_old_descendant_scope() {
        let (_parent, root) = root();
        let mut before = observations(5_000);
        before.domains.push(ResourceDomainObservation {
            id: "old".into(),
            parent: Some("host".into()),
            cpu_millis: Some(2_000),
            memory_bytes: Some(1024 * 1024 * 1024),
            available_memory_bytes: None,
            max_processes: None,
            available_processes: None,
        });
        let mut registry = Registry::open(&root).unwrap();
        registry
            .register("owner", requested(1_000), &before)
            .unwrap();
        let mut after = before.clone();
        after.domains[1].id = "new".into();
        after.domains[1].cpu_millis = Some(500);
        let database = root.join("capacity.sqlite3");
        let fenced = std::cell::Cell::new(false);
        let held = GovernorSnapshot {
            running: 1,
            reserved_cpu_millis: 1_000,
            reserved_memory_bytes: 64 * 1024 * 1024,
            reserved_processes: 1,
            ..GovernorSnapshot::default()
        };
        force_control(&registry);
        registry
            .renew(
                "owner",
                requested(1_000),
                0,
                0,
                &held,
                requested(1_000).operating(),
                &after,
                || {
                    fenced.set(true);
                    Ok(())
                },
                |_| {
                    assert!(fenced.get());
                    let check = Connection::open(&database)?;
                    let encoded: String = check.query_row(
                        "SELECT budget FROM allocations WHERE owner='owner' AND domain='new'",
                        [],
                        |row| row.get(0),
                    )?;
                    assert_eq!(serde_json::from_str::<Budget>(&encoded)?.cpu, 1_000);
                    Ok((held.clone(), 0))
                },
            )
            .unwrap();
        assert_eq!(
            allocation_domains(&registry.connection, "owner")
                .unwrap()
                .len(),
            3
        );
        assert_eq!(charge(&registry, "owner", "host").cpu, 1_000);
        assert!(registry.reason().is_some());
        renew(&mut registry, "owner", requested(1_000), &after);
        assert_eq!(
            allocation_domains(&registry.connection, "owner").unwrap(),
            vec!["host", "new"]
        );
    }

    #[test]
    fn missing_ancestry_blocks_new_shared_plans_and_cannot_create_a_root() {
        let (_parent, root) = root();
        let observed = observations(5_000);
        let mut registry = Registry::open(&root).unwrap();
        registry
            .register("owner", requested(1_000), &observed)
            .unwrap();
        let mut unknown = observed.clone();
        unknown.domains[0].id = "private-root".into();
        let fenced = std::cell::Cell::new(false);
        assert!(
            registry
                .renew(
                    "owner",
                    requested(1_000),
                    0,
                    0,
                    &GovernorSnapshot::default(),
                    requested(1_000).operating(),
                    &unknown,
                    || {
                        fenced.set(true);
                        Ok(())
                    },
                    |_| anyhow::bail!("unknown ancestry must not install")
                )
                .is_err()
        );
        assert!(fenced.get());
        assert_eq!(
            count(&registry.connection, "SELECT COUNT(*) FROM domains").unwrap(),
            1
        );
        let mut newcomer = Registry::open(&root).unwrap();
        let parked = newcomer
            .register("new", requested(1_000), &observed)
            .unwrap();
        assert_eq!(parked, Budget::default());
        assert!(
            Registry::open(&root)
                .unwrap()
                .register("invalid", requested(1_000), &unknown)
                .is_err()
        );
    }

    #[test]
    fn held_lifetime_witness_preserves_grant_despite_expiry_or_pid_change() {
        let (_parent, root) = root();
        let observed = observations(5_000);
        let mut registry = Registry::open(&root).unwrap();
        registry
            .register("gone", requested(1_000), &observed)
            .unwrap();
        let before = charge(&registry, "gone", "host");
        registry
            .connection
            .execute(
                "UPDATE owners SET heartbeat=0,pid=4294967295 WHERE id='gone'",
                [],
            )
            .unwrap();
        let mut newcomer = Registry::open(&root).unwrap();
        newcomer
            .register("new", requested(1_000), &observed)
            .unwrap();
        assert_eq!(charge(&newcomer, "gone", "host"), before);
        let state: String = newcomer
            .connection
            .query_row("SELECT state FROM owners WHERE id='gone'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(state, "suspect");
        assert!(newcomer.reason().is_some());
    }

    #[test]
    fn retained_lifetime_fences_recovery_after_registry_drop() {
        let (_parent, root) = root();
        let observed = observations(5_000);
        let mut registry = Registry::open(&root).unwrap();
        registry
            .register("owner", requested(1_000), &observed)
            .unwrap();
        let lifetime = registry.lifetime().unwrap();
        registry
            .connection
            .execute("UPDATE owners SET heartbeat=0", [])
            .unwrap();
        drop(registry);
        let reader = Registry::open(&root).unwrap();
        assert_eq!(
            recover(&reader.connection, &reader.files, sql_time().unwrap()).unwrap(),
            0
        );
        assert_eq!(
            count(&reader.connection, "SELECT COUNT(*) FROM owners").unwrap(),
            1
        );
        drop(lifetime);
        assert_eq!(
            recover(&reader.connection, &reader.files, sql_time().unwrap()).unwrap(),
            1
        );
        assert_eq!(
            count(&reader.connection, "SELECT COUNT(*) FROM allocations").unwrap(),
            0
        );
    }

    #[test]
    fn invalid_lifetime_identity_never_refunds_a_charge() {
        let (_parent, root) = root();
        let observed = observations(5_000);
        let mut registry = Registry::open(&root).unwrap();
        registry
            .register("owner", requested(1_000), &observed)
            .unwrap();
        let before = charge(&registry, "owner", "host");
        registry
            .connection
            .execute("UPDATE owners SET heartbeat=0", [])
            .unwrap();
        registry
            .connection
            .execute("UPDATE owner_witnesses SET file_key='replaced'", [])
            .unwrap();
        assert!(recover(&registry.connection, &registry.files, sql_time().unwrap()).is_err());
        assert_eq!(charge(&registry, "owner", "host"), before);
        registry
            .connection
            .execute("UPDATE owner_witnesses SET slot=255", [])
            .unwrap();
        assert!(recover(&registry.connection, &registry.files, sql_time().unwrap()).is_err());
        assert_eq!(charge(&registry, "owner", "host"), before);
    }

    #[cfg(unix)]
    #[test]
    fn retained_lifetime_witness_rejects_changed_permissions_and_links() {
        use std::os::unix::fs::PermissionsExt;

        let (parent, root) = root();
        let mut registry = Registry::open(&root).unwrap();
        registry
            .register("owner", requested(1_000), &observations(5_000))
            .unwrap();
        let witness = registry.witness.as_ref().unwrap();
        let path = root.join("owner-0.lock");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(witness.verify(&registry.files).is_err());
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        witness.verify(&registry.files).unwrap();
        let alias = parent.path().join("witness-alias");
        std::fs::hard_link(&path, &alias).unwrap();
        assert!(witness.verify(&registry.files).is_err());
        std::fs::remove_file(alias).unwrap();
        witness.verify(&registry.files).unwrap();
    }

    #[test]
    fn released_lifetime_slots_reuse_stable_files() {
        let (_parent, root) = root();
        let observed = observations(5_000);
        let mut registry = Registry::open(&root).unwrap();
        let mut identity = None;
        for index in 0..=MAX_PARTICIPANTS {
            registry
                .register(&format!("owner-{index}"), requested(1_000), &observed)
                .unwrap();
            let current: (u16, String) = registry
                .connection
                .query_row("SELECT slot,file_key FROM owner_witnesses", [], |row| {
                    Ok((row.get(0)?, row.get(1)?))
                })
                .unwrap();
            assert_eq!(current.0, 0);
            if let Some(previous) = &identity {
                assert_eq!(previous, &current.1);
            }
            identity = Some(current.1);
            registry.remove_uninstalled().unwrap();
        }
        assert_eq!(
            std::fs::read_dir(&root)
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| entry.file_name().to_string_lossy().starts_with("owner-"))
                .count(),
            1
        );
    }

    #[tokio::test]
    async fn graceful_release_fences_retained_handles_and_keeps_retry_authority() {
        let (_parent, root) = root();
        let observed = observations(5_000);
        let mut registry = Registry::open(&root).unwrap();
        let grant = registry
            .register("owner", requested(1_000), &observed)
            .unwrap();
        let governor = Arc::new(
            Governor::new_with_policy(ResourceLimits::default(), AdmissionPolicy::Automatic)
                .unwrap(),
        );
        governor
            .install_target(1, grant.target(governor.limits()), None)
            .unwrap();
        let resident = governor
            .acquire_memory("resident", BASELINE_MEMORY, &CancellationToken::new())
            .await
            .unwrap();
        assert!(registry.release_departed(&governor).is_err());
        assert_eq!(registry.owner.as_deref(), Some("owner"));
        assert!(governor.try_acquire_memory("late", 1).unwrap().is_none());
        drop(resident);
        let blocker = Connection::open(root.join("capacity.sqlite3")).unwrap();
        blocker.execute_batch("BEGIN IMMEDIATE").unwrap();
        assert!(registry.release_departed(&governor).is_err());
        assert_eq!(registry.owner.as_deref(), Some("owner"));
        blocker.execute_batch("COMMIT").unwrap();
        registry.release_departed(&governor).unwrap();
        assert!(registry.owner.is_none());
        assert_eq!(
            count(&registry.connection, "SELECT COUNT(*) FROM owners").unwrap(),
            0
        );
        assert!(
            governor
                .try_acquire_memory("retained-handle", 1)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn current_protocol_opens_while_a_wal_writer_is_active() {
        let (_parent, root) = root();
        let registry = Registry::open(&root).unwrap();
        registry
            .connection
            .execute_batch("BEGIN IMMEDIATE; UPDATE control SET at=1 WHERE id=1")
            .unwrap();
        let opened = Registry::open(&root).unwrap();
        assert_eq!(
            count(&opened.connection, "SELECT at FROM control WHERE id=1").unwrap(),
            0
        );
        registry.connection.execute_batch("ROLLBACK").unwrap();
    }

    #[test]
    fn protocol_mismatch_and_database_replacement_fail_closed() {
        let (_parent, root) = root();
        let mut registry = Registry::open(&root).unwrap();
        registry
            .connection
            .pragma_update(None, "user_version", 99)
            .unwrap();
        assert!(Registry::open(&root).is_err());
        assert_eq!(
            registry
                .connection
                .pragma_query_value::<u32, _>(None, "user_version", |row| row.get(0))
                .unwrap(),
            99
        );
        registry
            .connection
            .pragma_update(None, "user_version", PROTOCOL)
            .unwrap();
        let observed = observations(5_000);
        registry
            .register("owner", requested(1_000), &observed)
            .unwrap();
        let replacement =
            std::fs::rename(root.join("capacity.sqlite3"), root.join("retained.sqlite3"));
        #[cfg(windows)]
        if replacement.is_err() {
            // A retained exclusive native handle also prevents replacement.
            assert!(registry.files.verify().is_ok());
            return;
        }
        replacement.unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(root.join("capacity.sqlite3"))
                .unwrap();
        }
        assert!(registry.files.verify().is_err());
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_hardlinks_and_wide_modes_are_rejected() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let (parent, root) = root();
        drop(Registry::open(&root).unwrap());
        let link = parent.path().canonicalize().unwrap().join("link");
        symlink(&root, &link).unwrap();
        assert!(Registry::open(&link).is_err());
        let alias = parent.path().join("alias");
        std::fs::hard_link(root.join("capacity.sqlite3"), &alias).unwrap();
        assert!(Registry::open(&root).is_err());
        std::fs::remove_file(&alias).unwrap();
        symlink(root.join("missing"), root.join("capacity.sqlite3-wal")).unwrap();
        assert!(Registry::open(&root).is_err());
        std::fs::remove_file(root.join("capacity.sqlite3-wal")).unwrap();
        std::fs::set_permissions(
            root.join("capacity.sqlite3"),
            std::fs::Permissions::from_mode(0o644),
        )
        .unwrap();
        assert!(Registry::open(&root).is_err());
    }

    #[test]
    fn sqlite_sidecars_may_disappear_during_validation() {
        let (_parent, root) = root();
        let registry = Registry::open(&root).unwrap();
        registry
            .connection
            .execute("UPDATE control SET at=at+1", [])
            .unwrap();
        let sidecars: Vec<_> = ["capacity.sqlite3-wal", "capacity.sqlite3-shm"]
            .map(|name| {
                let path = root.join(name);
                let metadata = std::fs::symlink_metadata(&path).unwrap();
                (path, metadata)
            })
            .into();
        drop(registry);
        for (path, metadata) in sidecars {
            assert!(!path.exists(), "SQLite must remove its final sidecars");
            assert!(verify_entry(&path, false, true, &metadata).is_ok());
            assert!(verify_entry(&path, false, false, &metadata).is_err());
        }
    }

    #[cfg(unix)]
    #[test]
    fn vanished_unsafe_sidecars_are_still_rejected() {
        use std::os::unix::fs::symlink;
        let (_parent, root) = root();
        drop(Registry::open(&root).unwrap());
        let path = root.join("capacity.sqlite3-wal");
        symlink(root.join("missing"), &path).unwrap();
        let metadata = std::fs::symlink_metadata(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert!(verify_entry(&path, false, true, &metadata).is_err());
    }

    #[test]
    fn boot_reclamation_preserves_unexpired_provider_windows() {
        let (_parent, root) = root();
        let observed = observations(5_000);
        let mut registry = Registry::open(&root).unwrap();
        registry
            .register("old", requested(1_000), &observed)
            .unwrap();
        provider_registry::initialize(&registry.connection).unwrap();
        let now = timestamp_ms();
        let prepaid =
            provider_registry::allocate(&mut registry.connection, &quota_demand("old"), now)
                .unwrap();
        assert_eq!(prepaid.request_credit, Some(1));
        assert_eq!(prepaid.token_credit, Some(100));
        let expires: i64 = registry
            .connection
            .query_row(
                "SELECT json_extract(payload,'$.request.ends') FROM provider_domains",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let expires = u64::try_from(expires).unwrap();
        registry
            .connection
            .execute(
                "UPDATE owners SET boot='proved-old-boot' WHERE id='old'",
                [],
            )
            .unwrap();
        recover_with_boot(
            &registry.connection,
            &registry.files,
            sql_time().unwrap(),
            Some("proved-new-boot"),
        )
        .unwrap();
        assert_eq!(
            count(&registry.connection, "SELECT COUNT(*) FROM owners").unwrap(),
            0
        );
        assert_eq!(
            count(&registry.connection, "SELECT COUNT(*) FROM allocations").unwrap(),
            0
        );
        assert_eq!(
            count(
                &registry.connection,
                "SELECT COUNT(*) FROM provider_domains"
            )
            .unwrap(),
            1
        );
        drop(registry);
        let mut reopened = Registry::open(&root).unwrap();
        reopened
            .register("new", requested(1_000), &observed)
            .unwrap();
        let demand = quota_demand("new");
        for now in [now + 1, expires - 1] {
            let retained =
                provider_registry::allocate(&mut reopened.connection, &demand, now).unwrap();
            assert_eq!(retained.request_credit, Some(0));
            assert_eq!(retained.token_credit, Some(0));
        }
        let renewed =
            provider_registry::allocate(&mut reopened.connection, &demand, expires).unwrap();
        assert_eq!(renewed.request_credit, Some(1));
        assert_eq!(renewed.token_credit, Some(100));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn capacity_state_rejects_inherited_and_existing_allow_acls() {
        let (_parent, root) = root();
        let parent = root.parent().unwrap();
        assert!(
            std::process::Command::new("/bin/chmod")
                .args([
                    "+a",
                    "everyone allow read,write,file_inherit,directory_inherit"
                ])
                .arg(parent)
                .status()
                .unwrap()
                .success()
        );
        assert!(Registry::open(&root).is_err());
        assert!(!root.exists());
        assert!(
            std::process::Command::new("/bin/chmod")
                .arg("-N")
                .arg(parent)
                .status()
                .unwrap()
                .success()
        );
        drop(Registry::open(&root).unwrap());
        assert!(
            std::process::Command::new("/bin/chmod")
                .args(["+a", "everyone allow read"])
                .arg(root.join("capacity.sqlite3"))
                .status()
                .unwrap()
                .success()
        );
        assert!(Registry::open(&root).is_err());
    }

    pub(super) fn wait_file(path: &Path) {
        let until = std::time::Instant::now() + Duration::from_secs(10);
        while !path.exists() {
            assert!(
                std::time::Instant::now() < until,
                "process fixture did not reach its barrier"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn registry_subprocess_fixture() {
        let Some(parent) = std::env::var_os("EJM_CAPACITY_REGISTRY_FIXTURE") else {
            return;
        };
        let parent = PathBuf::from(parent);
        let child_key = std::env::var("EJM_CAPACITY_REGISTRY_CHILD").unwrap();
        wait_file(&parent.join("start"));
        if std::env::var_os("EJM_CAPACITY_REGISTRY_BOUNDARY").is_some() {
            faults::subprocess(&parent);
            return;
        }
        let root = parent.join("capacity");
        let observed = observations(5_000);
        let owner = format!("child-{}", std::process::id());
        let mut registry = Registry::open(&root).unwrap();
        registry
            .register(&owner, requested(1_000), &observed)
            .unwrap();
        std::fs::write(parent.join(format!("{child_key}.ready")), b"ready").unwrap();
        wait_file(&parent.join("finish"));
        let governor = Arc::new(Governor::new(ResourceLimits::default()).unwrap());
        registry.release_departed(&governor).unwrap();
    }

    pub(super) struct FixtureChildren(pub(super) Vec<std::process::Child>);
    impl Drop for FixtureChildren {
        fn drop(&mut self) {
            for child in &mut self.0 {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn sqlite_writer_subprocess_fixture() {
        let Some(root) = std::env::var_os("EJM_CAPACITY_WRITER_FIXTURE") else {
            return;
        };
        let connection = Connection::open_with_flags(
            PathBuf::from(root).join("capacity.sqlite3"),
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )
        .unwrap();
        connection.busy_timeout(Duration::from_millis(100)).unwrap();
        let result = connection.execute_batch("BEGIN IMMEDIATE");
        if std::env::var_os("EJM_CAPACITY_WRITER_BLOCKED").is_some() {
            assert!(matches!(
                result,
                Err(rusqlite::Error::SqliteFailure(error, _))
                    if error.code == rusqlite::ErrorCode::DatabaseBusy
            ));
        } else {
            result.unwrap();
            connection.execute_batch("ROLLBACK").unwrap();
        }
    }

    #[cfg(unix)]
    fn assert_subprocess_writer(root: &Path, blocked: bool) {
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "capacity::registry::tests::sqlite_writer_subprocess_fixture",
                "--nocapture",
            ])
            .env("EJM_CAPACITY_WRITER_FIXTURE", root)
            .env_remove("EJM_CAPACITY_WRITER_BLOCKED")
            .stdout(std::process::Stdio::null());
        if blocked {
            command.env("EJM_CAPACITY_WRITER_BLOCKED", "1");
        }
        let mut children = FixtureChildren(vec![command.spawn().unwrap()]);
        let until = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(status) = children.0[0].try_wait().unwrap() {
                assert!(status.success(), "SQLite writer exclusion fixture failed");
                break;
            }
            assert!(std::time::Instant::now() < until, "SQLite writer timed out");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[cfg(unix)]
    #[test]
    fn validation_and_peer_drop_preserve_cross_process_wal_writer_exclusion() {
        let (_parent, root) = root();
        let registry = Registry::open(&root).unwrap();
        let peer = Registry::open(&root).unwrap();
        registry
            .connection
            .execute_batch("BEGIN IMMEDIATE")
            .unwrap();
        peer.files.verify().unwrap();
        drop(peer);
        assert_subprocess_writer(&root, true);
        registry.connection.execute_batch("ROLLBACK").unwrap();
        assert_subprocess_writer(&root, false);
    }

    #[cfg(unix)]
    #[test]
    fn failed_open_preserves_cross_process_rollback_writer_exclusion() {
        let (_parent, root) = root();
        let registry = Registry::open(&root).unwrap();
        registry
            .connection
            .pragma_update(None, "journal_mode", "DELETE")
            .unwrap();
        registry
            .connection
            .execute_batch("BEGIN IMMEDIATE")
            .unwrap();
        assert!(Registry::open(&root).is_err());
        assert_subprocess_writer(&root, true);
        registry.connection.execute_batch("ROLLBACK").unwrap();
        assert_subprocess_writer(&root, false);
    }

    #[test]
    fn real_processes_register_concurrently_and_share_one_envelope() {
        let (parent, root) = root();
        let parent_path = parent.path().canonicalize().unwrap();
        let mut children = FixtureChildren(Vec::new());
        for index in 0..2 {
            children.0.push(
                std::process::Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "capacity::registry::tests::registry_subprocess_fixture",
                        "--nocapture",
                    ])
                    .env("EJM_CAPACITY_REGISTRY_FIXTURE", &parent_path)
                    .env("EJM_CAPACITY_REGISTRY_CHILD", index.to_string())
                    .stdout(std::process::Stdio::null())
                    .spawn()
                    .unwrap(),
            );
        }
        std::fs::write(parent_path.join("start"), b"start").unwrap();
        for index in 0..2 {
            wait_file(&parent_path.join(format!("{index}.ready")));
        }
        let registry = Registry::open(&root).unwrap();
        assert_eq!(
            count(&registry.connection, "SELECT COUNT(*) FROM owners").unwrap(),
            2
        );
        let total = charged_others(&registry.connection, "host", "").unwrap();
        assert_eq!(
            total.min(domain_budget(&registry.connection, "host").unwrap()),
            total
        );
        std::fs::write(parent_path.join("finish"), b"finish").unwrap();
        for child in &mut children.0 {
            assert!(child.wait().unwrap().success());
        }
        assert_eq!(
            count(&registry.connection, "SELECT COUNT(*) FROM owners").unwrap(),
            0
        );
    }

    #[test]
    fn actual_process_death_retires_only_its_allowance() {
        let (parent, root) = root();
        let parent_path = parent.path().canonicalize().unwrap();
        let mut children = FixtureChildren(vec![
            std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "capacity::registry::tests::registry_subprocess_fixture",
                    "--nocapture",
                ])
                .env("EJM_CAPACITY_REGISTRY_FIXTURE", &parent_path)
                .env("EJM_CAPACITY_REGISTRY_CHILD", "death")
                .stdout(std::process::Stdio::null())
                .spawn()
                .unwrap(),
        ]);
        std::fs::write(parent_path.join("start"), b"start").unwrap();
        wait_file(&parent_path.join("death.ready"));
        let owner = format!("child-{}", children.0[0].id());
        let mut observer = Registry::open(&root).unwrap();
        observer
            .register("live", requested(1_000), &observations(5_000))
            .unwrap();
        observer
            .connection
            .execute("UPDATE owners SET heartbeat=0 WHERE id=?1", [&owner])
            .unwrap();
        provider_registry::initialize(&observer.connection).unwrap();
        let prepaid = provider_registry::allocate(
            &mut observer.connection,
            &quota_demand(&owner),
            timestamp_ms(),
        )
        .unwrap();
        assert_eq!(prepaid.request_credit, Some(1));
        assert_eq!(prepaid.token_credit, Some(100));
        assert_eq!(
            recover(&observer.connection, &observer.files, sql_time().unwrap()).unwrap(),
            0
        );
        children.0[0].kill().unwrap();
        children.0[0].wait().unwrap();
        assert_eq!(
            recover(&observer.connection, &observer.files, sql_time().unwrap()).unwrap(),
            1
        );
        assert_eq!(
            count(&observer.connection, "SELECT COUNT(*) FROM owners").unwrap(),
            1
        );
        assert!(charge(&observer, "live", "host").memory > 0);
        assert_eq!(
            count(
                &observer.connection,
                "SELECT COUNT(*) FROM provider_domains"
            )
            .unwrap(),
            1
        );
        let retained = provider_registry::allocate(
            &mut observer.connection,
            &quota_demand("live"),
            timestamp_ms(),
        )
        .unwrap();
        assert_eq!(retained.request_credit, Some(0));
        assert_eq!(retained.token_credit, Some(0));
        assert_eq!(
            count(&observer.connection, "SELECT COUNT(*) FROM owner_witnesses").unwrap(),
            1
        );
    }
}
