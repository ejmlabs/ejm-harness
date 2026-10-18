use std::{
    collections::HashSet,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use anyhow::{Result, ensure};
use async_trait::async_trait;
use ejm_harness::{
    Effect, EventKind, Harness, HarnessConfig, ModelDelta, ModelRequest, ModelResponse, Provider,
    Role, RunStatus, SessionOptions, Tool, ToolCall, ToolContext, ToolDefinition, ToolOutput,
};
use rusqlite::Connection;
use serde_json::json;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

const CONTEXT_BYTES: usize = 512 * 1024;
const TOOL_STEPS: usize = 14;

struct LongTurnProvider {
    requests: AtomicUsize,
    system: String,
}

#[async_trait]
impl Provider for LongTurnProvider {
    async fn stream(
        &self,
        request: ModelRequest,
        _: CancellationToken,
        _: mpsc::Sender<ModelDelta>,
    ) -> Result<ModelResponse> {
        let step = self.requests.fetch_add(1, Ordering::SeqCst);
        ensure!(request.system == self.system, "system prefix changed");
        ensure!(serde_json::to_vec(&request.messages)?.len() <= CONTEXT_BYTES);
        ensure!(
            request.messages[0]
                .content
                .starts_with("Inspect a large repository.")
        );
        let mut pending = HashSet::new();
        for message in &request.messages {
            for call in &message.tool_calls {
                ensure!(pending.insert(call.id.as_str()), "duplicate tool call");
            }
            if let Some(id) = &message.tool_call_id {
                ensure!(pending.remove(id.as_str()), "orphan tool result");
            }
        }
        ensure!(pending.is_empty(), "incomplete tool exchange");
        if step > 0 {
            let recent = &request.messages[request.messages.len() - 16..];
            ensure!(
                recent.iter().all(
                    |message| message.role == Role::Tool && message.content == "é".repeat(8192)
                ),
                "latest observations were lost"
            );
        }
        if step == TOOL_STEPS {
            return Ok(ModelResponse {
                content: "Inspection finished.".into(),
                ..Default::default()
            });
        }
        Ok(ModelResponse {
            tool_calls: (0..16)
                .map(|index| ToolCall {
                    id: format!("{step}-{index}"),
                    name: "inspect".into(),
                    arguments: json!({"step":step,"index":index}),
                })
                .collect(),
            ..Default::default()
        })
    }
}

struct LargeObservation;

#[async_trait]
impl Tool for LargeObservation {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "inspect".into(),
            description: "Return one bounded observation.".into(),
            input_schema: json!({"type":"object"}),
            effect: Effect::Read,
            version: "1".into(),
        }
    }

    async fn execute(&self, _: ToolCall, _: ToolContext) -> Result<ToolOutput> {
        Ok(ToolOutput::text("é".repeat(8192)))
    }
}

#[tokio::test]
async fn long_single_turn_keeps_intent_pairs_and_full_durable_observations() -> Result<()> {
    let root = tempfile::tempdir()?;
    let config = HarnessConfig {
        state_dir: root.path().join("state"),
        context_max_bytes: CONTEXT_BYTES,
        ..Default::default()
    };
    let provider = Arc::new(LongTurnProvider {
        requests: AtomicUsize::new(0),
        system: config.system_prompt.clone(),
    });
    let harness = Harness::new(config, provider.clone(), vec![Arc::new(LargeObservation)])?;
    let session = harness
        .create_session(SessionOptions {
            cwd: root.path().into(),
            ..Default::default()
        })
        .await?;
    let summary = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        harness
            .start(&session.id, "Inspect a large repository.".into())
            .await?
            .wait()
            .await
    })
    .await??;
    assert_eq!(summary.status, RunStatus::Completed, "{:?}", summary.error);
    assert_eq!(provider.requests.load(Ordering::SeqCst), TOOL_STEPS + 1);
    let db = Connection::open(root.path().join("state/sessions.sqlite3"))?;
    let bytes: i64 = db.query_row("SELECT SUM(bytes) FROM messages", [], |row| row.get(0))?;
    assert!(bytes > i64::try_from(CONTEXT_BYTES + 2 * 1024 * 1024)?);
    let changed: i64 = db.query_row(
        "SELECT COUNT(*) FROM messages WHERE json_extract(payload,'$.role')='tool' AND json_extract(payload,'$.content')!=?1",
        ["é".repeat(8192)],
        |row| row.get(0),
    )?;
    assert_eq!(changed, 0);
    assert!(
        harness
            .events(&session.id, 0, 1000)
            .await?
            .iter()
            .any(|event| matches!(event.kind, EventKind::ContextTrimmed { .. }))
    );
    harness.shutdown().await?;
    Ok(())
}
