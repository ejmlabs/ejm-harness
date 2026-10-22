use std::{
    collections::BTreeSet,
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use anyhow::{Context, Result, ensure};
use async_trait::async_trait;
use ejm_harness::{
    CapacityContext, CapacityMode, CapacityPolicy, Effect, EventKind, Harness, HarnessConfig,
    ModelDelta, ModelRequest, ModelResponse, Provider, Role, RunHandle, RunStatus,
    SchedulingSnapshot, Session, SessionOptions, Tool, ToolCall, ToolContext, ToolDefinition,
    ToolOutput,
    ejm_harness_exec::{GovernorSnapshot, Reservation, ResourceLimits},
};
use serde_json::json;
use tokio::sync::{mpsc, oneshot, watch};
use tokio_util::sync::CancellationToken;

const WAIT: Duration = Duration::from_secs(120);

async fn opened(gate: &watch::Receiver<bool>, cancel: &CancellationToken) -> Result<()> {
    let mut gate = gate.clone();
    while !*gate.borrow_and_update() {
        tokio::select! {
            biased;
            () = cancel.cancelled() => anyhow::bail!("fixture cancelled"),
            changed = gate.changed() => changed?,
        }
    }
    ensure!(!cancel.is_cancelled(), "fixture cancelled");
    Ok(())
}

struct BurstProvider {
    gate: watch::Receiver<bool>,
    proposed: AtomicUsize,
}

#[async_trait]
impl Provider for BurstProvider {
    async fn stream(
        &self,
        request: ModelRequest,
        cancel: CancellationToken,
        _deltas: mpsc::Sender<ModelDelta>,
    ) -> Result<ModelResponse> {
        opened(&self.gate, &cancel).await?;
        if request
            .messages
            .last()
            .is_some_and(|message| message.role == Role::Tool)
        {
            return Ok(ModelResponse {
                content: "done".into(),
                ..ModelResponse::default()
            });
        }
        let index = self.proposed.fetch_add(1, Ordering::SeqCst);
        Ok(ModelResponse {
            tool_calls: vec![ToolCall {
                id: format!("burst-{index}"),
                name: "record".into(),
                arguments: json!({"index":index}),
            }],
            ..ModelResponse::default()
        })
    }
}

#[derive(Default)]
struct Calls {
    started: Mutex<BTreeSet<usize>>,
    completed: Mutex<BTreeSet<usize>>,
    active: AtomicUsize,
    peak: AtomicUsize,
}

impl Calls {
    fn begin(&self, index: usize) -> Result<Active<'_>> {
        ensure!(
            self.started
                .lock()
                .map_err(|_| anyhow::anyhow!("started poisoned"))?
                .insert(index),
            "duplicate tool start"
        );
        let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak.fetch_max(active, Ordering::SeqCst);
        Ok(Active(&self.active))
    }

    fn complete(&self, index: usize) -> Result<()> {
        ensure!(
            self.completed
                .lock()
                .map_err(|_| anyhow::anyhow!("completed poisoned"))?
                .insert(index),
            "duplicate effect"
        );
        Ok(())
    }

    fn snapshot(&self) -> Result<(BTreeSet<usize>, BTreeSet<usize>)> {
        let started = self
            .started
            .lock()
            .map_err(|_| anyhow::anyhow!("started poisoned"))?
            .clone();
        let completed = self
            .completed
            .lock()
            .map_err(|_| anyhow::anyhow!("completed poisoned"))?
            .clone();
        Ok((started, completed))
    }
}

struct Active<'a>(&'a AtomicUsize);

impl Drop for Active<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

struct GatedTool {
    gate: watch::Receiver<bool>,
    calls: Arc<Calls>,
}

fn reservation() -> Reservation {
    Reservation {
        cpu_millis: 10,
        memory_bytes: 64 * 1024,
        processes: 0,
        output_bytes: 0,
        heavy: false,
    }
}

fn tool_definition(effect: Effect) -> ToolDefinition {
    ToolDefinition {
        name: "record".into(),
        description: "Internal dispatch fixture".into(),
        input_schema: json!({"type":"object","properties":{"index":{"type":"integer","minimum":0}},"required":["index"],"additionalProperties":false}),
        effect,
        version: "1".into(),
    }
}

#[async_trait]
impl Tool for GatedTool {
    fn definition(&self) -> ToolDefinition {
        tool_definition(Effect::Read)
    }

    fn reservation(&self) -> Option<Reservation> {
        Some(reservation())
    }

    async fn execute(&self, call: ToolCall, context: ToolContext) -> Result<ToolOutput> {
        let index = usize::try_from(call.arguments["index"].as_u64().context("missing index")?)?;
        let _active = self.calls.begin(index)?;
        opened(&self.gate, &context.cancel).await?;
        self.calls.complete(index)?;
        Ok(ToolOutput::text("recorded"))
    }
}

struct Fixture {
    harness: Harness,
    capacity: CapacityContext,
    provider: Arc<BurstProvider>,
    calls: Arc<Calls>,
    release_models: watch::Sender<bool>,
    release_tools: watch::Sender<bool>,
    root: tempfile::TempDir,
    baseline: GovernorSnapshot,
}

impl Fixture {
    fn new(managed: usize, models: usize, timeout_ms: u64) -> Result<Self> {
        let root = tempfile::tempdir()?;
        let root_path = root.path().canonicalize()?;
        let mut config = HarnessConfig {
            state_dir: root_path.join("journal"),
            context_max_bytes: 32 * 1024,
            run_timeout_ms: timeout_ms,
            max_steps: 3,
            system_prompt: "Complete the requested inspection.".into(),
            resources: ResourceLimits {
                max_queue: 4,
                ..ResourceLimits::default()
            },
            ..HarnessConfig::default()
        };
        let capacity = CapacityContext::prepare_isolated(
            &mut config,
            CapacityPolicy {
                mode: CapacityMode::Fixed,
                max_sessions: Some(managed),
                max_model_requests: Some(models),
                max_jobs: Some(1),
                max_memory_bytes: Some(256 * 1024 * 1024),
                ..CapacityPolicy::default()
            },
            &root_path.join("capacity"),
        )?;
        let (release_models, model_gate) = watch::channel(false);
        let (release_tools, tool_gate) = watch::channel(false);
        let provider = Arc::new(BurstProvider {
            gate: model_gate,
            proposed: AtomicUsize::new(0),
        });
        let calls = Arc::new(Calls::default());
        let harness = Harness::with_capacity(
            config,
            Some(provider.clone()),
            vec![Arc::new(GatedTool {
                gate: tool_gate,
                calls: calls.clone(),
            })],
            vec![],
            capacity.clone(),
        )?;
        let baseline = harness.governor().snapshot();
        Ok(Self {
            harness,
            capacity,
            provider,
            calls,
            release_models,
            release_tools,
            root,
            baseline,
        })
    }

    async fn session(&self, title: String) -> Result<Session> {
        self.harness
            .create_session(SessionOptions {
                cwd: self.root.path().into(),
                title,
            })
            .await
    }

    async fn start_many(&self, count: usize) -> Result<Vec<RunHandle>> {
        let mut runs = Vec::with_capacity(count);
        for index in 0..count {
            let session = self.session(format!("burst-{index}")).await?;
            runs.push(self.harness.start(&session.id, "record".into()).await?);
        }
        Ok(runs)
    }

    async fn models_idle(&self) -> Result<()> {
        tokio::time::timeout(WAIT, async {
            loop {
                let status = self.harness.status().await;
                if status.model_requests == 0
                    && status.waiting_model_requests == 0
                    && self.capacity.snapshot().operating.memory_leases == 1
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await?;
        assert_eq!(self.capacity.snapshot().model_running, 0);
        assert_eq!(self.capacity.snapshot().model_waiting, 0);
        Ok(())
    }

    async fn assert_drained(&self) -> Result<()> {
        self.models_idle().await?;
        assert_eq!(self.harness.status().await.active_agents, 0);
        wait_for(|| self.harness.scheduling_snapshot() == SchedulingSnapshot::default()).await?;
        assert_eq!(self.calls.active.load(Ordering::SeqCst), 0);
        assert_eq!(self.harness.governor().snapshot(), self.baseline);
        assert_eq!(self.capacity.snapshot().operating.memory_leases, 1);
        Ok(())
    }
}

async fn wait_for(mut ready: impl FnMut() -> bool) -> Result<()> {
    tokio::time::timeout(WAIT, async {
        while !ready() {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await?;
    Ok(())
}

async fn finish_all(runs: &mut [RunHandle], expected: RunStatus) -> Result<()> {
    tokio::time::timeout(WAIT, async {
        for run in runs {
            let summary = run.wait().await?;
            assert_eq!(summary.status, expected, "{:?}", summary.error);
        }
        anyhow::Ok(())
    })
    .await??;
    Ok(())
}

async fn burst(managed: usize, cancel: bool) -> Result<()> {
    let fixture = Fixture::new(managed, 1, 120_000)?;
    let mut runs = fixture.start_many(managed).await?;
    assert_eq!(fixture.harness.status().await.active_agents, managed);
    assert_eq!(fixture.provider.proposed.load(Ordering::SeqCst), 0);
    fixture.release_models.send(true)?;
    wait_for(|| {
        fixture.provider.proposed.load(Ordering::SeqCst) == managed
            && fixture.calls.active.load(Ordering::SeqCst) == 1
    })
    .await?;
    fixture.models_idle().await?;
    wait_for(|| {
        fixture.harness.scheduling_snapshot()
            == SchedulingSnapshot {
                managed_runs: managed,
                tool_dispatch_waiting: managed - 1,
                tool_running: 1,
                ..SchedulingSnapshot::default()
            }
    })
    .await?;
    assert_eq!(fixture.harness.status().await.active_agents, managed);
    let blocked = fixture.harness.governor().snapshot();
    assert_eq!(blocked.running, 1);
    assert_eq!(blocked.queued, 0);
    assert_eq!(
        blocked.reserved_memory_bytes,
        fixture.baseline.reserved_memory_bytes + reservation().memory_bytes
    );
    assert_eq!(fixture.calls.snapshot()?.0.len(), 1);
    assert!(fixture.calls.snapshot()?.1.is_empty());
    if cancel {
        for run in &runs {
            run.cancel();
        }
        finish_all(&mut runs, RunStatus::Cancelled).await?;
        fixture.assert_drained().await?;
        fixture.release_tools.send(true)?;
        let mut followup = fixture
            .harness
            .start(&runs[0].session_id, "new work".into())
            .await?;
        finish_all(std::slice::from_mut(&mut followup), RunStatus::Completed).await?;
        let (started, completed) = fixture.calls.snapshot()?;
        assert_eq!(started.len(), 2);
        assert_eq!(completed, BTreeSet::from([managed]));
        assert_eq!(
            fixture.provider.proposed.load(Ordering::SeqCst),
            managed + 1
        );
    } else {
        fixture.release_tools.send(true)?;
        finish_all(&mut runs, RunStatus::Completed).await?;
        let (started, completed) = fixture.calls.snapshot()?;
        assert_eq!(started, (0..managed).collect());
        assert_eq!(completed, started);
        assert_eq!(fixture.provider.proposed.load(Ordering::SeqCst), managed);
    }
    assert_eq!(fixture.calls.peak.load(Ordering::SeqCst), 1);
    fixture.assert_drained().await?;
    tokio::time::timeout(WAIT, fixture.harness.shutdown()).await??;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dispatch_bursts_complete_and_cancel_without_late_effects() -> Result<()> {
    burst(32, false).await?;
    burst(32, true).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "target-scale dispatch contract; run explicitly"]
async fn thousand_managed_runs_complete_and_cancel_without_late_effects() -> Result<()> {
    burst(1_000, false).await?;
    burst(1_000, true).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "target-scale dispatch contract; run explicitly"]
async fn four_thousand_managed_runs_complete_and_cancel_without_late_effects() -> Result<()> {
    burst(4_096, false).await?;
    burst(4_096, true).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn managed_and_utf8_prompt_bounds_reject_before_effects() -> Result<()> {
    let fixture = Fixture::new(2, 1, 10_000)?;
    let valid = fixture.session("exact byte bound".into()).await?;
    let extra = fixture.session("rejected".into()).await?;
    let prompt = "é".repeat(8_192);
    assert_eq!(prompt.len(), 16_384);
    assert!(
        fixture
            .harness
            .start(&extra.id, format!("{prompt}x"))
            .await
            .is_err()
    );
    assert!(fixture.harness.events(&extra.id, 0, 10).await?.is_empty());
    let mut runs = vec![fixture.harness.start(&valid.id, prompt).await?];
    runs.extend(fixture.start_many(1).await?);
    assert!(
        fixture
            .harness
            .start(&extra.id, "over managed limit".into())
            .await
            .is_err()
    );
    assert_eq!(fixture.harness.status().await.active_agents, 2);
    assert_eq!(fixture.provider.proposed.load(Ordering::SeqCst), 0);
    assert!(fixture.calls.snapshot()?.0.is_empty());
    assert!(fixture.harness.events(&extra.id, 0, 10).await?.is_empty());
    assert!(fixture.harness.events(&valid.id, 0, 10).await?.iter().any(
        |event| matches!(&event.kind, EventKind::RunStarted { prompt } if prompt.len() == 16_384)
    ));
    for run in &runs {
        run.cancel();
    }
    finish_all(&mut runs, RunStatus::Cancelled).await?;
    fixture.assert_drained().await?;
    fixture.harness.shutdown().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn run_deadlines_cover_governor_and_dispatch_waits() -> Result<()> {
    let fixture = Fixture::new(2, 2, 2_000)?;
    let external_lease = fixture
        .harness
        .governor()
        .acquire("external-holder", reservation(), &CancellationToken::new())
        .await?;
    let blocked_baseline = fixture.harness.governor().snapshot();
    let mut runs = fixture.start_many(2).await?;
    wait_for(|| fixture.capacity.snapshot().model_running == 2).await?;
    fixture.release_models.send(true)?;
    wait_for(|| {
        fixture.provider.proposed.load(Ordering::SeqCst) == 2
            && fixture.harness.governor().snapshot().queued == 1
    })
    .await?;
    fixture.models_idle().await?;
    assert!(fixture.calls.snapshot()?.0.is_empty());
    finish_all(&mut runs, RunStatus::BudgetExhausted).await?;
    assert_eq!(fixture.harness.governor().snapshot(), blocked_baseline);
    assert!(fixture.calls.snapshot()?.0.is_empty());
    drop(external_lease);
    fixture.assert_drained().await?;
    fixture.release_tools.send(true)?;
    let mut followup = fixture
        .harness
        .start(&runs[0].session_id, "new work".into())
        .await?;
    finish_all(std::slice::from_mut(&mut followup), RunStatus::Completed).await?;
    assert_eq!(
        fixture.calls.snapshot()?,
        (BTreeSet::from([2]), BTreeSet::from([2]))
    );
    fixture.assert_drained().await?;
    fixture.harness.shutdown().await?;
    Ok(())
}

struct RetainedWrite {
    gate: watch::Receiver<bool>,
    calls: Arc<Calls>,
    entered: mpsc::Sender<usize>,
    jobs: Mutex<Vec<tokio::task::JoinHandle<()>>>,
}

#[async_trait]
impl Tool for RetainedWrite {
    fn definition(&self) -> ToolDefinition {
        tool_definition(Effect::Write)
    }

    fn reservation(&self) -> Option<Reservation> {
        None
    }

    async fn execute(&self, call: ToolCall, context: ToolContext) -> Result<ToolOutput> {
        let index = usize::try_from(call.arguments["index"].as_u64().context("missing index")?)?;
        let lease = context
            .governor
            .acquire(&context.session_id, reservation(), &context.cancel)
            .await?;
        let ownership = context.retain_execution();
        let calls = self.calls.clone();
        let gate = self.gate.clone();
        let entered = self.entered.clone();
        let (finished, result) = oneshot::channel();
        // This fixture represents work that cannot stop when its caller is dropped.
        // Both the effect guard and the actual lease must outlive that caller.
        let job = tokio::spawn(async move {
            let output = async {
                let _active = calls.begin(index)?;
                entered.send(index).await?;
                opened(&gate, &CancellationToken::new()).await?;
                calls.complete(index)?;
                anyhow::Ok(ToolOutput::text("recorded"))
            }
            .await;
            drop(ownership);
            drop(lease);
            let _ = finished.send(output);
        });
        self.jobs
            .lock()
            .map_err(|_| anyhow::anyhow!("jobs poisoned"))?
            .push(job);
        result.await?
    }
}

fn write_harness(
    root: &Path,
    calls: Arc<Calls>,
    entered: mpsc::Sender<usize>,
    gate: watch::Receiver<bool>,
) -> Result<(Harness, Arc<RetainedWrite>)> {
    let mut config = HarnessConfig {
        state_dir: root.join("journal"),
        run_timeout_ms: 10_000,
        resources: ResourceLimits {
            max_queue: 34,
            ..ResourceLimits::default()
        },
        ..HarnessConfig::default()
    };
    let capacity = CapacityContext::prepare_isolated(
        &mut config,
        CapacityPolicy {
            mode: CapacityMode::Fixed,
            max_sessions: Some(2),
            max_model_requests: Some(2),
            max_jobs: Some(2),
            max_memory_bytes: Some(256 * 1024 * 1024),
            ..CapacityPolicy::default()
        },
        &root.join("capacity"),
    )?;
    let (_, ready) = watch::channel(true);
    let tool = Arc::new(RetainedWrite {
        gate,
        calls,
        entered,
        jobs: Mutex::new(Vec::new()),
    });
    let harness = Harness::with_capacity(
        config,
        Some(Arc::new(BurstProvider {
            gate: ready,
            proposed: AtomicUsize::new(0),
        })),
        vec![tool.clone()],
        vec![],
        capacity,
    )?;
    Ok((harness, tool))
}

async fn tool_queued(harness: &Harness, run: &RunHandle) -> Result<()> {
    tokio::time::timeout(WAIT, async {
        loop {
            if harness
                .events(&run.session_id, 0, 100)
                .await?
                .iter()
                .any(|event| {
                    event.run_id.as_deref() == Some(run.id.as_str())
                        && matches!(event.kind, EventKind::ToolQueued { .. })
                })
            {
                return anyhow::Ok(());
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await??;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn canonical_workspace_writes_retain_ownership_through_caller_cancellation() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let root = temporary.path().canonicalize()?;
    let calls = Arc::new(Calls::default());
    let (release, gate) = watch::channel(false);
    let (entered, mut starts) = mpsc::channel(2);
    let (harness, tool) = write_harness(&root, calls.clone(), entered, gate)?;
    let baseline = harness.governor().snapshot();
    let first = harness
        .create_session(SessionOptions {
            cwd: root.clone(),
            title: "first".into(),
        })
        .await?;
    let second = harness
        .create_session(SessionOptions {
            cwd: root.join("."),
            title: "second".into(),
        })
        .await?;
    assert_eq!(first.cwd, second.cwd);
    let mut owner = harness.start(&first.id, "first write".into()).await?;
    assert_eq!(tokio::time::timeout(WAIT, starts.recv()).await?, Some(0));
    let mut waiter = harness.start(&second.id, "second write".into()).await?;
    tool_queued(&harness, &waiter).await?;
    assert!(
        tokio::time::timeout(Duration::from_millis(50), starts.recv())
            .await
            .is_err()
    );
    assert_eq!(harness.governor().snapshot().running, 1);
    assert_eq!(harness.governor().snapshot().queued, 0);

    waiter.cancel();
    finish_all(std::slice::from_mut(&mut waiter), RunStatus::Cancelled).await?;
    owner.cancel();
    finish_all(std::slice::from_mut(&mut owner), RunStatus::Cancelled).await?;
    assert_eq!(calls.snapshot()?, (BTreeSet::from([0]), BTreeSet::new()));
    assert_eq!(harness.governor().snapshot().running, 1);
    let mut next = harness
        .start(&first.id, "explicit replacement".into())
        .await?;
    tool_queued(&harness, &next).await?;
    assert!(
        tokio::time::timeout(Duration::from_millis(50), starts.recv())
            .await
            .is_err()
    );
    assert_eq!(harness.governor().snapshot().running, 1);

    release.send(true)?;
    assert_eq!(tokio::time::timeout(WAIT, starts.recv()).await?, Some(2));
    finish_all(std::slice::from_mut(&mut next), RunStatus::Completed).await?;
    let jobs = std::mem::take(
        &mut *tool
            .jobs
            .lock()
            .map_err(|_| anyhow::anyhow!("jobs poisoned"))?,
    );
    for job in jobs {
        tokio::time::timeout(WAIT, job).await??;
    }
    assert_eq!(
        calls.snapshot()?,
        (BTreeSet::from([0, 2]), BTreeSet::from([0, 2]))
    );
    assert_eq!(calls.peak.load(Ordering::SeqCst), 1);
    assert_eq!(harness.governor().snapshot(), baseline);
    harness.shutdown().await?;
    Ok(())
}
