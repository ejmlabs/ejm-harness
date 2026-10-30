//! The SQLite journal: one writer thread owns the connection and serves queued jobs.
mod binding;
mod events;
mod history;
mod open;
mod recovery;
mod sessions;
#[cfg(test)]
mod tests;

use std::{fs::File, path::Path, sync::Arc};

use anyhow::{Context, Result, ensure};
use rusqlite::Connection;
use tokio::sync::{mpsc, oneshot};

use open::{lock_state_dir, open_database, spawn_writer};
use recovery::recover;

type Job = Box<dyn FnOnce(&mut Connection) + Send>;

const MIN_STORE_BYTES: u64 = 1024 * 1024;
const MAX_STORE_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const JOB_QUEUE: usize = 256;

#[derive(Clone)]
pub(crate) struct Store {
    jobs: mpsc::Sender<Job>,
    _lock: Arc<File>,
}

impl Store {
    pub fn open(root: &Path, max_bytes: u64) -> Result<Self> {
        ensure!(
            (MIN_STORE_BYTES..=MAX_STORE_BYTES).contains(&max_bytes),
            "store budget must be between 1 MiB and 8 GiB"
        );
        let lock = lock_state_dir(root)?;
        let mut connection = open_database(root, max_bytes)?;
        recover(&mut connection)?;
        let (jobs, receiver) = mpsc::channel::<Job>(JOB_QUEUE);
        let lock = Arc::new(lock);
        spawn_writer(connection, receiver, lock.clone())?;
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
