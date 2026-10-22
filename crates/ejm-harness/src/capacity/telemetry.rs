//! Real engine boundaries: observing an attempt must not acquire or release credit.

use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use ejm_harness_exec::{GovernorSnapshot, Reservation};
use serde_json::json;
use tokio::sync::{mpsc, oneshot};

use super::*;
use crate::{
    Effect, Harness, Hook, HookContext, Message, ModelDelta, ModelResponse, Provider, Role,
    RunHandle, RunStatus, SchedulingSnapshot, SessionOptions, Tool, ToolCall, ToolContext,
    ToolDecision, ToolDefinition, ToolOutput,
};

const WAIT: Duration = Duration::from_secs(5);
type Gate = mpsc::Sender<oneshot::Sender<()>>;

fn gate() -> (Gate, mpsc::Receiver<oneshot::Sender<()>>) {
    mpsc::channel(4)
}

async fn pause(gate: Option<&Gate>, cancel: &CancellationToken) -> Result<()> {
    if let Some(gate) = gate {
        let (release, waiting) = oneshot::channel();
        gate.send(release).await?;
        tokio::select! {
            biased;
            () = cancel.cancelled() => anyhow::bail!("fixture cancelled"),
            result = waiting => result?,
        }
    }
    Ok(())
}

async fn entered(
    receiver: &mut mpsc::Receiver<oneshot::Sender<()>>,
) -> Result<oneshot::Sender<()>> {
    tokio::time::timeout(WAIT, receiver.recv())
        .await?
        .context("fixture did not enter callback")
}

async fn until(mut ready: impl FnMut() -> bool) -> Result<()> {
    tokio::time::timeout(WAIT, async {
        while !ready() {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await?;
    Ok(())
}

#[derive(Clone, Copy, Default)]
enum ToolResult {
    #[default]
    Success,
    ErrorOutput,
    Error,
}

struct TestProvider {
    gate: Option<Gate>,
    call: Option<ToolCall>,
    error: bool,
}

#[async_trait]
impl Provider for TestProvider {
    fn capacity(&self) -> ProviderCapacity {
        ProviderCapacity {
            max_in_flight: Some(1),
            ..ProviderCapacity::default()
        }
    }

    async fn stream(
        &self,
        request: ModelRequest,
        cancel: CancellationToken,
        _deltas: mpsc::Sender<ModelDelta>,
    ) -> Result<ModelResponse> {
        if request
            .messages
            .last()
            .is_some_and(|message| message.role == Role::Tool)
        {
            return Ok(ModelResponse::default());
        }
        pause(self.gate.as_ref(), &cancel).await?;
        ensure!(!self.error, "fixture provider error");
        Ok(ModelResponse {
            tool_calls: self.call.clone().into_iter().collect(),
            ..ModelResponse::default()
        })
    }
}

struct TestHook {
    gate: Option<Gate>,
    skip: bool,
}

#[async_trait]
impl Hook for TestHook {
    async fn before_model(
        &self,
        context: &HookContext,
        messages: Vec<Message>,
    ) -> Result<Vec<Message>> {
        if messages
            .last()
            .is_some_and(|message| message.role != Role::Tool)
        {
            pause(self.gate.as_ref(), &context.cancel).await?;
        }
        Ok(messages)
    }

    async fn before_tool(&self, _context: &HookContext, call: ToolCall) -> Result<ToolDecision> {
        Ok(if self.skip {
            ToolDecision::Skip {
                output: ToolOutput::text("skipped"),
            }
        } else {
            ToolDecision::Execute {
                arguments: call.arguments,
            }
        })
    }
}

struct TestTool {
    gate: Option<Gate>,
    effect: Effect,
    result: ToolResult,
    calls: AtomicUsize,
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

fn tool_call() -> ToolCall {
    ToolCall {
        id: "one".into(),
        name: "observe".into(),
        arguments: json!({"index":0}),
    }
}

#[async_trait]
impl Tool for TestTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "observe".into(),
            description: "Internal telemetry fixture".into(),
            input_schema: json!({"type":"object","properties":{"index":{"type":"integer"}},"required":["index"],"additionalProperties":false}),
            effect: self.effect.clone(),
            version: "1".into(),
        }
    }

    fn reservation(&self) -> Option<Reservation> {
        Some(reservation())
    }

    async fn execute(&self, _call: ToolCall, context: ToolContext) -> Result<ToolOutput> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        pause(self.gate.as_ref(), &context.cancel).await?;
        match self.result {
            ToolResult::Success => Ok(ToolOutput::text("observed")),
            ToolResult::ErrorOutput => Ok(ToolOutput::error("fixture error")),
            ToolResult::Error => anyhow::bail!("fixture error"),
        }
    }
}

struct Options {
    models: usize,
    jobs: usize,
    queue: usize,
    memory: u64,
    effect: Effect,
    result: ToolResult,
    call: Option<ToolCall>,
    skip: bool,
    model_gate: Option<Gate>,
    model_error: bool,
    prepare_gate: Option<Gate>,
    tool_gate: Option<Gate>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            models: 2,
            jobs: 1,
            queue: 4,
            memory: 256 * 1024 * 1024,
            effect: Effect::Read,
            result: ToolResult::Success,
            call: None,
            skip: false,
            model_gate: None,
            model_error: false,
            prepare_gate: None,
            tool_gate: None,
        }
    }
}

struct Fixture {
    harness: Harness,
    capacity: CapacityContext,
    tool: Arc<TestTool>,
    baseline: GovernorSnapshot,
    root: tempfile::TempDir,
}

impl Fixture {
    fn new(options: Options) -> Result<Self> {
        let root = tempfile::tempdir()?;
        let path = root.path().canonicalize()?;
        let mut config = HarnessConfig {
            state_dir: path.join("journal"),
            context_max_bytes: 16 * 1024,
            run_timeout_ms: 30_000,
            max_steps: 3,
            resources: ResourceLimits {
                max_queue: options.queue,
                ..ResourceLimits::default()
            },
            ..HarnessConfig::default()
        };
        let capacity = CapacityContext::prepare_isolated(
            &mut config,
            CapacityPolicy {
                mode: CapacityMode::Fixed,
                max_sessions: Some(8),
                max_model_requests: Some(options.models),
                max_jobs: Some(options.jobs),
                max_memory_bytes: Some(options.memory),
                ..CapacityPolicy::default()
            },
            &path.join("capacity"),
        )?;
        let tool = Arc::new(TestTool {
            gate: options.tool_gate,
            effect: options.effect,
            result: options.result,
            calls: AtomicUsize::new(0),
        });
        let harness = Harness::with_capacity(
            config,
            Some(Arc::new(TestProvider {
                gate: options.model_gate,
                call: options.call,
                error: options.model_error,
            })),
            vec![tool.clone()],
            vec![Arc::new(TestHook {
                gate: options.prepare_gate,
                skip: options.skip,
            })],
            capacity.clone(),
        )?;
        let baseline = harness.governor().snapshot();
        Ok(Self {
            harness,
            capacity,
            tool,
            baseline,
            root,
        })
    }

    async fn start(&self) -> Result<RunHandle> {
        let session = self
            .harness
            .create_session(SessionOptions {
                cwd: self.root.path().into(),
                title: "telemetry".into(),
            })
            .await?;
        self.harness.start(&session.id, "inspect".into()).await
    }

    async fn drained(&self) -> Result<()> {
        until(|| self.harness.scheduling_snapshot() == SchedulingSnapshot::default()).await?;
        assert_eq!(self.harness.governor().snapshot(), self.baseline);
        assert_eq!(self.capacity.snapshot().model_running, 0);
        Ok(())
    }
}

async fn finished(run: &mut RunHandle, status: RunStatus) -> Result<()> {
    let summary = tokio::time::timeout(WAIT, run.wait()).await??;
    assert_eq!(summary.status, status, "{:?}", summary.error);
    Ok(())
}

fn outcomes(lane: &LaneSnapshot, success: u64, failed: u64, cancelled: u64) {
    assert_eq!(
        (lane.successful, lane.failed, lane.cancelled),
        (success, failed, cancelled)
    );
    let attempts = success + failed + cancelled;
    assert_eq!(lane.queue.samples, attempts);
    assert_eq!(lane.preparation.samples, attempts);
    assert_eq!(lane.provider_wait.samples, attempts);
    assert_eq!(lane.service.samples, attempts);
}

fn request() -> ModelRequest {
    ModelRequest {
        system: String::new(),
        messages: vec![],
        tools: vec![],
        max_output_tokens: 16,
    }
}

#[tokio::test]
async fn model_cancellation_is_observed_before_each_service_boundary() -> Result<()> {
    for phase in ["static", "shared", "memory", "preparation", "provider"] {
        let (release, mut entries) = gate();
        let fixture = Fixture::new(Options {
            models: 1,
            model_gate: (phase == "static").then(|| release.clone()),
            prepare_gate: (phase == "preparation").then_some(release),
            ..Options::default()
        })?;
        let cancel = CancellationToken::new();
        let mut owner_gate = None;
        let mut owner = if phase == "static" {
            let run = fixture.start().await?;
            owner_gate = Some(entered(&mut entries).await?);
            Some(run)
        } else {
            None
        };
        let model = if phase == "shared" {
            Some(fixture.capacity.acquire_model(&cancel).await?)
        } else {
            None
        };
        let memory = if phase == "memory" {
            Some(
                fixture
                    .harness
                    .governor()
                    .try_acquire_memory(
                        "external",
                        fixture.capacity.snapshot().hard_resources.memory_bytes
                            - fixture.baseline.reserved_memory_bytes,
                    )?
                    .context("external memory did not fit")?,
            )
        } else {
            None
        };
        let quota = if phase == "provider" {
            Some(
                fixture
                    .capacity
                    .acquire_provider(&request(), &cancel)
                    .await?,
            )
        } else {
            None
        };
        let retained = fixture.harness.governor().snapshot();
        let mut run = fixture.start().await?;
        let mut preparation_gate = None;
        match phase {
            "static" => until(|| fixture.harness.scheduling_snapshot().model_waiting == 1).await?,
            "shared" => until(|| fixture.capacity.snapshot().model_waiting == 1).await?,
            "memory" => until(|| fixture.harness.governor().snapshot().queued == 1).await?,
            "preparation" => {
                preparation_gate = Some(entered(&mut entries).await?);
            }
            "provider" => until(|| fixture.capacity.snapshot().provider.waiting == 1).await?,
            _ => anyhow::bail!("unknown fixture phase"),
        }
        run.cancel();
        finished(&mut run, RunStatus::Cancelled).await?;
        let lane = fixture.capacity.snapshot().models;
        outcomes(&lane, 0, 0, 1);
        assert_eq!(lane.service.p95_micros, 0, "{phase}");
        if phase != "preparation" && phase != "provider" {
            assert_eq!(lane.preparation.p95_micros, 0, "{phase}");
        }
        if phase != "provider" {
            assert_eq!(lane.provider_wait.p95_micros, 0, "{phase}");
        }
        assert_eq!(fixture.harness.governor().snapshot(), retained, "{phase}");
        assert_eq!(
            fixture.capacity.snapshot().model_running,
            usize::from(owner.is_some() || model.is_some())
        );
        drop((model, memory, quota));
        if let Some(owner) = owner.as_mut() {
            owner.cancel();
            finished(owner, RunStatus::Cancelled).await?;
            outcomes(&fixture.capacity.snapshot().models, 0, 0, 2);
        }
        drop((owner_gate, preparation_gate));
        fixture.drained().await?;
    }
    Ok(())
}

#[tokio::test]
async fn model_failures_record_once_without_holding_credit() -> Result<()> {
    for model_error in [false, true] {
        let fixture = Fixture::new(Options {
            memory: if model_error {
                256 * 1024 * 1024
            } else {
                64 * 1024 * 1024
            },
            model_error,
            ..Options::default()
        })?;
        let mut run = fixture.start().await?;
        finished(&mut run, RunStatus::Failed).await?;
        fixture.drained().await?;
        let lane = fixture.capacity.snapshot().models;
        outcomes(&lane, 0, 1, 0);
        if !model_error {
            assert_eq!(lane.preparation.p95_micros, 0);
            assert_eq!(lane.provider_wait.p95_micros, 0);
            assert_eq!(lane.service.p95_micros, 0);
        }
    }
    Ok(())
}

#[tokio::test]
async fn controlled_model_intervals_are_recorded_separately() -> Result<()> {
    let (prepare_gate, mut preparations) = gate();
    let (model_gate, mut providers) = gate();
    let fixture = Fixture::new(Options {
        models: 1,
        prepare_gate: Some(prepare_gate),
        model_gate: Some(model_gate),
        ..Options::default()
    })?;
    let cancel = CancellationToken::new();
    let model = fixture.capacity.acquire_model(&cancel).await?;
    let quota = fixture
        .capacity
        .acquire_provider(&request(), &cancel)
        .await?;
    let mut run = fixture.start().await?;
    until(|| fixture.capacity.snapshot().model_waiting == 1).await?;
    tokio::time::sleep(Duration::from_millis(2)).await;
    drop(model);
    let preparation = entered(&mut preparations).await?;
    tokio::time::sleep(Duration::from_millis(2)).await;
    preparation
        .send(())
        .map_err(|()| anyhow::anyhow!("preparation stopped"))?;
    until(|| fixture.capacity.snapshot().provider.waiting == 1).await?;
    tokio::time::sleep(Duration::from_millis(2)).await;
    drop(quota);
    let provider = entered(&mut providers).await?;
    tokio::time::sleep(Duration::from_millis(2)).await;
    provider
        .send(())
        .map_err(|()| anyhow::anyhow!("provider stopped"))?;
    finished(&mut run, RunStatus::Completed).await?;
    fixture.drained().await?;
    let lane = fixture.capacity.snapshot().models;
    outcomes(&lane, 1, 0, 0);
    for interval in [
        &lane.queue,
        &lane.preparation,
        &lane.provider_wait,
        &lane.service,
    ] {
        assert!(interval.p50_micros > 0);
    }
    Ok(())
}

#[tokio::test]
async fn tool_cancellation_is_observed_before_callback_entry() -> Result<()> {
    for phase in ["dispatch", "resource", "workspace"] {
        let (tool_gate, mut tools) = gate();
        let fixture = Fixture::new(Options {
            jobs: if phase == "workspace" { 2 } else { 1 },
            queue: if phase == "workspace" { 34 } else { 4 },
            effect: if phase == "workspace" {
                Effect::Write
            } else {
                Effect::Read
            },
            call: Some(tool_call()),
            tool_gate: Some(tool_gate),
            ..Options::default()
        })?;
        let cancel = CancellationToken::new();
        let lease = if phase == "resource" {
            Some(
                fixture
                    .harness
                    .governor()
                    .acquire("external", reservation(), &cancel)
                    .await?,
            )
        } else {
            None
        };
        let mut owner_gate = None;
        let mut owner = if phase == "resource" {
            None
        } else {
            let run = fixture.start().await?;
            owner_gate = Some(entered(&mut tools).await?);
            Some(run)
        };
        let retained = fixture.harness.governor().snapshot();
        let mut run = fixture.start().await?;
        until(|| {
            let status = fixture.harness.scheduling_snapshot();
            match phase {
                "dispatch" => status.tool_dispatch_waiting == 1,
                "resource" => status.tool_resource_waiting == 1,
                "workspace" => status.workspace_waiting == 1,
                _ => false,
            }
        })
        .await?;
        run.cancel();
        finished(&mut run, RunStatus::Cancelled).await?;
        let lane = fixture.capacity.snapshot().tools;
        outcomes(&lane, 0, 0, 1);
        assert_eq!(lane.service.p95_micros, 0, "{phase}");
        assert_eq!(
            fixture.tool.calls.load(Ordering::SeqCst),
            usize::from(owner.is_some())
        );
        assert_eq!(fixture.harness.governor().snapshot(), retained, "{phase}");
        drop(lease);
        if let Some(owner) = owner.as_mut() {
            owner.cancel();
            finished(owner, RunStatus::Cancelled).await?;
            outcomes(&fixture.capacity.snapshot().tools, 0, 0, 2);
        }
        drop(owner_gate);
        fixture.drained().await?;
    }
    Ok(())
}

#[tokio::test]
async fn only_a_successful_executed_callback_counts_as_useful_tool_work() -> Result<()> {
    for case in [
        "success",
        "error-output",
        "error",
        "skip",
        "unknown",
        "invalid",
    ] {
        let mut call = tool_call();
        if case == "unknown" {
            call.name = "ungranted".into();
        }
        if case == "invalid" {
            call.arguments = json!({"index":"invalid"});
        }
        let fixture = Fixture::new(Options {
            call: Some(call),
            skip: case == "skip",
            result: match case {
                "error-output" => ToolResult::ErrorOutput,
                "error" => ToolResult::Error,
                _ => ToolResult::Success,
            },
            ..Options::default()
        })?;
        let mut run = fixture.start().await?;
        finished(&mut run, RunStatus::Completed).await?;
        fixture.drained().await?;
        outcomes(
            &fixture.capacity.snapshot().tools,
            u64::from(case == "success"),
            u64::from(case != "success"),
            0,
        );
        assert_eq!(
            fixture.tool.calls.load(Ordering::SeqCst),
            usize::from(matches!(case, "success" | "error-output" | "error")),
            "{case}"
        );
    }
    Ok(())
}
