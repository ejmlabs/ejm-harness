use std::collections::HashSet;

use anyhow::Result;
use rusqlite::Connection;

use super::{
    Store,
    history::{insert_message, load_messages, load_pending_calls, pending_calls},
};
use crate::{
    ContextPolicy, Event, EventKind, Message, ModelResponse, RecentContext, Role, Session,
    SessionStatus, ToolCall, ToolOutput,
};

#[test]
fn bounded_page_recovers_intent_beyond_the_row_limit() -> Result<()> {
    let mut db = Connection::open_in_memory()?;
    db.execute_batch("CREATE TABLE messages(sequence INTEGER PRIMARY KEY,session_id TEXT NOT NULL,payload TEXT NOT NULL,bytes INTEGER NOT NULL); CREATE TABLE tool_views(sequence INTEGER PRIMARY KEY,original_output TEXT NOT NULL,payload TEXT,bytes INTEGER)")?;
    let tx = db.transaction()?;
    let insert = |message: Message| insert_message(&tx, "s", &message);
    insert(Message::user("Keep the original intent."))?;
    for step in 0..130 {
        let calls: Vec<ToolCall> = (0..16)
            .map(|index| ToolCall {
                id: format!("{step}-{index}"),
                name: "read".into(),
                arguments: serde_json::json!({"step":step,"index":index}),
            })
            .collect();
        insert(Message::assistant(&ModelResponse {
            tool_calls: calls.clone(),
            ..Default::default()
        }))?;
        for call in calls {
            insert(Message::tool(call.id, &ToolOutput::text("result")))?;
        }
    }
    tx.commit()?;
    let page = load_messages(&db, "s", 1024 * 1024)?;
    assert!(page[0].content.starts_with("Keep the original intent."));
    assert!(page[0].content.contains("bounded history page"));
    assert_eq!(page[1].role, Role::Assistant);
    assert!(page.len() <= 2049);
    let mut pending = HashSet::new();
    for message in &page {
        for call in &message.tool_calls {
            assert!(pending.insert(call.id.as_str()));
        }
        if let Some(id) = &message.tool_call_id {
            assert!(pending.remove(id.as_str()), "orphan result {id}");
        }
    }
    assert!(pending.is_empty());
    let prepared = RecentContext.prepare(page, 96 * 1024)?;
    assert!(serde_json::to_vec(&prepared)?.len() <= 96 * 1024);
    let count: i64 = db.query_row("SELECT COUNT(*) FROM messages", [], |row| row.get(0))?;
    assert_eq!(count, 2211);
    assert!(load_pending_calls(&db, "s")?.is_empty());
    Ok(())
}

#[test]
fn orphan_history_is_rejected_instead_of_sent_to_the_provider() -> Result<()> {
    let db = Connection::open_in_memory()?;
    db.execute_batch("CREATE TABLE messages(sequence INTEGER PRIMARY KEY,session_id TEXT NOT NULL,payload TEXT NOT NULL,bytes INTEGER NOT NULL); CREATE TABLE tool_views(sequence INTEGER PRIMARY KEY,original_output TEXT NOT NULL,payload TEXT,bytes INTEGER)")?;
    insert_message(
        &db,
        "s",
        &Message::tool("orphan", &ToolOutput::text("result")),
    )?;
    assert!(load_messages(&db, "s", 4096).is_err());
    Ok(())
}

#[tokio::test]
async fn exhausted_prepared_view_budget_keeps_the_confirmed_original_pair() -> Result<()> {
    let root = tempfile::tempdir()?;
    let store = Store::open(root.path(), 1024 * 1024)?;
    store
        .create_session(Session {
            id: "s".into(),
            title: "view budget".into(),
            cwd: root.path().into(),
            created_at_ms: 0,
            status: SessionStatus::Running,
        })
        .await?;
    store.message("s", Message::user("Inspect once.")).await?;
    store
        .message(
            "s",
            Message::assistant(&ModelResponse {
                tool_calls: vec![ToolCall {
                    id: "confirmed".into(),
                    name: "read".into(),
                    arguments: serde_json::json!({}),
                }],
                ..Default::default()
            }),
        )
        .await?;
    let output = ToolOutput {
        artifact: Some("artifacts/confirmed.txt".into()),
        ..ToolOutput::text("Confirmed effect.")
    };
    let raw = Message::tool("confirmed", &output);
    let sequence = store.message("s", raw.clone()).await?;
    store.remember_tool_output(sequence, output.clone()).await?;
    store
        .call(|db| {
            let pages: i64 = db.query_row("PRAGMA page_count", [], |row| row.get(0))?;
            db.pragma_update(None, "max_page_count", pages)?;
            Ok(())
        })
        .await?;
    let large = Message::tool("confirmed", &ToolOutput::text("prepared".repeat(2048)));
    assert!(store.prepare_tool_result(sequence, large).await.is_err());
    assert!(store.pending_calls("s").await?.is_empty());
    let page = store.messages("s", 4096).await?;
    assert_eq!(
        serde_json::to_value(page.last())?,
        serde_json::to_value(&raw)?
    );
    store
        .call(move |db| {
            let (original, prepared): (String, Option<String>) = db.query_row(
                "SELECT original_output,payload FROM tool_views WHERE sequence=?1",
                [sequence],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            assert_eq!(
                serde_json::from_str::<ToolOutput>(&original)?.content,
                output.content
            );
            assert!(prepared.is_none());
            Ok(())
        })
        .await?;
    Ok(())
}

#[tokio::test]
async fn journals_replay_and_locks_ownership() -> Result<()> {
    let root = tempfile::tempdir()?;
    let store = Store::open(root.path(), 4 * 1024 * 1024)?;
    assert!(Store::open(root.path(), 4 * 1024 * 1024).is_err());
    store.call(|db| {
        for sql in [
            "EXPLAIN QUERY PLAN SELECT payload FROM messages WHERE session_id='s' AND sequence<10 AND json_extract(payload,'$.role')='user' ORDER BY sequence DESC LIMIT 1",
            "EXPLAIN QUERY PLAN SELECT sequence,payload FROM messages WHERE session_id='s' AND json_extract(payload,'$.role')='assistant' ORDER BY sequence DESC LIMIT 1",
        ] {
            let plan: String = db.query_row(sql, [], |row| row.get(3))?;
            assert!(plan.contains("messages_by_role"), "{plan}");
        }
        Ok(())
    }).await?;
    store
        .create_session(Session {
            id: "s".into(),
            title: "test".into(),
            cwd: root.path().into(),
            created_at_ms: 0,
            status: SessionStatus::Idle,
        })
        .await?;
    let event = store
        .append(Event {
            sequence: 0,
            session_id: "s".into(),
            run_id: None,
            timestamp_ms: 1,
            kind: EventKind::Queued {
                text: "hello".into(),
            },
        })
        .await?;
    assert!(event.sequence > 0);
    assert_eq!(store.events("s", 0, 10000).await?.len(), 1);
    assert!(store.events("s", event.sequence, 10).await?.is_empty());
    Ok(())
}

#[test]
fn reused_legacy_ids_are_reconciled_in_message_order() {
    let call = ToolCall {
        id: "legacy".into(),
        name: "effect".into(),
        arguments: serde_json::json!({}),
    };
    let response = ModelResponse {
        tool_calls: vec![call],
        ..Default::default()
    };
    let history = vec![
        Message::assistant(&response),
        Message::tool("legacy", &ToolOutput::text("done")),
        Message::assistant(&response),
    ];
    assert_eq!(pending_calls(&history).len(), 1);
}
