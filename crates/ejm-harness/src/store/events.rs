use anyhow::Result;
use rusqlite::{Connection, params};

use super::{Store, unsigned_column};
use crate::{Event, EventKind};

const MAX_EVENT_ROWS: usize = 1000;
const MAX_EVENT_PAGE_BYTES: usize = 2 * 1024 * 1024;

impl Store {
    pub async fn append(&self, mut event: Event) -> Result<Event> {
        self.call(move |db| {
            let tx = db.transaction()?;
            insert_event(&tx, &mut event)?;
            let status = match &event.kind {
                EventKind::RunStarted { .. } => Some("running"),
                EventKind::RunFinished { .. } => Some("idle"),
                _ => None,
            };
            if let Some(status) = status {
                tx.execute(
                    "UPDATE sessions SET status=?2 WHERE id=?1",
                    params![event.session_id, status],
                )?;
            }
            tx.commit()?;
            Ok(event)
        })
        .await
    }

    pub async fn events(&self, session_id: &str, after: u64, limit: usize) -> Result<Vec<Event>> {
        let id = session_id.to_owned();
        let limit = limit.clamp(1, MAX_EVENT_ROWS);
        self.call(move |db| {
            let mut statement = db.prepare(
                "SELECT sequence,run_id,created,payload FROM events WHERE session_id=?1 AND sequence>?2 ORDER BY sequence LIMIT ?3",
            )?;
            let rows = statement.query_map(
                params![
                    id,
                    i64::try_from(after).unwrap_or(i64::MAX),
                    i64::try_from(limit)?
                ],
                |row| {
                    Ok((
                        unsigned_column(row, 0)?,
                        row.get::<_, Option<String>>(1)?,
                        unsigned_column(row, 2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )?;
            let mut events = vec![];
            let mut bytes = 0;
            for row in rows {
                let (sequence, run_id, timestamp_ms, payload) = row?;
                bytes += payload.len();
                if bytes > MAX_EVENT_PAGE_BYTES && !events.is_empty() {
                    break;
                }
                events.push(Event {
                    sequence,
                    session_id: id.clone(),
                    run_id,
                    timestamp_ms,
                    kind: serde_json::from_str(&payload)?,
                });
            }
            Ok(events)
        })
        .await
    }
}

pub(super) fn insert_event(db: &Connection, event: &mut Event) -> Result<()> {
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
