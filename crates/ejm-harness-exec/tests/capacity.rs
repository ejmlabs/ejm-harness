#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "Capacity fixtures fail at the contract boundary."
)]

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use ejm_harness_exec::{
    AdmissionPolicy, Governor, HostSampler, OperatingTarget, Reservation, ResourceLimits,
    process_identity,
};
use tokio::{task::JoinHandle, time::timeout};
use tokio_util::sync::CancellationToken;

fn limits() -> ResourceLimits {
    ResourceLimits {
        max_jobs: 32,
        max_heavy_jobs: 1,
        cpu_millis: 100,
        memory_bytes: 100,
        max_processes: 32,
        max_output_bytes: 100,
        ..ResourceLimits::default()
    }
}

fn reservation(memory_bytes: u64) -> Reservation {
    Reservation {
        cpu_millis: 0,
        memory_bytes,
        processes: 0,
        output_bytes: 0,
        heavy: false,
    }
}

fn pending(
    governor: &Arc<Governor>,
    session: &'static str,
    bytes: u64,
) -> JoinHandle<anyhow::Result<ejm_harness_exec::Lease>> {
    let governor = governor.clone();
    tokio::spawn(async move {
        governor
            .acquire(session, reservation(bytes), &CancellationToken::new())
            .await
    })
}

async fn queued(governor: &Governor, expected: usize) {
    timeout(Duration::from_secs(5), async {
        while governor.snapshot().queued != expected {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("queue reached expected count");
}

#[tokio::test]
async fn memory_only_and_output_compete_in_one_account_and_release_on_drop() {
    let governor =
        Arc::new(Governor::new_with_policy(limits(), AdmissionPolicy::Automatic).unwrap());
    let memory = governor
        .acquire_memory("context", 60, &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(governor.snapshot().running, 0);
    assert_eq!(governor.operating_snapshot().memory_leases, 1);
    let tool = governor
        .acquire(
            "tool",
            Reservation {
                output_bytes: 10,
                ..reservation(30)
            },
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(governor.snapshot().reserved_memory_bytes, 100);
    let waiting = pending(&governor, "waiting", 1);
    queued(&governor, 1).await;
    assert!(!waiting.is_finished());
    drop(memory);
    drop(
        timeout(Duration::from_secs(5), waiting)
            .await
            .unwrap()
            .unwrap()
            .unwrap(),
    );
    assert_eq!(governor.snapshot().reserved_memory_bytes, 40);
    drop(tool);
    governor.wait_idle().await;
    assert_eq!(governor.snapshot().reserved_memory_bytes, 0);
    assert_eq!(governor.operating_snapshot().memory_leases, 0);
    assert!(
        governor
            .acquire_memory("too-large", 101, &CancellationToken::new())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn shrinking_targets_preserve_debt_and_reject_stale_or_invalid_updates() {
    let governor =
        Arc::new(Governor::new_with_policy(limits(), AdmissionPolicy::Automatic).unwrap());
    let held = governor
        .acquire_memory("held", 80, &CancellationToken::new())
        .await
        .unwrap();
    let target = OperatingTarget {
        memory_bytes: 20,
        ..OperatingTarget::from_limits(governor.limits())
    };
    governor.install_target(1, target, None).unwrap();
    assert_eq!(governor.operating_snapshot().debt.memory_bytes, 60);
    assert_eq!(governor.snapshot().reserved_memory_bytes, 80);
    assert!(
        governor
            .install_target(1, OperatingTarget::default(), None)
            .is_err()
    );
    assert!(
        governor
            .install_target(
                2,
                OperatingTarget {
                    memory_bytes: 101,
                    ..target
                },
                None
            )
            .is_err()
    );
    assert_eq!(governor.operating_snapshot().generation, 1);
    let waiting = pending(&governor, "next", 1);
    queued(&governor, 1).await;
    let cancel = CancellationToken::new();
    let drain_governor = governor.clone();
    let drain_cancel = cancel.clone();
    let drain = tokio::spawn(async move { drain_governor.wait_drained(1, &drain_cancel).await });
    tokio::task::yield_now().await;
    assert!(!drain.is_finished());
    cancel.cancel();
    assert!(drain.await.unwrap().is_err());
    drop(held);
    governor
        .wait_drained(1, &CancellationToken::new())
        .await
        .unwrap();
    drop(waiting.await.unwrap().unwrap());
    assert_eq!(
        governor.operating_snapshot().debt,
        OperatingTarget::default()
    );
    assert_eq!(governor.limits().memory_bytes, 100);
}

#[tokio::test]
async fn expired_contact_parks_new_work_until_a_fresh_generation_and_keeps_running_charges() {
    let governor =
        Arc::new(Governor::new_with_policy(limits(), AdmissionPolicy::Automatic).unwrap());
    let target = OperatingTarget::from_limits(governor.limits());
    governor
        .install_target(1, target, Some(Instant::now() + Duration::from_millis(20)))
        .unwrap();
    let held = governor
        .acquire("held", reservation(10), &CancellationToken::new())
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(25)).await;
    let waiting = pending(&governor, "waiting", 10);
    queued(&governor, 1).await;
    assert!(!governor.operating_snapshot().contact_valid);
    assert_eq!(governor.snapshot().reserved_memory_bytes, 10);
    assert!(!waiting.is_finished());
    assert!(
        governor
            .install_target(2, target, Some(Instant::now()))
            .is_err()
    );
    governor
        .install_target(2, target, Some(Instant::now() + Duration::from_secs(5)))
        .unwrap();
    drop(
        timeout(Duration::from_secs(5), waiting)
            .await
            .unwrap()
            .unwrap()
            .unwrap(),
    );
    drop(held);
    governor.wait_idle().await;
}

#[tokio::test]
async fn automatic_backfill_is_bounded_and_fixed_retains_head_order() {
    for policy in [AdmissionPolicy::Fixed, AdmissionPolicy::Automatic] {
        let governor = Arc::new(Governor::new_with_policy(limits(), policy).unwrap());
        let held = governor
            .acquire("held", reservation(60), &CancellationToken::new())
            .await
            .unwrap();
        let large = pending(&governor, "large", 80);
        queued(&governor, 1).await;
        let small = pending(&governor, "small", 10);
        if policy == AdmissionPolicy::Automatic {
            drop(
                timeout(Duration::from_secs(5), small)
                    .await
                    .unwrap()
                    .unwrap()
                    .unwrap(),
            );
            for _ in 1..8 {
                drop(
                    timeout(Duration::from_secs(5), pending(&governor, "small", 10))
                        .await
                        .unwrap()
                        .unwrap()
                        .unwrap(),
                );
            }
            let ninth = pending(&governor, "ninth", 10);
            queued(&governor, 2).await;
            assert!(!ninth.is_finished());
            drop(held);
            let large = timeout(Duration::from_secs(5), large)
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            drop(
                timeout(Duration::from_secs(5), ninth)
                    .await
                    .unwrap()
                    .unwrap()
                    .unwrap(),
            );
            drop(large);
        } else {
            queued(&governor, 2).await;
            assert!(!small.is_finished());
            drop(held);
            drop(
                timeout(Duration::from_secs(5), large)
                    .await
                    .unwrap()
                    .unwrap()
                    .unwrap(),
            );
            drop(
                timeout(Duration::from_secs(5), small)
                    .await
                    .unwrap()
                    .unwrap()
                    .unwrap(),
            );
        }
        governor.wait_idle().await;
    }
}

#[tokio::test]
async fn automatic_backfill_preserves_within_session_fifo_and_waiter_cancellation() {
    let governor =
        Arc::new(Governor::new_with_policy(limits(), AdmissionPolicy::Automatic).unwrap());
    let held = governor
        .acquire("held", reservation(60), &CancellationToken::new())
        .await
        .unwrap();
    let first = pending(&governor, "same", 80);
    queued(&governor, 1).await;
    let second = pending(&governor, "same", 10);
    queued(&governor, 2).await;
    assert!(!second.is_finished());
    first.abort();
    assert!(matches!(first.await, Err(error) if error.is_cancelled()));
    drop(
        timeout(Duration::from_secs(5), second)
            .await
            .unwrap()
            .unwrap()
            .unwrap(),
    );
    drop(held);
    governor.wait_idle().await;
}

#[test]
fn host_observations_keep_missing_and_stale_values_distinct_and_identity_bounded() {
    let mut sampler = HostSampler::default();
    let mut sample = sampler.sample();
    assert!(sample.cpu_millis.value.is_some());
    assert!(!sample.cpu_usage_percent.is_fresh());
    sample.sampled_at = Instant::now().checked_sub(Duration::from_secs(6)).unwrap();
    sample.refresh_age();
    assert!(!sample.cpu_millis.is_fresh());
    if let Some(owner) = HostSampler::owner_identity() {
        assert_eq!(owner.pid, std::process::id());
        assert_ne!(owner.boot_id, "");
        assert_eq!(process_identity(owner.pid), Some(owner));
    }
    assert!(process_identity(u32::MAX).is_none());
}

#[tokio::test]
async fn resident_processes_and_waiting_work_keep_distinct_lifetimes_and_demand() {
    let governor =
        Arc::new(Governor::new_with_policy(limits(), AdmissionPolicy::Automatic).unwrap());
    let resident = governor
        .acquire_resident("host", 60, 2, &CancellationToken::new())
        .await
        .unwrap();
    let job = governor
        .acquire("held", reservation(40), &CancellationToken::new())
        .await
        .unwrap();
    let waiting_governor = governor.clone();
    let waiting = tokio::spawn(async move {
        waiting_governor
            .acquire(
                "queued",
                Reservation {
                    processes: 3,
                    ..reservation(40)
                },
                &CancellationToken::new(),
            )
            .await
    });
    queued(&governor, 1).await;
    let demand = governor.admission_demand();
    assert_eq!(demand.memory_bytes, 100);
    assert_eq!(demand.max_processes, 5);
    assert_eq!(demand.max_jobs, 1);
    waiting.abort();
    assert!(matches!(waiting.await, Err(error) if error.is_cancelled()));
    drop(job);
    governor.wait_jobs_idle().await;
    assert_eq!(governor.snapshot().reserved_memory_bytes, 60);
    assert_eq!(governor.snapshot().reserved_processes, 2);
    assert_eq!(governor.snapshot().running, 0);
    assert_eq!(governor.operating_snapshot().memory_leases, 1);
    assert_eq!(governor.admission_demand().memory_bytes, 60);
    drop(resident);
    governor.wait_idle().await;
    assert_eq!(
        governor.snapshot(),
        ejm_harness_exec::GovernorSnapshot::default()
    );
    assert_eq!(governor.admission_demand(), OperatingTarget::default());
}

#[tokio::test]
async fn staged_demand_keeps_resident_floor_visible_without_a_job_lease() {
    let governor =
        Arc::new(Governor::new_with_policy(limits(), AdmissionPolicy::Automatic).unwrap());
    let resident = governor
        .acquire_resident_envelope("resident", 5, 60, 2, &CancellationToken::new())
        .await
        .unwrap();
    let staged = governor.stage("next", reservation(40)).unwrap();
    assert_eq!(governor.snapshot().queued, 1);
    assert_eq!(governor.snapshot().running, 0);
    assert_eq!(governor.snapshot().reserved_memory_bytes, 60);
    assert_eq!(governor.operating_snapshot().memory_leases, 1);
    assert_eq!(governor.admission_demand().memory_bytes, 100);
    assert_eq!(governor.admission_activity(), 0);
    assert_eq!(
        governor.admission_floor(),
        OperatingTarget {
            cpu_millis: 5,
            memory_bytes: 60,
            max_processes: 2,
            ..OperatingTarget::default()
        }
    );
    drop(staged);
    assert_eq!(governor.snapshot().queued, 0);
    assert_eq!(governor.admission_activity(), 0);
    drop(resident);
    assert_eq!(governor.admission_floor(), OperatingTarget::default());
}

#[tokio::test]
async fn staged_queue_bound_fifo_cancellation_and_drop_share_the_existing_queue() {
    let governor = Arc::new(
        Governor::new_with_policy(
            ResourceLimits {
                max_queue: 2,
                ..limits()
            },
            AdmissionPolicy::Automatic,
        )
        .unwrap(),
    );
    let held = governor
        .acquire("held", reservation(60), &CancellationToken::new())
        .await
        .unwrap();
    let first = governor.stage("same", reservation(80)).unwrap();
    let second = governor.stage("same", reservation(10)).unwrap();
    assert!(governor.stage("over-bound", reservation(1)).is_err());
    let cancel = CancellationToken::new();
    {
        let waiting = second.wait_eligible(&cancel);
        tokio::pin!(waiting);
        assert!(
            timeout(Duration::from_millis(20), &mut waiting)
                .await
                .is_err()
        );
        cancel.cancel();
        assert!(waiting.await.is_err());
    }
    drop(first);
    let eligible = timeout(
        Duration::from_secs(5),
        second.wait_eligible(&CancellationToken::new()),
    )
    .await
    .unwrap()
    .unwrap();
    let mut second = second;
    let lease = second.try_activate(eligible).unwrap().unwrap();
    assert_eq!(governor.snapshot().queued, 0);
    assert_eq!(governor.snapshot().reserved_memory_bytes, 70);
    drop(lease);
    drop(held);
    assert_eq!(
        governor.snapshot(),
        ejm_harness_exec::GovernorSnapshot::default()
    );
}

#[tokio::test]
async fn staged_activation_rejects_changed_fences_and_keeps_its_prospective_turn() {
    let governor =
        Arc::new(Governor::new_with_policy(limits(), AdmissionPolicy::Automatic).unwrap());
    let mut staged = governor.stage("first", reservation(80)).unwrap();
    let initial = staged
        .wait_eligible(&CancellationToken::new())
        .await
        .unwrap();
    let later = pending(&governor, "later", 10);
    queued(&governor, 2).await;
    assert!(!later.is_finished());
    assert_eq!(governor.admission_activity(), 0);
    let full = OperatingTarget::from_limits(governor.limits());
    governor.install_target(1, full, None).unwrap();
    assert!(staged.try_activate(initial).unwrap().is_none());
    assert_eq!(governor.snapshot().queued, 2);
    assert_eq!(governor.snapshot().reserved_memory_bytes, 0);
    assert_eq!(governor.admission_activity(), 0);
    assert!(!later.is_finished());
    let before_park = staged
        .wait_eligible(&CancellationToken::new())
        .await
        .unwrap();
    governor.park_admission();
    assert!(staged.try_activate(before_park).unwrap().is_none());
    assert_eq!(governor.snapshot().queued, 2);
    governor.install_target(2, full, None).unwrap();
    let current = staged
        .wait_eligible(&CancellationToken::new())
        .await
        .unwrap();
    let lease = staged.try_activate(current).unwrap().unwrap();
    assert!(staged.try_activate(current).is_err());
    let later = timeout(Duration::from_secs(5), later)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(governor.snapshot().reserved_memory_bytes, 90);
    assert_eq!(governor.admission_activity(), 2);
    drop(lease);
    drop(later);
    assert_eq!(
        governor.snapshot(),
        ejm_harness_exec::GovernorSnapshot::default()
    );
}

#[tokio::test]
async fn staged_retries_keep_bypass_protection() {
    let governor =
        Arc::new(Governor::new_with_policy(limits(), AdmissionPolicy::Automatic).unwrap());
    let held = governor
        .acquire("held", reservation(60), &CancellationToken::new())
        .await
        .unwrap();
    let mut oldest = governor.stage("oldest", reservation(80)).unwrap();
    for generation in 1..=8 {
        drop(
            timeout(Duration::from_secs(5), pending(&governor, "small", 10))
                .await
                .unwrap()
                .unwrap()
                .unwrap(),
        );
        governor
            .install_target(
                generation,
                OperatingTarget::from_limits(governor.limits()),
                None,
            )
            .unwrap();
    }
    let ninth = pending(&governor, "ninth", 10);
    queued(&governor, 2).await;
    assert!(!ninth.is_finished());
    drop(held);
    let eligible = oldest
        .wait_eligible(&CancellationToken::new())
        .await
        .unwrap();
    governor
        .install_target(9, OperatingTarget::from_limits(governor.limits()), None)
        .unwrap();
    assert!(oldest.try_activate(eligible).unwrap().is_none());
    assert!(!ninth.is_finished());
    let eligible = oldest
        .wait_eligible(&CancellationToken::new())
        .await
        .unwrap();
    let oldest_lease = oldest.try_activate(eligible).unwrap().unwrap();
    drop(
        timeout(Duration::from_secs(5), ninth)
            .await
            .unwrap()
            .unwrap()
            .unwrap(),
    );
    drop(oldest_lease);
    assert_eq!(
        governor.snapshot(),
        ejm_harness_exec::GovernorSnapshot::default()
    );
}

#[tokio::test]
async fn resident_transfer_keeps_retained_output_memory_with_the_job() {
    let governor =
        Arc::new(Governor::new_with_policy(limits(), AdmissionPolicy::Automatic).unwrap());
    let envelope = Reservation {
        output_bytes: 10,
        ..reservation(80)
    };
    let lease = governor
        .acquire(
            "too-large-share",
            envelope.clone(),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert!(lease.split_resident(81, 0).is_err());
    assert_eq!(
        governor.snapshot(),
        ejm_harness_exec::GovernorSnapshot::default()
    );
    let lease = governor
        .acquire("valid-share", envelope.clone(), &CancellationToken::new())
        .await
        .unwrap();
    let (resident, job) = lease.split_resident(80, 0).unwrap();
    drop(resident);
    assert_eq!(governor.snapshot().reserved_memory_bytes, 10);
    assert_eq!(governor.snapshot().reserved_output_bytes, 10);
    assert_eq!(governor.snapshot().running, 1);
    drop(job);
    assert_eq!(
        governor.snapshot(),
        ejm_harness_exec::GovernorSnapshot::default()
    );

    let legacy = Arc::new(Governor::new(limits()).unwrap());
    let lease = legacy
        .acquire("legacy", envelope, &CancellationToken::new())
        .await
        .unwrap();
    let (resident, job) = lease.split_resident(80, 0).unwrap();
    assert_eq!(legacy.snapshot().reserved_memory_bytes, 80);
    drop(resident);
    drop(job);
    assert_eq!(
        legacy.snapshot(),
        ejm_harness_exec::GovernorSnapshot::default()
    );
}

#[tokio::test]
async fn activated_startup_envelope_splits_without_recharging_or_releasing_early() {
    let governor =
        Arc::new(Governor::new_with_policy(limits(), AdmissionPolicy::Automatic).unwrap());
    let mut startup = governor
        .stage(
            "startup",
            Reservation {
                cpu_millis: 10,
                memory_bytes: 80,
                processes: 2,
                output_bytes: 10,
                heavy: true,
            },
        )
        .unwrap();
    let eligible = startup
        .wait_eligible(&CancellationToken::new())
        .await
        .unwrap();
    let lease = startup.try_activate(eligible).unwrap().unwrap();
    let before = governor.snapshot();
    assert_eq!(governor.admission_activity(), 1);
    let (resident, management) = lease.split_resident(64, 1).unwrap();
    assert_eq!(governor.snapshot(), before);
    assert_eq!(governor.admission_activity(), 1);
    assert_eq!(governor.operating_snapshot().memory_leases, 1);
    assert_eq!(governor.admission_floor().memory_bytes, 64);
    assert_eq!(governor.admission_floor().max_processes, 1);
    assert_eq!(governor.admission_floor().cpu_millis, 0);
    assert!(resident.split_resident(1, 0).is_err());
    assert_eq!(governor.admission_floor(), OperatingTarget::default());
    assert_eq!(governor.snapshot().reserved_memory_bytes, 26);
    assert_eq!(governor.snapshot().running, 1);
    drop(management);
    assert_eq!(
        governor.snapshot(),
        ejm_harness_exec::GovernorSnapshot::default()
    );

    let lease = governor
        .acquire("independent", reservation(80), &CancellationToken::new())
        .await
        .unwrap();
    let (resident, management) = lease.split_resident(64, 0).unwrap();
    drop(management);
    assert_eq!(governor.snapshot().running, 0);
    assert_eq!(governor.snapshot().reserved_memory_bytes, 64);
    assert_eq!(governor.admission_floor().memory_bytes, 64);
    drop(resident);
    assert_eq!(
        governor.snapshot(),
        ejm_harness_exec::GovernorSnapshot::default()
    );
}
