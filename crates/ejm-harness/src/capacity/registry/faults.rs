//! Faults exist only in unit-test builds and run on the calling fixture thread.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "Fault fixtures assert the grant protocol at its commit boundaries."
)]

use std::cell::RefCell;

use ejm_harness_exec::{AdmissionPolicy, Lease, Reservation};
use tokio_util::sync::CancellationToken;

use super::{
    tests::{FixtureChildren, charge, force_control, observations, requested, root, wait_file},
    *,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Point {
    ChargeBeforeCommit,
    ChargeCommitted,
    TargetInstalled,
    ReleaseBeforeCommit,
    ReleaseCommitted,
    RemovalCommitted,
}

impl Point {
    fn name(self) -> &'static str {
        match self {
            Self::ChargeBeforeCommit => "charge-before-commit",
            Self::ChargeCommitted => "charge-committed",
            Self::TargetInstalled => "target-installed",
            Self::ReleaseBeforeCommit => "release-before-commit",
            Self::ReleaseCommitted => "release-committed",
            Self::RemovalCommitted => "removal-committed",
        }
    }
}

type Hook = Box<dyn FnMut(Point) -> Result<()>>;

thread_local! {
    static HOOK: RefCell<Option<Hook>> = const { RefCell::new(None) };
}

pub(super) fn hit(point: Point) -> Result<()> {
    HOOK.with_borrow_mut(|hook| hook.as_mut().map_or(Ok(()), |hook| hook(point)))
}

struct FaultGuard;

impl Drop for FaultGuard {
    fn drop(&mut self) {
        HOOK.with_borrow_mut(|hook| *hook = None);
    }
}

fn inject(point: Point, mut action: impl FnMut() -> Result<()> + 'static) -> FaultGuard {
    HOOK.with_borrow_mut(|hook| {
        assert!(hook.is_none(), "fixture already has an active fault");
        *hook = Some(Box::new(
            move |at| {
                if at == point { action() } else { Ok(()) }
            },
        ));
    });
    FaultGuard
}

fn governor() -> Arc<Governor> {
    Arc::new(
        Governor::new_with_policy(ResourceLimits::default(), AdmissionPolicy::Automatic).unwrap(),
    )
}

fn install(governor: &Arc<Governor>, grant: Budget) -> Result<()> {
    governor.install_target(
        governor.operating_snapshot().generation + 1,
        grant.target(governor.limits()),
        None,
    )
}

fn renew_local(
    registry: &mut Registry,
    owner: &str,
    request: Budget,
    governor: &Arc<Governor>,
    cpu: u32,
) -> Result<Budget> {
    let mut used = governor.snapshot();
    used.queued = used.queued.max(usize::from(request.jobs > 0));
    registry.renew(
        owner,
        request,
        0,
        0,
        &used,
        request.operating(),
        &observations(cpu),
        || install(governor, Budget::default()),
        |grant| {
            install(governor, grant)?;
            Ok((governor.snapshot(), 0))
        },
    )
}

async fn acquire(governor: &Arc<Governor>) -> Lease {
    tokio::time::timeout(
        Duration::from_secs(2),
        governor.acquire(
            "running",
            Reservation {
                cpu_millis: 1_000,
                memory_bytes: 64 * 1024 * 1024,
                processes: 1,
                output_bytes: 0,
                heavy: false,
            },
            &CancellationToken::new(),
        ),
    )
    .await
    .unwrap()
    .unwrap()
}

async fn drain(registry: &mut Registry, owner: &str, governor: &Arc<Governor>, lease: Lease) {
    force_control(registry);
    renew_local(registry, owner, Budget::default(), governor, 2_000).unwrap();
    assert_eq!(governor.operating_snapshot().target.cpu_millis, 0);
    assert_eq!(governor.operating_snapshot().debt.cpu_millis, 1_000);
    assert_eq!(charge(registry, owner, "host").cpu, 1_000);
    let generation = governor.operating_snapshot().generation;
    let cancel = CancellationToken::new();
    assert!(
        tokio::time::timeout(
            Duration::from_millis(20),
            governor.wait_drained(generation, &cancel)
        )
        .await
        .is_err()
    );
    drop(lease);
    tokio::time::timeout(
        Duration::from_secs(2),
        governor.wait_drained(generation, &cancel),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(governor.snapshot(), GovernorSnapshot::default());
    assert_eq!(charge(registry, owner, "host").cpu, 1_000);
}

pub(super) fn subprocess(parent: &Path) {
    let selected = std::env::var("EJM_CAPACITY_REGISTRY_BOUNDARY").unwrap();
    let point = [
        Point::ChargeBeforeCommit,
        Point::ChargeCommitted,
        Point::TargetInstalled,
        Point::ReleaseBeforeCommit,
        Point::ReleaseCommitted,
        Point::RemovalCommitted,
    ]
    .into_iter()
    .find(|point| point.name() == selected)
    .unwrap();
    let transition = std::env::var("EJM_CAPACITY_REGISTRY_TRANSITION").unwrap();
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(subprocess_transition(parent, point, &transition));
}

async fn subprocess_transition(parent: &Path, point: Point, transition: &str) {
    let increasing = transition == "increase";
    let cpu = if increasing { 3_000 } else { 2_000 };
    let owner = format!("child-{}", std::process::id());
    let mut registry = Registry::open(&parent.join("capacity")).unwrap();
    let grant = registry
        .register(
            &owner,
            requested(if increasing { 500 } else { 1_000 }),
            &observations(cpu),
        )
        .unwrap();
    let governor = governor();
    install(&governor, grant).unwrap();
    let lease = if increasing {
        None
    } else {
        Some(acquire(&governor).await)
    };
    std::fs::write(parent.join("fault.ready"), b"ready").unwrap();
    wait_file(&parent.join("transition"));
    if let Some(lease) = lease {
        if point == Point::TargetInstalled {
            // Keep the actual lease alive through the target-installation fault.
            run_transition(&mut registry, &owner, &governor, parent, point, transition);
            drop(lease);
            return;
        }
        drain(&mut registry, &owner, &governor, lease).await;
    }
    run_transition(&mut registry, &owner, &governor, parent, point, transition);
}

fn run_transition(
    registry: &mut Registry,
    owner: &str,
    governor: &Arc<Governor>,
    parent: &Path,
    point: Point,
    transition: &str,
) {
    let parent = parent.to_path_buf();
    let held = governor.clone();
    let _fault = inject(point, move || {
        std::fs::write(
            parent.join("local.json"),
            serde_json::to_vec(&(held.operating_snapshot().target, held.snapshot()))?,
        )?;
        std::fs::write(parent.join("at-boundary"), b"ready")?;
        wait_file(&parent.join("finish"));
        anyhow::bail!("injected lost registry result");
    });
    if transition == "depart" {
        assert!(registry.release_departed(governor).is_err());
    } else {
        force_control(registry);
        let increasing = transition == "increase";
        assert!(
            renew_local(
                registry,
                owner,
                if increasing {
                    requested(2_000)
                } else {
                    Budget::default()
                },
                governor,
                if increasing { 3_000 } else { 2_000 },
            )
            .is_err()
        );
    }
}

fn spawn(parent: &Path, point: Point, transition: &str) -> FixtureChildren {
    FixtureChildren(vec![
        std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "capacity::registry::tests::registry_subprocess_fixture",
                "--nocapture",
            ])
            .env("EJM_CAPACITY_REGISTRY_FIXTURE", parent)
            .env("EJM_CAPACITY_REGISTRY_CHILD", "fault")
            .env("EJM_CAPACITY_REGISTRY_BOUNDARY", point.name())
            .env("EJM_CAPACITY_REGISTRY_TRANSITION", transition)
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap(),
    ])
}

fn assert_envelope(registry: &Registry) {
    let charged = charged_others(&registry.connection, "host", "").unwrap();
    assert_eq!(
        charged.min(domain_budget(&registry.connection, "host").unwrap()),
        charged
    );
}

fn assert_local_target(parent: &Path, point: Point, increasing: bool) {
    let (target, used): (OperatingTarget, GovernorSnapshot) =
        serde_json::from_slice(&std::fs::read(parent.join("local.json")).unwrap()).unwrap();
    let expected = if increasing {
        requested(if point == Point::TargetInstalled {
            2_000
        } else {
            500
        })
        .target(&ResourceLimits::default())
    } else {
        OperatingTarget::default()
    };
    assert_eq!(target, expected);
    assert_eq!(
        used,
        if !increasing && point == Point::TargetInstalled {
            GovernorSnapshot {
                running: 1,
                reserved_cpu_millis: 1_000,
                reserved_memory_bytes: 64 * 1024 * 1024,
                reserved_processes: 1,
                ..GovernorSnapshot::default()
            }
        } else {
            GovernorSnapshot::default()
        }
    );
}

fn assert_crash(point: Point, transition: &str) {
    let (parent, root) = root();
    let parent = parent.path().canonicalize().unwrap();
    let increasing = transition == "increase";
    let cpu = if increasing { 3_000 } else { 2_000 };
    let mut children = spawn(&parent, point, transition);
    std::fs::write(parent.join("start"), b"start").unwrap();
    wait_file(&parent.join("fault.ready"));
    let owner = format!("child-{}", children.0[0].id());
    let mut peer = Registry::open(&root).unwrap();
    peer.register("peer", Budget::default(), &observations(cpu))
        .unwrap();
    let governor = governor();
    install(&governor, Budget::default()).unwrap();
    let (slot, key): (u16, String) = peer
        .connection
        .query_row(
            "SELECT slot,file_key FROM owner_witnesses WHERE owner=?1",
            [&owner],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    std::fs::write(parent.join("transition"), b"transition").unwrap();
    wait_file(&parent.join("at-boundary"));
    assert_local_target(&parent, point, increasing);
    assert!(!witness::released(&peer.files, slot, &key).unwrap());
    assert_envelope(&peer);
    let locked = matches!(
        point,
        Point::ChargeBeforeCommit | Point::ReleaseBeforeCommit
    );
    let released = matches!(point, Point::ReleaseCommitted | Point::RemovalCommitted);
    if point != Point::RemovalCommitted {
        assert_eq!(
            charge(&peer, &owner, "host").cpu,
            if point == Point::ChargeBeforeCommit {
                500
            } else if released {
                0
            } else if increasing {
                2_000
            } else {
                1_000
            }
        );
    }
    let request = requested(if increasing { 2_000 } else { 1_000 });
    if locked {
        let error = renew_local(&mut peer, "peer", request, &governor, cpu).unwrap_err();
        assert!(matches!(
            error.downcast_ref::<rusqlite::Error>(),
            Some(rusqlite::Error::SqliteFailure(error, _))
                if error.code == rusqlite::ErrorCode::DatabaseBusy
        ));
        assert_eq!(governor.operating_snapshot().target.cpu_millis, 0);
    } else {
        peer.connection
            .execute("UPDATE owners SET heartbeat=0 WHERE id=?1", [&owner])
            .unwrap();
        assert_eq!(
            recover(&peer.connection, &peer.files, sql_time().unwrap()).unwrap(),
            0
        );
        force_control(&peer);
        assert_eq!(
            renew_local(&mut peer, "peer", request, &governor, cpu)
                .unwrap()
                .cpu,
            if released { request.cpu } else { 0 }
        );
    }
    assert_envelope(&peer);
    children.0[0].kill().unwrap();
    children.0[0].wait().unwrap();
    assert!(witness::released(&peer.files, slot, &key).unwrap());
    peer.connection
        .execute("UPDATE owners SET heartbeat=0 WHERE id=?1", [&owner])
        .unwrap();
    assert_eq!(
        recover(&peer.connection, &peer.files, sql_time().unwrap()).unwrap(),
        usize::from(point != Point::RemovalCommitted)
    );
    assert_eq!(
        recover(&peer.connection, &peer.files, sql_time().unwrap()).unwrap(),
        0
    );
    assert_eq!(
        count(&peer.connection, "SELECT COUNT(*) FROM owners").unwrap(),
        1
    );
    force_control(&peer);
    assert_eq!(
        renew_local(&mut peer, "peer", request, &governor, cpu)
            .unwrap()
            .cpu,
        request.cpu
    );
    assert_envelope(&peer);
}

#[test]
fn increase_crash_boundaries_keep_uninstalled_and_installed_grants_charged() {
    for point in [
        Point::ChargeBeforeCommit,
        Point::ChargeCommitted,
        Point::TargetInstalled,
    ] {
        assert_crash(point, "increase");
    }
}

#[test]
fn reduction_and_departure_crash_boundaries_cannot_double_spend() {
    for point in [
        Point::TargetInstalled,
        Point::ReleaseBeforeCommit,
        Point::ReleaseCommitted,
    ] {
        assert_crash(point, "reduce");
    }
    assert_crash(Point::RemovalCommitted, "depart");
}

fn release_grant(registry: &Registry) -> CommittedGrant {
    CommittedGrant {
        native_activity: false,
        budget: Budget::default(),
        revision: registry
            .connection
            .query_row("SELECT revision FROM owners WHERE id='owner'", [], |row| {
                row.get(0)
            })
            .unwrap(),
        epoch: 0,
        full_service: Budget::default(),
        retained: vec!["host".into()],
        domains: vec!["host".into()],
    }
}

#[tokio::test]
async fn lost_release_results_reconcile_once_and_reject_stale_grants() {
    for point in [Point::ReleaseBeforeCommit, Point::ReleaseCommitted] {
        let (_parent, root) = root();
        let mut registry = Registry::open(&root).unwrap();
        let initial = registry
            .register("owner", requested(1_000), &observations(2_000))
            .unwrap();
        let governor = governor();
        install(&governor, initial).unwrap();
        let lease = acquire(&governor).await;
        drain(&mut registry, "owner", &governor, lease).await;
        let fault = inject(point, || anyhow::bail!("injected lost release result"));
        force_control(&registry);
        assert!(renew_local(&mut registry, "owner", Budget::default(), &governor, 2_000).is_err());
        drop(fault);
        assert_eq!(
            charge(&registry, "owner", "host").cpu,
            if point == Point::ReleaseBeforeCommit {
                1_000
            } else {
                0
            }
        );
        let grant = release_grant(&registry);
        registry
            .complete_release("owner", &grant, &governor.snapshot(), 0)
            .unwrap();
        let after = charge(&registry, "owner", "host");
        registry
            .complete_release("owner", &grant, &governor.snapshot(), 0)
            .unwrap();
        assert_eq!(charge(&registry, "owner", "host"), after);
        assert_eq!(after.cpu, 0);
        force_control(&registry);
        renew_local(&mut registry, "owner", Budget::default(), &governor, 2_000).unwrap();
        assert!(
            registry
                .complete_release("owner", &grant, &governor.snapshot(), 0)
                .is_err()
        );
        assert!(
            charge_increase(
                &registry.connection,
                "owner",
                grant.revision,
                Budget::default(),
                initial,
                &grant.domains
            )
            .is_err()
        );
        let generation = governor.operating_snapshot().generation;
        assert!(
            governor
                .install_target(generation, initial.operating(), None)
                .is_err()
        );
        assert_eq!(governor.operating_snapshot().target.cpu_millis, 0);
        let mut peer = Registry::open(&root).unwrap();
        peer.register("peer", requested(1_000), &observations(2_000))
            .unwrap();
        assert_eq!(charge(&peer, "peer", "host").cpu, 1_000);
        assert_envelope(&peer);
    }
}

#[test]
fn lost_graceful_delete_result_keeps_local_admission_fenced_on_retry() {
    let (_parent, root) = root();
    let mut registry = Registry::open(&root).unwrap();
    let initial = registry
        .register("owner", requested(1_000), &observations(2_000))
        .unwrap();
    let governor = governor();
    install(&governor, initial).unwrap();
    let fault = inject(Point::RemovalCommitted, || {
        anyhow::bail!("injected lost deletion result")
    });
    assert!(registry.release_departed(&governor).is_err());
    drop(fault);
    assert_eq!(registry.owner.as_deref(), Some("owner"));
    assert!(registry.witness.is_some());
    assert_eq!(
        count(&registry.connection, "SELECT COUNT(*) FROM owners").unwrap(),
        0
    );
    let mut peer = Registry::open(&root).unwrap();
    peer.register("peer", requested(1_000), &observations(2_000))
        .unwrap();
    assert_eq!(charge(&peer, "peer", "host").cpu, 1_000);
    registry.release_departed(&governor).unwrap();
    registry.release_departed(&governor).unwrap();
    assert!(registry.owner.is_none());
    assert!(
        governor
            .try_acquire_memory("retained", 1)
            .unwrap()
            .is_none()
    );
    assert_envelope(&peer);
}
