#![allow(
    clippy::unwrap_used,
    reason = "Capacity fixtures assert owned admission boundaries."
)]

use super::*;

fn fresh_observation<T>(value: T) -> ejm_harness_exec::Observation<T> {
    ejm_harness_exec::Observation {
        value: Some(value),
        source: ejm_harness_exec::ObservationSource::NativeSystem,
        availability: ObservationAvailability::Measured,
        age_ms: 0,
        max_age_ms: 5_000,
    }
}

fn healthy_observations() -> HostObservations {
    HostObservations {
        sampled_at: Instant::now(),
        cpu_millis: fresh_observation(4_000),
        cpu_usage_percent: fresh_observation(25.0),
        memory_bytes: fresh_observation(8 * 1024 * 1024 * 1024),
        available_memory_bytes: fresh_observation(4 * 1024 * 1024 * 1024),
        process_limit: fresh_observation(1_024),
        descriptor_limit: fresh_observation(1_024),
        domains: Vec::new(),
        domain_visibility: ObservationAvailability::Measured,
    }
}

#[test]
fn resolution_preserves_every_explicit_ceiling_in_both_modes() {
    for mode in [CapacityMode::Fixed, CapacityMode::Auto] {
        let mut config = HarnessConfig::default();
        config.resources.max_queue = 47;
        config.resources.max_artifact_bytes = 3 * 1024 * 1024;
        config.resources.max_job_ms = 7_000;
        let policy = CapacityPolicy {
            mode,
            max_sessions: Some(17),
            max_model_requests: Some(3),
            max_jobs: Some(5),
            max_memory_bytes: Some(512 * 1024 * 1024),
            max_cpu_millis: Some(1_250),
            max_heavy_jobs: Some(2),
            max_processes: Some(13),
            max_output_bytes: Some(99 * 1024),
        };
        resolve(&mut config, &policy, &healthy_observations()).unwrap();
        assert_eq!(config.max_sessions, 17);
        assert_eq!(config.max_model_requests, 3);
        assert_eq!(config.resources.max_jobs, 5);
        assert_eq!(config.resources.memory_bytes, 512 * 1024 * 1024);
        assert_eq!(config.resources.cpu_millis, 1_250);
        assert_eq!(config.resources.max_heavy_jobs, 2);
        assert_eq!(config.resources.max_processes, 13);
        assert_eq!(config.resources.max_output_bytes, 99 * 1024);
        assert_eq!(config.resources.max_queue, 47);
        assert_eq!(config.resources.max_artifact_bytes, 3 * 1024 * 1024);
        assert_eq!(config.resources.max_job_ms, 7_000);
    }
}

#[test]
fn resolution_uses_tighter_observed_cpu_and_memory_in_both_modes() {
    for mode in [CapacityMode::Fixed, CapacityMode::Auto] {
        let mut config = HarnessConfig::default();
        let mut observations = healthy_observations();
        observations.cpu_millis.value = Some(750);
        observations.memory_bytes.value = Some(512 * 1024 * 1024);
        let policy = CapacityPolicy {
            mode,
            max_memory_bytes: Some(8 * 1024 * 1024 * 1024),
            max_cpu_millis: Some(8_000),
            ..CapacityPolicy::default()
        };
        resolve(&mut config, &policy, &observations).unwrap();
        assert_eq!(config.resources.cpu_millis, 750);
        assert_eq!(config.resources.memory_bytes, 256 * 1024 * 1024);
    }
}

fn context() -> (tempfile::TempDir, CapacityContext) {
    let parent = tempfile::tempdir().unwrap();
    let root = parent.path().canonicalize().unwrap().join("capacity");
    let context = CapacityContext::prepare_in(
        &mut HarnessConfig::default(),
        CapacityPolicy::default(),
        &root,
    )
    .unwrap();
    (parent, context)
}

#[test]
fn an_infeasible_startup_floor_creates_no_shared_owner() {
    let parent = tempfile::tempdir().unwrap();
    let root = parent.path().canonicalize().unwrap().join("capacity");
    let registry = Registry::open(&root).unwrap();
    let mut config = HarnessConfig::default();
    config.resources.max_queue = 33;
    let policy = CapacityPolicy {
        max_memory_bytes: Some(BASELINE_MEMORY),
        ..CapacityPolicy::default()
    };
    let started = Instant::now();
    assert!(CapacityContext::prepare_in(&mut config, policy, &root).is_err());
    assert!(started.elapsed() < Duration::from_secs(3));
    for table in ["owners", "allocations", "owner_witnesses"] {
        let count: u32 = registry
            .connection
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0, "{table}");
    }
}

async fn wait_for_release(root: &Path) {
    let until = Instant::now() + Duration::from_secs(3);
    loop {
        let registry = Registry::open(root).unwrap();
        let count: u32 = registry
            .connection
            .query_row("SELECT COUNT(*) FROM owners", [], |row| row.get(0))
            .unwrap();
        if count == 0 {
            return;
        }
        assert!(Instant::now() < until, "capacity context did not drain");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test]
async fn dropping_context_fences_a_retained_governor() {
    let (parent, context) = context();
    let governor = context.governor();
    assert!(governor.try_acquire_memory("before", 1).unwrap().is_some());
    drop(context);
    wait_for_release(&parent.path().canonicalize().unwrap().join("capacity")).await;
    assert!(governor.try_acquire_memory("after", 1).unwrap().is_none());
    assert_eq!(
        governor.operating_snapshot().target,
        OperatingTarget::default()
    );
}

#[tokio::test]
async fn model_timing_separates_preparation_and_provider_wait_from_service() {
    let (parent, context) = context();
    let cancel = CancellationToken::new();
    let mut work = context.model_work(&cancel, Instant::now());
    work.prepare_started();
    work.preparing = Instant::now().checked_sub(Duration::from_millis(30));
    work.prepared(4_096);
    work.provider_waiting = Instant::now().checked_sub(Duration::from_secs(1));
    work.provider_started();
    work.serving = Instant::now().checked_sub(Duration::from_millis(5));
    work.complete();
    drop(work);
    let measured = context.snapshot().models;
    assert_eq!(measured.successful, 1);
    assert!(measured.preparation.p50_micros >= 30_000);
    assert!(measured.provider_wait.p50_micros >= 1_000_000);
    assert!(measured.service.p95_micros < measured.provider_wait.p50_micros);
    drop(context);
    wait_for_release(&parent.path().canonicalize().unwrap().join("capacity")).await;
}

#[cfg(unix)]
async fn cancel_waiters_during_registry_failure(
    context: &CapacityContext,
    after: Instant,
    retained: &GovernorSnapshot,
) {
    let cancel = CancellationToken::new();
    let model = {
        let context = context.clone();
        let cancel = cancel.clone();
        tokio::spawn(async move { context.acquire_model(&cancel).await.is_err() })
    };
    let tool = {
        let governor = context.governor();
        let cancel = cancel.clone();
        tokio::spawn(async move {
            governor
                .acquire(
                    "cancelled-tool",
                    ejm_harness_exec::Reservation {
                        cpu_millis: 0,
                        memory_bytes: 1,
                        processes: 0,
                        output_bytes: 0,
                        heavy: false,
                    },
                    &cancel,
                )
                .await
                .is_err()
        })
    };
    context.notify_demand();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let snapshot = context.snapshot();
            if snapshot.observations.sampled_at > after
                && snapshot.reason.is_some()
                && snapshot.model_waiting == 1
                && context.governor().snapshot().queued == 1
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    cancel.cancel();
    tokio::time::timeout(Duration::from_millis(250), async {
        assert!(model.await.unwrap());
        assert!(tool.await.unwrap());
    })
    .await
    .unwrap();
    let snapshot = context.snapshot();
    assert_eq!(snapshot.model_running, 1);
    assert_eq!(snapshot.model_waiting, 0);
    assert_eq!(context.governor().snapshot(), *retained);
    // Ordinary local memory leases also remain independent of the shared file.
    drop(
        context
            .governor()
            .try_acquire_memory("cached-during-failure", 1)
            .unwrap()
            .unwrap(),
    );
    assert_eq!(context.governor().snapshot(), *retained);
}

#[cfg(unix)]
async fn wait_for_renewal(context: &CapacityContext, generation: u64) {
    context.notify_demand();
    tokio::time::timeout(Duration::from_secs(5), async {
        while context.snapshot().operating.generation <= generation {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn registry_contention_and_validation_failure_preserve_responsive_local_cancellation() {
    use std::os::unix::fs::PermissionsExt;

    struct RestorePermissions(PathBuf, std::fs::Permissions);
    impl Drop for RestorePermissions {
        fn drop(&mut self) {
            let _ = std::fs::set_permissions(&self.0, self.1.clone());
        }
    }

    let parent = tempfile::tempdir().unwrap();
    let root = parent.path().canonicalize().unwrap().join("capacity");
    let context = CapacityContext::prepare_in(
        &mut HarnessConfig::default(),
        CapacityPolicy {
            mode: CapacityMode::Fixed,
            max_model_requests: Some(1),
            max_jobs: Some(1),
            ..CapacityPolicy::default()
        },
        &root,
    )
    .unwrap();
    let cancel = CancellationToken::new();
    let model = context.acquire_model(&cancel).await.unwrap();
    let governor = context.governor();
    let tool = governor
        .acquire(
            "retained-tool",
            ejm_harness_exec::Reservation {
                cpu_millis: 0,
                memory_bytes: 1,
                processes: 0,
                output_bytes: 0,
                heavy: false,
            },
            &cancel,
        )
        .await
        .unwrap();
    let retained = governor.snapshot();
    let database = root.join("capacity.sqlite3");
    let mut blocker = rusqlite::Connection::open(&database).unwrap();
    blocker.busy_timeout(Duration::from_millis(100)).unwrap();
    let transaction = blocker
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let generation = context.snapshot().operating.generation;
    cancel_waiters_during_registry_failure(&context, Instant::now(), &retained).await;
    transaction.rollback().unwrap();
    wait_for_renewal(&context, generation).await;

    let restore = RestorePermissions(
        database.clone(),
        std::fs::symlink_metadata(&database).unwrap().permissions(),
    );
    std::fs::set_permissions(&database, std::fs::Permissions::from_mode(0o644)).unwrap();
    let generation = context.snapshot().operating.generation;
    cancel_waiters_during_registry_failure(&context, Instant::now(), &retained).await;
    drop(restore);
    wait_for_renewal(&context, generation).await;

    drop(model);
    drop(tool);
    assert_eq!(context.snapshot().model_running, 0);
    assert_eq!(governor.snapshot().running, 0);
    drop(blocker);
    drop(context);
    wait_for_release(&root).await;
}

#[test]
fn fresh_cpu_saturation_reduces_targets_and_stale_saturation_does_not() {
    let mut observed = healthy_observations();
    observed.cpu_usage_percent.value = Some(94.9);
    assert!(pressure(&observed, CONTROL_INTERVAL) == Pressure::Clear);
    observed.cpu_usage_percent.value = Some(95.0);
    assert!(pressure(&observed, CONTROL_INTERVAL) == Pressure::Busy);
    let mut tuner = Tuner::new(8, 4);
    tuner.tick(
        CONTROL_INTERVAL,
        [true, true],
        pressure(&observed, CONTROL_INTERVAL),
        [8, 4],
        [32, 8],
    );
    assert_eq!((tuner.models(), tuner.jobs()), (4, 2));
    observed.cpu_usage_percent.age_ms = observed.cpu_usage_percent.max_age_ms + 1;
    assert!(pressure(&observed, CONTROL_INTERVAL) == Pressure::Unknown);
}

#[test]
fn missing_critical_observations_block_growth_without_claiming_pressure() {
    let invalidate: [fn(&mut HostObservations); 6] = [
        |sample| sample.memory_bytes.value = None,
        |sample| sample.available_memory_bytes.value = None,
        |sample| sample.cpu_millis.value = None,
        |sample| sample.cpu_usage_percent.value = None,
        |sample| sample.cpu_usage_percent.availability = ObservationAvailability::Unavailable,
        |sample| sample.domain_visibility = ObservationAvailability::Unsupported,
    ];
    for invalidate in invalidate {
        let mut observed = healthy_observations();
        invalidate(&mut observed);
        let pressure = pressure(&observed, CONTROL_INTERVAL);
        assert!(pressure == Pressure::Unknown);
        let mut tuner = Tuner::new(8, 4);
        for _ in 0..10 {
            for _ in 0..8 {
                tuner.record_model(Outcome::Successful, &Timings::default(), 0);
                tuner.record_tool(Outcome::Successful, &Timings::default(), 0);
            }
            tuner.tick(CONTROL_INTERVAL, [true, true], pressure, [8, 4], [32, 8]);
        }
        assert_eq!((tuner.models(), tuner.jobs()), (8, 4));
    }
}

#[test]
fn execution_domain_changes_with_constraints_and_ignores_available_load() {
    let mut observed = HostSampler::default().sample();
    let before = execution_domain(&observed);
    observed.cpu_usage_percent.value = Some(95.0);
    observed.available_memory_bytes.value = Some(1);
    for domain in &mut observed.domains {
        domain.available_memory_bytes = Some(1);
        domain.available_processes = Some(0);
    }
    assert_eq!(before, execution_domain(&observed));
    observed.descriptor_limit.value = Some(observed.descriptor_limit.value.unwrap_or(1) + 1);
    assert_ne!(before, execution_domain(&observed));
}

#[test]
fn renewal_failure_categories_preserve_causes_without_exposing_error_text() {
    let private = "private-path-and-database-content";
    let busy = anyhow::Error::new(rusqlite::Error::SqliteFailure(
        rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_BUSY),
        Some(private.into()),
    ))
    .context(private);
    assert_eq!(renewal_failure_category(&busy), "database contention");
    let database = anyhow::Error::new(rusqlite::Error::InvalidParameterName(private.into()));
    let filesystem = anyhow::Error::new(std::io::Error::other(private)).context(private);
    let admission = anyhow::anyhow!("{private}");
    assert_eq!(renewal_failure_category(&database), "database failure");
    assert_eq!(renewal_failure_category(&filesystem), "filesystem failure");
    assert_eq!(renewal_failure_category(&admission), "admission failure");
    for error in [busy, database, filesystem, admission] {
        assert!(!renewal_failure_category(&error).contains(private));
    }
}
