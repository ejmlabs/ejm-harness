//! Bounded setup of SQLite's persistent journal mode before registration.

use std::time::{Duration, Instant};

use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, ErrorCode};

pub(super) const BUSY_BUDGET: Duration = Duration::from_millis(100);

pub(super) fn enable_wal(connection: &Connection) -> Result<()> {
    let deadline = Instant::now() + BUSY_BUDGET;
    let result = loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        connection.busy_timeout(remaining)?;
        let attempt = connection
            .pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get::<_, String>(0));
        match attempt {
            Err(rusqlite::Error::SqliteFailure(error, _))
                if error.code == ErrorCode::DatabaseBusy && Instant::now() < deadline =>
            {
                // SQLite can skip its busy handler when upgrading an existing
                // read transaction. Only this idempotent setup pragma retries.
                std::thread::sleep(
                    deadline
                        .saturating_duration_since(Instant::now())
                        .min(Duration::from_millis(1)),
                );
            }
            result => break result.context("enable shared capacity WAL mode"),
        }
    };
    connection.busy_timeout(BUSY_BUDGET)?;
    ensure!(
        result?.eq_ignore_ascii_case("wal"),
        "shared capacity registry did not enable WAL mode"
    );
    Ok(())
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "Bootstrap fixtures assert SQLite contention and journal-mode boundaries."
)]
mod tests {
    use super::*;

    #[test]
    fn wal_setup_retries_read_upgrade_without_extending_the_busy_budget() {
        let (_parent, root) = super::super::tests::root();
        let registry = super::super::Registry::open(&root).unwrap();
        registry
            .connection
            .pragma_update(None, "journal_mode", "DELETE")
            .unwrap();
        let reader = Connection::open(root.join("capacity.sqlite3")).unwrap();
        reader
            .execute_batch("BEGIN; SELECT COUNT(*) FROM owners")
            .unwrap();

        let started = Instant::now();
        let error = enable_wal(&registry.connection).unwrap_err();
        assert!(started.elapsed() < Duration::from_secs(2));
        assert!(matches!(
            error.downcast_ref::<rusqlite::Error>(),
            Some(rusqlite::Error::SqliteFailure(error, _))
                if error.code == ErrorCode::DatabaseBusy
        ));
        let timeout: i64 = registry
            .connection
            .pragma_query_value(None, "busy_timeout", |row| row.get(0))
            .unwrap();
        assert_eq!(timeout, 100);

        let release = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            reader.execute_batch("COMMIT").unwrap();
        });
        enable_wal(&registry.connection).unwrap();
        release.join().unwrap();
        let mode: String = registry
            .connection
            .pragma_query_value(None, "journal_mode", |row| row.get(0))
            .unwrap();
        assert_eq!(mode, "wal");
        let timeout: i64 = registry
            .connection
            .pragma_query_value(None, "busy_timeout", |row| row.get(0))
            .unwrap();
        assert_eq!(timeout, 100);
    }
}
