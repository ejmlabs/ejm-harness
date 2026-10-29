use std::{
    collections::BTreeSet,
    fs::OpenOptions,
    path::Path,
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use anyhow::{Context, Result};
use async_trait::async_trait;
use ejm_harness::*;
use rusqlite::{Connection, params};
use serde_json::json;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

struct RecordingProvider {
    requests: Mutex<Vec<ModelRequest>>,
    replay: ReplayProvider,
}

impl RecordingProvider {
    fn captured(&self) -> Result<MutexGuard<'_, Vec<ModelRequest>>> {
        self.requests
            .lock()
            .map_err(|_| anyhow::anyhow!("request capture lock poisoned"))
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
        self.captured()?.push(request.clone());
        self.replay.stream(request, cancel, deltas).await
    }
}

struct CountingWrite(Arc<AtomicUsize>);

#[async_trait]
impl Tool for CountingWrite {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "write_once".into(),
            description: "Count effects to detect an unsafe recovery replay.".into(),
            input_schema: json!({"type":"object"}),
            effect: Effect::Write,
            version: "1".into(),
        }
    }

    async fn execute(&self, _: ToolCall, _: ToolContext) -> Result<ToolOutput> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(ToolOutput::text("unexpected repeated effect"))
    }
}

async fn close_harness(harness: Harness, state_dir: &Path) -> Result<()> {
    harness.shutdown().await?;
    drop(harness);
    // The journal writer releases ownership after draining its closed channel.
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
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

fn seed_interrupted_run(state_dir: &Path, session: &Session, confirmed: &ToolOutput) -> Result<()> {
    let mut db = Connection::open(state_dir.join("sessions.sqlite3"))?;
    let tx = db.transaction()?;
    tx.execute(
        "UPDATE sessions SET status='running' WHERE id=?1",
        [&session.id],
    )?;
    let confirmed_call = ToolCall {
        id: "confirmed-call".into(),
        name: "write_once".into(),
        arguments: json!({"item":"confirmed"}),
    };
    let pending_call = ToolCall {
        id: "pending-call".into(),
        name: "write_once".into(),
        arguments: json!({"item":"pending"}),
    };
    let messages = [
        Message::user("Finish the requested writes without repeating either."),
        Message::assistant(&ModelResponse {
            tool_calls: vec![confirmed_call.clone()],
            ..Default::default()
        }),
        Message::tool(&confirmed_call.id, confirmed),
        Message::assistant(&ModelResponse {
            tool_calls: vec![pending_call.clone()],
            ..Default::default()
        }),
    ];
    for message in messages {
        let payload = serde_json::to_string(&message)?;
        tx.execute(
            "INSERT INTO messages(session_id,payload,bytes) VALUES(?1,?2,?3)",
            params![session.id, payload, i64::try_from(payload.len())?],
        )?;
    }
    for kind in [
        EventKind::RunStarted {
            prompt: "Finish the requested writes without repeating either.".into(),
        },
        EventKind::ToolStarted {
            call: confirmed_call.clone(),
        },
        EventKind::ToolFinished {
            call_id: confirmed_call.id,
            output: confirmed.clone(),
        },
        EventKind::ToolStarted { call: pending_call },
    ] {
        tx.execute(
            "INSERT INTO events(session_id,run_id,created,payload) VALUES(?1,?2,?3,?4)",
            params![
                session.id,
                "interrupted-run",
                1,
                serde_json::to_string(&kind)?
            ],
        )?;
    }
    tx.commit()?;
    Ok(())
}

fn assert_recovered_context(messages: &[Message], confirmed: &ToolOutput) -> Result<()> {
    let mut pending = BTreeSet::new();
    for message in messages {
        for call in &message.tool_calls {
            assert!(pending.insert(call.id.as_str()));
        }
        if let Some(id) = &message.tool_call_id {
            assert!(
                pending.remove(id.as_str()),
                "orphan or duplicate result: {id}"
            );
        }
    }
    assert!(pending.is_empty());
    let result = messages
        .iter()
        .find(|message| message.tool_call_id.as_deref() == Some("confirmed-call"))
        .context("confirmed result missing")?;
    assert_eq!(result.content, confirmed.content);
    assert_eq!(result.artifact, confirmed.artifact);
    assert!(!result.is_error);
    let unknown = messages
        .iter()
        .find(|message| message.tool_call_id.as_deref() == Some("pending-call"))
        .context("unknown result missing")?;
    assert!(unknown.content.contains("outcome is unknown"));
    assert!(unknown.content.contains("not replayed"));
    assert!(unknown.is_error);
    assert!(unknown.artifact.is_none());
    Ok(())
}

#[tokio::test]
async fn recovery_is_idempotent_and_preserves_confirmed_and_unknown_results() -> Result<()> {
    let root = tempfile::tempdir()?;
    let config = HarnessConfig {
        state_dir: root.path().join("state"),
        ..Default::default()
    };
    let provider = Arc::new(RecordingProvider {
        requests: Mutex::new(vec![]),
        replay: ReplayProvider::new(vec![ModelResponse {
            content: "Recovered context inspected.".into(),
            ..Default::default()
        }]),
    });
    let effects = Arc::new(AtomicUsize::new(0));
    let tools: Vec<Arc<dyn Tool>> = vec![Arc::new(CountingWrite(effects.clone()))];
    let harness = Harness::new(config.clone(), provider.clone(), tools.clone())?;
    let session = harness
        .create_session(SessionOptions {
            cwd: root.path().into(),
            ..Default::default()
        })
        .await?;
    close_harness(harness, &config.state_dir).await?;
    let confirmed = ToolOutput {
        content: "Confirmed write result.".into(),
        artifact: Some(root.path().join("confirmed.txt")),
        ..Default::default()
    };
    seed_interrupted_run(&config.state_dir, &session, &confirmed)?;

    let first = Harness::new(config.clone(), provider.clone(), tools.clone())?;
    let sessions = first.list_sessions().await?;
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].id, session.id);
    assert_eq!(sessions[0].status, SessionStatus::Interrupted);
    let recovered = first.events(&session.id, 0, 100).await?;
    assert_eq!(
        recovered
            .iter()
            .filter(
                |event| matches!(&event.kind, EventKind::RunFinished { summary }
                if summary.status == RunStatus::Interrupted && summary.run_id == "interrupted-run")
            )
            .count(),
        1
    );
    close_harness(first, &config.state_dir).await?;

    let second = Harness::new(config.clone(), provider.clone(), tools)?;
    assert_eq!(
        serde_json::to_value(&recovered)?,
        serde_json::to_value(second.events(&session.id, 0, 100).await?)?
    );
    assert_eq!(effects.load(Ordering::SeqCst), 0);
    assert!(provider.captured()?.is_empty());
    let summary = second
        .start(
            &session.id,
            "Inspect the recovered state before deciding on new work.".into(),
        )
        .await?
        .wait()
        .await?;
    assert_eq!(summary.status, RunStatus::Completed);
    assert_eq!(effects.load(Ordering::SeqCst), 0);
    {
        let requests = provider.captured()?;
        assert_eq!(requests.len(), 1);
        assert_recovered_context(&requests[0].messages, &confirmed)?;
    }
    close_harness(second, &config.state_dir).await?;
    Ok(())
}
