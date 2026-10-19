use std::{
    fs::OpenOptions,
    future::Future,
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::{Context, Result};
use async_trait::async_trait;
use ejm_harness::{
    Harness, HarnessConfig, Message, ModelDelta, ModelRequest, ModelResponse, Provider,
    ProviderContinuation, Role, RunStatus, Session, SessionOptions, ToolCall, ToolOutput,
};
use rusqlite::{Connection, params};
use serde_json::{Value, json};
use tokio::sync::{Notify, mpsc, watch};
use tokio_util::sync::CancellationToken;

struct RecordingProvider {
    response: ModelResponse,
    requests: Mutex<Vec<ModelRequest>>,
}

impl RecordingProvider {
    fn new(label: &'static str) -> Arc<Self> {
        Self::responding(ModelResponse {
            content: label.into(),
            ..Default::default()
        })
    }

    fn responding(response: ModelResponse) -> Arc<Self> {
        Arc::new(Self {
            response,
            requests: Mutex::new(vec![]),
        })
    }

    fn requests(&self) -> Result<Vec<ModelRequest>> {
        self.requests
            .lock()
            .map_err(|_| anyhow::anyhow!("request fixture lock poisoned"))
            .map(|requests| requests.clone())
    }
}

#[async_trait]
impl Provider for RecordingProvider {
    async fn stream(
        &self,
        request: ModelRequest,
        _: CancellationToken,
        _: mpsc::Sender<ModelDelta>,
    ) -> Result<ModelResponse> {
        self.requests
            .lock()
            .map_err(|_| anyhow::anyhow!("request fixture lock poisoned"))?
            .push(request);
        Ok(self.response.clone())
    }
}

struct GatedProvider {
    entered: Notify,
    gate: watch::Receiver<bool>,
    requests: Mutex<Vec<ModelRequest>>,
}

#[async_trait]
impl Provider for GatedProvider {
    async fn stream(
        &self,
        request: ModelRequest,
        cancel: CancellationToken,
        _: mpsc::Sender<ModelDelta>,
    ) -> Result<ModelResponse> {
        self.requests
            .lock()
            .map_err(|_| anyhow::anyhow!("request fixture lock poisoned"))?
            .push(request);
        self.entered.notify_one();
        let mut gate = self.gate.clone();
        while !*gate.borrow_and_update() {
            tokio::select! {
                () = cancel.cancelled() => anyhow::bail!("cancelled"),
                result = gate.changed() => result?,
            }
        }
        Ok(ModelResponse {
            content: "original provider".into(),
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

async fn session(harness: &Harness, cwd: &Path) -> Result<Session> {
    harness
        .create_session(SessionOptions {
            cwd: cwd.into(),
            ..Default::default()
        })
        .await
}

async fn idle(harness: &Harness) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(2), async {
        while harness.status().await.active_agents != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await?;
    Ok(())
}

async fn turn(harness: &Harness, session: &Session, prompt: &str) -> Result<String> {
    let summary = tokio::time::timeout(Duration::from_secs(2), async {
        harness
            .start(&session.id, prompt.into())
            .await?
            .wait()
            .await
    })
    .await??;
    assert_eq!(summary.status, RunStatus::Completed, "{:?}", summary.error);
    idle(harness).await?;
    Ok(summary.text)
}

async fn close(harness: Harness, state_dir: &Path) -> Result<()> {
    harness.shutdown().await?;
    drop(harness);
    let lock = OpenOptions::new()
        .read(true)
        .open(state_dir.join("owner.lock"))?;
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            match lock.try_lock() {
                Ok(()) => return Ok::<(), anyhow::Error>(()),
                Err(std::fs::TryLockError::Error(error)) => return Err(error.into()),
                Err(std::fs::TryLockError::WouldBlock) => tokio::task::yield_now().await,
            }
        }
    })
    .await??;
    Ok(())
}

fn db(root: &tempfile::TempDir) -> Result<Connection> {
    Ok(Connection::open(
        root.path().join("state/sessions.sqlite3"),
    )?)
}

fn seed_history(db: &Connection, session: &Session) -> Result<Vec<Message>> {
    let call = ToolCall {
        id: "old-call".into(),
        name: "inspect".into(),
        arguments: json!({"path":"café.txt"}),
    };
    let output = ToolOutput {
        content: "Permission denied: café.txt".into(),
        is_error: true,
        artifact: Some(session.cwd.join("evidence.txt")),
        ..Default::default()
    };
    let messages = vec![
        Message::user("Inspect café.txt"),
        Message::assistant(&ModelResponse {
            content: "Prior readable answer".into(),
            tool_calls: vec![call.clone()],
            continuation: Some(ProviderContinuation::OpenAiResponses {
                reasoning: vec![json!({"id":"opaque-old","encrypted_content":"fixture-only"})],
            }),
            ..Default::default()
        }),
        Message::tool(&call.id, &output),
    ];
    for message in &messages {
        let payload = serde_json::to_string(message)?;
        db.execute(
            "INSERT INTO messages(session_id,payload,bytes) VALUES(?1,?2,?3)",
            params![session.id, payload, i64::try_from(payload.len())?],
        )?;
    }
    let tool = messages.last().context("tool fixture missing")?;
    let view = serde_json::to_string(tool)?;
    db.execute(
        "INSERT INTO tool_views(sequence,original_output,payload,bytes) VALUES(?1,?2,?3,?4)",
        params![
            db.last_insert_rowid(),
            serde_json::to_string(&output)?,
            view,
            i64::try_from(view.len())?
        ],
    )?;
    Ok(messages)
}

fn continuation_count(db: &Connection) -> Result<i64> {
    Ok(db.query_row(
        "SELECT COUNT(*) FROM messages WHERE json_type(payload,'$.continuation') IS NOT NULL",
        [],
        |row| row.get(0),
    )?)
}

fn binding(db: &Connection) -> Result<String> {
    Ok(db.query_row(
        "SELECT value FROM settings WHERE key='provider_binding'",
        [],
        |row| row.get(0),
    )?)
}

#[tokio::test]
async fn unconfigured_turns_have_no_prompt_or_run_effects() -> Result<()> {
    let root = tempfile::tempdir()?;
    let harness = Harness::with_optional_provider(config(&root), None, vec![], vec![])?;
    let session = session(&harness, root.path()).await?;
    let prompt = "Keep this draft";
    assert!(harness.start(&session.id, prompt.into()).await.is_err());
    assert!(harness.enqueue(&session.id, prompt.into()).await.is_err());
    assert!(harness.events(&session.id, 0, 100).await?.is_empty());
    assert_eq!(harness.status().await.active_agents, 0);
    let db = db(&root)?;
    let messages: i64 = db.query_row("SELECT COUNT(*) FROM messages", [], |row| row.get(0))?;
    assert_eq!(messages, 0);
    let provider = RecordingProvider::new("ready");
    harness
        .configure_provider(Some(provider.clone()), "openai:account-a:model-a")
        .await?;
    assert_eq!(turn(&harness, &session, prompt).await?, "ready");
    assert_eq!(provider.requests()?[0].messages[0].content, prompt);
    harness
        .configure_provider(None, "openai:account-a:model-a")
        .await?;
    let events = harness.events(&session.id, 0, 100).await?;
    assert!(harness.start(&session.id, "later".into()).await.is_err());
    assert_eq!(
        harness.events(&session.id, 0, 100).await?.len(),
        events.len()
    );
    harness.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn idle_provider_changes_preserve_session_and_readable_history() -> Result<()> {
    let root = tempfile::tempdir()?;
    let original = RecordingProvider::new("original answer");
    let replacement = RecordingProvider::new("replacement answer");
    let harness = Harness::new(config(&root), original.clone(), vec![])?;
    let session = session(&harness, root.path()).await?;
    harness
        .configure_provider(Some(original.clone()), "provider:model-a")
        .await?;
    assert_eq!(
        turn(&harness, &session, "First turn").await?,
        "original answer"
    );
    harness
        .configure_provider(Some(replacement.clone()), "provider:model-b")
        .await?;
    assert_eq!(
        turn(&harness, &session, "Second turn").await?,
        "replacement answer"
    );
    let request = replacement.requests()?.remove(0);
    assert_eq!(request.messages[0].content, "First turn");
    assert_eq!(request.messages[1].content, "original answer");
    assert_eq!(harness.get_session(&session.id).await?.id, session.id);
    assert_eq!(original.requests()?.len(), 1);
    harness.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn any_active_session_and_its_queued_turns_block_configuration() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (gate, ready) = watch::channel(false);
    let original = Arc::new(GatedProvider {
        entered: Notify::new(),
        gate: ready,
        requests: Mutex::new(vec![]),
    });
    let replacement = RecordingProvider::new("replacement");
    let harness = Harness::new(config(&root), original.clone(), vec![])?;
    let busy = session(&harness, root.path()).await?;
    let other = session(&harness, root.path()).await?;
    let mut run = harness.start(&busy.id, "First".into()).await?;
    tokio::time::timeout(Duration::from_secs(2), original.entered.notified()).await?;
    harness.enqueue(&busy.id, "Queued".into()).await?;
    let error = harness
        .configure_provider(Some(replacement.clone()), "replacement")
        .await
        .err()
        .context("busy configuration succeeded")?;
    assert!(error.to_string().contains("all managed runs"));
    assert!(harness.configure_provider(None, "removed").await.is_err());
    gate.send(true)?;
    assert_eq!(run.wait().await?.text, "original provider");
    idle(&harness).await?;
    assert_eq!(
        original
            .requests
            .lock()
            .map_err(|_| anyhow::anyhow!("request fixture lock poisoned"))?
            .len(),
        2
    );
    assert!(replacement.requests()?.is_empty());
    harness
        .configure_provider(Some(replacement), "replacement")
        .await?;
    assert_eq!(turn(&harness, &other, "Next").await?, "replacement");
    harness.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn changed_binding_clears_opaque_state_and_preserves_tool_evidence() -> Result<()> {
    let root = tempfile::tempdir()?;
    let original = RecordingProvider::new("original");
    let harness = Harness::new(config(&root), original.clone(), vec![])?;
    harness
        .configure_provider(Some(original), "provider:account-a:model-a")
        .await?;
    let session = session(&harness, root.path()).await?;
    let db = db(&root)?;
    let old = seed_history(&db, &session)?;
    let evidence: String = db.query_row("SELECT original_output FROM tool_views", [], |row| {
        row.get(0)
    })?;
    let replacement = RecordingProvider::new("new answer");
    harness
        .configure_provider(Some(replacement.clone()), "provider:account-a:model-b")
        .await?;
    assert_eq!(continuation_count(&db)?, 0);
    assert_eq!(binding(&db)?, "provider:account-a:model-b");
    let changed: i64 = db.query_row(
        "SELECT COUNT(*) FROM messages WHERE bytes != length(CAST(payload AS BLOB))",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(changed, 0);
    assert_eq!(
        db.query_row("SELECT original_output FROM tool_views", [], |row| row
            .get::<_, String>(
            0
        ))?,
        evidence
    );
    assert_eq!(turn(&harness, &session, "Continue").await?, "new answer");
    let request = replacement.requests()?.remove(0);
    assert!(
        request
            .messages
            .iter()
            .all(|message| message.continuation.is_none())
    );
    for (before, after) in old.iter().zip(&request.messages) {
        let mut expected = serde_json::to_value(before)?;
        expected
            .as_object_mut()
            .context("message is not an object")?
            .remove("continuation");
        assert_eq!(serde_json::to_value(after)?, expected);
    }
    let tool = request
        .messages
        .iter()
        .find(|message| message.role == Role::Tool)
        .context("tool evidence missing")?;
    assert!(tool.is_error);
    assert!(tool.artifact.is_some());
    harness.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn same_binding_preserves_continuation_across_restart() -> Result<()> {
    let root = tempfile::tempdir()?;
    let config = config(&root);
    let state_dir = config.state_dir.clone();
    let original = RecordingProvider::new("original");
    let harness = Harness::new(config.clone(), original.clone(), vec![])?;
    harness
        .configure_provider(Some(original), "same-binding")
        .await?;
    let session = session(&harness, root.path()).await?;
    seed_history(&db(&root)?, &session)?;
    close(harness, &state_dir).await?;
    let resumed = RecordingProvider::new("resumed");
    let harness = Harness::with_optional_provider(config.clone(), None, vec![], vec![])?;
    harness
        .configure_provider(Some(resumed.clone()), "same-binding")
        .await?;
    assert_eq!(continuation_count(&db(&root)?)?, 1);
    assert_eq!(turn(&harness, &session, "Continue").await?, "resumed");
    assert!(
        resumed.requests()?[0]
            .messages
            .iter()
            .any(|message| message.continuation.is_some())
    );
    close(harness, &state_dir).await?;
    let changed = RecordingProvider::new("changed after restart");
    let harness = Harness::with_optional_provider(config, None, vec![], vec![])?;
    harness
        .configure_provider(Some(changed.clone()), "changed-binding")
        .await?;
    assert_eq!(continuation_count(&db(&root)?)?, 0);
    assert_eq!(
        turn(&harness, &session, "Continue again").await?,
        "changed after restart"
    );
    assert!(
        changed.requests()?[0]
            .messages
            .iter()
            .all(|message| message.continuation.is_none())
    );
    harness.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn first_explicit_binding_clears_legacy_unbound_continuation() -> Result<()> {
    let root = tempfile::tempdir()?;
    let provider = RecordingProvider::new("legacy");
    let harness = Harness::new(config(&root), provider.clone(), vec![])?;
    let session = session(&harness, root.path()).await?;
    let db = db(&root)?;
    seed_history(&db, &session)?;
    harness
        .configure_provider(Some(provider), "first-binding")
        .await?;
    assert_eq!(continuation_count(&db)?, 0);
    harness.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn legacy_turns_invalidate_a_previously_tracked_binding() -> Result<()> {
    let root = tempfile::tempdir()?;
    let config = config(&root);
    let state_dir = config.state_dir.clone();
    let original = RecordingProvider::new("original");
    let harness = Harness::new(config.clone(), original.clone(), vec![])?;
    harness
        .configure_provider(Some(original), "original-binding")
        .await?;
    let session = session(&harness, root.path()).await?;
    seed_history(&db(&root)?, &session)?;
    close(harness, &state_dir).await?;

    let legacy = RecordingProvider::responding(ModelResponse {
        content: "legacy answer".into(),
        continuation: Some(ProviderContinuation::OpenAiResponses {
            reasoning: vec![json!({"id":"legacy-opaque","encrypted_content":"fixture-only"})],
        }),
        ..Default::default()
    });
    let harness = Harness::new(config.clone(), legacy.clone(), vec![])?;
    assert_eq!(
        turn(&harness, &session, "Legacy turn").await?,
        "legacy answer"
    );
    assert!(
        legacy.requests()?[0]
            .messages
            .iter()
            .all(|message| message.continuation.is_none())
    );
    assert_eq!(continuation_count(&db(&root)?)?, 1);
    close(harness, &state_dir).await?;

    let harness = Harness::new(config.clone(), legacy.clone(), vec![])?;
    assert_eq!(
        turn(&harness, &session, "Legacy restart").await?,
        "legacy answer"
    );
    assert!(
        legacy.requests()?[1]
            .messages
            .iter()
            .any(|message| message.continuation.is_some())
    );
    close(harness, &state_dir).await?;

    let resumed = RecordingProvider::new("resumed");
    let harness = Harness::with_optional_provider(config, None, vec![], vec![])?;
    harness
        .configure_provider(Some(resumed.clone()), "original-binding")
        .await?;
    assert_eq!(continuation_count(&db(&root)?)?, 0);
    assert_eq!(turn(&harness, &session, "Resume").await?, "resumed");
    let request = resumed.requests()?.remove(0);
    assert!(
        request
            .messages
            .iter()
            .all(|message| message.continuation.is_none())
    );
    assert!(
        request
            .messages
            .iter()
            .any(|message| message.content == "legacy answer")
    );
    assert!(
        request
            .messages
            .iter()
            .any(|message| message.role == Role::Tool && message.is_error)
    );
    harness.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn invalid_bindings_leave_provider_and_continuation_unchanged() -> Result<()> {
    let root = tempfile::tempdir()?;
    let provider = RecordingProvider::new("original");
    let harness = Harness::new(config(&root), provider.clone(), vec![])?;
    harness
        .configure_provider(Some(provider), "original-binding")
        .await?;
    let session = session(&harness, root.path()).await?;
    let db = db(&root)?;
    seed_history(&db, &session)?;
    let replacement = RecordingProvider::new("wrong");
    for invalid in [
        String::new(),
        " ".into(),
        "hidden\nvalue".into(),
        "x".repeat(1025),
    ] {
        assert!(
            harness
                .configure_provider(Some(replacement.clone()), &invalid)
                .await
                .is_err()
        );
    }
    assert_eq!(continuation_count(&db)?, 1);
    assert_eq!(binding(&db)?, "original-binding");
    assert_eq!(turn(&harness, &session, "Continue").await?, "original");
    assert!(replacement.requests()?.is_empty());
    harness.configure_provider(None, &"x".repeat(1024)).await?;
    assert_eq!(binding(&db)?.len(), 1024);
    harness.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn journal_failure_rolls_back_binding_and_continuation_before_provider_swap() -> Result<()> {
    let root = tempfile::tempdir()?;
    let provider = RecordingProvider::new("original");
    let harness = Harness::new(config(&root), provider.clone(), vec![])?;
    harness
        .configure_provider(Some(provider), "original-binding")
        .await?;
    let session = session(&harness, root.path()).await?;
    let db = db(&root)?;
    seed_history(&db, &session)?;
    db.execute_batch(
        "CREATE TRIGGER reject_binding BEFORE UPDATE OF value ON settings BEGIN SELECT RAISE(ABORT,'fixture rejects binding'); END;",
    )?;
    let replacement = RecordingProvider::new("wrong");
    assert!(
        harness
            .configure_provider(Some(replacement.clone()), "new-binding")
            .await
            .is_err()
    );
    assert_eq!(binding(&db)?, "original-binding");
    assert_eq!(continuation_count(&db)?, 1);
    assert_eq!(turn(&harness, &session, "Continue").await?, "original");
    assert!(replacement.requests()?.is_empty());
    let payload: String = db.query_row(
        "SELECT payload FROM messages WHERE json_type(payload,'$.continuation') IS NOT NULL",
        [],
        |row| row.get(0),
    )?;
    assert!(serde_json::from_str::<Value>(&payload)?["continuation"].is_object());
    harness.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn dropping_an_admitted_configuration_cannot_expose_a_partial_change() -> Result<()> {
    let root = tempfile::tempdir()?;
    let original = RecordingProvider::new("original");
    let replacement = RecordingProvider::new("replacement");
    let harness = Harness::new(config(&root), original.clone(), vec![])?;
    harness
        .configure_provider(Some(original), "original-binding")
        .await?;
    let session = session(&harness, root.path()).await?;
    let db = db(&root)?;
    seed_history(&db, &session)?;
    let mut configuration =
        Box::pin(harness.configure_provider(Some(replacement.clone()), "replacement-binding"));
    // The current-thread runtime cannot run the spawned commit until this poll
    // returns, so the caller is dropped precisely after admission and before commit.
    std::future::poll_fn(|context| {
        assert!(configuration.as_mut().poll(context).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
    drop(configuration);
    assert_eq!(turn(&harness, &session, "Continue").await?, "replacement");
    assert_eq!(binding(&db)?, "replacement-binding");
    assert_eq!(continuation_count(&db)?, 0);
    assert!(
        replacement.requests()?[0]
            .messages
            .iter()
            .all(|message| message.continuation.is_none())
    );
    harness.shutdown().await?;
    Ok(())
}
