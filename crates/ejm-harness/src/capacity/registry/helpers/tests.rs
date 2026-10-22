#![allow(
    clippy::unwrap_used,
    reason = "Helper fixtures assert the coarse commitment and witness boundary."
)]

use std::{fs::OpenOptions, os::unix::fs::PermissionsExt, path::Path};

use ejm_harness_exec::{GovernorSnapshot, OperatingTarget, ResourceDomainObservation};

use super::*;
use crate::capacity::registry::{
    CONTACT_TIME_MS, PROTOCOL, domain_budget, prune_domains, recover_with_boot, shared_reason,
    tests::{charge, force_control, observations, quota_demand, requested, root},
};

mod staged;

fn ownership(root: &Path, domain: &str) -> File {
    let native = root.join("native");
    let directory = native.join(domain);
    std::fs::create_dir_all(&directory).unwrap();
    for path in [&native, &directory] {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let path = directory.join("owner.lock");
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
    file.try_lock().unwrap();
    file
}

fn nested_observations(memory: u64, processes: u64) -> HostObservations {
    let mut observed = observations(8_000);
    observed.domains.push(ResourceDomainObservation {
        id: "constrained".into(),
        parent: Some("host".into()),
        cpu_millis: Some(8_000),
        memory_bytes: Some(memory),
        available_memory_bytes: None,
        max_processes: Some(processes),
        available_processes: None,
    });
    observed
}

fn admit_generation(
    registry: &mut Registry,
    domain: &str,
    generation: Uuid,
    observed: &HostObservations,
) -> Result<bool> {
    super::admit_generation(registry, domain, generation, None, observed)
}

fn renew(
    registry: &mut Registry,
    owner: &str,
    requested: Budget,
    observed: &HostObservations,
    used: &GovernorSnapshot,
) -> Budget {
    force_control(registry);
    registry
        .renew_with_floor(
            owner,
            requested,
            0,
            0,
            used,
            requested.operating(),
            OperatingTarget {
                memory_bytes: BASELINE_MEMORY,
                ..OperatingTarget::default()
            },
            0,
            observed,
            || Ok(()),
            |_| Ok((used.clone(), 0)),
        )
        .unwrap()
        .granted
}

#[test]
fn pending_helper_reclaims_saturated_grants_only_after_drain() {
    let (_parent, root) = root();
    let observed = nested_observations(512 * 1024 * 1024, 16);
    let requested = Budget {
        memory: 256 * 1024 * 1024,
        processes: 8,
        ..requested(1_000)
    };
    let mut first = Registry::open(&root).unwrap();
    let mut second = Registry::open(&root).unwrap();
    first.register("first", requested, &observed).unwrap();
    second.register("second", requested, &observed).unwrap();
    for _ in 0..2 {
        renew(
            &mut first,
            "first",
            requested,
            &observed,
            &GovernorSnapshot::default(),
        );
        renew(
            &mut second,
            "second",
            requested,
            &observed,
            &GovernorSnapshot::default(),
        );
    }
    assert_eq!(
        charged_others(&first.connection, "constrained", "")
            .unwrap()
            .memory,
        512 * 1024 * 1024
    );
    let domain = crate::capacity::execution_domain(&observed);
    let _ownership = ownership(&root, &domain);
    let generation = Uuid::new_v4();
    let mut helper = Registry::open(&root).unwrap();
    assert!(
        !super::admit_generation(&mut helper, &domain, generation, Some("first"), &observed)
            .unwrap()
    );
    assert_eq!(snapshot(&helper.connection).unwrap().pending, 1);
    let held = GovernorSnapshot {
        queued: 1,
        reserved_memory_bytes: requested.memory,
        reserved_processes: requested.processes,
        ..GovernorSnapshot::default()
    };
    renew(&mut first, "first", requested, &observed, &held);
    let reduced = renew(&mut second, "second", requested, &observed, &held);
    assert!(reduced.memory < requested.memory || reduced.processes < requested.processes);
    assert_eq!(
        charge(&second, "second", "constrained").memory,
        requested.memory
    );
    assert!(
        !super::admit_generation(&mut helper, &domain, generation, Some("first"), &observed)
            .unwrap()
    );
    let drained = GovernorSnapshot {
        queued: 1,
        reserved_memory_bytes: BASELINE_MEMORY,
        ..GovernorSnapshot::default()
    };
    renew(&mut first, "first", requested, &observed, &drained);
    renew(&mut second, "second", requested, &observed, &drained);
    assert!(
        super::admit_generation(&mut helper, &domain, generation, Some("first"), &observed)
            .unwrap()
    );
    let total = charged_others(&helper.connection, "constrained", "").unwrap();
    let envelope = domain_budget(&helper.connection, "constrained").unwrap();
    assert_eq!(total.min(envelope), total);
    assert_eq!(
        charged(&helper.connection, "constrained").unwrap(),
        helper_budget()
    );
}

#[test]
fn one_generation_is_charged_once_and_survives_its_creator() {
    let (_parent, root) = root();
    let observed = nested_observations(512 * 1024 * 1024, 16);
    let mut creator = Registry::open(&root).unwrap();
    assert_eq!(snapshot(&creator.connection).unwrap().occupied, 0);
    creator
        .register(
            "creator",
            Budget {
                memory: 128 * 1024 * 1024,
                processes: 4,
                ..requested(500)
            },
            &observed,
        )
        .unwrap();
    let _ownership = ownership(&root, "shared");
    let generation = Uuid::new_v4();
    let mut helper = Registry::open(&root).unwrap();
    assert!(admit_generation(&mut helper, "shared", generation, &observed).unwrap());
    assert!(admit_generation(&mut helper, "shared", generation, &observed).unwrap());
    creator.remove_uninstalled().unwrap();
    prune_domains(&helper.connection).unwrap();
    assert_eq!(
        count(&helper.connection, "SELECT COUNT(*) FROM owners").unwrap(),
        0
    );
    assert_eq!(snapshot(&helper.connection).unwrap().committed, 1);
    for domain in ["host", "constrained"] {
        assert_eq!(
            charged(&helper.connection, domain).unwrap(),
            helper_budget()
        );
        assert_eq!(
            domain_budget(&helper.connection, domain).unwrap().models,
            256
        );
    }
    assert_eq!(
        recover_with_boot(&helper.connection, &helper.files, i64::MAX, None).unwrap(),
        0
    );
    assert_eq!(
        snapshot(&helper.connection).unwrap().memory_bytes,
        BASELINE_MEMORY
    );
}

#[test]
fn helper_slots_include_pending_generations_and_release_only_on_proof() {
    let (_parent, root) = root();
    let observed = nested_observations(BASELINE_MEMORY, u64::from(HELPER_PROCESSES));
    let mut registry = Registry::open(&root).unwrap();
    let mut witnesses = Vec::new();
    for index in 0..MAX_HELPERS {
        let domain = format!("helper-{index}");
        witnesses.push(ownership(&root, &domain));
        let committed =
            admit_generation(&mut registry, &domain, Uuid::new_v4(), &observed).unwrap();
        assert_eq!(committed, index == 0);
    }
    let status = snapshot(&registry.connection).unwrap();
    assert_eq!(status.occupied, MAX_HELPERS);
    assert_eq!(status.committed, 1);
    assert_eq!(status.pending, MAX_HELPERS - 1);
    let _ninth = ownership(&root, "ninth");
    assert!(admit_generation(&mut registry, "ninth", Uuid::new_v4(), &observed).is_err());
    assert_eq!(
        recover_with_boot(&registry.connection, &registry.files, i64::MAX, None).unwrap(),
        0
    );
    drop(witnesses.remove(1));
    assert_eq!(
        recover_with_boot(&registry.connection, &registry.files, i64::MAX, None).unwrap(),
        1
    );
    assert_eq!(
        recover_with_boot(&registry.connection, &registry.files, i64::MAX, None).unwrap(),
        0
    );
    assert!(!admit_generation(&mut registry, "ninth", Uuid::new_v4(), &observed).unwrap());
    assert_eq!(
        snapshot(&registry.connection).unwrap().occupied,
        MAX_HELPERS
    );
}

#[test]
fn same_lock_replacement_is_atomic_and_does_not_double_charge() {
    let (_parent, root) = root();
    let observed = observations(8_000);
    let mut registry = Registry::open(&root).unwrap();
    let first_witness = ownership(&root, "shared");
    let first = Uuid::new_v4();
    assert!(admit_generation(&mut registry, "shared", first, &observed).unwrap());
    drop(first_witness);
    let second_witness = OpenOptions::new()
        .read(true)
        .write(true)
        .open(root.join("native/shared/owner.lock"))
        .unwrap();
    second_witness.try_lock().unwrap();
    let second = Uuid::new_v4();
    assert!(admit_generation(&mut registry, "shared", second, &observed).unwrap());
    assert_eq!(snapshot(&registry.connection).unwrap().occupied, 1);
    assert_eq!(
        charged(&registry.connection, "host").unwrap(),
        helper_budget()
    );
    let stored: String = registry
        .connection
        .query_row("SELECT generation FROM native_helpers", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(stored, second.to_string());
}

#[test]
fn helper_limit_decrease_keeps_committed_drain_debt() {
    let (_parent, root) = root();
    let observed = nested_observations(BASELINE_MEMORY * 2, 8);
    let mut registry = Registry::open(&root).unwrap();
    let _ownership = ownership(&root, "shared");
    let generation = Uuid::new_v4();
    assert!(admit_generation(&mut registry, "shared", generation, &observed).unwrap());
    let tighter = nested_observations(BASELINE_MEMORY / 2, 0);
    assert!(admit_generation(&mut registry, "shared", generation, &tighter).unwrap());
    assert_eq!(
        charged(&registry.connection, "constrained").unwrap(),
        helper_budget()
    );
    assert!(
        shared_reason(&registry.connection)
            .unwrap()
            .unwrap()
            .contains("debt")
    );
    let _other = ownership(&root, "other");
    assert!(!admit_generation(&mut registry, "other", Uuid::new_v4(), &tighter).unwrap());
    assert_eq!(
        charged(&registry.connection, "constrained").unwrap(),
        helper_budget()
    );
}

#[test]
fn replaced_witness_never_reclaims_a_live_helper_charge() {
    let (_parent, root) = root();
    let mut registry = Registry::open(&root).unwrap();
    let _ownership = ownership(&root, "shared");
    assert!(
        admit_generation(
            &mut registry,
            "shared",
            Uuid::new_v4(),
            &observations(8_000)
        )
        .unwrap()
    );
    let path = root.join("native/shared/owner.lock");
    std::fs::rename(&path, path.with_file_name("replaced.lock")).unwrap();
    let replacement = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
    drop(replacement);
    assert!(recover_with_boot(&registry.connection, &registry.files, i64::MAX, None).is_err());
    assert_eq!(snapshot(&registry.connection).unwrap().committed, 1);
}

#[test]
fn verified_boot_change_retires_helpers_without_refunding_provider_windows() {
    let (_parent, root) = root();
    let observed = observations(8_000);
    let mut registry = Registry::open(&root).unwrap();
    registry
        .register("provider", requested(500), &observed)
        .unwrap();
    let now = crate::types::timestamp_ms();
    crate::capacity::provider_registry::initialize(&registry.connection).unwrap();
    let prepaid = crate::capacity::provider_registry::allocate(
        &mut registry.connection,
        &quota_demand("provider"),
        now,
    )
    .unwrap();
    assert_eq!(prepaid.request_credit, Some(1));
    assert_eq!(prepaid.token_credit, Some(100));
    let before: String = registry
        .connection
        .query_row("SELECT payload FROM provider_domains", [], |row| row.get(0))
        .unwrap();
    let _ownership = ownership(&root, "shared");
    assert!(admit_generation(&mut registry, "shared", Uuid::new_v4(), &observed).unwrap());
    registry
        .connection
        .execute("UPDATE native_helpers SET boot='previous-boot'", [])
        .unwrap();
    recover_with_boot(
        &registry.connection,
        &registry.files,
        CONTACT_TIME_MS,
        Some("current-boot"),
    )
    .unwrap();
    assert_eq!(snapshot(&registry.connection).unwrap().occupied, 0);
    let after: String = registry
        .connection
        .query_row("SELECT payload FROM provider_domains", [], |row| row.get(0))
        .unwrap();
    assert_eq!(before, after);
}

#[test]
fn protocol_upgrade_rejects_live_owners_and_preopened_old_registration() {
    let (_parent, root) = root();
    let observed = observations(8_000);
    let mut registry = Registry::open(&root).unwrap();
    registry.register("old", requested(500), &observed).unwrap();
    registry
        .connection
        .pragma_update(None, "user_version", 3)
        .unwrap();
    assert!(Registry::open(&root).is_err());
    registry.remove_uninstalled().unwrap();
    registry
        .connection
        .execute_batch(
            "DROP TABLE native_helpers;
         CREATE TABLE native_helpers(generation TEXT PRIMARY KEY,domain_key TEXT UNIQUE,
         boot TEXT,file_key TEXT,committed INTEGER);",
        )
        .unwrap();
    let old = Connection::open(root.join("capacity.sqlite3")).unwrap();
    let migrated = Registry::open(&root).unwrap();
    let version: u32 = migrated
        .connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, PROTOCOL);
    assert!(old.execute(
        "INSERT INTO owners(id,boot,pid,birth,heartbeat,revision,requested,grant_json,usage_json,working_demand,
         domain_ids,native_demand,model_demand,warm_until,last_native,last_model,state,unsafe_scope,plan_json)
         VALUES('stale','boot',1,1,1,0,'{}','{}','{}','{}','[]',0,0,0,0,0,'active',0,'{}')",
        [],
    ).is_err());
    assert!(
        old.execute(
            "INSERT INTO native_helpers VALUES('old','shared','boot','key',0)",
            [],
        )
        .is_err()
    );
    assert_eq!(
        count(&migrated.connection, "SELECT COUNT(*) FROM owners").unwrap(),
        0
    );
}

#[test]
fn protocol_upgrade_recovers_a_provably_released_old_owner() {
    let (_parent, root) = root();
    let observed = observations(8_000);
    let mut registry = Registry::open(&root).unwrap();
    registry
        .register("crashed", requested(500), &observed)
        .unwrap();
    registry
        .connection
        .pragma_update(None, "user_version", 3)
        .unwrap();
    registry
        .connection
        .execute("UPDATE owners SET heartbeat=0", [])
        .unwrap();
    registry.witness = None;
    let recovered = Registry::open(&root).unwrap();
    assert_eq!(
        count(&recovered.connection, "SELECT COUNT(*) FROM owners").unwrap(),
        0
    );
    let version: u32 = recovered
        .connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, PROTOCOL);
}
