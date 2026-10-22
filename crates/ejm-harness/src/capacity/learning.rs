//! Small disposable summaries. Shared allocation never depends on these writes.

use anyhow::{Result, ensure};
use ejm_harness_exec::HostObservations;
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{
    LearningState, LearningStatus,
    control::{Evidence, Tuner},
    registry::Registry,
};

const VERSION: u32 = 2;
const MAX_PROFILES: i64 = 256;
const MAX_BYTES: i64 = 1024 * 1024;
const MAX_PROFILE_BYTES: usize = 4096;
const FLUSH_MS: u64 = 60_000;
const EXPIRY_MS: u64 = 30 * 24 * 60 * 60 * 1000;
const DECAY_MS: u64 = 7 * 24 * 60 * 60 * 1000;
const MIN_CONFIDENCE: u64 = 32;

#[derive(Clone, Serialize, Deserialize)]
struct Profile {
    version: u32,
    samples: u64,
    target: usize,
    rate: f64,
    p95_micros: u64,
    class: usize,
}

pub(super) struct Learning {
    scopes: [Option<String>; 2],
    epoch: i64,
    seen: [[u64; 8]; 2],
    pending: [Option<Evidence>; 2],
    hints: [Option<usize>; 2],
    status: LearningStatus,
}

/// Only native constraint identities and numeric costs enter the digest. Neither
/// session paths nor credential-derived identifiers are valid inputs.
pub(super) fn environment_key(observations: &HostObservations, context_bytes: u64) -> String {
    let mut digest = Sha256::new();
    digest.update(b"ejm-capacity-learning-v1");
    digest.update(std::env::consts::OS.as_bytes());
    digest.update(std::env::consts::ARCH.as_bytes());
    digest.update(
        observations
            .cpu_millis
            .value
            .unwrap_or_default()
            .to_le_bytes(),
    );
    digest.update(
        observations
            .memory_bytes
            .value
            .unwrap_or_default()
            .to_le_bytes(),
    );
    digest.update(context_bytes.to_le_bytes());
    for domain in &observations.domains {
        digest.update(
            u64::try_from(domain.id.len())
                .unwrap_or(u64::MAX)
                .to_le_bytes(),
        );
        digest.update(domain.id.as_bytes());
        digest.update(domain.cpu_millis.unwrap_or_default().to_le_bytes());
        digest.update(domain.memory_bytes.unwrap_or_default().to_le_bytes());
    }
    format!("{:x}", digest.finalize())
}

impl Learning {
    pub fn open(
        registry: &mut Registry,
        environment: &str,
        domain: Option<&str>,
        now: u64,
    ) -> Self {
        let scopes = scopes(environment, domain);
        let mut learning = Self {
            scopes,
            epoch: 0,
            seen: [[0; 8]; 2],
            pending: [None, None],
            hints: [None, None],
            status: LearningStatus::default(),
        };
        if learning.initialize(registry, now).is_err() {
            learning.status.state = LearningState::Unavailable;
        }
        learning
    }

    fn initialize(&mut self, registry: &mut Registry, now: u64) -> Result<()> {
        create_schema(registry)?;
        self.epoch = registry.connection.query_row(
            "SELECT epoch FROM capacity_learning_meta WHERE singleton=1",
            [],
            |row| row.get(0),
        )?;
        self.load(registry, now)
    }

    fn load(&mut self, registry: &mut Registry, now: u64) -> Result<()> {
        self.status = LearningStatus::default();
        self.hints = [None, None];
        let mut statement = registry.connection.prepare("SELECT substr(scope,1,65),lane,updated,substr(data,1,4097) FROM capacity_learning_profiles WHERE epoch=?1 ORDER BY updated DESC LIMIT 256")?;
        let rows = statement.query_map([self.epoch], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;
        for row in rows {
            let (scope, lane, updated, data) = row?;
            let Ok(lane) = usize::try_from(lane) else {
                continue;
            };
            self.status.profile_count += 1;
            if lane >= 2 || self.scopes[lane].as_deref() != Some(scope.as_str()) {
                continue;
            }
            let Some((profile, confidence)) = decode(&data, updated, now) else {
                continue;
            };
            self.status.samples = self.status.samples.saturating_add(confidence);
            if confidence >= MIN_CONFIDENCE && self.hints[lane].is_none() {
                self.hints[lane] = Some(profile.target);
                self.status.state = LearningState::Learned;
            }
        }
        Ok(())
    }

    pub fn status(&self) -> LearningStatus {
        self.status.clone()
    }

    pub fn native_owner_lost(&mut self, evidence_counts: [[u64; 8]; 2]) {
        self.hints[1] = None;
        self.pending[1] = None;
        self.seen[1] = evidence_counts[1];
    }

    /// This read is part of the bounded control loop, never ordinary admission.
    pub fn refresh(
        &mut self,
        registry: &mut Registry,
        environment: &str,
        domain: Option<&str>,
        now: u64,
        samples: [[u64; 8]; 2],
    ) -> bool {
        let mut reset_evidence = false;
        let result = (|| -> Result<()> {
            let epoch: i64 = registry.connection.query_row(
                "SELECT epoch FROM capacity_learning_meta WHERE singleton=1",
                [],
                |row| row.get(0),
            )?;
            let current = scopes(environment, domain);
            if epoch != self.epoch || current != self.scopes {
                self.epoch = epoch;
                self.scopes = current;
                self.pending = [None, None];
                self.seen = samples;
                reset_evidence = true;
                self.load(registry, now)?;
            }
            Ok(())
        })();
        if result.is_err() {
            self.status.state = LearningState::Unavailable;
        }
        reset_evidence
    }

    /// A hint is installed only after fresh pressure observations. A later shared
    /// grant still controls actual admission, including tighter current caps.
    pub fn apply_hints(&mut self, tuner: &mut Tuner, ceilings: [usize; 2], healthy: bool) {
        if !healthy {
            return;
        }
        for (lane, hint) in self.hints.iter_mut().enumerate() {
            if let Some(target) = hint.take() {
                tuner.apply_hint(lane, target.min(ceilings[lane]));
            }
        }
    }

    pub fn observe(&mut self, tuner: &Tuner) {
        for (lane, evidence) in tuner.evidence().into_iter().enumerate() {
            if let Some(evidence) =
                evidence.filter(|evidence| evidence.samples > self.seen[lane][evidence.class])
            {
                self.pending[lane] = Some(evidence);
            }
        }
    }

    pub fn flush(&mut self, registry: &mut Registry, now: u64, force: bool) {
        if !force && now.saturating_sub(registry.last_flush) < FLUSH_MS {
            return;
        }
        registry.last_flush = now;
        if self.flush_inner(registry, now).is_err() {
            self.status.state = LearningState::Unavailable;
        }
    }

    fn flush_inner(&mut self, registry: &mut Registry, now: u64) -> Result<()> {
        let tx = registry
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let epoch: i64 = tx.query_row(
            "SELECT epoch FROM capacity_learning_meta WHERE singleton=1",
            [],
            |row| row.get(0),
        )?;
        // Reset won the race. Do not put old samples back after the deletion.
        if epoch != self.epoch {
            return Ok(());
        }
        tx.execute(
            "DELETE FROM capacity_learning_profiles WHERE updated<?1 OR epoch<>?2",
            params![integer(now.saturating_sub(EXPIRY_MS))?, self.epoch],
        )?;
        for (lane, evidence) in self.pending.iter().enumerate() {
            let (Some(scope), Some(evidence)) = (&self.scopes[lane], evidence) else {
                continue;
            };
            let delta = evidence
                .samples
                .saturating_sub(self.seen[lane][evidence.class]);
            if delta == 0 {
                continue;
            }
            let key = format!("{scope}:{lane}:{}", evidence.class);
            let prior: Option<(String, i64)> = tx
                .query_row(
                    "SELECT substr(data,1,4097),updated FROM capacity_learning_profiles WHERE key=?1 AND epoch=?2",
                    params![key, self.epoch],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            let prior = prior.and_then(|(data, updated)| decode(&data, updated, now));
            let mut profile = Profile {
                version: VERSION,
                samples: delta,
                target: evidence.target,
                rate: evidence.rate,
                p95_micros: evidence.p95_micros,
                class: evidence.class,
            };
            if let Some((old, confidence)) = prior {
                profile.samples = profile.samples.saturating_add(confidence);
                if old.target < profile.target && old.rate >= profile.rate / 1.10 {
                    profile.target = old.target;
                    profile.rate = old.rate;
                    profile.p95_micros = old.p95_micros;
                }
            }
            let data = serde_json::to_string(&profile)?;
            ensure!(
                data.len() <= MAX_PROFILE_BYTES,
                "capacity profile exceeds its bound"
            );
            tx.execute("INSERT INTO capacity_learning_profiles VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(key) DO UPDATE SET updated=excluded.updated,epoch=excluded.epoch,data=excluded.data", params![key, scope, i64::try_from(lane)?, integer(now)?, self.epoch, data])?;
        }
        loop {
            let (count, bytes): (i64, i64) = tx.query_row("SELECT COUNT(*),COALESCE(SUM(LENGTH(CAST(key AS BLOB))+LENGTH(CAST(scope AS BLOB))+LENGTH(CAST(data AS BLOB))),0) FROM capacity_learning_profiles", [], |row| Ok((row.get(0)?, row.get(1)?)))?;
            if count <= MAX_PROFILES && bytes <= MAX_BYTES {
                break;
            }
            tx.execute("DELETE FROM capacity_learning_profiles WHERE key=(SELECT key FROM capacity_learning_profiles ORDER BY updated,key LIMIT 1)", [])?;
        }
        tx.commit()?;
        for (lane, evidence) in self.pending.iter_mut().enumerate() {
            if let Some(evidence) = evidence.take() {
                self.seen[lane][evidence.class] = evidence.samples;
            }
        }
        let hints = self.hints;
        let result = self.load(registry, now);
        self.hints = hints;
        result
    }
}

fn scopes(environment: &str, domain: Option<&str>) -> [Option<String>; 2] {
    let tool = format!(
        "{:x}",
        Sha256::digest(format!("{VERSION}:{environment}:tools").as_bytes())
    );
    let model = domain.map(|domain| {
        format!(
            "{:x}",
            Sha256::digest(format!("{VERSION}:{environment}:{domain}:models").as_bytes())
        )
    });
    [model, Some(tool)]
}

fn decode(data: &str, updated: i64, now: u64) -> Option<(Profile, u64)> {
    if data.len() > MAX_PROFILE_BYTES {
        return None;
    }
    let age = now.checked_sub(u64::try_from(updated).ok()?)?;
    if age > EXPIRY_MS {
        return None;
    }
    let profile: Profile = serde_json::from_str(data).ok()?;
    if profile.version != VERSION
        || profile.target == 0
        || profile.target > 256
        || profile.class >= 8
        || !profile.rate.is_finite()
        || profile.rate < 0.0
    {
        return None;
    }
    let shift = u32::try_from(age / DECAY_MS).ok()?.min(63);
    let confidence = profile.samples >> shift;
    Some((profile, confidence))
}

fn integer(value: u64) -> Result<i64> {
    Ok(i64::try_from(value)?)
}

fn create_schema(registry: &mut Registry) -> Result<()> {
    let tx = registry
        .connection
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    tx.execute_batch("
      CREATE TABLE IF NOT EXISTS capacity_learning_meta(singleton INTEGER PRIMARY KEY CHECK(singleton=1),epoch INTEGER NOT NULL);
      INSERT OR IGNORE INTO capacity_learning_meta VALUES(1,0);
      CREATE TABLE IF NOT EXISTS capacity_learning_profiles(key TEXT PRIMARY KEY,scope TEXT NOT NULL,lane INTEGER NOT NULL,updated INTEGER NOT NULL,epoch INTEGER NOT NULL,data TEXT NOT NULL CHECK(LENGTH(CAST(data AS BLOB))<=4096));")?;
    tx.commit()?;
    Ok(())
}

pub(super) fn reset(registry: &mut Registry) -> Result<usize> {
    create_schema(registry)?;
    let tx = registry
        .connection
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let removed = tx.execute("DELETE FROM capacity_learning_profiles", [])?;
    tx.execute(
        "UPDATE capacity_learning_meta SET epoch=epoch+1 WHERE singleton=1",
        [],
    )?;
    tx.commit()?;
    Ok(removed)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "Private temporary-state fixtures assert persistence contracts."
)]
mod tests {
    use super::super::control::{Outcome, Pressure, Timings};
    use super::*;
    use crate::{
        CapacityContext, CapacityMode, CapacityPolicy, CapacitySnapshot, Effect, EventKind,
        Harness, HarnessConfig, ModelResponse, ReplayProvider, RunStatus, Session, SessionOptions,
        Tool, ToolCall, ToolContext, ToolDefinition, ToolOutput,
    };
    use async_trait::async_trait;
    use ejm_harness_auth::{
        CredentialStore, FileStore, SecretString, StoredAuth, StoredCredentials,
        StoredRegistration, VaultKeyStore,
    };
    use ejm_harness_exec::{
        Governor, GovernorSnapshot, HostSampler, Lease, Reservation, ResourceLimits,
    };
    use std::{
        collections::BTreeMap,
        fs,
        path::{Path, PathBuf},
        sync::{Arc, Mutex, mpsc},
        time::Duration,
    };
    use tokio_util::sync::CancellationToken;

    #[derive(Default)]
    struct FixtureKeys(Mutex<BTreeMap<String, SecretString>>);

    impl VaultKeyStore for FixtureKeys {
        fn get(&self, id: &str) -> Result<Option<SecretString>> {
            Ok(self.0.lock().unwrap().get(id).cloned())
        }

        fn set(&self, id: &str, value: &SecretString) -> Result<()> {
            self.0.lock().unwrap().insert(id.into(), value.clone());
            Ok(())
        }
    }

    const ARTIFACT_BYTES: &[u8] = b"\0durable artifact\xff\n";

    struct WriteArtifact;

    #[async_trait]
    impl Tool for WriteArtifact {
        fn definition(&self) -> ToolDefinition {
            ToolDefinition {
                name: "write_artifact".into(),
                description: "Write the preservation fixture artifact.".into(),
                input_schema: serde_json::json!({"type": "object"}),
                effect: Effect::Write,
                version: "1".into(),
            }
        }

        async fn execute(&self, _: ToolCall, context: ToolContext) -> Result<ToolOutput> {
            let path = context.cwd.join("preserved-artifact.bin");
            fs::write(&path, ARTIFACT_BYTES)?;
            Ok(ToolOutput {
                content: "Artifact saved.".into(),
                artifact: Some(path),
                ..ToolOutput::default()
            })
        }
    }

    async fn close_journal(harness: Harness, state_dir: &Path) -> Result<()> {
        harness.shutdown().await?;
        drop(harness);
        let lock = fs::OpenOptions::new()
            .read(true)
            .open(state_dir.join("owner.lock"))?;
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                match lock.try_lock() {
                    Ok(()) => return Ok::<(), anyhow::Error>(()),
                    Err(std::fs::TryLockError::Error(error)) => return Err(error.into()),
                    Err(std::fs::TryLockError::WouldBlock) => {
                        tokio::task::yield_now().await;
                    }
                }
            }
        })
        .await??;
        Ok(())
    }

    fn shared_authority(
        registry: &Registry,
        owner: &str,
    ) -> (super::super::Budget, Vec<(String, super::super::Budget)>) {
        let encoded: String = registry
            .connection
            .query_row(
                "SELECT grant_json FROM owners WHERE id=?1",
                [owner],
                |row| row.get(0),
            )
            .unwrap();
        let grant = serde_json::from_str(&encoded).unwrap();
        let mut statement = registry
            .connection
            .prepare("SELECT domain,budget FROM allocations WHERE owner=?1 ORDER BY domain")
            .unwrap();
        let allocations = statement
            .query_map([owner], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .unwrap()
            .map(|row| {
                let (domain, budget) = row.unwrap();
                (domain, serde_json::from_str(&budget).unwrap())
            })
            .collect();
        (grant, allocations)
    }

    async fn seed_credentials(path: &Path, keys: Arc<FixtureKeys>) -> Result<Vec<u8>> {
        FileStore::open_with_key_store(path, keys)?
            .lock(CancellationToken::new())
            .await?
            .save(&StoredAuth {
                host_id: "preserved-host".into(),
                registrations: vec![StoredRegistration {
                    id: "preserved-registration".into(),
                    issuer: "https://issuer.example".into(),
                    subject: "fixture-subject".into(),
                    client_id: "fixture-client".into(),
                    email: None,
                    credentials: Some(StoredCredentials {
                        access_token: Some(SecretString::new("preserved-access")),
                        refresh_token: Some(SecretString::new("preserved-refresh")),
                        id_token: SecretString::new("preserved-identity"),
                        scopes: vec!["openid".into()],
                        expires_at: None,
                        earliest_refresh_at: None,
                    }),
                    authorization_revision: 7,
                    refresh_pending: false,
                }],
            })?;
        Ok(fs::read(path.join("vault.bin"))?)
    }

    struct PreservedSession {
        session: Session,
        history: serde_json::Value,
        artifact: PathBuf,
    }

    async fn create_durable_session(
        config: &HarnessConfig,
        root: &Path,
    ) -> Result<PreservedSession> {
        let harness = Harness::new(
            config.clone(),
            Arc::new(ReplayProvider::new(vec![
                ModelResponse {
                    tool_calls: vec![ToolCall {
                        id: "artifact-call".into(),
                        name: "write_artifact".into(),
                        arguments: serde_json::json!({}),
                    }],
                    ..ModelResponse::default()
                },
                ModelResponse {
                    content: "Artifact retained.".into(),
                    ..ModelResponse::default()
                },
            ])),
            vec![Arc::new(WriteArtifact)],
        )?;
        let session = harness
            .create_session(SessionOptions {
                cwd: root.into(),
                title: "Preserved session".into(),
            })
            .await?;
        let first = tokio::time::timeout(
            Duration::from_secs(5),
            harness
                .start(&session.id, "Save an artifact.".into())
                .await?
                .wait(),
        )
        .await??;
        assert_eq!(first.status, RunStatus::Completed);
        let history = harness.events(&session.id, 0, 100).await?;
        let artifact = root.join("preserved-artifact.bin");
        assert!(history.iter().any(|event| matches!(
            &event.kind,
            EventKind::ToolFinished { output, .. }
                if output.artifact.as_ref() == Some(&artifact) && !output.is_error
        )));
        let session = harness.get_session(&session.id).await?;
        assert_eq!(fs::read(&artifact)?, ARTIFACT_BYTES);
        close_journal(harness, &config.state_dir).await?;
        Ok(PreservedSession {
            session,
            history: serde_json::to_value(history)?,
            artifact,
        })
    }

    fn failing_learning_capacity(
        config: &mut HarnessConfig,
        path: &Path,
    ) -> Result<(CapacityContext, Registry)> {
        let mut registry = Registry::open(path)?;
        create_schema(&mut registry)?;
        let now = crate::types::timestamp_ms();
        let scope = scopes("expired-environment", None)[1].clone().unwrap();
        registry.connection.execute(
            "INSERT INTO capacity_learning_profiles VALUES(?1,?2,1,?3,0,?4)",
            params![
                format!("{scope}:1:0"),
                scope,
                i64::try_from(now.saturating_sub(EXPIRY_MS + 1))?,
                serde_json::to_string(&Profile {
                    version: VERSION,
                    samples: 64,
                    target: 1,
                    rate: 1.0,
                    p95_micros: 1,
                    class: 0,
                })?
            ],
        )?;
        // Reject the real coordinator's first optional expiry write.
        registry.connection.execute_batch(
            "CREATE TRIGGER fail_learning_cleanup BEFORE DELETE ON capacity_learning_profiles
             BEGIN SELECT RAISE(ABORT,'fixture learning write failure'); END;",
        )?;
        let capacity = CapacityContext::prepare_isolated(
            config,
            CapacityPolicy {
                mode: CapacityMode::Fixed,
                max_sessions: Some(2),
                max_model_requests: Some(1),
                max_jobs: Some(2),
                max_memory_bytes: Some(512 * 1024 * 1024),
                ..CapacityPolicy::default()
            },
            path,
        )?;
        Ok((capacity, registry))
    }

    async fn hold_capacity(capacity: &CapacityContext, session: &str) -> Result<(Lease, Lease)> {
        let governor = capacity.governor();
        let cancel = CancellationToken::new();
        let job = tokio::time::timeout(
            Duration::from_secs(3),
            governor.acquire(
                session,
                Reservation {
                    cpu_millis: 1,
                    memory_bytes: 1,
                    processes: 0,
                    output_bytes: 0,
                    heavy: false,
                },
                &cancel,
            ),
        )
        .await??;
        let resident = governor
            .try_acquire_memory("preserved-resident", 64 * 1024)?
            .unwrap();
        Ok((job, resident))
    }

    fn assert_capacity_preserved(
        capacity: &CapacityContext,
        registry: &Registry,
        before: &CapacitySnapshot,
        shared: &(super::super::Budget, Vec<(String, super::super::Budget)>),
        held: &GovernorSnapshot,
    ) -> Result<()> {
        let current = capacity.snapshot();
        assert!(current.coordinated && current.operating.contact_valid);
        assert_eq!(current.hard_model_requests, before.hard_model_requests);
        assert_eq!(
            serde_json::to_value(&current.hard_resources)?,
            serde_json::to_value(&before.hard_resources)?
        );
        assert_eq!(current.model_target, before.model_target);
        assert_eq!(current.operating.target, before.operating.target);
        assert_eq!(&shared_authority(registry, &capacity.inner.owner), shared);
        assert_eq!(&capacity.governor().snapshot(), held);
        Ok(())
    }

    async fn assert_durable_state(
        harness: &Harness,
        preserved: &PreservedSession,
        credentials: &Path,
        keys: Arc<FixtureKeys>,
        ciphertext: &[u8],
    ) -> Result<()> {
        assert_eq!(
            serde_json::to_value(harness.get_session(&preserved.session.id).await?)?,
            serde_json::to_value(&preserved.session)?
        );
        assert_eq!(
            serde_json::to_value(harness.events(&preserved.session.id, 0, 100).await?)?,
            preserved.history
        );
        assert_eq!(fs::read(&preserved.artifact)?, ARTIFACT_BYTES);
        assert_eq!(fs::read(credentials.join("vault.bin"))?, ciphertext);
        let loaded = FileStore::open_with_key_store(credentials, keys)?
            .lock(CancellationToken::new())
            .await?
            .load()?;
        assert_eq!(loaded.host_id, "preserved-host");
        assert_eq!(loaded.registrations[0].authorization_revision, 7);
        let saved = loaded.registrations[0].credentials.as_ref().unwrap();
        assert_eq!(
            saved.access_token.as_ref().unwrap().expose_secret(),
            "preserved-access"
        );
        assert_eq!(
            saved.refresh_token.as_ref().unwrap().expose_secret(),
            "preserved-refresh"
        );
        assert_eq!(saved.id_token.expose_secret(), "preserved-identity");
        Ok(())
    }

    async fn continue_session(
        harness: &Harness,
        preserved: &mut PreservedSession,
        response: &str,
    ) -> Result<()> {
        let continued = tokio::time::timeout(
            Duration::from_secs(5),
            harness
                .start(&preserved.session.id, "Continue ordinary work.".into())
                .await?
                .wait(),
        )
        .await??;
        assert_eq!(continued.status, RunStatus::Completed);
        assert_eq!(continued.text, response);
        let events = harness.events(&preserved.session.id, 0, 100).await?;
        let previous_count = preserved.history.as_array().unwrap().len();
        assert!(events.len() > previous_count);
        assert_eq!(
            serde_json::to_value(&events[..previous_count])?,
            preserved.history
        );
        preserved.history = serde_json::to_value(events)?;
        Ok(())
    }

    async fn wait_for_owners(registry: &Registry, expected: u32) -> Result<()> {
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let owners: u32 = registry
                    .connection
                    .query_row("SELECT COUNT(*) FROM owners", [], |row| row.get(0))
                    .unwrap();
                if owners == expected {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await?;
        Ok(())
    }

    fn observed(tuner: &mut Tuner) {
        for _ in 0..3 {
            for _ in 0..16 {
                tuner.record_model(
                    Outcome::Successful,
                    &Timings {
                        service: Duration::from_millis(5),
                        ..Timings::default()
                    },
                    1,
                );
            }
            tuner.tick(
                Duration::from_secs(2),
                [true, false],
                Pressure::Clear,
                [tuner.models(), tuner.jobs()],
                [8, 4],
            );
        }
    }

    fn seed_profiles(registry: &mut Registry, padded: bool) {
        create_schema(registry).unwrap();
        let data = serde_json::to_string(&Profile {
            version: VERSION,
            samples: 64,
            target: 4,
            rate: 8.0,
            p95_micros: 5_000,
            class: 1,
        })
        .unwrap();
        // Older compatible payloads can contain whitespace within the row cap.
        let data = if padded {
            format!("{data:<4096}")
        } else {
            data
        };
        let tx = registry.connection.transaction().unwrap();
        for index in 0..256 {
            let scope = scopes(&format!("fixture-env-{index}"), Some("opaque-account"))[0]
                .clone()
                .unwrap();
            tx.execute(
                "INSERT INTO capacity_learning_profiles(key,scope,lane,updated,epoch,data) VALUES(?1,?2,0,?3,0,?4)",
                params![format!("{scope}:0:1"), scope, 100_000 + index, data],
            )
            .unwrap();
        }
        tx.commit().unwrap();
    }

    fn profile_footprint(registry: &Registry) -> (usize, usize) {
        let mut statement = registry
            .connection
            .prepare("SELECT key,scope,data FROM capacity_learning_profiles")
            .unwrap();
        let mut rows = statement.query([]).unwrap();
        let (mut count, mut bytes) = (0, 0);
        while let Some(row) = rows.next().unwrap() {
            count += 1;
            for column in 0..3 {
                bytes += row.get::<_, String>(column).unwrap().len();
            }
        }
        (count, bytes)
    }

    #[test]
    fn concurrent_profile_updates_merge_samples_and_evict_the_oldest() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().canonicalize().unwrap().join("registry");
        let mut registry = Registry::open(&path).unwrap();
        seed_profiles(&mut registry, false);
        assert_eq!(profile_footprint(&registry).0, 256);

        // Both learners start before either saves, so neither sees the other's
        // samples until the serialized read/merge/write transaction.
        let writers: Vec<_> = (0..2)
            .map(|_| {
                let mut writer = Registry::open(&path).unwrap();
                let mut learning = Learning::open(
                    &mut writer,
                    "shared-new-environment",
                    Some("opaque-account"),
                    200_000,
                );
                let mut tuner = Tuner::new(4, 2);
                observed(&mut tuner);
                learning.observe(&tuner);
                (writer, learning)
            })
            .collect();
        std::thread::scope(|threads| {
            let mut starts = Vec::new();
            let mut handles = Vec::new();
            for (mut writer, mut learning) in writers {
                let (start, ready) = mpsc::channel();
                starts.push(start);
                handles.push(threads.spawn(move || {
                    ready.recv_timeout(Duration::from_secs(3)).unwrap();
                    learning.flush(&mut writer, 200_000, true);
                    assert_eq!(learning.status().state, LearningState::Learned);
                }));
            }
            for start in starts {
                start.send(()).unwrap();
            }
            for handle in handles {
                handle.join().unwrap();
            }
        });

        let (count, bytes) = profile_footprint(&registry);
        assert_eq!(count, 256);
        assert!(bytes <= 1024 * 1024);
        assert_eq!(
            Learning::open(
                &mut registry,
                "shared-new-environment",
                Some("opaque-account"),
                200_001,
            )
            .status()
            .samples,
            96,
            "concurrent learners must retain both sample deltas"
        );
        assert_eq!(
            Learning::open(
                &mut registry,
                "fixture-env-0",
                Some("opaque-account"),
                200_001,
            )
            .status()
            .samples,
            0,
        );
        assert_eq!(
            Learning::open(
                &mut registry,
                "fixture-env-1",
                Some("opaque-account"),
                200_001,
            )
            .status()
            .samples,
            64,
        );
    }

    #[test]
    fn serialized_byte_budget_evicts_profiles_below_the_count_limit() {
        let root = tempfile::tempdir().unwrap();
        let mut registry =
            Registry::open(&root.path().canonicalize().unwrap().join("registry")).unwrap();
        seed_profiles(&mut registry, true);
        let (count, bytes) = profile_footprint(&registry);
        assert_eq!(count, 256);
        assert!(bytes > 1024 * 1024);

        let mut learning = Learning::open(
            &mut registry,
            "fixture-env-255",
            Some("opaque-account"),
            200_000,
        );
        learning.flush(&mut registry, 200_000, true);
        let (count, bytes) = profile_footprint(&registry);
        assert!(count < 256, "the byte cap must act before the count cap");
        assert!(bytes <= 1024 * 1024);
        assert_eq!(learning.status().samples, 64);
        assert_eq!(
            Learning::open(
                &mut registry,
                "fixture-env-0",
                Some("opaque-account"),
                200_001,
            )
            .status()
            .samples,
            0,
        );
    }

    #[tokio::test]
    async fn optional_learning_failure_and_reset_preserve_durable_state() -> Result<()> {
        let parent = tempfile::tempdir()?;
        let root = parent.path().canonicalize()?;
        let credential_path = root.join("credentials");
        let keys = Arc::new(FixtureKeys::default());
        let ciphertext = seed_credentials(&credential_path, keys.clone()).await?;
        let mut config = HarnessConfig {
            state_dir: root.join("journal"),
            ..HarnessConfig::default()
        };
        let mut preserved = create_durable_session(&config, &root).await?;
        let (capacity, mut registry) =
            failing_learning_capacity(&mut config, &root.join("capacity"))?;
        let harness = Harness::with_capacity(
            config.clone(),
            Some(Arc::new(ReplayProvider::new(vec![
                ModelResponse {
                    content: "Continued with unavailable learning.".into(),
                    ..ModelResponse::default()
                },
                ModelResponse {
                    content: "Continued after learning reset.".into(),
                    ..ModelResponse::default()
                },
            ]))),
            vec![Arc::new(WriteArtifact)],
            vec![],
            capacity.clone(),
        )?;
        let governor = capacity.governor();
        let (job, resident) = hold_capacity(&capacity, &preserved.session.id).await?;
        let held = governor.snapshot();
        let authority = capacity.snapshot();
        let shared = shared_authority(&registry, &capacity.inner.owner);
        assert_eq!(shared.1.len(), authority.observations.domains.len());
        assert_eq!(
            shared.0.target(&authority.hard_resources),
            authority.operating.target
        );
        assert_eq!(shared.0.models, authority.model_target);
        for (expected, response) in [
            (
                LearningState::Unavailable,
                "Continued with unavailable learning.",
            ),
            (LearningState::Cold, "Continued after learning reset."),
        ] {
            if expected == LearningState::Cold {
                registry
                    .connection
                    .execute_batch("DROP TRIGGER fail_learning_cleanup")?;
                assert_eq!(reset(&mut registry)?, 1);
                assert_eq!(profile_footprint(&registry).0, 0);
            }
            capacity.notify_demand();
            tokio::time::timeout(Duration::from_secs(5), async {
                while capacity.snapshot().learning.state != expected {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await?;
            assert_eq!(
                harness.capacity_snapshot().unwrap().learning.state,
                expected
            );
            assert_capacity_preserved(&capacity, &registry, &authority, &shared, &held)?;
            assert_durable_state(
                &harness,
                &preserved,
                &credential_path,
                keys.clone(),
                &ciphertext,
            )
            .await?;
            continue_session(&harness, &mut preserved, response).await?;
            assert_eq!(governor.snapshot(), held);
        }
        assert_eq!(fs::read(&preserved.artifact)?, ARTIFACT_BYTES);
        drop(resident);
        drop(job);
        assert_eq!(governor.snapshot().running, 0);
        assert_eq!(
            governor.snapshot().reserved_memory_bytes,
            held.reserved_memory_bytes - 64 * 1024 - 1
        );
        close_journal(harness, &config.state_dir).await?;
        drop(capacity);
        wait_for_owners(&registry, 0).await?;
        Ok(())
    }

    #[tokio::test]
    async fn learning_write_failure_and_reset_preserve_live_capacity_leases() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().canonicalize().unwrap().join("registry");
        let context = CapacityContext::prepare_isolated(
            &mut HarnessConfig::default(),
            CapacityPolicy {
                mode: CapacityMode::Fixed,
                ..CapacityPolicy::default()
            },
            &path,
        )
        .unwrap();
        let governor = context.governor();
        let cancel = CancellationToken::new();
        let job = tokio::time::timeout(
            Duration::from_secs(3),
            governor.acquire(
                "active-job",
                Reservation {
                    cpu_millis: 1,
                    memory_bytes: 1,
                    processes: 0,
                    output_bytes: 0,
                    heavy: false,
                },
                &cancel,
            ),
        )
        .await
        .unwrap()
        .unwrap();
        let resident = governor
            .try_acquire_memory("active-resident", 64 * 1024)
            .unwrap()
            .unwrap();
        let before = governor.snapshot();
        let ceilings = serde_json::to_value(context.snapshot().hard_resources).unwrap();
        let now = crate::types::timestamp_ms();
        let mut registry = Registry::open(&path).unwrap();
        let mut learning =
            Learning::open(&mut registry, "fixture-env", Some("opaque-account"), now);
        let mut tuner = Tuner::new(4, 2);
        observed(&mut tuner);
        learning.observe(&tuner);
        learning.flush(&mut registry, now, true);
        assert_eq!(profile_footprint(&registry).0, 1);

        registry.connection.execute_batch("CREATE TRIGGER fail_learning_write BEFORE INSERT ON capacity_learning_profiles BEGIN SELECT RAISE(ABORT,'fixture write failure'); END;").unwrap();
        let mut failed = Learning::open(
            &mut registry,
            "other-environment",
            Some("opaque-account"),
            now,
        );
        failed.observe(&tuner);
        failed.flush(&mut registry, now + 1, true);
        assert_eq!(failed.status().state, LearningState::Unavailable);
        assert_eq!(governor.snapshot(), before);

        // Exercise the public context and leases with the same reset backend,
        // without changing the process's real HOME or its shared host profiles.
        assert_eq!(reset(&mut registry).unwrap(), 1);
        assert_eq!(profile_footprint(&registry).0, 0);
        assert_eq!(governor.snapshot(), before);
        assert_eq!(
            serde_json::to_value(context.snapshot().hard_resources).unwrap(),
            ceilings
        );
        drop(resident);
        drop(job);
        assert_eq!(governor.snapshot().running, 0);
        assert_eq!(
            governor.snapshot().reserved_memory_bytes,
            before.reserved_memory_bytes - 64 * 1024 - 1
        );
        drop(context);
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let count: u32 = registry
                    .connection
                    .query_row("SELECT COUNT(*) FROM owners", [], |row| row.get(0))
                    .unwrap();
                if count == 0 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
    }

    fn constrained_startup(
        registry: &mut Registry,
        root: &Path,
        profile: &str,
    ) -> Result<(CapacityContext, ResourceLimits)> {
        let observations = HostSampler::default().sample();
        let mut previous_environment = observations.clone();
        if profile == "larger-environment" {
            previous_environment.cpu_millis.value = previous_environment
                .cpu_millis
                .value
                .map(|value| value.saturating_mul(2));
            previous_environment.memory_bytes.value = previous_environment
                .memory_bytes
                .value
                .map(|value| value.saturating_mul(2));
            for domain in &mut previous_environment.domains {
                domain.cpu_millis = domain.cpu_millis.map(|value| value.saturating_mul(2));
                domain.memory_bytes = domain.memory_bytes.map(|value| value.saturating_mul(2));
            }
        }
        // One bounded dispatch slot keeps this fixture's baseline small.
        let mut config = HarnessConfig {
            resources: ResourceLimits {
                max_queue: 33,
                ..ResourceLimits::default()
            },
            ..HarnessConfig::default()
        };
        let environment = environment_key(
            &previous_environment,
            u64::try_from(config.context_max_bytes)?,
        );
        let scope = scopes(&environment, None)[1].clone().unwrap();
        let data = if profile == "corrupt" {
            "invalid learning JSON".into()
        } else {
            serde_json::to_string(&Profile {
                version: VERSION,
                samples: 64,
                target: 128,
                rate: 8.0,
                p95_micros: 5_000,
                class: 0,
            })?
        };
        registry.connection.execute(
            "INSERT INTO capacity_learning_profiles VALUES(?1,?2,1,?3,0,?4)",
            params![
                format!("{scope}:1:0"),
                scope,
                i64::try_from(crate::types::timestamp_ms())?,
                data
            ],
        )?;

        // This live participant retains all model credits and the current
        // native CPU slice. A learned hint cannot reclaim either charge.
        let occupied = ResourceLimits {
            max_jobs: 1,
            cpu_millis: observations.cpu_millis.value.unwrap(),
            memory_bytes: 64 * 1024 * 1024,
            ..ResourceLimits::default()
        };
        let grant = registry.register(
            "existing-startup-owner",
            super::super::Budget::requested(&occupied, 256, 1),
            &observations,
        )?;
        assert_eq!(grant.models, 256);
        assert!(grant.target(&occupied).cpu_millis > 0);
        let capacity = CapacityContext::prepare_isolated(
            &mut config,
            CapacityPolicy {
                max_model_requests: Some(2),
                max_jobs: Some(1),
                max_memory_bytes: Some(128 * 1024 * 1024),
                max_heavy_jobs: Some(0),
                max_processes: Some(1),
                ..CapacityPolicy::default()
            },
            root,
        )?;
        Ok((capacity, occupied))
    }

    #[tokio::test]
    async fn learned_and_cold_startups_obey_current_caps_and_existing_shared_grants() -> Result<()>
    {
        for profile in ["compatible", "larger-environment", "corrupt"] {
            let parent = tempfile::tempdir()?;
            let root = parent.path().canonicalize()?.join("capacity");
            let mut registry = Registry::open(&root)?;
            create_schema(&mut registry)?;
            let (capacity, occupied) = constrained_startup(&mut registry, &root, profile)?;
            let initial = capacity.snapshot();
            let expected = if profile == "compatible" {
                LearningState::Learned
            } else {
                LearningState::Cold
            };
            assert_eq!(initial.learning.state, expected, "{profile}");
            assert_eq!(initial.model_target, 0, "{profile}");
            assert_eq!(initial.operating.target.cpu_millis, 0, "{profile}");
            let baseline = super::super::BASELINE_MEMORY + super::super::DISPATCH_MEMORY;
            assert_eq!(
                capacity.governor().snapshot().reserved_memory_bytes,
                baseline,
                "{profile}"
            );
            assert_eq!(initial.operating.memory_leases, 1, "{profile}");
            assert!(
                initial.operating.target.memory_bytes >= baseline,
                "{profile}"
            );
            let (grant, allocations) = shared_authority(&registry, &capacity.inner.owner);
            assert!(grant.target(&initial.hard_resources).memory_bytes >= baseline);
            assert_eq!(allocations.len(), initial.observations.domains.len());
            assert!(allocations.iter().all(|(_, charge)| {
                charge.target(&initial.hard_resources).memory_bytes >= baseline
            }));
            capacity.notify_demand();
            tokio::time::timeout(Duration::from_secs(3), async {
                while capacity.snapshot().operating.generation == initial.operating.generation {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await?;
            let current = capacity.snapshot();
            assert!(
                current.coordinated && current.operating.contact_valid,
                "{profile}"
            );
            assert_eq!(current.hard_model_requests, 2);
            assert_eq!(current.hard_resources.max_jobs, 1);
            assert_eq!(current.hard_resources.memory_bytes, 128 * 1024 * 1024);
            assert_eq!(current.model_target, 0, "{profile}");
            assert_eq!(current.operating.target.cpu_millis, 0, "{profile}");
            let shared = shared_authority(&registry, &capacity.inner.owner).0;
            assert_eq!(
                current.operating.target,
                shared.target(&current.hard_resources)
            );
            assert_eq!(current.model_target, shared.models);
            assert!(super::super::lock(&capacity.inner.state).tuner.jobs() <= 1);
            assert!(
                tokio::time::timeout(
                    Duration::from_millis(50),
                    capacity.acquire_model(&CancellationToken::new()),
                )
                .await
                .is_err(),
                "{profile}: a cached hint admitted ungranted work"
            );
            assert_eq!(capacity.snapshot().model_running, 0);
            assert_eq!(capacity.snapshot().model_waiting, 0);
            drop(capacity);
            wait_for_owners(&registry, 1).await?;
            registry.release_departed(&Arc::new(Governor::new(occupied)?))?;
        }
        Ok(())
    }

    #[test]
    fn learned_hints_are_scoped_and_reset_cannot_republish_old_samples() {
        let root = tempfile::tempdir().unwrap();
        let mut registry =
            Registry::open(&root.path().canonicalize().unwrap().join("registry")).unwrap();
        let mut learning = Learning::open(
            &mut registry,
            "fixture-env",
            Some("opaque-account"),
            100_000,
        );
        let mut tuner = Tuner::new(4, 2);
        observed(&mut tuner);
        learning.observe(&tuner);
        learning.flush(&mut registry, 100_000, true);
        assert_eq!(learning.status().samples, 48);
        let mut fresh = Learning::open(
            &mut registry,
            "fixture-env",
            Some("opaque-account"),
            100_001,
        );
        let mut restarted = Tuner::new(1, 1);
        fresh.apply_hints(&mut restarted, [2, 2], true);
        assert_eq!(
            restarted.models(),
            2,
            "current hard ceiling must tighten a learned hint"
        );
        assert_eq!(
            Learning::open(&mut registry, "fixture-env", Some("other-account"), 100_001)
                .status()
                .samples,
            0
        );
        reset(&mut registry).unwrap();
        learning.flush(&mut registry, 200_000, true);
        assert_eq!(
            Learning::open(
                &mut registry,
                "fixture-env",
                Some("opaque-account"),
                200_001
            )
            .status()
            .samples,
            0
        );
        if learning.refresh(
            &mut registry,
            "fixture-env",
            Some("opaque-account"),
            200_001,
            tuner.class_samples(),
        ) {
            tuner.reset_evidence();
        }
        learning.observe(&tuner);
        learning.flush(&mut registry, 300_000, true);
        assert_eq!(learning.status().samples, 0);
    }

    #[test]
    fn normal_flush_waits_one_minute_and_shutdown_can_save_pending_evidence() {
        let root = tempfile::tempdir().unwrap();
        let mut registry =
            Registry::open(&root.path().canonicalize().unwrap().join("registry")).unwrap();
        let now = 100_000;
        let mut learning =
            Learning::open(&mut registry, "fixture-env", Some("opaque-account"), now);
        let mut tuner = Tuner::new(4, 2);
        observed(&mut tuner);
        learning.observe(&tuner);
        learning.flush(&mut registry, now, false);
        let first_samples = learning.status().samples;
        assert!(first_samples > 0);

        observed(&mut tuner);
        learning.observe(&tuner);
        let changes = registry.connection.total_changes();
        learning.flush(&mut registry, now + FLUSH_MS - 1, false);
        assert_eq!(registry.connection.total_changes(), changes);
        assert_eq!(learning.status().samples, first_samples);

        learning.flush(&mut registry, now + FLUSH_MS, false);
        let normal_samples = learning.status().samples;
        assert!(normal_samples > first_samples);
        observed(&mut tuner);
        learning.observe(&tuner);
        learning.flush(&mut registry, now + FLUSH_MS + 1, true);
        assert!(learning.status().samples > normal_samples);
        let persisted = Learning::open(
            &mut registry,
            "fixture-env",
            Some("opaque-account"),
            now + FLUSH_MS + 1,
        );
        assert_eq!(persisted.status().samples, learning.status().samples);
    }

    #[test]
    fn corrupt_expired_and_old_profiles_are_disposable() {
        let valid = Profile {
            version: VERSION,
            samples: 64,
            target: 4,
            rate: 1.0,
            p95_micros: 1,
            class: 1,
        };
        let data = serde_json::to_string(&valid).unwrap();
        assert_eq!(decode(&data, 1, DECAY_MS + 1).unwrap().1, 32);
        assert!(decode(&data, 1, EXPIRY_MS + 2).is_none());
        assert!(decode("broken", 1, 2).is_none());
        for version in [VERSION - 1, VERSION + 1] {
            assert!(
                decode(
                    &serde_json::to_string(&Profile {
                        version,
                        ..valid.clone()
                    })
                    .unwrap(),
                    1,
                    2
                )
                .is_none()
            );
        }
    }

    #[test]
    fn failed_optional_schema_rolls_back_without_poisoning_admission() {
        let root = tempfile::tempdir().unwrap();
        let mut registry =
            Registry::open(&root.path().canonicalize().unwrap().join("registry")).unwrap();
        registry
            .connection
            .execute_batch(
                "CREATE VIEW capacity_learning_meta AS SELECT 1 AS singleton,0 AS epoch;",
            )
            .unwrap();
        let learning = Learning::open(&mut registry, "fixture-env", None, 1);
        assert_eq!(learning.status().state, LearningState::Unavailable);
        assert!(registry.connection.is_autocommit());
        let transaction = registry
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        transaction.commit().unwrap();
    }
}
