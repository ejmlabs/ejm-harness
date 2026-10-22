//! Real staged queues and grant installation protect useful helper progress.

use std::{sync::Arc, time::Duration};

use ejm_harness_exec::{
    AdmissionPolicy, Governor, Lease, QueuedAdmission, Reservation, ResourceLimits,
};
use tokio_util::sync::CancellationToken;

use super::*;

const MIB: u64 = 1024 * 1024;

struct Participant {
    id: &'static str,
    registry: Registry,
    governor: Arc<Governor>,
    requested: Budget,
    observed: HostObservations,
    native_domain: String,
    _baseline: Lease,
}

impl Participant {
    fn new(root: &Path, id: &'static str, observed: &HostObservations) -> Self {
        let limits = ResourceLimits {
            max_jobs: 1,
            cpu_millis: 1_000,
            memory_bytes: 128 * MIB,
            max_processes: 4,
            ..ResourceLimits::default()
        };
        let requested = Budget::requested(&limits, 1, 1);
        let governor =
            Arc::new(Governor::new_with_policy(limits, AdmissionPolicy::Automatic).unwrap());
        let mut registry = Registry::open(root).unwrap();
        let grant = registry.register(id, requested, observed).unwrap();
        install(&governor, grant).unwrap();
        let baseline = governor
            .try_acquire_memory("baseline", BASELINE_MEMORY)
            .unwrap()
            .unwrap();
        Self {
            id,
            registry,
            governor,
            requested,
            observed: observed.clone(),
            native_domain: crate::capacity::execution_domain(observed),
            _baseline: baseline,
        }
    }

    fn stage(&self, memory: u64) -> QueuedAdmission {
        self.governor
            .stage(
                "native",
                Reservation {
                    cpu_millis: 100,
                    memory_bytes: memory,
                    processes: 1,
                    output_bytes: 0,
                    heavy: false,
                },
            )
            .unwrap()
    }

    fn renew(&mut self) {
        force_control(&self.registry);
        self.renew_current();
    }

    fn renew_current(&mut self) {
        self.registry
            .renew_with_floor(
                self.id,
                self.requested,
                0,
                0,
                &self.governor.snapshot(),
                self.governor.admission_demand(),
                self.governor.admission_floor(),
                self.governor.admission_activity(),
                &self.observed,
                || install(&self.governor, Budget::default()),
                |grant| {
                    install(&self.governor, grant)?;
                    Ok((self.governor.snapshot(), 0))
                },
            )
            .unwrap();
    }

    fn admit(&self, helper: &mut Registry, generation: Uuid) -> bool {
        super::super::admit_generation(
            helper,
            &self.native_domain,
            generation,
            Some(self.id),
            &self.observed,
        )
        .unwrap()
    }

    fn priority(&self) -> i64 {
        self.registry
            .connection
            .query_row(
                "SELECT last_native FROM owners WHERE id=?1",
                [self.id],
                |row| row.get(0),
            )
            .unwrap()
    }

    async fn activate(&self, queued: &mut QueuedAdmission) -> Lease {
        let eligible = tokio::time::timeout(
            Duration::from_secs(2),
            queued.wait_eligible(&CancellationToken::new()),
        )
        .await
        .unwrap()
        .unwrap();
        queued.try_activate(eligible).unwrap().unwrap()
    }
}

fn install(governor: &Arc<Governor>, grant: Budget) -> Result<()> {
    governor.install_target(
        governor.operating_snapshot().generation + 1,
        grant.target(governor.limits()),
        None,
    )
}

fn branch(memory: u64, name: &str) -> HostObservations {
    let mut observed = nested_observations(memory, 16);
    observed.domains.push(ResourceDomainObservation {
        id: name.into(),
        parent: Some("constrained".into()),
        cpu_millis: Some(8_000),
        memory_bytes: Some(memory),
        available_memory_bytes: None,
        max_processes: Some(16),
        available_processes: None,
    });
    observed
}

fn selection(helper: &Registry, generation: Uuid) -> (i64, Budget, String) {
    let (selected, encoded, beneficiary, epoch, at): (i64, String, String, i64, i64) = helper.connection.query_row(
        "SELECT selected_epoch,selected_minimum,selected_owner,epoch,at FROM native_helpers,control WHERE generation=?1",
        [generation.to_string()], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?)),
    ).unwrap();
    assert!(selected > 0 && selected == epoch && at > 0);
    (
        selected,
        serde_json::from_str(&encoded).unwrap(),
        beneficiary,
    )
}

#[test]
fn selected_helpers_commit_in_one_epoch_without_reviving_canceled_or_stale_turns() {
    let (_parent, root) = root();
    let mut participants = ["first", "canceled", "stale", "last"]
        .map(|id| Participant::new(&root, id, &branch(1024 * MIB, id)));
    let mut queued = participants
        .each_ref()
        .map(|participant| Some(participant.stage(16 * MIB)));
    let _lifetimes: Vec<_> = participants
        .iter()
        .map(|participant| ownership(&root, &participant.native_domain))
        .collect();
    let generations = participants.each_ref().map(|_| Uuid::new_v4());
    let mut helper = Registry::open(&root).unwrap();
    for (participant, generation) in participants.iter().zip(generations) {
        assert!(!participant.admit(&mut helper, generation));
    }
    for _ in 0..3 {
        for participant in &mut participants {
            participant.renew();
        }
    }
    let epoch = selection(&helper, generations[0]).0;
    for generation in generations {
        assert_eq!(selection(&helper, generation).0, epoch);
    }
    assert!(participants[0].admit(&mut helper, generations[0]));
    // Converting one planned allowance must leave its siblings eligible.
    for generation in generations {
        assert_eq!(selection(&helper, generation).0, epoch);
    }
    drop(queued[1].take());
    participants[1].renew_current();
    drop(queued[2].take());
    let _resident = participants[2]
        .governor
        .try_acquire_memory("resident", 64 * MIB)
        .unwrap()
        .unwrap();
    queued[2] = Some(participants[2].stage(16 * MIB));
    participants[2].renew_current();
    assert!(!participants[1].admit(&mut helper, generations[1]));
    assert!(!participants[2].admit(&mut helper, generations[2]));
    assert!(participants[3].admit(&mut helper, generations[3]));
    let helpers = snapshot(&helper.connection).unwrap();
    assert_eq!(helpers.committed, 2);
    assert_eq!(helpers.pending, 2);
    for domain in ["host", "constrained"] {
        assert_eq!(
            charged(&helper.connection, domain).unwrap(),
            helper_budget().add(helper_budget())
        );
    }
    for (index, participant) in participants.iter().enumerate() {
        assert_eq!(selection(&helper, generations[index]).0, epoch);
        assert_eq!(
            charged(&helper.connection, participant.id).unwrap(),
            if index == 0 || index == 3 {
                helper_budget()
            } else {
                Budget::default()
            }
        );
        for domain in &participant.observed.domains {
            let total = charged_others(&helper.connection, &domain.id, "").unwrap();
            let envelope = domain_budget(&helper.connection, &domain.id).unwrap();
            assert_eq!(total.min(envelope), total);
        }
    }
}

#[tokio::test]
async fn pending_subset_preserves_a_committed_helpers_still_staged_turn() {
    let (_parent, root) = root();
    let mut first = Participant::new(&root, "first", &branch(160 * MIB, "first-branch"));
    let mut second = Participant::new(&root, "second", &branch(160 * MIB, "second-branch"));
    let mut first_job = first.stage(64 * MIB);
    let mut second_job = second.stage(64 * MIB);
    let _first_lifetime = ownership(&root, &first.native_domain);
    let _second_lifetime = ownership(&root, &second.native_domain);
    let first_generation = Uuid::new_v4();
    let second_generation = Uuid::new_v4();
    let mut helper = Registry::open(&root).unwrap();
    assert!(!first.admit(&mut helper, first_generation));
    assert!(!second.admit(&mut helper, second_generation));
    for _ in 0..3 {
        first.renew();
        second.renew();
    }
    let first_committed = first.admit(&mut helper, first_generation);
    let second_committed = second.admit(&mut helper, second_generation);
    assert_ne!(first_committed, second_committed);
    assert_eq!(snapshot(&helper.connection).unwrap().committed, 1);
    let (participant, queued, other, other_generation) = if first_committed {
        (&mut first, &mut first_job, &mut second, second_generation)
    } else {
        (&mut second, &mut second_job, &mut first, first_generation)
    };
    let priority = participant.priority();
    // A committed helper still owes its staged claimant a useful turn.
    for _ in 0..3 {
        participant.renew();
        other.renew();
    }
    assert_eq!(participant.priority(), priority);
    assert_eq!(participant.governor.admission_activity(), 0);
    assert!(!other.admit(&mut helper, other_generation));
    let lease = participant.activate(queued).await;
    assert_eq!(
        participant.governor.snapshot().reserved_memory_bytes,
        96 * MIB
    );
    assert_eq!(participant.governor.admission_activity(), 1);
    for domain in ["host", "constrained"] {
        assert_eq!(
            charged_others(&helper.connection, domain, "")
                .unwrap()
                .memory,
            160 * MIB
        );
    }
    assert_eq!(snapshot(&helper.connection).unwrap().memory_bytes, 32 * MIB);
    // This progress required no helper lifetime release.
    drop(lease);
    let _next = participant.stage(16 * MIB);
    participant.renew();
    assert!(
        participant.priority() > priority,
        "short completed work earns native service credit"
    );
}

#[tokio::test]
async fn older_compatible_client_can_benefit_from_another_clients_pending_startup() {
    let (_parent, root) = root();
    let observed = nested_observations(160 * MIB, 16);
    let mut launcher = Participant::new(&root, "launcher", &observed);
    let mut older = Participant::new(&root, "older", &observed);
    assert!(older.priority() < launcher.priority());
    let mut launcher_job = launcher.stage(64 * MIB);
    let mut older_job = older.stage(64 * MIB);
    let _lifetime = ownership(&root, &launcher.native_domain);
    let generation = Uuid::new_v4();
    let mut helper = Registry::open(&root).unwrap();
    assert!(!launcher.admit(&mut helper, generation));
    for _ in 0..3 {
        launcher.renew();
        older.renew();
    }
    assert_eq!(selection(&helper, generation).2, older.id);
    assert!(launcher.admit(&mut helper, generation));
    let lease = older.activate(&mut older_job).await;
    drop(lease);
    // The same helper serves the other staged client after the first turn.
    older.renew();
    launcher.renew();
    let lease = launcher.activate(&mut launcher_job).await;
    assert_eq!(snapshot(&helper.connection).unwrap().committed, 1);
    assert_eq!(snapshot(&helper.connection).unwrap().memory_bytes, 32 * MIB);
    drop(lease);
}

#[test]
fn current_retained_floor_invalidates_an_older_helper_selection() {
    let (_parent, root) = root();
    let observed = nested_observations(512 * MIB, 16);
    let mut participant = Participant::new(&root, "claimant", &observed);
    let queued = participant.stage(16 * MIB);
    let _lifetime = ownership(&root, &participant.native_domain);
    let generation = Uuid::new_v4();
    let mut helper = Registry::open(&root).unwrap();
    assert!(!participant.admit(&mut helper, generation));
    participant.renew();
    let before = selection(&helper, generation);
    assert_eq!(before.1.memory, 48 * MIB);
    drop(queued);
    let resident = participant
        .governor
        .try_acquire_memory("resident", 64 * MIB)
        .unwrap()
        .unwrap();
    let _replacement = participant.stage(16 * MIB);
    participant.renew_current();
    assert_eq!(selection(&helper, generation).0, before.0);
    assert!(!participant.admit(&mut helper, generation));
    assert_eq!(snapshot(&helper.connection).unwrap().committed, 0);
    participant.renew();
    assert!(participant.admit(&mut helper, generation));
    assert_eq!(
        participant.governor.admission_floor().memory_bytes,
        96 * MIB
    );
    assert_eq!(snapshot(&helper.connection).unwrap().memory_bytes, 32 * MIB);
    drop(resident);
}

#[test]
fn canceled_staging_cannot_commit_a_selected_helper() {
    let (_parent, root) = root();
    let observed = nested_observations(512 * MIB, 16);
    let mut participant = Participant::new(&root, "claimant", &observed);
    let queued = participant.stage(16 * MIB);
    let _lifetime = ownership(&root, &participant.native_domain);
    let generation = Uuid::new_v4();
    let mut helper = Registry::open(&root).unwrap();
    assert!(!participant.admit(&mut helper, generation));
    participant.renew();
    let before = selection(&helper, generation).0;
    drop(queued);
    participant.renew_current();
    assert_eq!(selection(&helper, generation).0, before);
    assert!(!participant.admit(&mut helper, generation));
    assert_eq!(snapshot(&helper.connection).unwrap().pending, 1);
    assert_eq!(snapshot(&helper.connection).unwrap().memory_bytes, 0);
    assert_eq!(participant.governor.snapshot().running, 0);
}

#[test]
fn another_unsafe_owner_blocks_a_selected_helper_in_the_same_epoch() {
    let (_parent, root) = root();
    let observed = nested_observations(512 * MIB, 16);
    let mut participant = Participant::new(&root, "claimant", &observed);
    let mut peer = Participant::new(&root, "peer", &observed);
    let _queued = participant.stage(16 * MIB);
    let _lifetime = ownership(&root, &participant.native_domain);
    let generation = Uuid::new_v4();
    let mut helper = Registry::open(&root).unwrap();
    assert!(!participant.admit(&mut helper, generation));
    for _ in 0..2 {
        participant.renew();
        peer.renew();
    }
    let before = selection(&helper, generation).0;
    let mut unavailable = observed.clone();
    unavailable.domains.clear();
    assert!(
        peer.registry
            .observe_and_fence("peer", &unavailable, || Ok(()))
            .is_err()
    );
    assert_eq!(selection(&helper, generation).0, before);
    assert!(!participant.admit(&mut helper, generation));
    assert_eq!(snapshot(&helper.connection).unwrap().committed, 0);
}
