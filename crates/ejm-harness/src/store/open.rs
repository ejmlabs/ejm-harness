//! Opening the state directory: exclusive ownership, schema, budget, and the writer thread.
use std::{
    fs::{File, OpenOptions},
    path::Path,
    sync::Arc,
    time::Duration,
};

use anyhow::{Context, Result};
use rusqlite::Connection;
use tokio::sync::mpsc;

use super::Job;

const DATABASE_FILE: &str = "sessions.sqlite3";
const LOCK_FILE: &str = "owner.lock";
const MIN_PAGE_COUNT: i64 = 256;

const SCHEMA: &str = "PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON; PRAGMA trusted_schema=OFF; PRAGMA wal_autocheckpoint=64;
    CREATE TABLE IF NOT EXISTS sessions(id TEXT PRIMARY KEY,title TEXT NOT NULL,cwd TEXT NOT NULL,created INTEGER NOT NULL,status TEXT NOT NULL);
    CREATE TABLE IF NOT EXISTS messages(sequence INTEGER PRIMARY KEY, session_id TEXT NOT NULL REFERENCES sessions(id),payload TEXT NOT NULL,bytes INTEGER NOT NULL);
    CREATE INDEX IF NOT EXISTS messages_by_session ON messages(session_id,sequence);
    CREATE INDEX IF NOT EXISTS messages_by_role ON messages(session_id,json_extract(payload,'$.role'),sequence);
    CREATE TABLE IF NOT EXISTS tool_views(sequence INTEGER PRIMARY KEY REFERENCES messages(sequence),original_output TEXT NOT NULL,payload TEXT,bytes INTEGER);
    CREATE TABLE IF NOT EXISTS events(sequence INTEGER PRIMARY KEY,session_id TEXT NOT NULL REFERENCES sessions(id),run_id TEXT,created INTEGER NOT NULL,payload TEXT NOT NULL);
    CREATE INDEX IF NOT EXISTS events_by_session ON events(session_id,sequence);
    CREATE TABLE IF NOT EXISTS settings(key TEXT PRIMARY KEY,value TEXT NOT NULL);";

/// Create the private state directory and take the exclusive owner lock.
pub(super) fn lock_state_dir(root: &Path) -> Result<File> {
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
            .open(root.join(DATABASE_FILE))?;
    }
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(root.join(LOCK_FILE))?;
    lock.try_lock().context(
        "state directory is already in use; share one harness or choose another state directory",
    )?;
    Ok(lock)
}

/// Open the journal database with its schema and a page budget of `max_bytes`.
pub(super) fn open_database(root: &Path, max_bytes: u64) -> Result<Connection> {
    let connection = Connection::open(root.join(DATABASE_FILE))?;
    connection.busy_timeout(Duration::from_secs(2))?;
    connection.execute_batch(SCHEMA)?;
    let page_size: i64 = connection.query_row("PRAGMA page_size", [], |r| r.get(0))?;
    connection.pragma_update(
        None,
        "max_page_count",
        (i64::try_from(max_bytes)? / page_size).max(MIN_PAGE_COUNT),
    )?;
    Ok(connection)
}

/// Run queued jobs on one thread. It holds the owner lock until the queue closes.
pub(super) fn spawn_writer(
    mut connection: Connection,
    mut jobs: mpsc::Receiver<Job>,
    lock: Arc<File>,
) -> Result<()> {
    std::thread::Builder::new()
        .name("ejm-harness-journal".into())
        .spawn(move || {
            let _lock = lock;
            while let Some(job) = jobs.blocking_recv() {
                job(&mut connection);
            }
            let _ = connection.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)");
        })?;
    Ok(())
}
