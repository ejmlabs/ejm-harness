use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use anyhow::{Context, Result};
use async_trait::async_trait;
use ejm_harness::*;
use rusqlite::{Connection, params};
use serde_json::json;
use tokio::sync::{Notify, mpsc, watch};
use tokio_util::sync::CancellationToken;

struct CountingTool {
    calls: Arc<AtomicUsize>,
    active: Arc<AtomicUsize>,
    peak: Arc<AtomicUsize>,
}

#[async_trait]
impl Tool for CountingTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "inspect".into(),
            description: "fixture".into(),
            input_schema: json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"],"additionalProperties":false}),
            effect: Effect::Read,
            version: "1".into(),
        }
    }

    async fn execute(&self, _call: ToolCall, context: ToolContext) -> Result<ToolOutput> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak.fetch_max(active, Ordering::SeqCst);
        tokio::select! { () = context.cancel.cancelled() => {}, () = tokio::time::sleep(std::time::Duration::from_millis(2)) => {} }
        self.active.fetch_sub(1, Ordering::SeqCst);
        Ok(ToolOutput::text("inspected"))
    }
}

struct FixtureProvider {
    gate: watch::Receiver<bool>,
}
#[async_trait]
impl Provider for FixtureProvider {
    async fn stream(
        &self,
        request: ModelRequest,
        _: CancellationToken,
        deltas: mpsc::Sender<ModelDelta>,
    ) -> Result<ModelResponse> {
        let mut gate = self.gate.clone();
        while !*gate.borrow_and_update() {
            gate.changed().await?;
        }
        let response = if request
            .messages
            .last()
            .is_some_and(|m| m.role == Role::Tool)
        {
            ModelResponse {
                content: "Verified.".into(),
                usage: Usage {
                    input_tokens: 10,
                    output_tokens: 2,
                    ..Default::default()
                },
                ..Default::default()
            }
        } else {
            ModelResponse {
                tool_calls: vec![ToolCall {
                    id: uuid::Uuid::new_v4().to_string(),
                    name: "inspect".into(),
                    arguments: json!({"path":"."}),
                }],
                ..Default::default()
            }
        };
        if !response.content.is_empty() {
            deltas
                .send(ModelDelta::Text(response.content.clone()))
                .await?;
        }
        Ok(response)
    }
}

fn config(root: &tempfile::TempDir) -> HarnessConfig {
    HarnessConfig {
        state_dir: root.path().join("state"),
        ..Default::default()
    }
}

#[tokio::test]
async fn exact_session_lookup_includes_sessions_beyond_the_recent_page() -> Result<()> {
    let root = tempfile::tempdir()?;
    let config = config(&root);
    let state_dir = config.state_dir.clone();
    let harness = Harness::new(config, Arc::new(ReplayProvider::echo()), vec![])?;
    let original = harness
        .create_session(SessionOptions {
            cwd: root.path().into(),
            title: "Original session".into(),
        })
        .await?;
    {
        let db = Connection::open(state_dir.join("sessions.sqlite3"))?;
        db.execute(
            "WITH RECURSIVE recent(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM recent WHERE n<1000) INSERT INTO sessions(id,title,cwd,created,status) SELECT 'recent-'||n,'Recent session',?1,?2+n,'idle' FROM recent",
            params![original.cwd.to_string_lossy(), i64::try_from(original.created_at_ms)?],
        )?;
    }
    let recent = harness.list_sessions().await?;
    assert_eq!(recent.len(), 1000);
    assert!(recent.iter().all(|session| session.id != original.id));
    let found = harness.get_session(&original.id).await?;
    assert_eq!(found.id, original.id);
    assert_eq!(found.title, original.title);
    assert_eq!(found.cwd, original.cwd);
    assert!(harness.get_session("missing-session").await.is_err());
    harness.shutdown().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn hundred_agents_share_one_governor_and_durable_events() -> Result<()> {
    let root = tempfile::tempdir()?;
    let calls = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let (gate, ready) = watch::channel(false);
    let harness = Harness::new(
        config(&root),
        Arc::new(FixtureProvider { gate: ready }),
        vec![Arc::new(CountingTool {
            calls: calls.clone(),
            active: Arc::new(AtomicUsize::new(0)),
            peak: peak.clone(),
        })],
    )?;
    let mut handles = vec![];
    let mut ids = vec![];
    for index in 0..100 {
        let session = harness
            .create_session(SessionOptions {
                cwd: root.path().into(),
                title: format!("agent-{index}"),
            })
            .await?;
        handles.push(harness.start(&session.id, "inspect".into()).await?);
        ids.push(session.id);
    }
    assert_eq!(harness.status().await.active_agents, 100);
    gate.send(true)?;
    for handle in &mut handles {
        let result =
            tokio::time::timeout(std::time::Duration::from_secs(20), handle.wait()).await??;
        assert_eq!(result.status, RunStatus::Completed, "{:?}", result.error);
        assert_eq!(result.text, "Verified.");
    }
    assert_eq!(calls.load(Ordering::SeqCst), 100);
    assert!(peak.load(Ordering::SeqCst) <= 4);
    assert_eq!(harness.governor().snapshot().running, 0);
    for id in &ids {
        let events = harness.events(id, 0, 100).await?;
        assert!(matches!(
            events.last().context("missing completion")?.kind,
            EventKind::RunFinished { .. }
        ));
        assert!(events.windows(2).all(|w| w[0].sequence < w[1].sequence));
        assert!(
            events
                .iter()
                .any(|e| matches!(e.kind, EventKind::ToolFinished { .. }))
        );
    }
    harness.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn invalid_schema_arguments_cannot_invoke_tools() -> Result<()> {
    let root = tempfile::tempdir()?;
    let calls = Arc::new(AtomicUsize::new(0));
    let provider = ReplayProvider::new(vec![
        ModelResponse {
            tool_calls: vec![ToolCall {
                id: "bad".into(),
                name: "inspect".into(),
                arguments: json!({"path":42}),
            }],
            ..Default::default()
        },
        ModelResponse {
            content: "Invalid arguments were rejected.".into(),
            ..Default::default()
        },
    ]);
    let harness = Harness::new(
        config(&root),
        Arc::new(provider),
        vec![Arc::new(CountingTool {
            calls: calls.clone(),
            active: Arc::new(AtomicUsize::new(0)),
            peak: Arc::new(AtomicUsize::new(0)),
        })],
    )?;
    let session = harness
        .create_session(SessionOptions {
            cwd: root.path().into(),
            ..Default::default()
        })
        .await?;
    let result = harness
        .start(&session.id, "inspect".into())
        .await?
        .wait()
        .await?;
    assert_eq!(result.status, RunStatus::Completed);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(harness.events(&session.id, 0, 100).await?.iter().any(|e| matches!(&e.kind, EventKind::ToolFinished { output, .. } if output.is_error && output.content.contains("invalid arguments"))));
    harness.shutdown().await?;
    Ok(())
}

struct BlockedProvider {
    entered: Arc<Notify>,
}
#[async_trait]
impl Provider for BlockedProvider {
    async fn stream(
        &self,
        _: ModelRequest,
        _: CancellationToken,
        _: mpsc::Sender<ModelDelta>,
    ) -> Result<ModelResponse> {
        self.entered.notify_one();
        std::future::pending().await
    }
}

#[tokio::test]
async fn cancellation_does_not_wait_for_a_model_or_consumer() -> Result<()> {
    let root = tempfile::tempdir()?;
    let entered = Arc::new(Notify::new());
    let harness = Harness::new(
        config(&root),
        Arc::new(BlockedProvider {
            entered: entered.clone(),
        }),
        vec![],
    )?;
    let session = harness
        .create_session(SessionOptions {
            cwd: root.path().into(),
            ..Default::default()
        })
        .await?;
    let mut run = harness.start(&session.id, "wait".into()).await?;
    entered.notified().await;
    harness.enqueue(&session.id, "never execute".into()).await?;
    run.cancel();
    let summary = tokio::time::timeout(std::time::Duration::from_millis(500), run.wait()).await??;
    assert_eq!(summary.status, RunStatus::Cancelled);
    harness.shutdown().await?;
    assert_eq!(harness.governor().snapshot().queued, 0);
    Ok(())
}

#[tokio::test]
async fn repeated_failures_stop_and_unknown_tools_cannot_add_grants() -> Result<()> {
    let root = tempfile::tempdir()?;
    let responses = (0..8)
        .map(|index| ModelResponse {
            tool_calls: vec![ToolCall {
                id: format!("{index}"),
                name: "shell".into(),
                arguments: json!({"command":"forbidden"}),
            }],
            ..Default::default()
        })
        .collect();
    let harness = Harness::new(
        config(&root),
        Arc::new(ReplayProvider::new(responses)),
        vec![],
    )?;
    let session = harness
        .create_session(SessionOptions {
            cwd: root.path().into(),
            ..Default::default()
        })
        .await?;
    let summary = harness
        .start(&session.id, "run".into())
        .await?
        .wait()
        .await?;
    assert_eq!(summary.status, RunStatus::BudgetExhausted);
    assert!(
        summary
            .error
            .context("missing failure")?
            .contains("repeated tool call")
    );
    let events = harness.events(&session.id, 0, 100).await?;
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e.kind, EventKind::ToolStarted { .. }))
            .count(),
        0
    );
    harness.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn queued_turns_get_distinct_runs_and_slow_receivers_can_replay() -> Result<()> {
    let root = tempfile::tempdir()?;
    let harness = Harness::new(config(&root), Arc::new(ReplayProvider::echo()), vec![])?;
    let session = harness
        .create_session(SessionOptions {
            cwd: root.path().into(),
            ..Default::default()
        })
        .await?;
    let mut live = harness.subscribe();
    let mut run = harness.start(&session.id, "first".into()).await?;
    harness.enqueue(&session.id, "second".into()).await?;
    assert_eq!(run.wait().await?.status, RunStatus::Completed);
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if let Ok(event) = live.recv().await && matches!(event.kind, EventKind::RunFinished { ref summary } if summary.text.contains("second")) { break; }
        }
    }).await?;
    let events = harness.events(&session.id, 0, 100).await?;
    let runs: Vec<_> = events
        .iter()
        .filter_map(|e| match &e.kind {
            EventKind::RunStarted { .. } => e.run_id.clone(),
            _ => None,
        })
        .collect();
    assert_eq!(runs.len(), 2);
    assert_ne!(runs[0], runs[1]);
    harness.shutdown().await?;
    Ok(())
}

struct QueuedProvider {
    first_entered: Arc<Notify>,
    first_ready: Arc<tokio::sync::Semaphore>,
    second_entered: Arc<Notify>,
}
#[async_trait]
impl Provider for QueuedProvider {
    async fn stream(
        &self,
        request: ModelRequest,
        _: CancellationToken,
        _: mpsc::Sender<ModelDelta>,
    ) -> Result<ModelResponse> {
        if request
            .messages
            .last()
            .is_some_and(|message| message.content == "first")
        {
            self.first_entered.notify_one();
            self.first_ready.acquire().await?.forget();
            Ok(ModelResponse::default())
        } else {
            self.second_entered.notify_one();
            std::future::pending().await
        }
    }
}

#[tokio::test]
async fn old_run_controls_cannot_change_a_new_queued_run() -> Result<()> {
    let root = tempfile::tempdir()?;
    let first_entered = Arc::new(Notify::new());
    let first_ready = Arc::new(tokio::sync::Semaphore::new(0));
    let second_entered = Arc::new(Notify::new());
    let harness = Harness::new(
        config(&root),
        Arc::new(QueuedProvider {
            first_entered: first_entered.clone(),
            first_ready: first_ready.clone(),
            second_entered: second_entered.clone(),
        }),
        vec![],
    )?;
    let session = harness
        .create_session(SessionOptions {
            cwd: root.path().into(),
            ..Default::default()
        })
        .await?;
    let mut first = harness.start(&session.id, "first".into()).await?;
    first_entered.notified().await;
    harness
        .enqueue_for_run(&session.id, &first.id, "second".into())
        .await?;
    first_ready.add_permits(1);
    assert_eq!(first.wait().await?.status, RunStatus::Completed);
    tokio::time::timeout(std::time::Duration::from_secs(2), second_entered.notified()).await?;
    assert!(harness.cancel_run(&session.id, &first.id).await.is_err());
    assert!(
        harness
            .steer_run(&session.id, &first.id, "stale".into())
            .await
            .is_err()
    );
    assert!(
        harness
            .enqueue_for_run(&session.id, &first.id, "stale".into())
            .await
            .is_err()
    );
    assert_eq!(harness.status().await.active_agents, 1);
    harness.shutdown().await?;
    Ok(())
}

struct PanickingProvider;
#[async_trait]
impl Provider for PanickingProvider {
    #[expect(
        clippy::panic,
        reason = "The fixture verifies cleanup after a third party provider panics."
    )]
    async fn stream(
        &self,
        _: ModelRequest,
        _: CancellationToken,
        _: mpsc::Sender<ModelDelta>,
    ) -> Result<ModelResponse> {
        panic!("provider fixture failure")
    }
}

#[tokio::test]
async fn provider_panic_finishes_the_run_and_releases_shutdown() -> Result<()> {
    let root = tempfile::tempdir()?;
    let harness = Harness::new(config(&root), Arc::new(PanickingProvider), vec![])?;
    let session = harness
        .create_session(SessionOptions {
            cwd: root.path().into(),
            ..Default::default()
        })
        .await?;
    let mut run = harness.start(&session.id, "trigger".into()).await?;
    let summary = tokio::time::timeout(std::time::Duration::from_secs(2), run.wait()).await??;
    assert_eq!(summary.status, RunStatus::Failed);
    assert!(
        summary
            .error
            .context("missing panic summary")?
            .contains("panicked")
    );
    tokio::time::timeout(std::time::Duration::from_secs(2), harness.shutdown()).await??;
    assert_eq!(harness.status().await.active_agents, 0);
    Ok(())
}

struct SequencedTool {
    write: bool,
    entered: Arc<Notify>,
    release: Arc<Notify>,
    effects: Arc<AtomicUsize>,
}
#[async_trait]
impl Tool for SequencedTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: if self.write { "write" } else { "slow_read" }.into(),
            description: "steering fixture".into(),
            input_schema: json!({"type":"object"}),
            effect: if self.write {
                Effect::Write
            } else {
                Effect::Read
            },
            version: "1".into(),
        }
    }
    async fn execute(&self, _: ToolCall, _: ToolContext) -> Result<ToolOutput> {
        if self.write {
            self.effects.fetch_add(1, Ordering::SeqCst);
        } else {
            self.entered.notify_one();
            self.release.notified().await;
        }
        Ok(ToolOutput::text("done"))
    }
}

#[tokio::test]
async fn steering_during_read_skips_the_following_write() -> Result<()> {
    let root = tempfile::tempdir()?;
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let effects = Arc::new(AtomicUsize::new(0));
    let tools = [false, true]
        .into_iter()
        .map(|write| {
            Arc::new(SequencedTool {
                write,
                entered: entered.clone(),
                release: release.clone(),
                effects: effects.clone(),
            }) as Arc<dyn Tool>
        })
        .collect();
    let response = ModelResponse {
        tool_calls: ["slow_read", "write"]
            .into_iter()
            .enumerate()
            .map(|(index, name)| ToolCall {
                id: format!("c{index}"),
                name: name.into(),
                arguments: json!({}),
            })
            .collect(),
        ..Default::default()
    };
    let harness = Harness::new(
        config(&root),
        Arc::new(ReplayProvider::new(vec![
            response,
            ModelResponse::default(),
        ])),
        tools,
    )?;
    let session = harness
        .create_session(SessionOptions {
            cwd: root.path().into(),
            ..Default::default()
        })
        .await?;
    let mut run = harness.start(&session.id, "read then write".into()).await?;
    tokio::time::timeout(std::time::Duration::from_secs(2), entered.notified()).await?;
    harness
        .steer_run(&session.id, &run.id, "Do not write.".into())
        .await?;
    release.notify_one();
    let result = tokio::time::timeout(std::time::Duration::from_secs(2), run.wait()).await??;
    assert_eq!(result.status, RunStatus::Completed);
    assert_eq!(effects.load(Ordering::SeqCst), 0);
    assert!(harness.events(&session.id, 0, 100).await?.iter().any(|event| matches!(&event.kind, EventKind::ToolFinished { output, .. } if output.content.contains("Skipped because the user steered"))));
    harness.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn full_journal_returns_a_failed_summary_and_stops_admission() -> Result<()> {
    let root = tempfile::tempdir()?;
    let harness = Harness::new(
        HarnessConfig {
            max_store_bytes: 1024 * 1024,
            ..config(&root)
        },
        Arc::new(ReplayProvider::echo()),
        vec![],
    )?;
    let session = harness
        .create_session(SessionOptions {
            cwd: root.path().into(),
            ..Default::default()
        })
        .await?;
    for _ in 0..40 {
        let mut run = harness.start(&session.id, "x".repeat(16_000)).await?;
        let summary = tokio::time::timeout(std::time::Duration::from_secs(2), run.wait()).await??;
        if harness.is_stopped() {
            assert_eq!(summary.status, RunStatus::Failed);
            assert!(
                harness
                    .start(&session.id, "further effects".into())
                    .await
                    .is_err()
            );
            harness.shutdown().await?;
            return Ok(());
        }
    }
    anyhow::bail!("fixture did not fill its journal budget")
}
