use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use ejm_harness::{
    CapacityContext, CapacityMode, CapacityPolicy, Event, EventKind, Harness, HarnessConfig,
    HarnessSnapshot, Hook, HookContext, Message, ModelDelta, ModelRequest, ModelResponse, Provider,
    ProviderCapacity, ReplayProvider, RunStatus, SessionOptions,
    ejm_harness_exec::{Governor, GovernorSnapshot, Lease, ResourceLimits},
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

const WAIT: Duration = Duration::from_secs(5);

// Complete literals protect the public source contract against required fields.
fn config(state_dir: PathBuf) -> HarnessConfig {
    HarnessConfig {
        state_dir,
        resources: ResourceLimits {
            max_jobs: 2,
            max_heavy_jobs: 1,
            cpu_millis: 1_000,
            memory_bytes: 256 * 1024 * 1024,
            max_processes: 8,
            max_queue: 64,
            max_output_bytes: 256 * 1024,
            max_artifact_bytes: 8 * 1024 * 1024,
            max_job_ms: 2_000,
        },
        max_sessions: 7,
        max_model_requests: 2,
        context_max_bytes: 4_096,
        max_steps: 3,
        max_repeated_calls: 2,
        run_timeout_ms: 5_000,
        system_prompt: "Complete the requested inspection.".into(),
        max_store_bytes: 16 * 1024 * 1024,
        max_input_tokens: 8_192,
        max_output_tokens: 1_024,
    }
}

#[test]
fn legacy_config_status_and_event_literals_remain_usable() -> Result<()> {
    let config = config(PathBuf::from("unused-contract-journal"));
    let governor = Governor::new(config.resources.clone())?;
    let status = HarnessSnapshot {
        active_agents: 0,
        model_requests: 0,
        waiting_model_requests: 0,
        tools: GovernorSnapshot {
            running: 0,
            queued: 0,
            heavy_running: 0,
            reserved_cpu_millis: 0,
            reserved_memory_bytes: 0,
            reserved_processes: 0,
            reserved_output_bytes: 0,
        },
    };
    assert_eq!(status.tools, governor.snapshot());
    let decoded: HarnessSnapshot = serde_json::from_value(serde_json::to_value(&status)?)?;
    assert_eq!(decoded.tools, status.tools);
    let event = Event {
        sequence: 9,
        session_id: "legacy-session".into(),
        run_id: Some("legacy-run".into()),
        timestamp_ms: 123,
        kind: EventKind::ModelStarted { step: 1 },
    };
    let decoded: Event = serde_json::from_value(serde_json::to_value(&event)?)?;
    assert_eq!(decoded.sequence, event.sequence);
    assert_eq!(decoded.run_id, event.run_id);
    assert!(matches!(decoded.kind, EventKind::ModelStarted { step: 1 }));
    Ok(())
}

struct PreparedExtension {
    capacity: CapacityContext,
    context_max_bytes: usize,
    _resident: Lease,
    capacity_reads: AtomicUsize,
    provider_calls: AtomicUsize,
    hook_calls: AtomicUsize,
    provider: ReplayProvider,
}

impl PreparedExtension {
    fn new(capacity: &CapacityContext, context_max_bytes: usize) -> Result<Self> {
        let resident = capacity
            .governor()
            .try_acquire_memory("extension-before-harness", 4_096)?
            .context("prepared capacity did not admit extension initialization")?;
        Ok(Self {
            capacity: capacity.clone(),
            context_max_bytes,
            _resident: resident,
            capacity_reads: AtomicUsize::new(0),
            provider_calls: AtomicUsize::new(0),
            hook_calls: AtomicUsize::new(0),
            provider: ReplayProvider::new(vec![ModelResponse {
                content: "Prepared work completed.".into(),
                ..ModelResponse::default()
            }]),
        })
    }
}

#[async_trait]
impl Provider for PreparedExtension {
    fn capacity(&self) -> ProviderCapacity {
        self.capacity_reads.fetch_add(1, Ordering::SeqCst);
        ProviderCapacity::default()
    }

    async fn stream(
        &self,
        request: ModelRequest,
        cancel: CancellationToken,
        deltas: mpsc::Sender<ModelDelta>,
    ) -> Result<ModelResponse> {
        assert_eq!(self.capacity.snapshot().model_running, 1);
        self.provider_calls.fetch_add(1, Ordering::SeqCst);
        self.provider.stream(request, cancel, deltas).await
    }
}

#[async_trait]
impl Hook for PreparedExtension {
    async fn before_model(
        &self,
        context: &HookContext,
        messages: Vec<Message>,
    ) -> Result<Vec<Message>> {
        assert!(Arc::ptr_eq(&context.governor, &self.capacity.governor()));
        assert_eq!(context.context_max_bytes, self.context_max_bytes);
        assert_eq!(self.capacity.snapshot().model_running, 1);
        self.hook_calls.fetch_add(1, Ordering::SeqCst);
        Ok(messages)
    }
}

fn policy(max_sessions: usize) -> CapacityPolicy {
    CapacityPolicy {
        mode: CapacityMode::Fixed,
        max_sessions: Some(max_sessions),
        max_model_requests: Some(1),
        max_jobs: Some(2),
        max_memory_bytes: Some(256 * 1024 * 1024),
        ..CapacityPolicy::default()
    }
}

#[tokio::test]
async fn prepared_context_precedes_provider_and_hooks_and_keeps_one_governor() -> Result<()> {
    let parent = tempfile::tempdir()?;
    let root = parent.path().canonicalize()?;
    let initial_journal = root.join("before-composition");
    let mut config = config(initial_journal.clone());
    let capacity =
        CapacityContext::prepare_isolated(&mut config, policy(7), &root.join("capacity"))?;
    let governor = capacity.governor();
    let baseline = governor.snapshot();
    let extension = Arc::new(PreparedExtension::new(&capacity, config.context_max_bytes)?);
    let initialized = governor.snapshot();
    assert_eq!(
        initialized.reserved_memory_bytes,
        baseline.reserved_memory_bytes + 4_096
    );
    assert!(!initial_journal.exists());
    config.state_dir = root.join("journal");
    let harness = Harness::with_capacity(
        config,
        Some(extension.clone()),
        vec![],
        vec![extension.clone()],
        capacity.clone(),
    )?;
    assert!(Arc::ptr_eq(&harness.governor(), &governor));
    let session = harness
        .create_session(SessionOptions {
            cwd: root,
            title: "Prepared extension".into(),
        })
        .await?;
    let mut run = harness.start(&session.id, "inspect".into()).await?;
    let summary = tokio::time::timeout(WAIT, run.wait()).await??;
    assert_eq!(summary.status, RunStatus::Completed);
    assert_eq!(summary.text, "Prepared work completed.");
    assert_eq!(extension.provider_calls.load(Ordering::SeqCst), 1);
    assert_eq!(extension.hook_calls.load(Ordering::SeqCst), 1);
    assert!(!initial_journal.exists());
    assert_eq!(governor.snapshot(), initialized);
    assert_eq!(capacity.snapshot().model_running, 0);
    tokio::time::timeout(WAIT, harness.shutdown()).await??;
    drop(harness);
    assert_eq!(governor.snapshot(), initialized);
    drop(extension);
    assert_eq!(governor.snapshot(), baseline);
    Ok(())
}

#[tokio::test]
async fn changed_prepared_envelopes_reject_before_journal_or_provider_effects() -> Result<()> {
    for (prepared_bytes, final_bytes, prepared_sessions, final_sessions) in [
        (4_096, 1024 * 1024, 2, 2),
        (1024 * 1024, 4_096, 2, 2),
        (4_096, 4_096, 2, 4),
        (4_096, 4_096, 4, 2),
    ] {
        let parent = tempfile::tempdir()?;
        let root = parent.path().canonicalize()?;
        let journal = root.join("journal");
        let mut config = config(journal.clone());
        config.context_max_bytes = prepared_bytes;
        let capacity = CapacityContext::prepare_isolated(
            &mut config,
            policy(prepared_sessions),
            &root.join("capacity"),
        )?;
        let extension = Arc::new(PreparedExtension::new(&capacity, prepared_bytes)?);
        let initialized = capacity.governor().snapshot();
        config.context_max_bytes = final_bytes;
        config.max_sessions = final_sessions;
        match Harness::with_capacity(
            config,
            Some(extension.clone()),
            vec![],
            vec![extension.clone()],
            capacity.clone(),
        ) {
            Ok(harness) => {
                tokio::time::timeout(WAIT, harness.shutdown()).await??;
                bail!("changed prepared envelope was accepted");
            }
            Err(error) => assert!(error.to_string().contains("prepared capacity")),
        }
        assert!(!journal.exists());
        assert_eq!(extension.capacity_reads.load(Ordering::SeqCst), 0);
        assert_eq!(extension.provider_calls.load(Ordering::SeqCst), 0);
        assert_eq!(extension.hook_calls.load(Ordering::SeqCst), 0);
        assert_eq!(capacity.governor().snapshot(), initialized);
    }
    Ok(())
}
