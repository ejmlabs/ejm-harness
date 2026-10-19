use std::{
    fmt::Write as _,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use ejm_harness::*;
use rusqlite::Connection;
use serde_json::json;
use tokio::sync::{Notify, mpsc};
use tokio_util::sync::CancellationToken;

struct RecordingProvider {
    requests: Mutex<Vec<ModelRequest>>,
    replay: ReplayProvider,
}

impl RecordingProvider {
    fn tool_then_complete(name: &str) -> Self {
        Self {
            requests: Mutex::new(vec![]),
            replay: ReplayProvider::new(vec![
                ModelResponse {
                    tool_calls: vec![ToolCall {
                        id: "provider-call".into(),
                        name: name.into(),
                        arguments: json!({"path":"original"}),
                    }],
                    continuation: Some(ProviderContinuation::OpenAiResponses {
                        reasoning: vec![json!({"id":"reasoning","encrypted_content":"opaque"})],
                    }),
                    ..Default::default()
                },
                ModelResponse {
                    content: "Finished.".into(),
                    ..Default::default()
                },
            ]),
        }
    }
}

#[async_trait]
impl Provider for RecordingProvider {
    async fn stream(
        &self,
        request: ModelRequest,
        cancel: CancellationToken,
        deltas: mpsc::Sender<ModelDelta>,
    ) -> Result<ModelResponse> {
        self.requests
            .lock()
            .map_err(|_| anyhow::anyhow!("request capture lock poisoned"))?
            .push(request.clone());
        self.replay.stream(request, cancel, deltas).await
    }
}

struct InspectTool {
    state: PathBuf,
    calls: Arc<AtomicUsize>,
}

#[async_trait]
impl Tool for InspectTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "inspect".into(),
            description: "Fixture with a durable effect boundary.".into(),
            input_schema: json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"],"additionalProperties":false}),
            effect: Effect::Write,
            version: "1".into(),
        }
    }

    async fn execute(&self, call: ToolCall, context: ToolContext) -> Result<ToolOutput> {
        let db = Connection::open(self.state.join("sessions.sqlite3"))?;
        let journaled: String = db.query_row(
            "SELECT payload FROM events WHERE session_id=?1 AND json_extract(payload,'$.type')='tool_started' ORDER BY sequence DESC LIMIT 1",
            [&context.session_id],
            |row| row.get(0),
        )?;
        let EventKind::ToolStarted { call: stored } = serde_json::from_str(&journaled)? else {
            bail!("missing effect boundary");
        };
        assert_eq!(stored.id, call.id);
        assert_eq!(stored.arguments, call.arguments);
        assert_eq!(context.governor.snapshot().running, 1);
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(ToolOutput {
            content: format!("confirmed:{}", call.arguments["path"]),
            data: json!({"confirmed":true,"arguments":call.arguments}),
            artifact: Some("artifacts/confirmed.txt".into()),
            ..Default::default()
        })
    }
}

fn config(root: &tempfile::TempDir) -> HarnessConfig {
    HarnessConfig {
        state_dir: root.path().join("state"),
        ..Default::default()
    }
}

async fn session(harness: &Harness, root: &tempfile::TempDir) -> Result<Session> {
    harness
        .create_session(SessionOptions {
            cwd: root.path().into(),
            ..Default::default()
        })
        .await
}

struct OrderedHook {
    name: &'static str,
    state: PathBuf,
    order: Arc<Mutex<Vec<String>>>,
}

impl OrderedHook {
    fn record(&self, event: &str, context: &HookContext) -> Result<()> {
        assert!(!context.cancel.is_cancelled());
        assert_eq!(context.governor.snapshot().running, 0);
        let metadata = serde_json::to_value(context)?;
        assert!(metadata.get("cancel").is_none());
        assert!(metadata.get("governor").is_none());
        self.order
            .lock()
            .map_err(|_| anyhow::anyhow!("hook order lock poisoned"))?
            .push(format!("{event}:{}:{}", self.name, context.step));
        Ok(())
    }
}

#[async_trait]
impl Hook for OrderedHook {
    async fn run_start(&self, context: &HookContext, prompt: String) -> Result<String> {
        self.record("run_start", context)?;
        Ok(format!("{prompt}|{}", self.name))
    }

    async fn before_model(
        &self,
        context: &HookContext,
        mut messages: Vec<Message>,
    ) -> Result<Vec<Message>> {
        self.record("before_model", context)?;
        messages.insert(0, Message::user(format!("context:{}", self.name)));
        Ok(messages)
    }

    async fn before_tool(&self, context: &HookContext, mut call: ToolCall) -> Result<ToolDecision> {
        self.record("before_tool", context)?;
        call.arguments["path"] = json!(format!(
            "{}|{}",
            call.arguments["path"].as_str().context("missing path")?,
            self.name
        ));
        Ok(ToolDecision::Execute {
            arguments: call.arguments,
        })
    }

    async fn after_tool(
        &self,
        context: &HookContext,
        call: ToolCall,
        mut output: ToolOutput,
    ) -> Result<ToolOutput> {
        self.record("after_tool", context)?;
        let db = Connection::open(self.state.join("sessions.sqlite3"))?;
        let (message, original): (String, String) = db.query_row(
            "SELECT m.payload,v.original_output FROM messages m JOIN tool_views v ON v.sequence=m.sequence WHERE m.session_id=?1 AND json_extract(m.payload,'$.tool_call_id')=?2",
            [&context.session_id, &call.id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let raw: Message = serde_json::from_str(&message)?;
        let original: ToolOutput = serde_json::from_str(&original)?;
        assert_eq!(raw.content, "confirmed:\"original|a|b\"");
        assert_eq!(original.content, raw.content);
        assert_eq!(original.data["confirmed"], true);
        write!(output.content, "|result:{}", self.name)?;
        Ok(output)
    }

    async fn run_end(&self, context: &HookContext, summary: &RunSummary) -> Result<()> {
        self.record("run_end", context)?;
        let db = Connection::open(self.state.join("sessions.sqlite3"))?;
        let persisted: String = db.query_row(
            "SELECT payload FROM events WHERE run_id=?1 AND json_extract(payload,'$.type')='run_finished' ORDER BY sequence DESC LIMIT 1",
            [&context.run_id],
            |row| row.get(0),
        )?;
        let EventKind::RunFinished { summary: stored } = serde_json::from_str(&persisted)? else {
            bail!("completion was not durable before run_end");
        };
        assert_eq!(stored.status, summary.status);
        Ok(())
    }
}

const EXPECTED_ORDER: [&str; 12] = [
    "run_start:a:0",
    "run_start:b:0",
    "before_model:a:1",
    "before_model:b:1",
    "before_tool:a:1",
    "before_tool:b:1",
    "after_tool:a:1",
    "after_tool:b:1",
    "before_model:a:2",
    "before_model:b:2",
    "run_end:a:2",
    "run_end:b:2",
];

#[tokio::test]
async fn hooks_compose_in_order_without_extra_model_calls_or_mutating_raw_evidence() -> Result<()> {
    let root = tempfile::tempdir()?;
    let config = config(&root);
    let provider = Arc::new(RecordingProvider::tool_then_complete("inspect"));
    let calls = Arc::new(AtomicUsize::new(0));
    let order = Arc::new(Mutex::new(vec![]));
    let hooks: Vec<Arc<dyn Hook>> = ["a", "b"]
        .into_iter()
        .map(|name| {
            Arc::new(OrderedHook {
                name,
                state: config.state_dir.clone(),
                order: order.clone(),
            }) as Arc<dyn Hook>
        })
        .collect();
    let harness = Harness::with_hooks(
        config.clone(),
        provider.clone(),
        vec![Arc::new(InspectTool {
            state: config.state_dir.clone(),
            calls: calls.clone(),
        })],
        hooks,
    )?;
    let session = session(&harness, &root).await?;
    let result = harness
        .start(&session.id, "request".into())
        .await?
        .wait()
        .await?;
    assert_eq!(result.status, RunStatus::Completed, "{:?}", result.error);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        order
            .lock()
            .map_err(|_| anyhow::anyhow!("hook order lock poisoned"))?
            .as_slice(),
        EXPECTED_ORDER
    );
    {
        let requests = provider
            .requests
            .lock()
            .map_err(|_| anyhow::anyhow!("request capture lock poisoned"))?;
        assert_eq!(requests.len(), 2);
        assert_eq!(
            requests[0]
                .messages
                .last()
                .context("missing prompt")?
                .content,
            "request|a|b"
        );
        let prepared = requests[1]
            .messages
            .last()
            .context("missing prepared result")?;
        assert_eq!(
            prepared.content,
            "confirmed:\"original|a|b\"|result:a|result:b"
        );
        assert_eq!(prepared.artifact, Some("artifacts/confirmed.txt".into()));
        assert!(
            requests[1]
                .messages
                .iter()
                .any(|message| message.continuation.is_some())
        );
    }
    let db = Connection::open(config.state_dir.join("sessions.sqlite3"))?;
    let raw: String = db.query_row(
        "SELECT payload FROM messages WHERE session_id=?1 AND json_extract(payload,'$.role')='tool'",
        [&session.id],
        |row| row.get(0),
    )?;
    let raw: Message = serde_json::from_str(&raw)?;
    assert_eq!(raw.content, "confirmed:\"original|a|b\"");
    let events = harness.events(&session.id, 0, 100).await?;
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event.kind, EventKind::ToolFinished { .. }))
            .count(),
        1
    );
    harness.shutdown().await?;
    Ok(())
}

#[derive(Clone, Copy)]
enum GateMode {
    Skip,
    Invalid,
    Oversized,
    Error,
    Pass,
}

struct GateHook {
    mode: GateMode,
    entered: Arc<AtomicUsize>,
}

#[async_trait]
impl Hook for GateHook {
    async fn before_tool(&self, _: &HookContext, call: ToolCall) -> Result<ToolDecision> {
        self.entered.fetch_add(1, Ordering::SeqCst);
        Ok(match self.mode {
            GateMode::Skip => ToolDecision::Skip {
                output: ToolOutput::text("Host blocked this effect."),
            },
            GateMode::Invalid => ToolDecision::Execute {
                arguments: json!({"path":42}),
            },
            GateMode::Oversized => ToolDecision::Execute {
                arguments: json!({"path":"x".repeat(65536)}),
            },
            GateMode::Error => bail!("Host gate unavailable."),
            GateMode::Pass => ToolDecision::Execute {
                arguments: call.arguments,
            },
        })
    }
}

#[tokio::test]
async fn hook_gates_cannot_bypass_grants_schema_or_argument_limits() -> Result<()> {
    for (mode, name, expected) in [
        (GateMode::Skip, "inspect", "Host blocked"),
        (GateMode::Invalid, "inspect", "invalid arguments"),
        (GateMode::Oversized, "inspect", "argument byte budget"),
        (GateMode::Error, "inspect", "Host gate unavailable"),
        (GateMode::Pass, "ungranted", "not granted"),
    ] {
        let root = tempfile::tempdir()?;
        let config = config(&root);
        let provider = Arc::new(RecordingProvider::tool_then_complete(name));
        let calls = Arc::new(AtomicUsize::new(0));
        let entered = Arc::new(AtomicUsize::new(0));
        let harness = Harness::with_hooks(
            config.clone(),
            provider.clone(),
            vec![Arc::new(InspectTool {
                state: config.state_dir,
                calls: calls.clone(),
            })],
            vec![Arc::new(GateHook {
                mode,
                entered: entered.clone(),
            })],
        )?;
        let session = session(&harness, &root).await?;
        let result = harness
            .start(&session.id, "inspect".into())
            .await?
            .wait()
            .await?;
        assert_eq!(result.status, RunStatus::Completed, "{:?}", result.error);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert_eq!(
            entered.load(Ordering::SeqCst),
            usize::from(name == "inspect")
        );
        assert!(harness.events(&session.id, 0, 100).await?.iter().any(|event|
            matches!(&event.kind, EventKind::ToolFinished { output, .. } if output.is_error && output.content.contains(expected))
        ));
        harness.shutdown().await?;
    }
    Ok(())
}

struct BlockingHook {
    entered: Arc<Notify>,
    block_before_tool: bool,
    block_once: AtomicBool,
}

#[async_trait]
impl Hook for BlockingHook {
    async fn before_tool(&self, _: &HookContext, call: ToolCall) -> Result<ToolDecision> {
        if self.block_before_tool {
            self.entered.notify_one();
            std::future::pending::<()>().await;
        }
        Ok(ToolDecision::Execute {
            arguments: call.arguments,
        })
    }

    async fn after_tool(
        &self,
        _: &HookContext,
        _: ToolCall,
        output: ToolOutput,
    ) -> Result<ToolOutput> {
        if !self.block_before_tool && self.block_once.swap(false, Ordering::SeqCst) {
            self.entered.notify_one();
            std::future::pending::<()>().await;
        }
        Ok(output)
    }
}

#[tokio::test]
async fn cancellation_during_tool_hooks_preserves_effect_and_recovery_boundaries() -> Result<()> {
    for block_before_tool in [true, false] {
        let root = tempfile::tempdir()?;
        let config = config(&root);
        let provider = Arc::new(RecordingProvider::tool_then_complete("inspect"));
        let calls = Arc::new(AtomicUsize::new(0));
        let entered = Arc::new(Notify::new());
        let harness = Harness::with_hooks(
            config.clone(),
            provider.clone(),
            vec![Arc::new(InspectTool {
                state: config.state_dir.clone(),
                calls: calls.clone(),
            })],
            vec![Arc::new(BlockingHook {
                entered: entered.clone(),
                block_before_tool,
                block_once: AtomicBool::new(true),
            })],
        )?;
        let session = session(&harness, &root).await?;
        let mut handle = harness.start(&session.id, "inspect".into()).await?;
        tokio::time::timeout(Duration::from_secs(5), entered.notified())
            .await
            .with_context(|| {
                format!("tool hook entry timed out (block_before_tool={block_before_tool})")
            })?;
        handle.cancel();
        let result = tokio::time::timeout(Duration::from_secs(5), handle.wait())
            .await
            .with_context(|| {
                format!(
                    "cancelled run completion timed out (block_before_tool={block_before_tool})"
                )
            })??;
        assert_eq!(result.status, RunStatus::Cancelled);
        assert_eq!(
            calls.load(Ordering::SeqCst),
            usize::from(!block_before_tool)
        );
        let result = harness
            .start(&session.id, "Inspect existing evidence.".into())
            .await?
            .wait()
            .await?;
        assert_eq!(result.status, RunStatus::Completed, "{:?}", result.error);
        assert_eq!(
            calls.load(Ordering::SeqCst),
            usize::from(!block_before_tool)
        );
        {
            let requests = provider
                .requests
                .lock()
                .map_err(|_| anyhow::anyhow!("request capture lock poisoned"))?;
            assert_eq!(requests.len(), 2);
            let result = requests[1]
                .messages
                .iter()
                .find(|message| message.role == Role::Tool)
                .context("missing durable result")?;
            if block_before_tool {
                assert!(result.is_error);
                assert!(result.content.contains("outcome may be unknown"));
            } else {
                assert!(!result.is_error);
                assert_eq!(result.content, "confirmed:\"original\"");
                assert_eq!(result.artifact, Some("artifacts/confirmed.txt".into()));
                assert!(!result.content.contains("unknown"));
            }
        }
        harness.shutdown().await?;
    }
    Ok(())
}

struct BadResultHook {
    strip_evidence: bool,
}

#[async_trait]
impl Hook for BadResultHook {
    async fn after_tool(
        &self,
        _: &HookContext,
        _: ToolCall,
        mut output: ToolOutput,
    ) -> Result<ToolOutput> {
        if !self.strip_evidence {
            bail!("Result service unavailable.");
        }
        output.artifact = None;
        Ok(output)
    }
}

#[tokio::test]
async fn failed_result_hooks_retain_confirmed_content_status_and_artifact() -> Result<()> {
    for strip_evidence in [true, false] {
        let root = tempfile::tempdir()?;
        let config = config(&root);
        let provider = Arc::new(RecordingProvider::tool_then_complete("inspect"));
        let calls = Arc::new(AtomicUsize::new(0));
        let harness = Harness::with_hooks(
            config.clone(),
            provider.clone(),
            vec![Arc::new(InspectTool {
                state: config.state_dir,
                calls: calls.clone(),
            })],
            vec![Arc::new(BadResultHook { strip_evidence })],
        )?;
        let session = session(&harness, &root).await?;
        let result = harness
            .start(&session.id, "inspect".into())
            .await?
            .wait()
            .await?;
        assert_eq!(result.status, RunStatus::Completed, "{:?}", result.error);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        {
            let requests = provider
                .requests
                .lock()
                .map_err(|_| anyhow::anyhow!("request capture lock poisoned"))?;
            let result = requests[1].messages.last().context("missing tool result")?;
            assert!(result.content.starts_with("confirmed:\"original\""));
            assert!(
                result
                    .content
                    .contains("Result hook failed; confirmed result retained")
            );
            assert!(!result.is_error);
            assert_eq!(result.artifact, Some("artifacts/confirmed.txt".into()));
        }
        harness.shutdown().await?;
    }
    Ok(())
}

struct BlockingContextHook {
    entered: Arc<Notify>,
    block_end: bool,
    end_token: Arc<Mutex<Option<CancellationToken>>>,
}

struct InvalidContextHook;

#[async_trait]
impl Hook for InvalidContextHook {
    async fn before_model(
        &self,
        _: &HookContext,
        mut messages: Vec<Message>,
    ) -> Result<Vec<Message>> {
        messages.last_mut().context("missing prompt")?.content = "Forged intent.".into();
        Ok(messages)
    }
}

#[tokio::test]
async fn invalid_context_hooks_fail_before_model_admission_and_registration_is_bounded()
-> Result<()> {
    let root = tempfile::tempdir()?;
    let provider = Arc::new(RecordingProvider::tool_then_complete("inspect"));
    let hooks: Vec<Arc<dyn Hook>> =
        std::iter::repeat_with(|| Arc::new(InvalidContextHook) as Arc<dyn Hook>)
            .take(33)
            .collect();
    assert!(Harness::with_hooks(config(&root), provider.clone(), vec![], hooks).is_err());
    let harness = Harness::with_hooks(
        config(&root),
        provider.clone(),
        vec![],
        vec![Arc::new(InvalidContextHook)],
    )?;
    let session = session(&harness, &root).await?;
    let result = harness
        .start(&session.id, "Original intent.".into())
        .await?
        .wait()
        .await?;
    assert_eq!(result.status, RunStatus::Failed);
    assert!(
        result
            .error
            .context("missing hook failure")?
            .contains("changed the latest user intent")
    );
    assert!(
        provider
            .requests
            .lock()
            .map_err(|_| anyhow::anyhow!("request capture lock poisoned"))?
            .is_empty()
    );
    harness.shutdown().await?;
    Ok(())
}

#[async_trait]
impl Hook for BlockingContextHook {
    async fn before_model(&self, _: &HookContext, _: Vec<Message>) -> Result<Vec<Message>> {
        self.entered.notify_one();
        std::future::pending().await
    }

    async fn run_end(&self, context: &HookContext, summary: &RunSummary) -> Result<()> {
        assert!(!context.cancel.is_cancelled());
        assert!(matches!(
            summary.status,
            RunStatus::Cancelled | RunStatus::BudgetExhausted
        ));
        *self
            .end_token
            .lock()
            .map_err(|_| anyhow::anyhow!("token lock poisoned"))? = Some(context.cancel.clone());
        if self.block_end {
            std::future::pending::<()>().await;
        }
        bail!("Observer error must not change the durable completion.")
    }
}

#[tokio::test]
async fn hook_deadlines_and_run_end_observers_are_bounded() -> Result<()> {
    for block_end in [false, true] {
        let root = tempfile::tempdir()?;
        let mut config = config(&root);
        config.run_timeout_ms = if block_end { 10_000 } else { 1_000 };
        let provider = Arc::new(RecordingProvider::tool_then_complete("inspect"));
        let entered = Arc::new(Notify::new());
        let end_token = Arc::new(Mutex::new(None));
        let harness = Harness::with_hooks(
            config,
            provider.clone(),
            vec![],
            vec![Arc::new(BlockingContextHook {
                entered: entered.clone(),
                block_end,
                end_token: end_token.clone(),
            })],
        )?;
        let session = session(&harness, &root).await?;
        let mut handle = harness.start(&session.id, "inspect".into()).await?;
        tokio::time::timeout(Duration::from_secs(5), entered.notified())
            .await
            .with_context(|| {
                format!("before_model hook entry timed out (block_end={block_end})")
            })?;
        if block_end {
            handle.cancel();
        }
        let summary = tokio::time::timeout(Duration::from_secs(5), handle.wait())
            .await
            .with_context(|| format!("run completion timed out (block_end={block_end})"))??;
        assert_eq!(
            summary.status,
            if block_end {
                RunStatus::Cancelled
            } else {
                RunStatus::BudgetExhausted
            }
        );
        assert!(
            provider
                .requests
                .lock()
                .map_err(|_| anyhow::anyhow!("request capture lock poisoned"))?
                .is_empty()
        );
        assert!(
            end_token
                .lock()
                .map_err(|_| anyhow::anyhow!("token lock poisoned"))?
                .as_ref()
                .context("run_end did not observe completion")?
                .is_cancelled()
        );
        let events = harness.events(&session.id, 0, 100).await?;
        let EventKind::RunFinished { summary: stored } =
            &events.last().context("missing completion")?.kind
        else {
            bail!("completion event missing");
        };
        assert_eq!(stored.status, summary.status);
        harness.shutdown().await?;
    }
    Ok(())
}
