//! Coarse helper generations, independent of any participating client.

use std::{collections::BTreeMap, fs::File};

use anyhow::{Context, Result, ensure};
use ejm_harness_exec::{HostObservations, HostSampler, native::NativeOwnerRequest};
use rusqlite::{Connection, OptionalExtension, params};
use uuid::Uuid;

use super::{
    BASELINE_MEMORY, Budget, CONTACT_TIME_MS, PrivateFiles, Registry, charged_others, count,
    domain_budget, file_key, install_domains, observed_domains, path_key, read_owners,
    recover as recover_all, sql_time, table_empty_if_present,
};
use crate::capacity::NativeHelperSnapshot;

const MAX_HELPERS: usize = 8;
const HELPER_PROCESSES: u32 = if cfg!(target_os = "linux") { 3 } else { 1 };

#[cfg(all(test, unix))]
mod tests;

pub(super) fn empty_if_present(connection: &Connection) -> Result<bool> {
    Ok(table_empty_if_present(connection, "native_helpers")?
        && table_empty_if_present(connection, "native_helper_allocations")?)
}

pub(super) fn create_schema(connection: &Connection) -> Result<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS native_helpers(
          generation TEXT PRIMARY KEY CHECK(length(generation)=36),
          domain_key TEXT NOT NULL UNIQUE CHECK(length(domain_key) BETWEEN 1 AND 128),
          boot TEXT NOT NULL CHECK(length(boot) BETWEEN 1 AND 128),
          file_key TEXT NOT NULL CHECK(length(file_key) BETWEEN 1 AND 128),
          committed INTEGER NOT NULL CHECK(committed IN(0,1)),
          claimant TEXT CHECK(length(claimant) BETWEEN 1 AND 128),
          selected_epoch INTEGER NOT NULL CHECK(selected_epoch>=0),
          selected_minimum TEXT NOT NULL CHECK(length(selected_minimum)<=1024),
          selected_owner TEXT CHECK(length(selected_owner) BETWEEN 1 AND 128));
         CREATE TABLE IF NOT EXISTS native_helper_allocations(
          generation TEXT NOT NULL, domain TEXT NOT NULL, budget TEXT NOT NULL CHECK(length(budget)<=1024),
          PRIMARY KEY(generation,domain));
         CREATE INDEX IF NOT EXISTS native_helper_domain ON native_helper_allocations(domain,generation);",
    )?;
    Ok(())
}

pub(super) fn admit(
    registry: &mut Registry,
    owner: &str,
    request: &NativeOwnerRequest,
    observations: &HostObservations,
) -> Result<bool> {
    let root = registry
        .files
        .database
        .parent()
        .context("capacity state root missing")?;
    ensure!(
        std::path::absolute(request.state_root())? == root,
        "native helper uses a different capacity registry"
    );
    ensure!(
        request.lifetime_path()
            == root
                .join("native")
                .join(request.domain_key())
                .join("owner.lock"),
        "native helper lifetime path differs from its stable ownership identity"
    );
    admit_generation(
        registry,
        request.domain_key(),
        request.generation(),
        Some(owner),
        observations,
    )
}

fn admit_generation(
    registry: &mut Registry,
    domain_key: &str,
    generation: Uuid,
    claimant: Option<&str>,
    observations: &HostObservations,
) -> Result<bool> {
    let domains = observed_domains(observations)?;
    registry.files.verify()?;
    let witness = open_witness(&registry.files, domain_key)?;
    let key = witness_key(&witness, &registry.files, domain_key)?;
    ensure!(
        matches!(witness.try_lock(), Err(std::fs::TryLockError::WouldBlock)),
        "native admission requires the executor's retained exclusive ownership witness"
    );
    let boot = HostSampler::boot_identity();
    let generation = generation.to_string();
    let now = sql_time()?;
    let tx = registry
        .connection
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    recover_all(&tx, &registry.files, now)?;
    let existing = tx
        .query_row(
            "SELECT generation,file_key,committed,claimant FROM native_helpers WHERE domain_key=?1",
            [domain_key],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, bool>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            },
        )
        .optional()?;
    let committed = if let Some((old, old_key, committed, recorded_claimant)) = existing {
        ensure!(
            key == old_key,
            "native helper lifetime witness was replaced"
        );
        if old == generation {
            ensure!(
                committed || recorded_claimant.as_deref() == claimant,
                "native helper startup claimant changed"
            );
            let recorded = allocation_domains(&tx, &generation)?;
            ensure!(
                recorded.len() == domains.len()
                    && domains.iter().all(|domain| recorded.contains(&domain.id)),
                "native helper ancestry changed during startup"
            );
            committed
        } else {
            // Only the executor can construct this request, after acquiring
            // this exact stable lock. That proves the earlier generation ended.
            remove(&tx, &old)?;
            insert(
                &tx,
                domain_key,
                &generation,
                &key,
                boot.as_deref(),
                claimant,
                &domains,
            )?;
            false
        }
    } else {
        insert(
            &tx,
            domain_key,
            &generation,
            &key,
            boot.as_deref(),
            claimant,
            &domains,
        )?;
        false
    };
    install_domains(&tx, &domains)?;
    let selected = committed || selected_beneficiary(&tx, &generation, claimant, &domains, now)?;
    let fits = committed
        || selected
            && domains.iter().try_fold(true, |fits, domain| {
                let envelope = domain_budget(&tx, &domain.id)?;
                let total = charged_others(&tx, &domain.id, "")?.add(helper_budget());
                Ok::<_, anyhow::Error>(fits && total.min(envelope) == total)
            })?;
    if fits && !committed {
        tx.execute(
            "UPDATE native_helpers SET committed=1 WHERE generation=?1 AND committed=0",
            [&generation],
        )?;
        // This epoch already reserved the selected helper allowance alongside
        // its useful minimum. Committing it preserves every sibling selection.
    }
    witness_key(&witness, &registry.files, domain_key)?;
    tx.commit()?;
    registry.files.verify()?;
    Ok(fits)
}

fn insert(
    connection: &Connection,
    domain_key: &str,
    generation: &str,
    file_key: &str,
    boot: Option<&str>,
    claimant: Option<&str>,
    domains: &[super::ObservedDomain],
) -> Result<()> {
    ensure!(
        count(connection, "SELECT COUNT(*) FROM native_helpers")? < MAX_HELPERS,
        "shared native helper pool is full; existing generations must drain"
    );
    connection.execute(
        "INSERT INTO native_helpers VALUES(?1,?2,?3,?4,0,?5,0,?6,NULL)",
        params![
            generation,
            domain_key,
            boot.unwrap_or("unavailable"),
            file_key,
            claimant,
            serde_json::to_string(&Budget::default())?
        ],
    )?;
    let budget = serde_json::to_string(&helper_budget())?;
    for domain in domains {
        connection.execute(
            "INSERT INTO native_helper_allocations VALUES(?1,?2,?3)",
            params![generation, domain.id, budget],
        )?;
    }
    // A first native request must shrink participant plans, even while its
    // baseline still waits for their committed grants to drain.
    connection.execute("UPDATE control SET at=0 WHERE id=1", [])?;
    Ok(())
}

pub(super) fn helper_budget() -> Budget {
    Budget {
        memory: BASELINE_MEMORY,
        processes: HELPER_PROCESSES,
        ..Budget::default()
    }
}

pub(super) struct PendingHelper {
    pub generation: String,
    pub domain_key: String,
    pub domains: Vec<String>,
}

pub(super) fn prepare_plans(
    connection: &Connection,
    remaining: &mut BTreeMap<String, Budget>,
) -> Result<Vec<PendingHelper>> {
    let mut statement = connection.prepare(
        "SELECT domain,budget FROM native_helper_allocations JOIN native_helpers USING(generation)
         WHERE committed=1 LIMIT 513",
    )?;
    let mut records = 0;
    for row in statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })? {
        records += 1;
        ensure!(
            records <= MAX_HELPERS * super::MAX_DOMAINS,
            "native helper ancestry exceeds its bound"
        );
        let (domain, encoded) = row?;
        if let Some(budget) = remaining.get_mut(&domain) {
            *budget = budget.sub(serde_json::from_str(&encoded)?);
        }
    }
    drop(statement);
    // Selection and participant plans share one control transaction. A pending
    // generation occupies its bounded slot without becoming an actual charge.
    connection.execute(
        "UPDATE native_helpers SET selected_epoch=0,selected_owner=NULL WHERE committed=0",
        [],
    )?;
    let mut statement = connection.prepare(
        "SELECT generation,domain_key FROM native_helpers WHERE committed=0 ORDER BY generation LIMIT 9",
    )?;
    let records = statement
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    ensure!(
        records.len() <= MAX_HELPERS,
        "native helper pool exceeds its bound"
    );
    records
        .into_iter()
        .map(|(generation, domain_key)| {
            let domains = allocation_domains(connection, &generation)?;
            Ok(PendingHelper {
                generation,
                domain_key,
                domains,
            })
        })
        .collect()
}

pub(super) fn select(
    connection: &Connection,
    helper: &PendingHelper,
    owner: &str,
    epoch: i64,
    minimum: Budget,
) -> Result<()> {
    connection.execute(
        "UPDATE native_helpers SET selected_epoch=?2,selected_minimum=?3,selected_owner=?4 WHERE generation=?1 AND committed=0",
        params![helper.generation, epoch, serde_json::to_string(&minimum)?, owner],
    )?;
    Ok(())
}

fn selected_beneficiary(
    connection: &Connection,
    generation: &str,
    claimant: Option<&str>,
    domains: &[super::ObservedDomain],
    now: i64,
) -> Result<bool> {
    let Some(claimant) = claimant else {
        // Witness-only fixtures isolate helper accounting without a job owner.
        // Production admission always binds a participant nonce.
        return Ok(cfg!(test));
    };
    let (selected, encoded, owner_id, native_domain): (i64, String, Option<String>, String) = connection.query_row(
        "SELECT selected_epoch,selected_minimum,selected_owner,domain_key FROM native_helpers WHERE generation=?1 AND claimant=?2",
        params![generation, claimant],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    )?;
    let (at, epoch): (i64, i64) =
        connection.query_row("SELECT at,epoch FROM control WHERE id=1", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?;
    if selected == 0 || selected != epoch || at == 0 {
        return Ok(false);
    }
    let owners = read_owners(connection, now)?;
    if owners.iter().any(|owner| owner.unsafe_scope) {
        return Ok(false);
    }
    let Some(owner) = owners
        .iter()
        .find(|owner| Some(&owner.id) == owner_id.as_ref())
    else {
        return Ok(false);
    };
    if owner.state != "active"
        || owner.unsafe_scope
        || !owner.demand.queued
        || owner.native_domain != native_domain
        || owner.domains.len() != domains.len()
        || domains
            .iter()
            .any(|domain| !owner.domains.contains(&domain.id))
    {
        return Ok(false);
    }
    let (heartbeat, grant): (i64, String) = connection.query_row(
        "SELECT heartbeat,grant_json FROM owners WHERE id=?1",
        [&owner.id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let minimum: Budget = serde_json::from_str(&encoded)?;
    let grant: Budget = serde_json::from_str(&grant)?;
    let current = owner.minimum(connection)?;
    Ok(now >= heartbeat
        && now.saturating_sub(heartbeat) <= CONTACT_TIME_MS
        && current.min(minimum) == current
        && minimum.min(grant) == minimum)
}

pub(super) fn charged(connection: &Connection, domain: &str) -> Result<Budget> {
    let mut statement = connection.prepare(
        "SELECT budget FROM native_helper_allocations JOIN native_helpers USING(generation)
         WHERE domain=?1 AND committed=1 LIMIT 9",
    )?;
    let mut total = Budget::default();
    let mut records = 0;
    for row in statement.query_map([domain], |row| row.get::<_, String>(0))? {
        records += 1;
        ensure!(
            records <= MAX_HELPERS,
            "native helper pool exceeds its bound"
        );
        total = total.add(serde_json::from_str(&row?)?);
    }
    Ok(total)
}

pub(super) fn snapshot(connection: &Connection) -> Result<NativeHelperSnapshot> {
    let (occupied, committed): (i64, i64) = connection.query_row(
        "SELECT COUNT(*),COALESCE(SUM(committed),0) FROM native_helpers",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let occupied = usize::try_from(occupied)?;
    let committed = usize::try_from(committed)?;
    ensure!(
        occupied <= MAX_HELPERS,
        "native helper pool exceeds its bound"
    );
    Ok(NativeHelperSnapshot {
        occupied,
        pending: occupied.saturating_sub(committed),
        committed,
        memory_bytes: u64::try_from(committed)?.saturating_mul(BASELINE_MEMORY),
        processes: u32::try_from(committed)?.saturating_mul(HELPER_PROCESSES),
    })
}

pub(super) fn recover(
    connection: &Connection,
    files: &PrivateFiles,
    boot: Option<&str>,
) -> Result<usize> {
    let mut statement = connection
        .prepare("SELECT generation,domain_key,boot,file_key FROM native_helpers LIMIT 9")?;
    let records = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    ensure!(
        records.len() <= MAX_HELPERS,
        "native helper recovery exceeds its bound"
    );
    drop(statement);
    let mut retired = 0;
    for (generation, domain, recorded_boot, recorded_key) in records {
        if boot.is_some_and(|current| recorded_boot != "unavailable" && recorded_boot != current) {
            remove(connection, &generation)?;
            retired += 1;
            continue;
        }
        let witness = open_witness(files, &domain)?;
        ensure!(
            witness_key(&witness, files, &domain)? == recorded_key,
            "native helper lifetime witness was replaced"
        );
        match witness.try_lock() {
            Ok(()) => {
                remove(connection, &generation)?;
                retired += 1;
            }
            Err(std::fs::TryLockError::WouldBlock) => {}
            Err(std::fs::TryLockError::Error(error)) => return Err(error.into()),
        }
    }
    Ok(retired)
}

fn remove(connection: &Connection, generation: &str) -> Result<()> {
    connection.execute(
        "DELETE FROM native_helper_allocations WHERE generation=?1",
        [generation],
    )?;
    connection.execute(
        "DELETE FROM native_helpers WHERE generation=?1",
        [generation],
    )?;
    connection.execute("UPDATE control SET at=0 WHERE id=1", [])?;
    Ok(())
}

fn allocation_domains(connection: &Connection, generation: &str) -> Result<Vec<String>> {
    let mut statement = connection
        .prepare("SELECT domain FROM native_helper_allocations WHERE generation=?1 LIMIT 65")?;
    let domains = statement
        .query_map([generation], |row| row.get(0))?
        .collect::<rusqlite::Result<Vec<String>>>()?;
    ensure!(
        !domains.is_empty() && domains.len() <= super::MAX_DOMAINS,
        "invalid native helper ancestry"
    );
    Ok(domains)
}

fn witness_key(file: &File, files: &PrivateFiles, domain: &str) -> Result<String> {
    let path = files
        .database
        .with_file_name("native")
        .join(domain)
        .join("owner.lock");
    let (device, inode) = file_key(file, &path)?;
    ensure!(
        path_key(&path)? == (device, inode),
        "native helper lifetime witness identity changed"
    );
    Ok(format!("{device}:{inode}"))
}

fn open_witness(files: &PrivateFiles, domain: &str) -> Result<File> {
    ensure!(
        !domain.is_empty()
            && domain.len() <= 128
            && domain
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-'),
        "invalid native helper domain"
    );
    #[cfg(target_os = "macos")]
    {
        let path = files.database.with_file_name("native").join(domain);
        let directory = rightkit_fs::StateDir::open(&path)?;
        rightkit_fs::private::verify_owner_only(&path)?;
        let file = directory.open_read(
            "owner.lock",
            &rightkit_fs::handle::ReadOptions::new().require_single_link(true),
        )?;
        rightkit_fs::private::verify_owner_only(&path.join("owner.lock"))?;
        ensure!(
            file.metadata()?.is_file() && file.metadata()?.len() == 0,
            "native helper witness must remain an empty regular file"
        );
        Ok(file)
    }
    #[cfg(target_os = "linux")]
    {
        use rustix::fs::{Mode, OFlags};
        use std::os::unix::fs::MetadataExt;
        let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
        let native = File::from(rustix::fs::openat(
            &files.directory,
            "native",
            flags,
            Mode::empty(),
        )?);
        let directory = File::from(rustix::fs::openat(&native, domain, flags, Mode::empty())?);
        for directory in [&native, &directory] {
            let metadata = directory.metadata()?;
            ensure!(
                metadata.mode() & 0o077 == 0
                    && metadata.uid() == rustix::process::geteuid().as_raw(),
                "native helper directories must remain owner-only"
            );
        }
        let file = File::from(rustix::fs::openat(
            &directory,
            "owner.lock",
            OFlags::RDWR | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
            Mode::empty(),
        )?);
        let metadata = file.metadata()?;
        ensure!(
            metadata.is_file() && metadata.len() == 0 && metadata.nlink() == 1,
            "native helper witness must remain an empty regular file with one link"
        );
        ensure!(
            metadata.mode() & 0o077 == 0 && metadata.uid() == rustix::process::geteuid().as_raw(),
            "native helper witness must remain owner-only"
        );
        Ok(file)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = files;
        anyhow::bail!("shared native helper lifetime witnesses are unsupported")
    }
}
