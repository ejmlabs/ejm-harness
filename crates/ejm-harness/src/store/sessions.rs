use std::path::PathBuf;

use anyhow::{Context, Result};
use rusqlite::{OptionalExtension, params};

use super::{Store, unsigned_column};
use crate::{Session, SessionStatus};

impl Store {
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
