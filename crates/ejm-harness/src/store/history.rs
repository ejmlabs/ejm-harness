//! Durable messages and their bounded read pages.
use std::collections::BTreeMap;

use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OptionalExtension, params};

use super::Store;
use crate::{Message, Role, ToolCall, ToolOutput};

/// Most messages in one history page.
const MAX_PAGE_MESSAGES: usize = 2048;
/// A page may pass the context budget by this much while it reaches the start of a turn.
const PAGE_OVERSHOOT_BYTES: usize = 2 * 1024 * 1024;
/// Rows read after the latest assistant message to match results to its calls.
const PENDING_WINDOW: i64 = 17;

impl Store {
    pub async fn message(&self, session_id: &str, message: Message) -> Result<i64> {
        let id = session_id.to_owned();
        self.call(move |db| insert_message(db, &id, &message)).await
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

    pub async fn pending_calls(&self, session_id: &str) -> Result<Vec<ToolCall>> {
        let id = session_id.to_owned();
        self.call(move |db| load_pending_calls(db, &id)).await
    }
}

pub(super) fn insert_message(db: &Connection, session_id: &str, message: &Message) -> Result<i64> {
    let payload = serde_json::to_string(message)?;
    db.execute(
        "INSERT INTO messages(session_id,payload,bytes) VALUES(?1,?2,?3)",
        params![session_id, payload, i64::try_from(payload.len())?],
    )?;
    Ok(db.last_insert_rowid())
}

/// A bounded page of the newest history, oldest first, starting at a user message. A page
/// with no user message gets the latest earlier user message, noted as followed by omissions.
pub(super) fn load_messages(db: &Connection, id: &str, max_bytes: usize) -> Result<Vec<Message>> {
    let (mut page, bounded) = newest_page(db, id, max_bytes)?;
    page.reverse();
    if let Some(start) = page.iter().position(|(_, m)| m.role == Role::User) {
        page.drain(..start);
        return Ok(page.into_iter().map(|(_, message)| message).collect());
    }
    let Some(&(first, _)) = page.first() else {
        return Ok(vec![]);
    };
    ensure!(bounded, "session history has no user intent");
    let mut messages = vec![omitted_history_anchor(db, id, first)?];
    messages.extend(page.into_iter().map(|(_, message)| message));
    Ok(messages)
}

/// Read rows newest first until the byte or row bound, or the start of a turn past the
/// budget. Returns the rows and whether older rows were left out.
fn newest_page(db: &Connection, id: &str, max_bytes: usize) -> Result<(Vec<(i64, Message)>, bool)> {
    let mut query = db.prepare(
        "SELECT m.sequence,COALESCE(v.payload,m.payload),COALESCE(v.bytes,m.bytes) FROM messages m LEFT JOIN tool_views v ON v.sequence=m.sequence WHERE m.session_id=?1 ORDER BY m.sequence DESC LIMIT ?2",
    )?;
    let mut messages = vec![];
    let mut total: usize = 0;
    // Length of the newest prefix whose oldest message is not a tool result.
    let mut complete = 0;
    let mut bounded = false;
    let mut rows = query.query(params![id, i64::try_from(MAX_PAGE_MESSAGES + 1)?])?;
    while let Some(row) = rows.next()? {
        let bytes = usize::try_from(row.get::<_, i64>(2)?)?;
        if messages.len() == MAX_PAGE_MESSAGES
            || total.saturating_add(bytes) > max_bytes.saturating_add(PAGE_OVERSHOOT_BYTES)
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
    Ok((messages, bounded))
}

/// The latest user message before `first_sequence`, noted as followed by omitted history.
fn omitted_history_anchor(db: &Connection, id: &str, first_sequence: i64) -> Result<Message> {
    let anchor: Option<String> = db
        .query_row(
            "SELECT payload FROM messages WHERE session_id=?1 AND sequence<?2 AND json_extract(payload,'$.role')='user' ORDER BY sequence DESC LIMIT 1",
            params![id, first_sequence],
            |row| row.get(0),
        )
        .optional()?;
    let mut anchor: Message =
        serde_json::from_str(&anchor.context("bounded history page has no user intent")?)?;
    anchor.content.push_str("\n[ejm-harness context note: older exchanges were omitted from this bounded history page. Complete events remain in the session journal. Inspect current files before relying on omitted observations.]");
    Ok(anchor)
}

/// Calls of the latest assistant message that have no stored result.
pub(super) fn load_pending_calls(db: &Connection, id: &str) -> Result<Vec<ToolCall>> {
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
        "SELECT payload FROM messages WHERE session_id=?1 AND sequence>?2 ORDER BY sequence LIMIT ?3",
    )?;
    for row in query.query_map(params![id, sequence, PENDING_WINDOW], |row| {
        row.get::<_, String>(0)
    })? {
        messages.push(serde_json::from_str(&row?)?);
    }
    Ok(pending_calls(&messages))
}

/// Calls with no later result. A reused id is matched in message order.
pub(super) fn pending_calls(messages: &[Message]) -> Vec<ToolCall> {
    let mut pending = BTreeMap::new();
    for message in messages {
        for call in &message.tool_calls {
            pending.insert(call.id.clone(), call.clone());
        }
        if let Some(id) = &message.tool_call_id {
            pending.remove(id);
        }
    }
    pending.into_values().collect()
}
