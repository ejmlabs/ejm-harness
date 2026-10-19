use std::{
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OptionalExtension, params};
use tokio::sync::{mpsc, oneshot};

use crate::{
    Event, EventKind, Message, Role, RunStatus, RunSummary, Session, SessionStatus, ToolOutput,
    Usage,
    types::{pending_calls, timestamp_ms},
};

type Job = Box<dyn FnOnce(&mut Connection) + Send>;

#[derive(Clone)]
pub(crate) struct Store {
    jobs: mpsc::Sender<Job>,
    _lock: Arc<File>,
}

impl Store {
    pub fn open(root: &Path, max_bytes: u64) -> Result<Self> {
        ensure!(
            (1024 * 1024..=8 * 1024 * 1024 * 1024).contains(&max_bytes),
            "store budget must be between 1 MiB and 8 GiB"
        );
        std::fs::create_dir_all(root)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
            std::fs::set_permissions(root, std::fs::Permissions::from_mode(0o700))?;
            let _ = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(false)
                .mode(0o600)
                .open(root.join("sessions.sqlite3"))?;
        }
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(root.join("owner.lock"))?;
        lock.try_lock().context("state directory is already in use; share one harness or choose another state directory")?;
        let mut connection = Connection::open(root.join("sessions.sqlite3"))?;
        connection.busy_timeout(std::time::Duration::from_secs(2))?;
        connection.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON; PRAGMA trusted_schema=OFF; PRAGMA wal_autocheckpoint=64;
            CREATE TABLE IF NOT EXISTS sessions(id TEXT PRIMARY KEY,title TEXT NOT NULL,cwd TEXT NOT NULL,created INTEGER NOT NULL,status TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS messages(sequence INTEGER PRIMARY KEY, session_id TEXT NOT NULL REFERENCES sessions(id),payload TEXT NOT NULL,bytes INTEGER NOT NULL);
            CREATE INDEX IF NOT EXISTS messages_by_session ON messages(session_id,sequence);
            CREATE INDEX IF NOT EXISTS messages_by_role ON messages(session_id,json_extract(payload,'$.role'),sequence);
            CREATE TABLE IF NOT EXISTS tool_views(sequence INTEGER PRIMARY KEY REFERENCES messages(sequence),original_output TEXT NOT NULL,payload TEXT,bytes INTEGER);
            CREATE TABLE IF NOT EXISTS events(sequence INTEGER PRIMARY KEY,session_id TEXT NOT NULL REFERENCES sessions(id),run_id TEXT,created INTEGER NOT NULL,payload TEXT NOT NULL);
            CREATE INDEX IF NOT EXISTS events_by_session ON events(session_id,sequence);
            CREATE TABLE IF NOT EXISTS settings(key TEXT PRIMARY KEY,value TEXT NOT NULL);")?;
        let page_size: i64 = connection.query_row("PRAGMA page_size", [], |r| r.get(0))?;
        connection.pragma_update(
            None,
            "max_page_count",
            (i64::try_from(max_bytes)? / page_size).max(256),
        )?;
        recover(&mut connection)?;
        let (jobs, mut receiver) = mpsc::channel::<Job>(256);
        let lock = Arc::new(lock);
        let writer_lock = lock.clone();
        std::thread::Builder::new()
            .name("ejm-harness-journal".into())
            .spawn(move || {
                let _lock = writer_lock;
                while let Some(job) = receiver.blocking_recv() {
                    job(&mut connection);
                }
                let _ = connection.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)");
            })?;
        Ok(Self { jobs, _lock: lock })
    }

    async fn call<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Connection) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let (tx, rx) = oneshot::channel();
        self.jobs
            .send(Box::new(move |db| {
                let _ = tx.send(f(db));
            }))
            .await
            .map_err(|_| anyhow::anyhow!("journal stopped"))?;
        rx.await.context("journal writer stopped")?
    }

    pub async fn create_session(&self, session: Session) -> Result<Session> {
        self.call(move |db| {
            db.execute(
                "INSERT INTO sessions VALUES(?1,?2,?3,?4,'idle')",
                params![
                    session.id,
                    session.title,
                    session.cwd.to_string_lossy(),
                    i64::try_from(session.created_at_ms)?
                ],
            )?;
            Ok(session)
        })
        .await
    }

    pub async fn session(&self, id: &str) -> Result<Session> {
        let id = id.to_owned();
        self.call(move |db| {
            db.query_row(
                "SELECT id,title,cwd,created,status FROM sessions WHERE id=?1",
                [id],
                row_session,
            )
            .optional()?
            .context("unknown session")
        })
        .await
    }

    pub async fn list_sessions(&self) -> Result<Vec<Session>> {
        self.call(|db| {
            let mut query = db.prepare(
                "SELECT id,title,cwd,created,status FROM sessions ORDER BY created DESC LIMIT 1000",
            )?;
            let rows = query.query_map([], row_session)?;
            rows.collect::<rusqlite::Result<Vec<_>>>()
                .map_err(Into::into)
        })
        .await
    }

    pub async fn configure_provider_binding(&self, binding: Option<String>) -> Result<()> {
        self.call(move |db| {
            let tx = db.transaction()?;
            let current: Option<String> = tx
                .query_row(
                    "SELECT value FROM settings WHERE key='provider_binding'",
                    [],
                    |row| row.get(0),
                )
                .optional()?;
            if current != binding {
                tx.execute(
                    "UPDATE messages SET payload=json_remove(payload,'$.continuation'),bytes=length(CAST(json_remove(payload,'$.continuation') AS BLOB)) WHERE json_type(payload,'$.continuation') IS NOT NULL",
                    [],
                )?;
                tx.execute(
                    "UPDATE tool_views SET payload=json_remove(payload,'$.continuation'),bytes=length(CAST(json_remove(payload,'$.continuation') AS BLOB)) WHERE payload IS NOT NULL AND json_type(payload,'$.continuation') IS NOT NULL",
                    [],
                )?;
                if let Some(binding) = binding {
                    tx.execute(
                        "INSERT INTO settings(key,value) VALUES('provider_binding',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                        [binding],
                    )?;
                } else {
                    tx.execute("DELETE FROM settings WHERE key='provider_binding'", [])?;
                }
            }
            tx.commit()?;
            Ok(())
        })
        .await
    }

    pub async fn append(&self, mut event: Event) -> Result<Event> {
        self.call(move |db| {
            let tx = db.transaction()?;
            insert_event(&tx, &mut event)?;
            match &event.kind {
                EventKind::RunStarted { .. } => {
                    tx.execute(
                        "UPDATE sessions SET status='running' WHERE id=?1",
                        [&event.session_id],
                    )?;
                }
                EventKind::RunFinished { .. } => {
                    tx.execute(
                        "UPDATE sessions SET status='idle' WHERE id=?1",
                        [&event.session_id],
                    )?;
                }
                _ => {}
            }
            tx.commit()?;
            Ok(event)
        })
        .await
    }

    pub async fn message(&self, session_id: &str, message: Message) -> Result<i64> {
        let id = session_id.to_owned();
        self.call(move |db| {
            let payload = serde_json::to_string(&message)?;
            db.execute(
                "INSERT INTO messages(session_id,payload,bytes) VALUES(?1,?2,?3)",
                params![id, payload, i64::try_from(payload.len())?],
            )?;
            Ok(db.last_insert_rowid())
        })
        .await
    }

    pub async fn remember_tool_output(&self, sequence: i64, output: ToolOutput) -> Result<()> {
        self.call(move |db| {
            db.execute(
                "INSERT INTO tool_views(sequence,original_output) VALUES(?1,?2)",
                params![sequence, serde_json::to_string(&output)?],
            )?;
            Ok(())
        })
        .await
    }

    pub async fn prepare_tool_result(&self, sequence: i64, message: Message) -> Result<()> {
        self.call(move |db| {
            let payload = serde_json::to_string(&message)?;
            let updated = db.execute(
                "UPDATE tool_views SET payload=?2,bytes=?3 WHERE sequence=?1",
                params![sequence, payload, i64::try_from(payload.len())?],
            )?;
            ensure!(updated == 1, "original tool output is missing");
            Ok(())
        })
        .await
    }

    pub async fn messages(&self, session_id: &str, max_bytes: usize) -> Result<Vec<Message>> {
        let id = session_id.to_owned();
        self.call(move |db| load_messages(db, &id, max_bytes)).await
    }

    pub async fn pending_calls(&self, session_id: &str) -> Result<Vec<crate::ToolCall>> {
        let id = session_id.to_owned();
        self.call(move |db| load_pending_calls(db, &id)).await
    }

    pub async fn events(&self, session_id: &str, after: u64, limit: usize) -> Result<Vec<Event>> {
        let id = session_id.to_owned();
        let limit = limit.clamp(1, 1000);
        self.call(move |db| {
            let mut statement = db.prepare("SELECT sequence,run_id,created,payload FROM events WHERE session_id=?1 AND sequence>?2 ORDER BY sequence LIMIT ?3")?;
            let rows = statement.query_map(params![id, i64::try_from(after).unwrap_or(i64::MAX), i64::try_from(limit)?], |row| {
                Ok((unsigned_column(row, 0)?, row.get::<_,Option<String>>(1)?, unsigned_column(row, 2)?, row.get::<_,String>(3)?))
            })?;
            let mut result = vec![];
            let mut bytes = 0;
            for row in rows {
                let (sequence, run_id, timestamp_ms, payload) = row?;
                bytes += payload.len();
                if bytes > 2 * 1024 * 1024 && !result.is_empty() { break; }
                result.push(Event { sequence, session_id: id.clone(), run_id, timestamp_ms, kind: serde_json::from_str(&payload)? });
            }
            Ok(result)
        }).await
    }
}

fn unsigned_column(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<u64> {
    u64::try_from(row.get::<_, i64>(index)?).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Integer,
            Box::new(error),
        )
    })
}

fn row_session(row: &rusqlite::Row<'_>) -> rusqlite::Result<Session> {
    let status: String = row.get(4)?;
    Ok(Session {
        id: row.get(0)?,
        title: row.get(1)?,
        cwd: PathBuf::from(row.get::<_, String>(2)?),
        created_at_ms: unsigned_column(row, 3)?,
        status: match status.as_str() {
            "running" => SessionStatus::Running,
            "interrupted" => SessionStatus::Interrupted,
            _ => SessionStatus::Idle,
        },
    })
}

fn insert_event(db: &Connection, event: &mut Event) -> Result<()> {
    db.execute(
        "INSERT INTO events(session_id,run_id,created,payload) VALUES(?1,?2,?3,?4)",
        params![
            event.session_id,
            event.run_id,
            i64::try_from(event.timestamp_ms)?,
            serde_json::to_string(&event.kind)?
        ],
    )?;
    event.sequence = u64::try_from(db.last_insert_rowid())?;
    Ok(())
}

fn load_messages(db: &Connection, id: &str, max_bytes: usize) -> Result<Vec<Message>> {
    let mut query = db.prepare(
        "SELECT m.sequence,COALESCE(v.payload,m.payload),COALESCE(v.bytes,m.bytes) FROM messages m LEFT JOIN tool_views v ON v.sequence=m.sequence WHERE m.session_id=?1 ORDER BY m.sequence DESC LIMIT 2049",
    )?;
    let mut messages = vec![];
    let mut total: usize = 0;
    let mut complete = 0;
    let mut bounded = false;
    let mut rows = query.query([id])?;
    while let Some(row) = rows.next()? {
        let bytes = usize::try_from(row.get::<_, i64>(2)?)?;
        if messages.len() == 2048
            || total.saturating_add(bytes) > max_bytes.saturating_add(2 * 1024 * 1024)
        {
            bounded = true;
            break;
        }
        let sequence: i64 = row.get(0)?;
        let payload: String = row.get(1)?;
        total = total.saturating_add(payload.len());
        let message: Message = serde_json::from_str(&payload)?;
        let at_turn = message.role == Role::User;
        if message.role != Role::Tool {
            complete = messages.len() + 1;
        }
        messages.push((sequence, message));
        if total >= max_bytes && at_turn {
            break;
        }
    }
    if bounded {
        ensure!(
            complete > 0,
            "latest intent or tool exchange exceeds bounded history page"
        );
        // A reverse page can stop inside an exchange. Keep its complete newer suffix.
        messages.truncate(complete);
    }
    messages.reverse();
    if let Some(start) = messages.iter().position(|(_, m)| m.role == Role::User) {
        messages.drain(..start);
        return Ok(messages.into_iter().map(|(_, message)| message).collect());
    }
    let Some((first, _)) = messages.first() else {
        return Ok(vec![]);
    };
    ensure!(bounded, "session history has no user intent");
    let anchor: Option<String> = db
        .query_row(
            "SELECT payload FROM messages WHERE session_id=?1 AND sequence<?2 AND json_extract(payload,'$.role')='user' ORDER BY sequence DESC LIMIT 1",
            params![id, first],
            |row| row.get(0),
        )
        .optional()?;
    let mut anchor: Message =
        serde_json::from_str(&anchor.context("bounded history page has no user intent")?)?;
    anchor.content.push_str("\n[ejm-harness context note: older exchanges were omitted from this bounded history page. Complete events remain in the session journal. Inspect current files before relying on omitted observations.]");
    let mut page = vec![anchor];
    page.extend(messages.into_iter().map(|(_, message)| message));
    Ok(page)
}

fn load_pending_calls(db: &Connection, id: &str) -> Result<Vec<crate::ToolCall>> {
    let latest: Option<(i64, String)> = db
        .query_row(
            "SELECT sequence,payload FROM messages WHERE session_id=?1 AND json_extract(payload,'$.role')='assistant' ORDER BY sequence DESC LIMIT 1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((sequence, payload)) = latest else {
        return Ok(vec![]);
    };
    let assistant: Message = serde_json::from_str(&payload)?;
    if assistant.tool_calls.is_empty() {
        return Ok(vec![]);
    }
    let mut messages = vec![assistant];
    let mut query = db.prepare(
        "SELECT payload FROM messages WHERE session_id=?1 AND sequence>?2 ORDER BY sequence LIMIT 17",
    )?;
    for row in query.query_map(params![id, sequence], |row| row.get::<_, String>(0))? {
        messages.push(serde_json::from_str(&row?)?);
    }
    Ok(pending_calls(&messages))
}

fn recover(db: &mut Connection) -> Result<()> {
    let ids: Vec<String> = {
        let mut statement = db.prepare("SELECT id FROM sessions WHERE status='running'")?;
        statement
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?
    };
    for id in ids {
        let run_id: Option<String> = db.query_row("SELECT run_id FROM events WHERE session_id=?1 AND run_id IS NOT NULL ORDER BY sequence DESC LIMIT 1", [&id], |r| r.get(0)).optional()?.flatten();
        let missing = load_pending_calls(db, &id)?;
        let tx = db.transaction()?;
        for call in missing {
            let message = Message::tool(
                call.id,
                &crate::ToolOutput::error(
                    "Execution was interrupted. Its outcome is unknown. Inspect the workspace before deciding whether another action is needed. This action was not replayed.",
                ),
            );
            let payload = serde_json::to_string(&message)?;
            tx.execute(
                "INSERT INTO messages(session_id,payload,bytes) VALUES(?1,?2,?3)",
                params![id, payload, i64::try_from(payload.len())?],
            )?;
        }
        let summary = RunSummary {
            run_id: run_id.clone().unwrap_or_default(),
            status: RunStatus::Interrupted,
            steps: 0,
            text: String::new(),
            usage: Usage::default(),
            error: Some("Previous process stopped. Pending effects were not replayed.".into()),
        };
        insert_event(
            &tx,
            &mut Event {
                sequence: 0,
                session_id: id.clone(),
                run_id,
                timestamp_ms: timestamp_ms(),
                kind: EventKind::RunFinished { summary },
            },
        )?;
        tx.execute(
            "UPDATE sessions SET status='interrupted' WHERE id=?1",
            [&id],
        )?;
        tx.commit()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ContextPolicy, ModelResponse, RecentContext, ToolCall, ToolOutput};
    use std::collections::HashSet;

    #[test]
    fn bounded_page_recovers_intent_beyond_the_row_limit() -> Result<()> {
        let mut db = Connection::open_in_memory()?;
        db.execute_batch("CREATE TABLE messages(sequence INTEGER PRIMARY KEY,session_id TEXT NOT NULL,payload TEXT NOT NULL,bytes INTEGER NOT NULL); CREATE TABLE tool_views(sequence INTEGER PRIMARY KEY,original_output TEXT NOT NULL,payload TEXT,bytes INTEGER)")?;
        let tx = db.transaction()?;
        let insert = |message: Message| -> Result<()> {
            let payload = serde_json::to_string(&message)?;
            tx.execute(
                "INSERT INTO messages(session_id,payload,bytes) VALUES('s',?1,?2)",
                params![payload, i64::try_from(payload.len())?],
            )?;
            Ok(())
        };
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
        let payload = serde_json::to_string(&Message::tool("orphan", &ToolOutput::text("result")))?;
        db.execute(
            "INSERT INTO messages(session_id,payload,bytes) VALUES('s',?1,?2)",
            params![payload, i64::try_from(payload.len())?],
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
}
