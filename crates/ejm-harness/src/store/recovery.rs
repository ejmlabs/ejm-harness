//! Startup recovery: a session left running becomes interrupted. Pending effects are never replayed.
use anyhow::Result;
use rusqlite::{Connection, OptionalExtension};

use super::{
    events::insert_event,
    history::{insert_message, load_pending_calls},
};
use crate::{
    Event, EventKind, Message, RunStatus, RunSummary, ToolOutput, Usage, types::timestamp_ms,
};

const UNKNOWN_OUTCOME: &str = "Execution was interrupted. Its outcome is unknown. Inspect the workspace before deciding whether another action is needed. This action was not replayed.";
const INTERRUPTED_RUN: &str = "Previous process stopped. Pending effects were not replayed.";

pub(super) fn recover(db: &mut Connection) -> Result<()> {
    for id in running_sessions(db)? {
        interrupt_session(db, &id)?;
    }
    Ok(())
}

fn running_sessions(db: &Connection) -> Result<Vec<String>> {
    let mut statement = db.prepare("SELECT id FROM sessions WHERE status='running'")?;
    let ids = statement
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(ids)
}

/// Give every unanswered call an error result, finish the last run as interrupted, and
/// mark the session so the host decides what to do next.
fn interrupt_session(db: &mut Connection, id: &str) -> Result<()> {
    let run_id: Option<String> = db.query_row("SELECT run_id FROM events WHERE session_id=?1 AND run_id IS NOT NULL ORDER BY sequence DESC LIMIT 1", [id], |r| r.get(0)).optional()?.flatten();
    let missing = load_pending_calls(db, id)?;
    let tx = db.transaction()?;
    for call in missing {
        let message = Message::tool(call.id, &ToolOutput::error(UNKNOWN_OUTCOME));
        insert_message(&tx, id, &message)?;
    }
    let summary = RunSummary {
        run_id: run_id.clone().unwrap_or_default(),
        status: RunStatus::Interrupted,
        steps: 0,
        text: String::new(),
        usage: Usage::default(),
        error: Some(INTERRUPTED_RUN.into()),
    };
    insert_event(
        &tx,
        &mut Event {
            sequence: 0,
            session_id: id.to_owned(),
            run_id,
            timestamp_ms: timestamp_ms(),
            kind: EventKind::RunFinished { summary },
        },
    )?;
    tx.execute("UPDATE sessions SET status='interrupted' WHERE id=?1", [id])?;
    tx.commit()?;
    Ok(())
}
