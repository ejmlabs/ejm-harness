#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "bounded provider ledger fixtures"
)]

use super::*;
use crate::QuotaDomain;
use std::{
    process::Command,
    time::{Duration, Instant},
};

fn connection() -> Connection {
    let connection = Connection::open_in_memory().unwrap();
    owners(&connection);
    connection
}

fn owners(connection: &Connection) {
    connection
        .execute_batch(
            "CREATE TABLE owners(id TEXT PRIMARY KEY,state TEXT NOT NULL);
        INSERT INTO owners VALUES('one','active'),('two','active'),('three','active');",
        )
        .unwrap();
    initialize(connection).unwrap();
}

fn demand(owner: &str, family: &str) -> Demand {
    let domain = QuotaDomain::new(
        "https://provider.invalid",
        "00000000-0000-0000-0000-000000000001",
        family,
    )
    .unwrap();
    Demand {
        owner: owner.into(),
        domain: domain.id().into(),
        policy: ProviderCapacity {
            domain: Some(domain),
            max_in_flight: Some(2),
            requests: Some(ProviderRate {
                limit: 6,
                window_ms: 60_000,
            }),
            tokens: Some(ProviderRate {
                limit: 6000,
                window_ms: 60_000,
            }),
        },
        hard: 1,
        desired: 1,
        active: 0,
        waiting: 4,
        tokens_needed: 4000,
        largest_request: 1000,
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

fn remaining(demand: &mut Demand, grant: &Grant) {
    demand.requests_left = grant.request_credit.unwrap_or(0);
    demand.tokens_left = grant.token_credit.unwrap_or(0);
    demand.expires = grant.expires;
    demand.epoch = grant.epoch;
}

#[test]
fn shared_domains_charge_rates_and_unrelated_accounts_stay_available() {
    let mut connection = connection();
    let first = allocate(&mut connection, &demand("one", "shared"), 10_000).unwrap();
    let second = allocate(&mut connection, &demand("two", "shared"), 10_000).unwrap();
    assert_eq!(first.target + second.target, 2);
    assert_eq!(
        first.request_credit.unwrap() + second.request_credit.unwrap(),
        6
    );
    assert_eq!(
        first.token_credit.unwrap() + second.token_credit.unwrap(),
        6000
    );
    let unrelated = allocate(&mut connection, &demand("three", "other"), 10_000).unwrap();
    assert_eq!(unrelated.request_credit, Some(4));
    assert_eq!(unrelated.token_credit, Some(4000));
}

#[test]
fn rolling_charges_survive_owner_removal_and_calendar_boundaries() {
    let mut connection = connection();
    let mut first = demand("one", "shared");
    first.policy.requests.as_mut().unwrap().limit = 4;
    let grant = allocate(&mut connection, &first, 59_900).unwrap();
    assert_eq!(grant.request_credit, Some(4));
    connection
        .execute("DELETE FROM owners WHERE id='one'", [])
        .unwrap();
    let mut next = demand("two", "shared");
    next.policy.requests = first.policy.requests;
    assert_eq!(
        allocate(&mut connection, &next, 60_001)
            .unwrap()
            .request_credit,
        Some(0)
    );
    let retained = load_domain(&connection, &next.domain).unwrap();
    let until = retained.request.as_ref().unwrap().ends;
    assert!(until >= 59_900 + CREDIT_TIME_MS + 60_000);
    assert_eq!(
        allocate(&mut connection, &next, until - 1)
            .unwrap()
            .request_credit,
        Some(0)
    );
    assert_eq!(
        allocate(&mut connection, &next, until)
            .unwrap()
            .request_credit,
        Some(4)
    );
}

#[test]
fn prepaid_rates_survive_database_reopen_after_all_owners_depart() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("provider.sqlite");
    let mut connection = Connection::open(&path).unwrap();
    owners(&connection);
    let first = allocate(&mut connection, &demand("one", "shared"), 10_000).unwrap();
    let second = allocate(&mut connection, &demand("two", "shared"), 10_000).unwrap();
    assert_eq!(
        first.request_credit.unwrap() + second.request_credit.unwrap(),
        6
    );
    assert_eq!(
        first.token_credit.unwrap() + second.token_credit.unwrap(),
        6000
    );
    connection.execute("DELETE FROM owners", []).unwrap();
    drop(connection);

    let mut reopened = Connection::open(path).unwrap();
    reopened
        .execute("INSERT INTO owners VALUES('three','active')", [])
        .unwrap();
    let retained = allocate(&mut reopened, &demand("three", "shared"), 10_001).unwrap();
    assert_eq!(retained.request_credit, Some(0));
    assert_eq!(retained.token_credit, Some(0));
    assert_eq!(
        load_domain(&reopened, &demand("three", "shared").domain)
            .unwrap()
            .request
            .unwrap()
            .charged,
        6
    );
}

#[test]
fn renewals_do_not_extend_old_prepaid_credit_or_refund_unknown_usage() {
    let mut connection = connection();
    let mut first = demand("one", "shared");
    let grant = allocate(&mut connection, &first, 10_000).unwrap();
    remaining(&mut first, &grant);
    let renewed = allocate(&mut connection, &first, 10_250).unwrap();
    assert_eq!(renewed.expires, grant.expires);
    assert_eq!(renewed.request_credit, Some(0));
    assert_eq!(renewed.token_credit, Some(0));
    first.waiting = 0;
    first.requests_left = 0;
    first.tokens_left = 0;
    let drained = allocate(&mut connection, &first, grant.expires).unwrap();
    assert_eq!(drained.request_credit, Some(0));
    let retained = load_domain(&connection, &first.domain).unwrap();
    assert_eq!(retained.request.unwrap().charged, 4);
    assert_eq!(retained.token.unwrap().charged, 4000);
}

#[test]
fn slot_reduction_retains_usage_and_stale_releases_cannot_refund_it() {
    let mut connection = connection();
    let mut first = demand("one", "shared");
    first.hard = 4;
    first.desired = 4;
    first.policy.max_in_flight = Some(4);
    let old = allocate(&mut connection, &first, 10_000).unwrap();
    assert_eq!(old.target, 4);
    let mut second = demand("two", "shared");
    second.hard = 4;
    second.desired = 4;
    second.policy.max_in_flight = Some(4);
    assert_eq!(
        allocate(&mut connection, &second, 10_000).unwrap().target,
        0
    );
    first.desired = 0;
    first.active = 1;
    let lower = allocate(&mut connection, &first, 10_500).unwrap();
    assert!(release_slots(&mut connection, "one", &old, 0).is_err());
    assert_eq!(
        allocate(&mut connection, &second, 10_500).unwrap().target,
        0
    );
    release_slots(&mut connection, "one", &lower, 1).unwrap();
    assert_eq!(
        allocate(&mut connection, &second, 10_500).unwrap().target,
        3
    );
    connection
        .execute("UPDATE owners SET state='quarantined' WHERE id='one'", [])
        .unwrap();
    assert_eq!(
        allocate(&mut connection, &second, 11_000).unwrap().target,
        3
    );
}

#[test]
fn cooldown_reentry_is_shared_and_new_feedback_survives_an_explicit_clear() {
    let mut connection = connection();
    let mut first = demand("one", "shared");
    let deadline = 10_000 + 20 * 60 * 1000;
    first.feedback.push((
        10_000,
        ProviderObservation::Throttled {
            retry: ProviderRetry::BeyondHorizon {
                retry_at_ms: Some(deadline),
            },
        },
    ));
    assert_eq!(
        allocate(&mut connection, &first, 10_000).unwrap().cooldown,
        deadline
    );
    let second = demand("two", "shared");
    assert_eq!(
        allocate(&mut connection, &second, deadline - 1)
            .unwrap()
            .target,
        0
    );
    first.feedback.clear();
    let reentry = allocate(&mut connection, &first, deadline).unwrap();
    assert_eq!(reentry.target, 1);
    assert_eq!(
        allocate(&mut connection, &second, deadline).unwrap().target,
        0
    );
    first.clear_cooldown = true;
    first.feedback.push((
        deadline,
        ProviderObservation::Throttled {
            retry: ProviderRetry::BeyondHorizon { retry_at_ms: None },
        },
    ));
    assert!(allocate(&mut connection, &first, deadline).unwrap().blocked);
    assert!(
        allocate(&mut connection, &second, deadline)
            .unwrap()
            .blocked
    );
    let other = allocate(&mut connection, &demand("three", "other"), deadline).unwrap();
    assert_eq!(other.target, 1);
}

#[test]
fn excess_actual_usage_is_durable_without_a_following_waiter() {
    let mut connection = connection();
    let mut first = demand("one", "shared");
    first.waiting = 0;
    first.tokens_needed = 0;
    first.token_debt = 7000;
    let grant = allocate(&mut connection, &first, 10_000).unwrap();
    assert_eq!(grant.token_credit, Some(0));
    connection
        .execute("DELETE FROM owners WHERE id='one'", [])
        .unwrap();
    assert_eq!(
        allocate(&mut connection, &demand("two", "shared"), 10_001)
            .unwrap()
            .token_credit,
        Some(0)
    );
    assert_eq!(
        load_domain(&connection, &first.domain)
            .unwrap()
            .token
            .unwrap()
            .charged,
        7000
    );
}

#[test]
fn incompatible_policy_and_failed_commit_leave_previous_charges_intact() {
    let mut connection = connection();
    let first = demand("one", "shared");
    allocate(&mut connection, &first, 10_000).unwrap();
    let mut incompatible = demand("two", "shared");
    incompatible.policy.requests.as_mut().unwrap().window_ms = 30_000;
    let error = allocate(&mut connection, &incompatible, 10_000)
        .err()
        .unwrap();
    assert!(error.downcast_ref::<ContractError>().is_some());
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM provider_owners WHERE owner='two'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    connection.execute_batch("CREATE TRIGGER refuse_domain BEFORE INSERT ON provider_domains BEGIN SELECT RAISE(ABORT,'fixture write failure'); END;").unwrap();
    assert!(allocate(&mut connection, &demand("three", "other"), 10_000).is_err());
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM provider_owners WHERE owner='three'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    assert_eq!(
        load_domain(&connection, &first.domain)
            .unwrap()
            .request
            .unwrap()
            .charged,
        4
    );
}

#[test]
fn concurrent_processes_cannot_each_spend_the_shared_rate() {
    const FIXTURE: &str = "EJM_PROVIDER_LEDGER_FIXTURE";
    if let Some(path) = std::env::var_os(FIXTURE) {
        let owner = std::env::var("EJM_PROVIDER_LEDGER_OWNER").unwrap();
        let path = std::path::PathBuf::from(path);
        std::fs::write(path.join(format!("ready-{owner}")), []).unwrap();
        let until = Instant::now() + Duration::from_secs(10);
        while !path.join("go").exists() {
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(5));
        }
        let mut connection = Connection::open(path.join("ledger.sqlite")).unwrap();
        connection.busy_timeout(Duration::from_secs(2)).unwrap();
        let grant = allocate(&mut connection, &demand(&owner, "shared"), 10_000).unwrap();
        std::fs::write(
            path.join(owner),
            serde_json::to_vec(&[
                u64::try_from(grant.target).unwrap(),
                grant.request_credit.unwrap(),
                grant.token_credit.unwrap(),
            ])
            .unwrap(),
        )
        .unwrap();
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let connection = Connection::open(root.path().join("ledger.sqlite")).unwrap();
    owners(&connection);
    drop(connection);
    let mut children: Vec<_> = ["one", "two"].into_iter().map(|owner| {
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "capacity::provider_registry::tests::concurrent_processes_cannot_each_spend_the_shared_rate", "--nocapture"])
            .env(FIXTURE, root.path()).env("EJM_PROVIDER_LEDGER_OWNER", owner)
            .spawn().unwrap()
    }).collect();
    let until = Instant::now() + Duration::from_secs(10);
    while !["one", "two"]
        .iter()
        .all(|owner| root.path().join(format!("ready-{owner}")).exists())
    {
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(5));
    }
    std::fs::write(root.path().join("go"), []).unwrap();
    for child in &mut children {
        assert!(child.wait().unwrap().success());
    }
    let mut totals = [0_u64; 3];
    for owner in ["one", "two"] {
        let result: [u64; 3] =
            serde_json::from_slice(&std::fs::read(root.path().join(owner)).unwrap()).unwrap();
        for (total, value) in totals.iter_mut().zip(result) {
            *total += value;
        }
    }
    assert_eq!(totals, [2, 6, 6000]);
}
