#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "Assertions in bounded local subprocess fixtures report contract failures."
)]

#[cfg(unix)]
mod occupancy;

#[cfg(unix)]
mod startup;

#[cfg(all(feature = "native-test-seams", any(unix, windows)))]
mod stable_locks;

#[test]
fn competing_startup_clients_share_one_stable_lock() {
    use std::{fs::TryLockError, sync::Barrier, thread};

    let root = tempfile::tempdir().unwrap();
    let directory =
        super::wire::directory(&root.path().canonicalize().unwrap(), "startup-race").unwrap();
    let barrier = Barrier::new(8);
    let files = thread::scope(|scope| {
        let clients = (0..8)
            .map(|_| {
                scope.spawn(|| {
                    barrier.wait();
                    super::wire::stable_lock(&directory, "startup.lock")
                })
            })
            .collect::<Vec<_>>();
        clients
            .into_iter()
            .map(|client| client.join().unwrap().unwrap())
            .collect::<Vec<_>>()
    });

    files[0].try_lock().unwrap();
    for file in &files[1..] {
        assert!(matches!(file.try_lock(), Err(TryLockError::WouldBlock)));
    }
    let reused = super::wire::stable_lock(&directory, "startup.lock").unwrap();
    assert!(matches!(reused.try_lock(), Err(TryLockError::WouldBlock)));
    files[0].unlock().unwrap();
    reused.try_lock().unwrap();
}

#[test]
fn hard_linked_startup_lock_is_rejected_without_replacement() {
    use rightkit_fs::{FsError, handle::ReadOptions};

    let root = tempfile::tempdir().unwrap();
    let directory =
        super::wire::directory(&root.path().canonicalize().unwrap(), "interrupted-publish")
            .unwrap();
    directory.write("publication.tmp", b"").unwrap();
    std::fs::hard_link(
        directory.path().join("publication.tmp"),
        directory.path().join("startup.lock"),
    )
    .unwrap();
    let original = directory.stat("startup.lock").unwrap().unwrap();
    let held = directory
        .open_read("publication.tmp", &ReadOptions::new())
        .unwrap();
    held.try_lock().unwrap();

    let error = super::wire::stable_lock(&directory, "startup.lock").unwrap_err();
    assert!(matches!(
        error.downcast_ref::<FsError>(),
        Some(FsError::HardLinked(_))
    ));
    for name in ["publication.tmp", "startup.lock"] {
        let retained = directory.stat(name).unwrap().unwrap();
        assert_eq!((retained.dev, retained.ino), (original.dev, original.ino));
        let file = directory.open_read(name, &ReadOptions::new()).unwrap();
        assert!(matches!(
            file.try_lock(),
            Err(std::fs::TryLockError::WouldBlock)
        ));
    }
}

#[cfg(unix)]
async fn serving_owner(
    root: &std::path::Path,
) -> (
    tokio::task::JoinHandle<anyhow::Result<()>>,
    super::NativeOwner,
    super::wire::Advertisement,
) {
    serving_domain(root, "fixture", false).await
}

#[cfg(unix)]
async fn serving_domain(
    root: &std::path::Path,
    domain: &str,
    accounted: bool,
) -> (
    tokio::task::JoinHandle<anyhow::Result<()>>,
    super::NativeOwner,
    super::wire::Advertisement,
) {
    use super::{NativeOwner, owner, wire};
    let serving_root = root.to_owned();
    let serving_domain = domain.to_owned();
    let directory = wire::directory(root, domain).unwrap();
    let ownership = wire::stable_lock(&directory, "owner.lock").unwrap();
    ownership.try_lock().unwrap();
    let generation = uuid::Uuid::new_v4();
    let server = tokio::spawn(async move {
        let _ownership = ownership;
        owner::serve(&serving_root, &serving_domain, generation, accounted).await
    });
    let ad = loop {
        if let Ok(directory) = wire::directory(root, domain)
            && let Some(bytes) = directory.read_opt(wire::DISCOVERY, 4096).unwrap()
        {
            let ad = serde_json::from_slice::<wire::Advertisement>(&bytes).unwrap();
            if ad.epoch == generation {
                break ad;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    };
    let handle = NativeOwner::new(root, domain).unwrap();
    *crate::governor::lock(&handle.inner.cached) = Some(ad.clone());
    let mut snapshot = crate::governor::lock(&handle.inner.snapshot);
    snapshot.epoch = Some(ad.epoch);
    snapshot.owner_pid = Some(ad.pid);
    drop(snapshot);
    (server, handle, ad)
}

#[cfg(unix)]
#[tokio::test]
async fn domain_selection_is_shared_and_existing_readiness_stays_fixed() {
    use std::{
        sync::{
            Arc,
            atomic::{AtomicBool, AtomicUsize, Ordering},
        },
        time::Duration,
    };

    use super::{
        NativeCleanupState, NativeCommand, NativeOperation, NativeOwnerAdmission,
        NativeOwnerPermit, NativeOwnerRequest, client, recovery, wire,
    };

    struct Domain {
        changed: AtomicBool,
        probes: AtomicUsize,
    }
    impl NativeOwnerAdmission for Domain {
        fn domain_key(&self) -> anyhow::Result<Option<String>> {
            self.probes.fetch_add(1, Ordering::Relaxed);
            Ok(Some(
                if self.changed.load(Ordering::Relaxed) {
                    "changed"
                } else {
                    "fixture"
                }
                .into(),
            ))
        }

        fn admit(&self, _request: &NativeOwnerRequest) -> anyhow::Result<NativeOwnerPermit> {
            anyhow::bail!("fixture unexpectedly started another helper")
        }
    }

    tokio::time::timeout(Duration::from_secs(8), async {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap().join("state");
        let (initial_server, handle, initial_ad) = serving_domain(&root, "fixture", true).await;
        let (changed_server, _, changed_ad) = serving_domain(&root, "changed", true).await;
        let domain = Arc::new(Domain {
            changed: AtomicBool::new(false),
            probes: AtomicUsize::new(0),
        });
        let handle = handle.with_admission(domain.clone());
        let mut guards = Vec::new();
        for _ in 0..=wire::MAX_READY_CLIENTS {
            guards.push(handle.clone().ready().await.unwrap());
        }
        assert_eq!(domain.probes.load(Ordering::Relaxed), 1);
        let initial = guards.pop().unwrap();
        drop(guards);
        domain.changed.store(true, Ordering::Relaxed);
        tokio::time::sleep(client::DOMAIN_CHECK_INTERVAL + Duration::from_millis(10)).await;
        let changed = handle.ready().await.unwrap();
        assert_eq!(domain.probes.load(Ordering::Relaxed), 2);
        assert_eq!(changed.generation(), changed_ad.epoch);
        assert_eq!(handle.snapshot().epoch, Some(changed_ad.epoch));
        assert_eq!(initial.generation(), initial_ad.epoch);
        assert_eq!(initial.owner().snapshot().epoch, Some(initial_ad.epoch));
        let mut child = initial
            .spawn(NativeCommand::new("/usr/bin/true").unwrap())
            .await
            .unwrap();
        assert!(child.wait().await.unwrap().success());
        child.kill().await.unwrap();
        initial_server.abort();
        assert!(initial_server.await.unwrap_err().is_cancelled());
        let cleanup = recovery::resolve(
            initial.owner(),
            &initial_ad,
            NativeOperation {
                epoch: initial_ad.epoch,
                id: 1,
            },
        )
        .await;
        assert_eq!(cleanup.cleanup, NativeCleanupState::OwnerDied);
        assert_eq!(handle.snapshot().owner_deaths, 1);
        assert_eq!(changed.owner().snapshot().owner_deaths, 1);
        assert_eq!(handle.snapshot().last_dead_epoch, Some(initial_ad.epoch));
        assert_eq!(handle.snapshot().epoch, Some(changed_ad.epoch));
        drop(initial);
        drop(changed);
        changed_server.await.unwrap().unwrap();
    })
    .await
    .unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn parked_readiness_shares_one_connection_and_pins_its_generation() {
    use std::time::Duration;

    use super::{NativeCommand, wire};

    tokio::time::timeout(Duration::from_secs(8), async {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap().join("state");
        let (server, handle, ad) = serving_owner(&root).await;
        let mut guards = Vec::new();
        // More waiting agents than the server's client-connection bound must
        // still share one client pin, without creating operations or effects.
        for _ in 0..=wire::MAX_READY_CLIENTS {
            guards.push(handle.ready().await.unwrap());
        }
        assert!(handle.snapshot().last_operation.is_none());
        let ready = guards.pop().unwrap();
        drop(guards);
        tokio::time::sleep(wire::IDLE + Duration::from_millis(20)).await;
        assert_eq!(ready.generation(), ad.epoch);
        let mut child = ready
            .spawn(NativeCommand::new("/usr/bin/true").unwrap())
            .await
            .unwrap();
        assert!(child.wait().await.unwrap().success());
        child.kill().await.unwrap();
        drop(ready);
        server.await.unwrap().unwrap();
    })
    .await
    .unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn stale_readiness_cannot_launch_in_a_replacement_generation() {
    use std::time::Duration;

    use super::NativeCommand;

    tokio::time::timeout(Duration::from_secs(8), async {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap().join("state");
        let (server, handle, _) = serving_owner(&root).await;
        let ready = handle.ready().await.unwrap();
        server.abort();
        assert!(server.await.unwrap_err().is_cancelled());
        let (replacement, _, ad) = serving_owner(&root).await;
        let marker = temp.path().join("effects");
        assert_ne!(ready.generation(), ad.epoch);
        assert!(
            ready
                .spawn(NativeCommand::new("/bin/sh").unwrap().args([
                    std::ffi::OsStr::new("-c"),
                    std::ffi::OsStr::new("printf x > \"$1\""),
                    std::ffi::OsStr::new("fixture"),
                    marker.as_os_str(),
                ]))
                .await
                .is_err()
        );
        assert!(!marker.exists());
        let fresh = handle.ready().await.unwrap();
        assert_eq!(fresh.generation(), ad.epoch);
        assert_ne!(fresh.generation(), ready.generation());
        let mut child = fresh
            .spawn(NativeCommand::new("/usr/bin/true").unwrap())
            .await
            .unwrap();
        assert!(child.wait().await.unwrap().success());
        child.kill().await.unwrap();
        drop(ready);
        drop(fresh);
        replacement.await.unwrap().unwrap();
    })
    .await
    .unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn staged_demand_precedes_helper_readiness_and_spawn_failure_releases_witness() {
    use std::{
        fs::TryLockError,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        time::Duration,
    };

    use super::{NativeOwner, NativeOwnerAdmission, NativeOwnerPermit, NativeOwnerRequest, wire};
    use crate::{CommandRequest, Executor, Governor, OperatingTarget, ResourceLimits};

    struct Probe {
        governor: Arc<Governor>,
        observed: AtomicBool,
    }
    impl NativeOwnerAdmission for Probe {
        fn admit(&self, request: &NativeOwnerRequest) -> anyhow::Result<NativeOwnerPermit> {
            let witness = std::fs::File::open(request.lifetime_path())?;
            self.observed.store(
                self.governor.snapshot().running == 0
                    && self.governor.snapshot().queued == 1
                    && self.governor.snapshot().reserved_memory_bytes == 0
                    && self.governor.admission_demand().memory_bytes > 0
                    && matches!(witness.try_lock(), Err(TryLockError::WouldBlock)),
                Ordering::Release,
            );
            Ok(request.permit())
        }
    }

    tokio::time::timeout(Duration::from_secs(3), async {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap().join("state");
        let governor = Arc::new(Governor::new(ResourceLimits::default()).unwrap());
        governor
            .install_target(1, OperatingTarget::default(), None)
            .unwrap();
        let probe = Arc::new(Probe {
            governor: governor.clone(),
            observed: AtomicBool::new(false),
        });
        let owner = NativeOwner::new(&root, "fixture")
            .unwrap()
            .with_admission(probe.clone())
            .with_helper_executable(temp.path().join("missing-helper"));
        let executor = Executor::new(governor.clone(), temp.path().join("artifacts"))
            .unwrap()
            .with_native_owner(owner);
        let task = tokio::spawn(async move {
            executor
                .run(
                    CommandRequest {
                        program: "/usr/bin/true".into(),
                        ..CommandRequest::default()
                    },
                    tokio_util::sync::CancellationToken::new(),
                )
                .await
        });
        while governor.snapshot().queued != 1 {
            tokio::task::yield_now().await;
        }
        assert!(!probe.observed.load(Ordering::Acquire));
        assert_eq!(governor.snapshot().running, 0);
        governor
            .install_target(2, OperatingTarget::from_limits(governor.limits()), None)
            .unwrap();
        assert!(task.await.unwrap().is_err());
        assert!(probe.observed.load(Ordering::Acquire));
        assert_eq!(governor.snapshot().running, 0);
        assert_eq!(governor.snapshot().queued, 0);
        let directory = wire::directory(&root, "fixture").unwrap();
        let witness = wire::stable_lock(&directory, "owner.lock").unwrap();
        witness.try_lock().unwrap();
    })
    .await
    .unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn reused_helper_readiness_is_released_before_waiting_after_failed_activation() {
    use std::{
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        time::Duration,
    };

    use super::{NativeOwnerAdmission, NativeOwnerPermit, NativeOwnerRequest};
    use crate::{CommandRequest, Executor, Governor, OperatingTarget, ResourceLimits};

    struct ChangeGrant {
        governor: Arc<Governor>,
        probes: AtomicUsize,
    }
    impl NativeOwnerAdmission for ChangeGrant {
        fn domain_key(&self) -> anyhow::Result<Option<String>> {
            if self.probes.fetch_add(1, Ordering::AcqRel) == 0 {
                self.governor
                    .install_target(1, OperatingTarget::default(), None)?;
            }
            Ok(None)
        }

        fn admit(&self, _request: &NativeOwnerRequest) -> anyhow::Result<NativeOwnerPermit> {
            anyhow::bail!("fixture unexpectedly started another helper")
        }
    }

    tokio::time::timeout(Duration::from_secs(6), async {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap().join("state");
        let (server, handle, _) = serving_domain(&root, "fixture", true).await;
        let governor = Arc::new(Governor::new(ResourceLimits::default()).unwrap());
        let probe = Arc::new(ChangeGrant {
            governor: governor.clone(),
            probes: AtomicUsize::new(0),
        });
        let owner = handle.with_admission(probe.clone());
        let observed_owner = owner.clone();
        let executor = Executor::new(governor.clone(), temp.path().join("artifacts"))
            .unwrap()
            .with_native_owner(owner);
        let cancel = tokio_util::sync::CancellationToken::new();
        let child_cancel = cancel.clone();
        let task = tokio::spawn(async move {
            executor
                .run(
                    CommandRequest {
                        program: "/usr/bin/true".into(),
                        ..CommandRequest::default()
                    },
                    child_cancel,
                )
                .await
        });
        // A retained Ready would prevent this existing helper from exiting.
        server.await.unwrap().unwrap();
        assert_eq!(probe.probes.load(Ordering::Acquire), 1);
        assert_eq!(governor.snapshot().queued, 1);
        assert_eq!(governor.snapshot().running, 0);
        assert_eq!(governor.snapshot().reserved_memory_bytes, 0);
        assert!(observed_owner.snapshot().last_operation.is_none());
        assert!(!task.is_finished());
        cancel.cancel();
        assert!(task.await.unwrap().unwrap().cancelled);
        assert_eq!(governor.snapshot().queued, 0);
    })
    .await
    .unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn lost_readiness_reply_rediscovers_before_submitting_a_launch() {
    use super::{
        NativeCommand,
        wire::{self, Action, Hello},
    };
    use std::time::Duration;

    tokio::time::timeout(Duration::from_secs(10), async {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap().join("state");
        let (server, handle, mut cached) = serving_owner(&root).await;
        let stale = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        cached.port = stale.local_addr().unwrap().port();
        *crate::governor::lock(&handle.inner.cached) = Some(cached);
        let marker = temp.path().join("effects");
        let unsubmitted_marker = marker.clone();
        let disconnected = tokio::spawn(async move {
            let (stream, _) = stale.accept().await.unwrap();
            let mut transport = wire::framed(stream);
            let hello: Hello = wire::receive(&mut transport).await.unwrap();
            assert!(matches!(hello.action, Action::Hold));
            assert!(!unsubmitted_marker.exists());
            // The cached endpoint disappears after Hello and before Held.
            // No Launch has crossed the effect boundary on this connection.
            drop(transport);
        });
        let mut child = handle
            .spawn(NativeCommand::new("/bin/sh").unwrap().args([
                std::ffi::OsStr::new("-c"),
                std::ffi::OsStr::new("printf x >> \"$1\""),
                std::ffi::OsStr::new("fixture"),
                marker.as_os_str(),
            ]))
            .await
            .unwrap();
        assert!(child.wait().await.unwrap().success());
        child.kill().await.unwrap();
        disconnected.await.unwrap();
        assert_eq!(std::fs::read(&marker).unwrap(), b"x");
        assert_eq!(handle.snapshot().last_operation.unwrap().id, 1);
        assert_eq!(handle.snapshot().owner_deaths, 0);
        server.await.unwrap().unwrap();
    })
    .await
    .unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn lost_launch_reply_reconciles_without_replay() {
    use super::{
        NativeCommand, NativeOperationState,
        wire::{self, Action, Hello, Launch, Reply},
    };
    use std::time::Duration;

    tokio::time::timeout(Duration::from_secs(10), async {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap().join("state");
        let (server, handle, ad) = serving_owner(&root).await;
        let mut transport = wire::connect(&ad).await.unwrap();
        wire::send(
            &mut transport,
            &Hello {
                protocol: wire::PROTOCOL,
                epoch: ad.epoch,
                capability: ad.capability,
                domain_key: "fixture".into(),
                domain: Some(wire::observe_domain().await.unwrap()),
                action: Action::Prepare,
            },
        )
        .await
        .unwrap();
        transport
            .codec_mut()
            .set_max_frame_length(wire::MAX_FRAME_BYTES);
        let operation = match wire::receive(&mut transport).await.unwrap() {
            Reply::Prepared(operation) => Some(operation),
            _ => None,
        }
        .expect("prepare failed");
        let marker = temp.path().join("effects");
        let command = NativeCommand::new("/bin/sh").unwrap().args([
            std::ffi::OsStr::new("-c"),
            std::ffi::OsStr::new("printf x >> \"$1\"; exec /bin/sleep 30"),
            std::ffi::OsStr::new("fixture"),
            marker.as_os_str(),
        ]);
        wire::send(
            &mut transport,
            &Launch {
                operation,
                command,
                domain: wire::observe_domain().await.unwrap(),
            },
        )
        .await
        .unwrap();
        while !std::fs::read(&marker).is_ok_and(|bytes| bytes == b"x") {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        // Deliberately lose Started after an observable effect. Closing the
        // lifetime link must clean up, and Query must never launch again.
        drop(transport);
        loop {
            if handle.reconcile(operation).await.unwrap() == NativeOperationState::Finished {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(std::fs::read(&marker).unwrap(), b"x");
        let expired = super::NativeOperation {
            epoch: operation.epoch,
            id: u64::MAX,
        };
        assert_eq!(
            handle.reconcile(expired).await.unwrap(),
            NativeOperationState::Expired
        );
        assert_eq!(std::fs::read(&marker).unwrap(), b"x");
        server.await.unwrap().unwrap();
    })
    .await
    .unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn incompatible_requests_are_rejected_before_native_effects() {
    use super::{
        NativeCommand, NativeOperation,
        wire::{self, Action, Hello, Launch, Reply},
    };
    use std::time::Duration;

    tokio::time::timeout(Duration::from_secs(10), async {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap().join("state");
        let (server, handle, ad) = serving_owner(&root).await;
        let marker = temp.path().join("effects");
        for mismatch in ["protocol", "capability", "domain", "affinity", "namespace"] {
            let mut hello = Hello {
                protocol: wire::PROTOCOL,
                epoch: ad.epoch,
                capability: ad.capability,
                domain_key: "fixture".into(),
                domain: Some(wire::observe_domain().await.unwrap()),
                action: Action::Prepare,
            };
            match mismatch {
                "protocol" => hello.protocol += 1,
                "capability" => hello.capability = uuid::Uuid::new_v4(),
                "domain" => {
                    let mut domain = serde_json::to_value(hello.domain.as_ref().unwrap()).unwrap();
                    domain["visibility"] = "incompatible-fixture".into();
                    hello.domain = Some(serde_json::from_value(domain).unwrap());
                }
                placement => {
                    let mut domain = serde_json::to_value(hello.domain.as_ref().unwrap()).unwrap();
                    let field = if placement == "affinity" {
                        "affinity"
                    } else {
                        "namespaces"
                    };
                    let values = domain["placement"][field].as_array_mut().unwrap();
                    let incompatible = if field == "affinity" {
                        serde_json::json!(u32::MAX)
                    } else {
                        serde_json::json!([u64::MAX, u64::MAX])
                    };
                    if values.is_empty() {
                        values.push(incompatible);
                    } else {
                        values[0] = incompatible;
                    }
                    hello.domain = Some(serde_json::from_value(domain).unwrap());
                }
            }
            let mut transport = wire::connect(&ad).await.unwrap();
            wire::send(&mut transport, &hello).await.unwrap();
            let command = NativeCommand::new("/bin/sh").unwrap().args([
                std::ffi::OsStr::new("-c"),
                std::ffi::OsStr::new("printf x > \"$1\""),
                std::ffi::OsStr::new("fixture"),
                marker.as_os_str(),
            ]);
            let _ = wire::send(
                &mut transport,
                &Launch {
                    operation: NativeOperation {
                        epoch: ad.epoch,
                        id: 1,
                    },
                    command,
                    domain: wire::observe_domain().await.unwrap(),
                },
            )
            .await;
            let reply = wire::receive::<Reply>(&mut transport).await;
            assert!(reply.is_err() || matches!(reply, Ok(Reply::Rejected(_))));
        }
        assert!(!marker.exists());
        let mut child = handle
            .spawn(NativeCommand::new("/usr/bin/true").unwrap())
            .await
            .unwrap();
        assert!(child.wait().await.unwrap().success());
        assert_eq!(handle.snapshot().last_operation.unwrap().id, 1);
        assert!(!marker.exists());
        server.await.unwrap().unwrap();
    })
    .await
    .unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn placement_change_after_prepare_blocks_effects_and_does_not_block_query() {
    use super::{
        NativeCommand, NativeOperationState,
        wire::{self, Action, Hello, Launch, Reply},
    };
    use std::time::Duration;

    tokio::time::timeout(Duration::from_secs(10), async {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap().join("state");
        let (server, handle, ad) = serving_owner(&root).await;
        let mut transport = wire::connect(&ad).await.unwrap();
        let mut hello = Hello {
            protocol: wire::PROTOCOL,
            epoch: ad.epoch,
            capability: ad.capability,
            domain_key: "fixture".into(),
            domain: Some(wire::observe_domain().await.unwrap()),
            action: Action::Prepare,
        };
        wire::send(&mut transport, &hello).await.unwrap();
        let operation = match wire::receive(&mut transport).await.unwrap() {
            Reply::Prepared(operation) => Some(operation),
            _ => None,
        }
        .expect("native prepare failed");
        let mut changed = serde_json::to_value(hello.domain.as_ref().unwrap()).unwrap();
        changed["visibility"] = "changed-before-effects".into();
        let changed = serde_json::from_value(changed).unwrap();
        let marker = temp.path().join("effects");
        let command = NativeCommand::new("/bin/sh").unwrap().args([
            std::ffi::OsStr::new("-c"),
            std::ffi::OsStr::new("printf x > \"$1\""),
            std::ffi::OsStr::new("fixture"),
            marker.as_os_str(),
        ]);
        wire::send(
            &mut transport,
            &Launch {
                operation,
                command,
                domain: changed,
            },
        )
        .await
        .unwrap();
        assert!(wire::receive::<Reply>(&mut transport).await.is_err());
        assert_eq!(
            handle.reconcile(operation).await.unwrap(),
            NativeOperationState::Finished
        );
        // Recovery is read-only and must work even after the caller moves.
        hello.action = Action::Query(operation);
        hello.domain = None;
        let mut query = wire::connect(&ad).await.unwrap();
        wire::send(&mut query, &hello).await.unwrap();
        assert!(matches!(
            wire::receive::<Reply>(&mut query).await.unwrap(),
            Reply::State(NativeOperationState::Finished)
        ));
        assert!(!marker.exists());
        drop(query);
        drop(transport);
        server.await.unwrap().unwrap();
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn park_admission_retains_existing_leases_at_generation_exhaustion() {
    use crate::{Governor, OperatingTarget, Reservation, ResourceLimits};
    use std::{
        sync::Arc,
        time::{Duration, Instant},
    };
    use tokio_util::sync::CancellationToken;

    let governor = Arc::new(Governor::new(ResourceLimits::default()).unwrap());
    let lease = governor
        .acquire("running", Reservation::default(), &CancellationToken::new())
        .await
        .unwrap();
    governor
        .install_target(
            u64::MAX,
            OperatingTarget::from_limits(governor.limits()),
            Some(Instant::now() + Duration::from_secs(5)),
        )
        .unwrap();
    governor.park_admission();
    let parked = governor.operating_snapshot();
    assert_eq!(parked.generation, u64::MAX);
    assert!(!parked.contact_valid);
    assert_eq!(governor.snapshot().running, 1);
    let cancel = CancellationToken::new();
    let pending = governor.acquire("queued", Reservation::default(), &cancel);
    assert!(
        tokio::time::timeout(Duration::from_millis(20), pending)
            .await
            .is_err()
    );
    drop(lease);
    assert_eq!(governor.snapshot().running, 0);
}

#[tokio::test]
async fn stale_recovery_observation_cannot_unpark_admission() {
    use crate::{Governor, OperatingTarget, Reservation, ResourceLimits};
    use std::sync::Arc;
    use tokio_util::sync::CancellationToken;

    let governor = Arc::new(Governor::new(ResourceLimits::default()).unwrap());
    let lease = governor
        .acquire("running", Reservation::default(), &CancellationToken::new())
        .await
        .unwrap();
    let stale = governor.admission_revision();
    let target = OperatingTarget::from_limits(governor.limits());
    governor.park_admission();
    assert!(
        governor
            .install_target_for_revision(stale, 1, target, None)
            .is_err()
    );
    assert!(!governor.operating_snapshot().contact_valid);
    assert_eq!(governor.snapshot().running, 1);
    let fresh = governor.admission_revision();
    assert_ne!(fresh, stale);
    governor
        .install_target_for_revision(fresh, 1, target, None)
        .unwrap();
    assert!(governor.operating_snapshot().contact_valid);
    drop(lease);
    assert_eq!(governor.snapshot().running, 0);
}
