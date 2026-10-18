#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "Test fixtures and assertions must fail at their call site."
)]

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;

use ejm_harness_exec::{Governor, GovernorSnapshot, Reservation, ResourceLimits};
use tokio::{
    sync::{Notify, mpsc},
    time::timeout,
};
use tokio_util::sync::CancellationToken;

fn limits() -> ResourceLimits {
    ResourceLimits {
        max_jobs: 1,
        cpu_millis: 100,
        memory_bytes: 100,
        max_processes: 10,
        max_output_bytes: 100,
        ..ResourceLimits::default()
    }
}

fn reservation() -> Reservation {
    Reservation {
        cpu_millis: 10,
        memory_bytes: 10,
        processes: 1,
        output_bytes: 10,
        heavy: false,
    }
}

async fn queued(governor: &Governor, count: usize) {
    timeout(Duration::from_secs(5), async {
        while governor.snapshot().queued != count {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("queue reached expected count");
}

#[tokio::test]
async fn rotates_sessions_instead_of_draining_one_backlog() {
    let governor = Arc::new(Governor::new(limits()).unwrap());
    let held = governor
        .acquire("held", reservation(), &CancellationToken::new())
        .await
        .unwrap();
    let (sender, mut receiver) = mpsc::channel(4);
    let gate = Arc::new(Notify::new());
    let mut tasks = Vec::new();
    for (index, (session, name)) in [("a", "a1"), ("a", "a2"), ("b", "b1"), ("b", "b2")]
        .into_iter()
        .enumerate()
    {
        let worker_governor = governor.clone();
        let sender = sender.clone();
        let gate = gate.clone();
        tasks.push(tokio::spawn(async move {
            let _lease = worker_governor
                .acquire(session, reservation(), &CancellationToken::new())
                .await
                .unwrap();
            sender.send(name).await.unwrap();
            gate.notified().await;
        }));
        queued(&governor, index + 1).await;
    }
    drop(held);
    for expected in ["a1", "b1", "a2", "b2"] {
        assert_eq!(
            timeout(Duration::from_secs(5), receiver.recv())
                .await
                .unwrap(),
            Some(expected)
        );
        gate.notify_one();
    }
    for task in tasks {
        task.await.unwrap();
    }
    assert_eq!(governor.snapshot(), GovernorSnapshot::default());
}

#[tokio::test]
async fn each_aggregate_dimension_blocks_oversubscription() {
    for dimension in 0..4 {
        let governor = Arc::new(
            Governor::new(ResourceLimits {
                max_jobs: 2,
                ..limits()
            })
            .unwrap(),
        );
        let mut held = reservation();
        match dimension {
            0 => held.cpu_millis = 100,
            1 => held.memory_bytes = 100,
            2 => held.processes = 10,
            _ => held.output_bytes = 100,
        }
        let lease = governor
            .acquire("held", held.clone(), &CancellationToken::new())
            .await
            .unwrap();
        let other = governor.clone();
        let task = tokio::spawn(async move {
            other
                .acquire("next", reservation(), &CancellationToken::new())
                .await
        });
        queued(&governor, 1).await;
        let snapshot = governor.snapshot();
        assert_eq!(snapshot.running, 1);
        assert_eq!(snapshot.reserved_cpu_millis, held.cpu_millis);
        assert_eq!(snapshot.reserved_memory_bytes, held.memory_bytes);
        assert_eq!(snapshot.reserved_processes, held.processes);
        assert_eq!(snapshot.reserved_output_bytes, held.output_bytes);
        drop(lease);
        drop(task.await.unwrap().unwrap());
        assert_eq!(governor.snapshot(), GovernorSnapshot::default());
    }
}

#[tokio::test]
async fn cancellation_and_dropped_waiters_remove_head_without_leaking() {
    let governor = Arc::new(Governor::new(limits()).unwrap());
    let held = governor
        .acquire("held", reservation(), &CancellationToken::new())
        .await
        .unwrap();
    let cancelled = CancellationToken::new();
    let worker_cancel = cancelled.clone();
    let other = governor.clone();
    let first =
        tokio::spawn(async move { other.acquire("first", reservation(), &worker_cancel).await });
    queued(&governor, 1).await;
    cancelled.cancel();
    assert!(first.await.unwrap().is_err());
    queued(&governor, 0).await;
    let other = governor.clone();
    let dropped = tokio::spawn(async move {
        other
            .acquire("dropped", reservation(), &CancellationToken::new())
            .await
    });
    queued(&governor, 1).await;
    dropped.abort();
    assert!(matches!(dropped.await, Err(error) if error.is_cancelled()));
    queued(&governor, 0).await;
    drop(held);
    assert_eq!(governor.snapshot(), GovernorSnapshot::default());
}

#[tokio::test]
async fn queue_and_impossible_reservations_fail_without_launching() {
    let governor = Arc::new(
        Governor::new(ResourceLimits {
            max_queue: 1,
            ..limits()
        })
        .unwrap(),
    );
    let held = governor
        .acquire("held", reservation(), &CancellationToken::new())
        .await
        .unwrap();
    let other = governor.clone();
    let waiting = tokio::spawn(async move {
        other
            .acquire("waiting", reservation(), &CancellationToken::new())
            .await
    });
    queued(&governor, 1).await;
    assert!(
        governor
            .acquire("overflow", reservation(), &CancellationToken::new())
            .await
            .is_err()
    );
    let impossible = Reservation {
        memory_bytes: 101,
        ..reservation()
    };
    assert!(
        governor
            .acquire("impossible", impossible, &CancellationToken::new())
            .await
            .is_err()
    );
    waiting.abort();
    let _ = waiting.await;
    drop(held);
    assert_eq!(governor.snapshot(), GovernorSnapshot::default());
}

#[tokio::test]
async fn only_one_heavy_job_runs_even_with_free_slots() {
    let governor = Arc::new(
        Governor::new(ResourceLimits {
            max_jobs: 4,
            ..limits()
        })
        .unwrap(),
    );
    let heavy = Reservation {
        heavy: true,
        ..reservation()
    };
    let held = governor
        .acquire("one", heavy.clone(), &CancellationToken::new())
        .await
        .unwrap();
    let other = governor.clone();
    let task =
        tokio::spawn(async move { other.acquire("two", heavy, &CancellationToken::new()).await });
    queued(&governor, 1).await;
    assert_eq!(governor.snapshot().heavy_running, 1);
    assert_eq!(governor.snapshot().running, 1);
    drop(held);
    let next = task.await.unwrap().unwrap();
    assert_eq!(governor.snapshot().heavy_running, 1);
    drop(next);
    assert_eq!(governor.snapshot(), GovernorSnapshot::default());
}

#[tokio::test]
async fn a_hundred_managed_sessions_share_two_execution_slots() {
    let governor = Arc::new(
        Governor::new(ResourceLimits {
            max_jobs: 2,
            ..limits()
        })
        .unwrap(),
    );
    let running = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let mut jobs = Vec::new();
    for id in 0..100 {
        let governor = governor.clone();
        let running = running.clone();
        let peak = peak.clone();
        jobs.push(tokio::spawn(async move {
            let _lease = governor
                .acquire(
                    &format!("session-{id}"),
                    reservation(),
                    &CancellationToken::new(),
                )
                .await
                .unwrap();
            let active = running.fetch_add(1, Ordering::SeqCst) + 1;
            peak.fetch_max(active, Ordering::SeqCst);
            assert!(active <= 2);
            tokio::task::yield_now().await;
            running.fetch_sub(1, Ordering::SeqCst);
        }));
    }
    for job in jobs {
        job.await.unwrap();
    }
    assert_eq!(governor.snapshot(), GovernorSnapshot::default());
    assert_eq!(running.load(Ordering::SeqCst), 0);
    assert!(peak.load(Ordering::SeqCst) <= 2);
}

#[tokio::test]
async fn idle_barrier_waits_for_queued_cancellation_and_final_lease_drop() {
    let governor = Arc::new(Governor::new(limits()).unwrap());
    let lease = governor
        .acquire("held", reservation(), &CancellationToken::new())
        .await
        .unwrap();
    let waiting_cancel = CancellationToken::new();
    let worker_cancel = waiting_cancel.clone();
    let worker_governor = governor.clone();
    let waiting = tokio::spawn(async move {
        worker_governor
            .acquire("waiting", reservation(), &worker_cancel)
            .await
    });
    queued(&governor, 1).await;
    let draining_governor = governor.clone();
    let idle = tokio::spawn(async move {
        draining_governor.wait_idle().await;
    });
    waiting_cancel.cancel();
    assert!(waiting.await.unwrap().is_err());
    assert!(!idle.is_finished());
    drop(lease);
    timeout(Duration::from_secs(5), idle)
        .await
        .unwrap()
        .unwrap();
    governor.wait_idle().await;
    assert_eq!(governor.snapshot(), GovernorSnapshot::default());
}

#[test]
fn invalid_limits_and_partial_json_defaults_are_checked() {
    assert!(
        Governor::new(ResourceLimits {
            max_jobs: 0,
            ..limits()
        })
        .is_err()
    );
    assert!(
        Governor::new(ResourceLimits {
            max_heavy_jobs: 2,
            ..limits()
        })
        .is_err()
    );
    let limits: ResourceLimits = serde_json::from_str(r#"{"max_jobs":2}"#).unwrap();
    assert_eq!(limits.max_jobs, 2);
    assert_eq!(limits.max_heavy_jobs, 1);
    assert!(Governor::new(limits).is_ok());
}
