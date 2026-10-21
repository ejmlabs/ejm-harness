use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

use anyhow::{Context, Result};
use async_trait::async_trait;
use ejm_harness::{
    CapacityContext, CapacityMode, CapacityPolicy, Effect, Harness, HarnessConfig, Hook,
    HookContext, Message, ModelResponse, Provider, ReplayProvider, RunHandle, RunStatus,
    RunSummary, SessionOptions, Tool, ToolCall, ToolContext, ToolDefinition, ToolOutput,
    ejm_harness_exec::Reservation,
};
use serde_json::json;
use tokio::sync::{mpsc, oneshot};

const WAIT: Duration = Duration::from_secs(5);

fn coordinated(
    root: &Path,
    provider: Arc<dyn Provider>,
    tools: Vec<Arc<dyn Tool>>,
    hooks: Vec<Arc<dyn Hook>>,
) -> Result<(Harness, CapacityContext)> {
    let mut config = HarnessConfig {
        state_dir: root.join("journal"),
        context_max_bytes: 512 * 1024,
        ..HarnessConfig::default()
    };
    let capacity = CapacityContext::prepare_isolated(
        &mut config,
        CapacityPolicy {
            mode: CapacityMode::Fixed,
            max_model_requests: Some(1),
            max_jobs: Some(1),
            max_memory_bytes: Some(512 * 1024 * 1024),
            ..CapacityPolicy::default()
        },
        &root.join("capacity"),
    )?;
    let harness = Harness::with_capacity(config, Some(provider), tools, hooks, capacity.clone())?;
    Ok((harness, capacity))
}

async fn finish(run: &mut RunHandle) -> Result<RunSummary> {
    tokio::time::timeout(WAIT, run.wait()).await?
}

struct Preparation {
    bytes: usize,
    release: oneshot::Sender<()>,
}

struct PreparationGate {
    armed: AtomicBool,
    entered: mpsc::Sender<Preparation>,
}

#[async_trait]
impl Hook for PreparationGate {
    async fn before_model(
        &self,
        _context: &HookContext,
        messages: Vec<Message>,
    ) -> Result<Vec<Message>> {
        if self.armed.load(Ordering::SeqCst) {
            let (release, wait) = oneshot::channel();
            self.entered
                .send(Preparation {
                    bytes: serde_json::to_vec(&messages)?.len(),
                    release,
                })
                .await?;
            wait.await?;
        }
        Ok(messages)
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn queued_large_histories_prepare_once_and_cancellation_releases_context_credits()
-> Result<()> {
    const SESSIONS: usize = 8;
    let parent = tempfile::tempdir()?;
    let root = parent.path().canonicalize()?;
    let (entered, mut preparations) = mpsc::channel(SESSIONS);
    let hook = Arc::new(PreparationGate {
        armed: AtomicBool::new(false),
        entered,
    });
    let (harness, capacity) = coordinated(
        &root,
        Arc::new(ReplayProvider::echo()),
        vec![],
        vec![hook.clone()],
    )?;
    let mut sessions = Vec::with_capacity(SESSIONS);
    for index in 0..SESSIONS {
        let session = harness
            .create_session(SessionOptions {
                cwd: root.clone(),
                title: format!("Large history {index}"),
            })
            .await?;
        for _ in 0..4 {
            let mut run = harness.start(&session.id, "x".repeat(16 * 1024)).await?;
            assert_eq!(finish(&mut run).await?.status, RunStatus::Completed);
        }
        sessions.push(session);
    }
    let baseline = harness.governor().snapshot();
    let baseline_memory_leases = capacity.snapshot().operating.memory_leases;
    hook.armed.store(true, Ordering::SeqCst);
    let mut runs = Vec::with_capacity(SESSIONS);
    runs.push(harness.start(&sessions[0].id, "next".into()).await?);
    let prepared = tokio::time::timeout(WAIT, preparations.recv())
        .await?
        .context("admitted history did not reach preparation")?;
    assert!(prepared.bytes >= 128 * 1024);
    let preparing = harness.governor().snapshot();
    assert!(preparing.reserved_memory_bytes > baseline.reserved_memory_bytes);
    assert_eq!(capacity.snapshot().model_running, 1);
    assert_eq!(
        capacity.snapshot().operating.memory_leases,
        baseline_memory_leases + 1
    );
    for session in sessions.iter().skip(1) {
        runs.push(harness.start(&session.id, "next".into()).await?);
    }
    tokio::time::timeout(WAIT, async {
        while harness.status().await.waiting_model_requests != SESSIONS - 1 {
            tokio::task::yield_now().await;
        }
    })
    .await?;
    assert_eq!(harness.status().await.model_requests, 1);
    assert_eq!(harness.governor().snapshot(), preparing);
    assert_eq!(
        capacity.snapshot().operating.memory_leases,
        baseline_memory_leases + 1
    );
    assert!(matches!(
        preparations.try_recv(),
        Err(mpsc::error::TryRecvError::Empty)
    ));

    // Finish queued cancellations while preparation still holds the only model slot.
    for (session, run) in sessions.iter().zip(&mut runs).skip(1) {
        harness.cancel_run(&session.id, &run.id).await?;
        assert_eq!(finish(run).await?.status, RunStatus::Cancelled);
    }
    harness.cancel_run(&sessions[0].id, &runs[0].id).await?;
    assert_eq!(finish(&mut runs[0]).await?.status, RunStatus::Cancelled);
    assert!(prepared.release.send(()).is_err());
    assert_eq!(capacity.snapshot().model_running, 0);
    assert_eq!(capacity.snapshot().model_waiting, 0);
    assert_eq!(harness.status().await.model_requests, 0);
    assert_eq!(harness.status().await.waiting_model_requests, 0);
    assert_eq!(harness.governor().snapshot(), baseline);
    assert_eq!(
        capacity.snapshot().operating.memory_leases,
        baseline_memory_leases
    );

    let mut next = harness
        .start(&sessions[0].id, "reuse credits".into())
        .await?;
    tokio::time::timeout(WAIT, preparations.recv())
        .await?
        .context("cancelled preparation did not release admission")?
        .release
        .send(())
        .map_err(|()| anyhow::anyhow!("replacement preparation stopped"))?;
    assert_eq!(finish(&mut next).await?.status, RunStatus::Completed);
    assert_eq!(harness.governor().snapshot(), baseline);
    tokio::time::timeout(WAIT, harness.shutdown()).await??;
    Ok(())
}

struct InspectTool {
    calls: AtomicUsize,
    entered: mpsc::Sender<oneshot::Sender<()>>,
}

#[async_trait]
impl Tool for InspectTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "inspect".into(),
            description: "Count an admitted inspection".into(),
            input_schema: json!({"type":"object","additionalProperties":false}),
            effect: Effect::Read,
            version: "1".into(),
        }
    }

    fn reservation(&self) -> Option<Reservation> {
        Some(reservation())
    }

    async fn execute(&self, _call: ToolCall, context: ToolContext) -> Result<ToolOutput> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let (release, wait) = oneshot::channel();
        self.entered.send(release).await?;
        tokio::select! {
            () = context.cancel.cancelled() => anyhow::bail!("cancelled inspection"),
            released = wait => released?,
        }
        Ok(ToolOutput::text("inspected"))
    }
}

fn reservation() -> Reservation {
    Reservation {
        cpu_millis: 0,
        memory_bytes: 1,
        processes: 0,
        output_bytes: 0,
        heavy: false,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tool_admission_and_execution_waits_do_not_hold_model_credits() -> Result<()> {
    let parent = tempfile::tempdir()?;
    let root = parent.path().canonicalize()?;
    let (entered, mut inspections) = mpsc::channel(1);
    let tool = Arc::new(InspectTool {
        calls: AtomicUsize::new(0),
        entered,
    });
    let responses = vec![
        ModelResponse {
            tool_calls: vec![ToolCall {
                id: "inspect-call".into(),
                name: "inspect".into(),
                arguments: json!({}),
            }],
            ..ModelResponse::default()
        },
        ModelResponse {
            content: "Independent model completed".into(),
            ..ModelResponse::default()
        },
        ModelResponse {
            content: "Inspection completed".into(),
            ..ModelResponse::default()
        },
    ];
    let (harness, capacity) = coordinated(
        &root,
        Arc::new(ReplayProvider::new(responses)),
        vec![tool.clone()],
        vec![],
    )?;
    let baseline = harness.governor().snapshot();
    let memory_leases = capacity.snapshot().operating.memory_leases;
    let governor = harness.governor();
    let held = governor
        .acquire(
            "held-job-slot",
            reservation(),
            &tokio_util::sync::CancellationToken::new(),
        )
        .await?;
    let first = harness
        .create_session(SessionOptions {
            cwd: root.clone(),
            title: "Waiting tool".into(),
        })
        .await?;
    let mut waiting = harness.start(&first.id, "inspect".into()).await?;
    tokio::time::timeout(WAIT, async {
        while governor.snapshot().queued != 1 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .context("tool never reached admission")?;
    assert_eq!(capacity.snapshot().model_running, 0);
    assert_eq!(harness.status().await.model_requests, 0);
    assert_eq!(capacity.snapshot().operating.memory_leases, memory_leases);
    assert_eq!(tool.calls.load(Ordering::SeqCst), 0);
    drop(held);
    let release = tokio::time::timeout(WAIT, inspections.recv())
        .await?
        .context("admitted tool never reached execution")?;
    assert_eq!(capacity.snapshot().model_running, 0);
    assert_eq!(harness.status().await.model_requests, 0);
    let second = harness
        .create_session(SessionOptions {
            cwd: root,
            title: "Independent model".into(),
        })
        .await?;
    let mut independent = harness.start(&second.id, "finish".into()).await?;
    let summary = finish(&mut independent).await?;
    assert_eq!(summary.status, RunStatus::Completed);
    assert_eq!(summary.text, "Independent model completed");
    assert_eq!(tool.calls.load(Ordering::SeqCst), 1);
    assert_eq!(governor.snapshot().running, 1);
    assert!(harness.active_run_id(&first.id).await.is_some());
    assert_eq!(capacity.snapshot().model_running, 0);
    release
        .send(())
        .map_err(|()| anyhow::anyhow!("waiting tool stopped"))?;
    let summary = finish(&mut waiting).await?;
    assert_eq!(summary.status, RunStatus::Completed);
    assert_eq!(summary.text, "Inspection completed");
    assert_eq!(tool.calls.load(Ordering::SeqCst), 1);
    assert_eq!(governor.snapshot(), baseline);
    tokio::time::timeout(WAIT, harness.shutdown()).await??;
    Ok(())
}
