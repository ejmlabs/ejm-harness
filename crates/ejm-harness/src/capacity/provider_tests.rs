#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "bounded cached provider admission fixtures"
)]

use super::*;
use crate::{ProviderRate, ProviderRateWindow, ProviderRetry, QuotaDomain, Usage};
use futures_util::{FutureExt, poll};

fn fixture() -> (ProviderGate, rusqlite::Connection) {
    let (wake, _received) = mpsc::sync_channel(1);
    let gate = ProviderGate::new("one".into(), 4, wake);
    gate.configure(policy("shared")).unwrap();
    let connection = rusqlite::Connection::open_in_memory().unwrap();
    connection.execute_batch("CREATE TABLE owners(id TEXT PRIMARY KEY,state TEXT NOT NULL); INSERT INTO owners VALUES('one','active'),('two','active');").unwrap();
    (gate, connection)
}

fn policy(family: &str) -> ProviderCapacity {
    ProviderCapacity {
        domain: Some(
            QuotaDomain::new(
                "https://provider.invalid",
                "00000000-0000-0000-0000-000000000001",
                family,
            )
            .unwrap(),
        ),
        max_in_flight: Some(4),
        requests: Some(ProviderRate {
            limit: 8,
            window_ms: 60_000,
        }),
        tokens: Some(ProviderRate {
            limit: 10_000,
            window_ms: 60_000,
        }),
    }
}

fn request() -> ModelRequest {
    ModelRequest {
        system: "fixture".into(),
        messages: Vec::new(),
        tools: Vec::new(),
        max_output_tokens: 8,
    }
}

fn throttle(retry: ProviderRetry) -> ProviderObservation {
    ProviderObservation::Throttled { retry }
}

#[tokio::test]
async fn prepared_configuration_blocks_admission_and_rolls_back_or_commits_without_io() {
    let (wake, _) = mpsc::sync_channel(1);
    let gate = ProviderGate::new("one".into(), 4, wake);
    let next = ProviderCapacity {
        max_in_flight: Some(2),
        ..ProviderCapacity::default()
    };
    let prepared = gate.prepare(next.clone()).unwrap();
    assert!(gate.configure(ProviderCapacity::default()).is_err());
    let request = request();
    let cancel = CancellationToken::new();
    let wait = gate.acquire(&request, &cancel);
    tokio::pin!(wait);
    assert!(poll!(&mut wait).is_pending());
    assert_eq!(gate.snapshot().in_flight, 0);
    drop(prepared);
    let lease = wait.await.unwrap();
    assert_eq!(lock(&gate.inner.state).policy, ProviderCapacity::default());
    assert_eq!(lock(&gate.inner.state).generation, 1);
    drop(lease);

    let prepared = gate.prepare(next.clone()).unwrap();
    let wait = gate.acquire(&request, &cancel);
    tokio::pin!(wait);
    assert!(poll!(&mut wait).is_pending());
    prepared.commit();
    let lease = wait.await.unwrap();
    assert_eq!(lock(&gate.inner.state).policy, next);
    assert_eq!(lock(&gate.inner.state).generation, 2);
    assert_eq!(gate.snapshot().target, 2);
    drop(lease);

    let prepared = gate.prepare(ProviderCapacity::default()).unwrap();
    let wait = gate.acquire(&request, &cancel);
    tokio::pin!(wait);
    assert!(poll!(&mut wait).is_pending());
    cancel.cancel();
    assert!(wait.await.is_err());
    assert_eq!(gate.snapshot().waiting, 0);
    drop(prepared);
    assert_eq!(lock(&gate.inner.state).policy, next);
}

#[tokio::test]
async fn cached_admission_and_release_do_not_write_the_registry() {
    let (gate, mut connection) = fixture();
    let request = request();
    let cancel = CancellationToken::new();
    let mut waits: Vec<_> = (0..4)
        .map(|_| Box::pin(gate.acquire(&request, &cancel)))
        .collect();
    for wait in &mut waits {
        assert!(poll!(wait).is_pending());
    }
    gate.coordinate_connection(&mut connection).unwrap();
    let changes = connection.total_changes();
    for wait in waits {
        let lease = wait.await.unwrap();
        lease.observer().observe(ProviderObservation::Usage(Usage {
            input_tokens: 1,
            ..Usage::default()
        }));
        drop(lease);
    }
    assert_eq!(connection.total_changes(), changes);
    let snapshot = gate.snapshot();
    assert_eq!(snapshot.in_flight, 0);
    assert_eq!(snapshot.waiting, 0);
    assert_eq!(snapshot.request_credits, Some(0));
    assert_eq!(
        snapshot.token_credits,
        Some(request_cost(&request).unwrap() * 4 - 4)
    );
}

#[tokio::test]
async fn generous_quota_feedback_allows_a_thousand_cached_requests_but_tighter_hints_fence() {
    let (gate, mut connection) = fixture();
    let mut configured = policy("shared");
    configured.requests.as_mut().unwrap().limit = 1_000_000;
    configured.tokens.as_mut().unwrap().limit = 1_000_000_000;
    gate.configure(configured).unwrap();
    let request = request();
    let cancel = CancellationToken::new();
    let limits = |remaining| ProviderObservation::Limits {
        requests: Some(ProviderRateWindow {
            limit: 1_000_000,
            remaining,
            reset_at_ms: timestamp_ms() + 60_000,
        }),
        tokens: Some(ProviderRateWindow {
            limit: 1_000_000_000,
            remaining: 999_999_999,
            reset_at_ms: timestamp_ms() + 60_000,
        }),
    };
    let wait = gate.acquire(&request, &cancel);
    tokio::pin!(wait);
    assert!(poll!(&mut wait).is_pending());
    gate.coordinate_connection(&mut connection).unwrap();
    let lease = wait.await.unwrap();
    lease.observer().observe(limits(999_999));
    drop(lease);

    // The first supported hint must enter the shared budget before reuse.
    let wait = gate.acquire(&request, &cancel);
    tokio::pin!(wait);
    assert!(poll!(&mut wait).is_pending());
    gate.coordinate_connection(&mut connection).unwrap();
    drop(wait.await.unwrap());
    let changes = connection.total_changes();
    for index in 0..1000 {
        let lease = gate
            .acquire(&request, &cancel)
            .now_or_never()
            .expect("a prepaid quota admission must not wait for coordination")
            .unwrap();
        lease.observer().observe(limits(999_998 - index));
        lease.observer().observe(ProviderObservation::Usage(Usage {
            input_tokens: 1,
            ..Usage::default()
        }));
        drop(lease);
    }
    assert_eq!(connection.total_changes(), changes);
    assert_eq!(lock(&gate.inner.feedback).len(), 1);
    assert!(!gate.snapshot().blocked);

    let lease = gate.acquire(&request, &cancel).await.unwrap();
    lease.observer().observe(limits(0));
    drop(lease);
    let wait = gate.acquire(&request, &cancel);
    tokio::pin!(wait);
    assert!(poll!(&mut wait).is_pending());
    gate.coordinate_connection(&mut connection).unwrap();
    assert_eq!(gate.snapshot().request_credits, Some(0));
    assert!(poll!(&mut wait).is_pending());
    cancel.cancel();
    assert!(wait.await.is_err());
}

#[tokio::test]
async fn a_larger_grant_rechecks_quota_feedback_received_during_its_commit() {
    let (gate, mut connection) = fixture();
    let mut configured = policy("shared");
    configured.requests.as_mut().unwrap().limit = 10_000;
    configured.tokens = None;
    gate.configure(configured).unwrap();
    let request = request();
    let cost = request_cost(&request).unwrap();
    let cancel = CancellationToken::new();
    let limits = |remaining| ProviderObservation::Limits {
        requests: Some(ProviderRateWindow {
            limit: 10_000,
            remaining,
            reset_at_ms: timestamp_ms() + 60_000,
        }),
        tokens: None,
    };
    let wait = gate.acquire(&request, &cancel);
    tokio::pin!(wait);
    assert!(poll!(&mut wait).is_pending());
    gate.coordinate_connection(&mut connection).unwrap();
    let lease = wait.await.unwrap();
    lease.observer().observe(limits(9999));
    drop(lease);
    gate.coordinate_connection(&mut connection).unwrap();
    for _ in 0..399 {
        drop(gate.acquire(&request, &cancel).await.unwrap());
    }
    let lease = gate.acquire(&request, &cancel).await.unwrap();
    let reserve = lock(&gate.inner.state).request_reserve.unwrap();
    let ticket = gate.enqueue(cost).unwrap();
    let _waiter = Waiter {
        inner: gate.inner.clone(),
        ticket,
    };
    let sequence = gate.inner.feedback_sequence.load(Ordering::Acquire);
    let now = timestamp_ms();
    let (demand, generation, _, clear) = gate.demand(sequence, Vec::new(), now).unwrap();
    let grant = ledger::allocate(&mut connection, &demand, now).unwrap();
    assert!(grant.request_credit.unwrap() > reserve);

    // This hint fits the small old pool, but cannot cover the pending increase.
    lease.observer().observe(limits(reserve + 1));
    assert!(gate.inner.fenced_sequence.load(Ordering::Acquire) <= sequence);
    gate.install(&demand, &grant, generation, sequence, clear, now)
        .unwrap();
    assert!(gate.inner.fenced_sequence.load(Ordering::Acquire) > sequence);
    assert!(gate.try_admit(ticket, cost, false).unwrap().is_none());
    drop(lease);
}

#[tokio::test]
async fn cancelled_quota_wait_keeps_the_committed_pool_and_does_not_start() {
    let (gate, mut connection) = fixture();
    let request = request();
    let cancel = CancellationToken::new();
    let wait = gate.acquire(&request, &cancel);
    tokio::pin!(wait);
    assert!(poll!(&mut wait).is_pending());
    gate.coordinate_connection(&mut connection).unwrap();
    let before = gate.snapshot();
    cancel.cancel();
    assert!(
        tokio::time::timeout(Duration::from_millis(100), wait)
            .await
            .unwrap()
            .is_err()
    );
    let after = gate.snapshot();
    assert_eq!(after.waiting, 0);
    assert_eq!(after.in_flight, 0);
    assert_eq!(after.request_credits, before.request_credits);
    assert_eq!(after.token_credits, before.token_credits);
}

#[tokio::test]
async fn a_valid_long_cooldown_fails_local_waits_without_shortening_remote_reset() {
    let (gate, mut connection) = fixture();
    let request = request();
    let cancel = CancellationToken::new();
    let wait = gate.acquire(&request, &cancel);
    tokio::pin!(wait);
    assert!(poll!(&mut wait).is_pending());
    gate.coordinate_connection(&mut connection).unwrap();
    let lease = wait.await.unwrap();
    let deadline = timestamp_ms() + 20 * 60 * 1000;
    lease
        .observer()
        .observe(throttle(ProviderRetry::BeyondHorizon {
            retry_at_ms: Some(deadline),
        }));
    drop(lease);
    gate.coordinate_connection(&mut connection).unwrap();
    let error = gate
        .acquire(&request, &cancel)
        .now_or_never()
        .expect("a known unsupported wait horizon must fail immediately")
        .err()
        .unwrap();
    assert!(error.to_string().contains("15 minute"));
    let snapshot = gate.snapshot();
    assert_eq!(snapshot.cooldown_until_ms, Some(deadline));
    assert_eq!(snapshot.waiting, 0);
    assert_eq!(snapshot.in_flight, 0);
}

#[tokio::test]
async fn feedback_survives_storage_failure_and_configuration_cannot_discard_it() {
    let (gate, mut connection) = fixture();
    let request = request();
    let cancel = CancellationToken::new();
    let wait = gate.acquire(&request, &cancel);
    tokio::pin!(wait);
    assert!(poll!(&mut wait).is_pending());
    gate.coordinate_connection(&mut connection).unwrap();
    let lease = wait.await.unwrap();
    lease
        .observer()
        .observe(throttle(ProviderRetry::Delay { millis: 12_000 }));
    drop(lease);
    assert!(gate.configure(policy("other")).is_err());
    connection.execute_batch("CREATE TRIGGER refuse_provider BEFORE INSERT ON provider_domains BEGIN SELECT RAISE(ABORT,'fixture'); END;").unwrap();
    assert!(gate.coordinate_connection(&mut connection).is_err());
    assert_eq!(lock(&gate.inner.state).unsent.len(), 1);
    connection
        .execute_batch("DROP TRIGGER refuse_provider")
        .unwrap();
    gate.coordinate_connection(&mut connection).unwrap();
    assert!(gate.snapshot().cooldown_until_ms.is_some());
    gate.configure(policy("shared")).unwrap();
    assert!(gate.snapshot().cooldown_until_ms.is_some());
    gate.configure(policy("other")).unwrap();
}

#[tokio::test]
async fn explicit_reset_clears_old_overflow_but_keeps_a_new_throttle() {
    let (gate, mut connection) = fixture();
    let request = request();
    let cancel = CancellationToken::new();
    let wait = gate.acquire(&request, &cancel);
    tokio::pin!(wait);
    assert!(poll!(&mut wait).is_pending());
    gate.coordinate_connection(&mut connection).unwrap();
    let lease = wait.await.unwrap();
    let observer = lease.observer();
    for _ in 0..=MAX_FEEDBACK {
        observer.observe(throttle(ProviderRetry::BeyondHorizon { retry_at_ms: None }));
    }
    assert!(gate.snapshot().blocked);
    gate.coordinate_connection(&mut connection).unwrap();
    gate.request_clear_cooldown();
    observer.observe(throttle(ProviderRetry::Delay { millis: 9000 }));
    gate.coordinate_connection(&mut connection).unwrap();
    assert!(!gate.snapshot().blocked);
    assert!(gate.snapshot().cooldown_until_ms.is_some());
    gate.request_clear_cooldown();
    gate.coordinate_connection(&mut connection).unwrap();
    assert!(!gate.snapshot().blocked);
    assert_eq!(gate.snapshot().cooldown_until_ms, None);
    drop(lease);
    observer.observe(throttle(ProviderRetry::BeyondHorizon { retry_at_ms: None }));
    gate.coordinate_connection(&mut connection).unwrap();
    assert!(!gate.snapshot().blocked);
}

#[tokio::test]
async fn usage_above_the_estimate_is_settled_even_after_credit_expiry() {
    let (gate, mut connection) = fixture();
    let request = request();
    let cost = request_cost(&request).unwrap();
    let cancel = CancellationToken::new();
    let wait = gate.acquire(&request, &cancel);
    tokio::pin!(wait);
    assert!(poll!(&mut wait).is_pending());
    gate.coordinate_connection(&mut connection).unwrap();
    let lease = wait.await.unwrap();
    let prepaid = cost + gate.snapshot().token_credits.unwrap();
    lock(&gate.inner.state).expires = 0;
    lease.observer().observe(ProviderObservation::Usage(Usage {
        input_tokens: cost + 1000,
        ..Usage::default()
    }));
    drop(lease);
    assert_eq!(gate.snapshot().token_debt, 1000);
    assert!(gate.configure(policy("other")).is_err());
    gate.coordinate_connection(&mut connection).unwrap();
    assert_eq!(gate.snapshot().token_debt, 0);
    let payload: String = connection
        .query_row("SELECT payload FROM provider_domains", [], |row| row.get(0))
        .unwrap();
    let value: serde_json::Value = serde_json::from_str(&payload).unwrap();
    assert_eq!(value["token"]["charged"], prepaid + 1000);
}

#[tokio::test]
async fn incompatible_shared_windows_return_an_actionable_wait_error() {
    let (gate, mut connection) = fixture();
    gate.coordinate_connection(&mut connection).unwrap();
    let (wake, _) = mpsc::sync_channel(1);
    let second = ProviderGate::new("two".into(), 4, wake);
    let mut other = policy("shared");
    other.requests.as_mut().unwrap().window_ms = 30_000;
    second.configure(other).unwrap();
    assert!(second.coordinate_connection(&mut connection).is_err());
    let request = request();
    let cancel = CancellationToken::new();
    let result = tokio::time::timeout(
        Duration::from_millis(100),
        second.acquire(&request, &cancel),
    )
    .await
    .unwrap();
    assert!(result.err().unwrap().to_string().contains("incompatible"));
}
