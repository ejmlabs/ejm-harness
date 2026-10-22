use std::{
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OptionalExtension, params};
use tokio::sync::{mpsc, oneshot};

use crate::{
    Event, EventKind, EventPage, JournalSnapshot, Message, Role, RunStatus, RunSummary, Session,
    SessionCursor, SessionPage, SessionQuery, SessionRunTransition, SessionStatus,
    SessionSubscription, SessionTailOptions, SessionView, ToolOutput, Usage,
    engine::views::SelectedSubscriptions,
    types::{pending_calls, timestamp_ms},
};

type CallJob = Box<dyn FnOnce(&mut Connection) + Send>;

enum Job {
    Call(CallJob),
    Append(Box<AppendJob>),
}

struct AppendJob {
    event: Event,
    queued: Instant,
    reply: oneshot::Sender<Result<Event>>,
}

const APPEND_BATCH_EVENTS: usize = 32;
const APPEND_BATCH_BYTES: usize = 128 * 1024;
const APPEND_COLLECT_TIME: Duration = Duration::from_millis(4);

#[derive(Default)]
struct Timing {
    samples: AtomicU64,
    total: AtomicU64,
    max: AtomicU64,
    histogram: [AtomicU64; 32],
}

impl Timing {
    fn record(&self, started: Instant) {
        let elapsed = u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX);
        let bucket = usize::try_from(elapsed.checked_ilog2().unwrap_or(0))
            .unwrap_or(31)
            .min(31);
        self.samples.fetch_add(1, Ordering::Relaxed);
        self.total.fetch_add(elapsed, Ordering::Relaxed);
        self.max.fetch_max(elapsed, Ordering::Relaxed);
        self.histogram[bucket].fetch_add(1, Ordering::Relaxed);
    }

    fn histogram(&self) -> [u64; 32] {
        std::array::from_fn(|index| self.histogram[index].load(Ordering::Relaxed))
    }
}

#[derive(Default)]
struct JournalTimings {
    queue: Timing,
    work: Timing,
    commit: Timing,
}

struct StoreFiles {
    database: PathBuf,
    _lock: File,
    #[cfg(windows)]
    _directory: rightkit_fs::StateDir,
    #[cfg(windows)]
    _database: File,
}

#[cfg(not(windows))]
impl StoreFiles {
    fn open(root: &Path) -> Result<Self> {
        #[cfg(target_os = "macos")]
        {
            rightkit_fs::private::ensure_private_dir(root)?;
            rightkit_fs::private::verify_owner_only(root)?;
            for name in ["owner.lock", "sessions.sqlite3"] {
                let path = root.join(name);
                if !path.try_exists()? {
                    match rightkit_fs::private::create_private_new(&path) {
                        Ok(_) => {}
                        Err(_) if path.try_exists()? => {}
                        Err(error) => return Err(error).context("create private journal file"),
                    }
                }
            }
            for name in [
                "owner.lock",
                "sessions.sqlite3",
                "sessions.sqlite3-wal",
                "sessions.sqlite3-shm",
                "sessions.sqlite3-journal",
            ] {
                let path = root.join(name);
                if path.try_exists()? {
                    rightkit_fs::private::verify_owner_only(&path)?;
                }
            }
        }
        #[cfg(not(target_os = "macos"))]
        std::fs::create_dir_all(root)?;
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
            std::fs::set_permissions(root, std::fs::Permissions::from_mode(0o700))?;
            options.mode(0o600);
            let _ = options.open(root.join("sessions.sqlite3"))?;
        }
        let lock = options.open(root.join("owner.lock"))?;
        lock.try_lock().context("state directory is already in use; share one harness or choose another state directory")?;
        Ok(Self {
            database: root.join("sessions.sqlite3"),
            _lock: lock,
        })
    }
}

#[cfg(windows)]
impl StoreFiles {
    fn open(root: &Path) -> Result<Self> {
        use std::os::windows::fs::{MetadataExt, OpenOptionsExt};

        use rightkit_fs::{StateDir, handle::ReadOptions};

        let directory = StateDir::create(&std::path::absolute(root)?)
            .context("open protected journal directory")?;
        rightkit_fs::private::verify_owner_only(directory.path())
            .context("journal directory must have an owner-only Windows ACL")?;
        // SQLite creates WAL/SHM files itself. A trusted directory must also pass
        // an inheritable owner-only DACL to each file created by the native VFS.
        rightkit_fs::private::restrict_owner_only(directory.path())?;
        ensure!(
            rightkit_fs::private::is_owner_only_strict(directory.path())?,
            "journal directory requires an inheritable owner-only Windows ACL"
        );
        for name in ["owner.lock", "sessions.sqlite3"] {
            if !directory.exists(name)? {
                match directory.write_new(name, &[]) {
                    Ok(()) => {}
                    Err(_) if directory.exists(name)? => {}
                    Err(error) => return Err(error).context("create journal state file"),
                }
            }
        }
        let lock = directory.open_read(
            "owner.lock",
            &ReadOptions::new()
                .require_single_link(true)
                .share_read_only(true),
        )?;
        rightkit_fs::private::verify_owner_only(&directory.path().join("owner.lock"))?;
        lock.try_lock().context("state directory is already in use; share one harness or choose another state directory")?;
        let database = directory.path().join("sessions.sqlite3");
        // Share reads and SQLite writes, but deny deletion for the connection's
        // lifetime. The retained directory prevents parent replacement as well.
        let database_anchor = OpenOptions::new()
            .read(true)
            .share_mode(0x3)
            .custom_flags(0x0020_0000)
            .open(&database)?;
        let metadata = database_anchor.metadata()?;
        ensure!(
            metadata.is_file() && metadata.file_attributes() & 0x400 == 0,
            "journal database must be a regular file without a reparse point"
        );
        ensure!(
            rightkit_fs::handle::link_count(&database_anchor)? == 1,
            "journal database cannot have multiple hard links"
        );
        rightkit_fs::private::verify_owner_only(&database)?;
        for name in [
            "sessions.sqlite3-wal",
            "sessions.sqlite3-shm",
            "sessions.sqlite3-journal",
        ] {
            if directory.exists(name)? {
                let _file =
                    directory.open_read(name, &ReadOptions::new().require_single_link(true))?;
                rightkit_fs::private::verify_owner_only(&directory.path().join(name))?;
            }
        }
        Ok(Self {
            database,
            _lock: lock,
            _directory: directory,
            _database: database_anchor,
        })
    }
}

#[derive(Clone)]
pub(crate) struct Store {
    jobs: mpsc::Sender<Job>,
    selected: Arc<SelectedSubscriptions>,
    timings: Arc<JournalTimings>,
    _ownership: Arc<StoreFiles>,
}

impl Store {
    pub fn open(root: &Path, max_bytes: u64) -> Result<Self> {
        ensure!(
            (1024 * 1024..=8 * 1024 * 1024 * 1024).contains(&max_bytes),
            "store budget must be between 1 MiB and 8 GiB"
        );
        let ownership = Arc::new(StoreFiles::open(root)?);
        let mut connection = Connection::open(&ownership.database)?;
        connection.busy_timeout(std::time::Duration::from_secs(2))?;
        let page_size: i64 = connection.query_row("PRAGMA page_size", [], |r| r.get(0))?;
        connection.pragma_update(
            None,
            "max_page_count",
            (i64::try_from(max_bytes)? / page_size).max(256),
        )?;
        connection.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON; PRAGMA trusted_schema=OFF; PRAGMA wal_autocheckpoint=64;
            CREATE TABLE IF NOT EXISTS sessions(id TEXT PRIMARY KEY,title TEXT NOT NULL,cwd TEXT NOT NULL,created INTEGER NOT NULL,status TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS messages(sequence INTEGER PRIMARY KEY, session_id TEXT NOT NULL REFERENCES sessions(id),payload TEXT NOT NULL,bytes INTEGER NOT NULL);
            CREATE INDEX IF NOT EXISTS messages_by_session ON messages(session_id,sequence);
            CREATE INDEX IF NOT EXISTS messages_by_role ON messages(session_id,json_extract(payload,'$.role'),sequence);
            CREATE TABLE IF NOT EXISTS tool_views(sequence INTEGER PRIMARY KEY REFERENCES messages(sequence),original_output TEXT NOT NULL,payload TEXT,bytes INTEGER);
            CREATE TABLE IF NOT EXISTS events(sequence INTEGER PRIMARY KEY,session_id TEXT NOT NULL REFERENCES sessions(id),run_id TEXT,created INTEGER NOT NULL,payload TEXT NOT NULL);
            CREATE INDEX IF NOT EXISTS events_by_session ON events(session_id,sequence);
            CREATE INDEX IF NOT EXISTS events_by_transition ON events(session_id,sequence DESC) WHERE json_extract(payload,'$.type') IN ('run_started','run_finished');
            CREATE TABLE IF NOT EXISTS session_authority(session_id TEXT PRIMARY KEY REFERENCES sessions(id),sequence INTEGER NOT NULL,run_id TEXT,outcome TEXT);
            CREATE INDEX IF NOT EXISTS sessions_by_created ON sessions(created DESC,id DESC);
            CREATE INDEX IF NOT EXISTS sessions_by_title ON sessions(title COLLATE NOCASE);
            CREATE INDEX IF NOT EXISTS sessions_by_identity ON sessions(id COLLATE NOCASE);
            CREATE TABLE IF NOT EXISTS settings(key TEXT PRIMARY KEY,value TEXT NOT NULL);")?;
        recover(&mut connection)?;
        let (jobs, receiver) = mpsc::channel::<Job>(256);
        let selected = Arc::new(SelectedSubscriptions::default());
        let timings = Arc::new(JournalTimings::default());
        let writer_selected = Arc::clone(&selected);
        let writer_timings = Arc::clone(&timings);
        let writer_ownership = Arc::clone(&ownership);
        std::thread::Builder::new()
            .name("ejm-harness-journal".into())
            .spawn(move || {
                let _ownership = writer_ownership;
                journal_worker(&mut connection, receiver, &writer_selected, &writer_timings);
                let _ = connection.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)");
                drop(connection);
            })?;
        Ok(Self {
            jobs,
            selected,
            timings,
            _ownership: ownership,
        })
    }

    async fn call<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Connection) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let (tx, rx) = oneshot::channel();
        let queued = Instant::now();
        let timings = Arc::clone(&self.timings);
        self.jobs
            .send(Job::Call(Box::new(move |db| {
                timings.queue.record(queued);
                let started = Instant::now();
                let result = f(db);
                timings.work.record(started);
                let _ = tx.send(result);
            })))
            .await
            .map_err(|_| anyhow::anyhow!("journal stopped"))?;
        rx.await.context("journal writer stopped")?
    }

    pub fn journal_status(&self) -> JournalSnapshot {
        let timings = &self.timings;
        JournalSnapshot {
            calls: timings.work.samples.load(Ordering::Relaxed),
            append_commits: timings.commit.samples.load(Ordering::Relaxed),
            queue_wait_ns: timings.queue.total.load(Ordering::Relaxed),
            work_ns: timings.work.total.load(Ordering::Relaxed),
            commit_ns: timings.commit.total.load(Ordering::Relaxed),
            max_queue_wait_ns: timings.queue.max.load(Ordering::Relaxed),
            max_work_ns: timings.work.max.load(Ordering::Relaxed),
            max_commit_ns: timings.commit.max.load(Ordering::Relaxed),
            queue_wait_histogram: timings.queue.histogram(),
            work_histogram: timings.work.histogram(),
            commit_histogram: timings.commit.histogram(),
        }
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

    pub async fn sessions_page(&self, query: SessionQuery) -> Result<SessionPage> {
        let search = query.query.clone();
        ensure!(search.len() <= 256, "session query exceeds 256 UTF-8 bytes");
        if let Some(cursor) = &query.cursor {
            ensure!(
                cursor.query == search,
                "session cursor belongs to another query"
            );
            ensure!(
                cursor.id.len() <= 256 && cursor.upper_rowid >= 0,
                "invalid session cursor"
            );
        }
        let limit = query.limit.clamp(1, 100);
        self.call(move |db| {
            let upper_rowid = if let Some(cursor) = &query.cursor {
                cursor.upper_rowid
            } else {
                db.query_row("SELECT COALESCE(MAX(rowid),0) FROM sessions", [], |row| row.get(0))?
            };
            let before_created = query.cursor.as_ref().map_or(i64::MAX, |cursor| i64::try_from(cursor.created_at_ms).unwrap_or(i64::MAX));
            let before_id = query.cursor.as_ref().map_or("\u{10ffff}", |cursor| cursor.id.as_str());
            let pattern = format!("{}%", escape_like(&search));
            let sql = if search.is_empty() {
                "SELECT id,title,cwd,created,status FROM sessions WHERE rowid<=?1 AND (created,id)<(?2,?3) ORDER BY created DESC,id DESC LIMIT ?5"
            } else {
                "SELECT id,title,cwd,created,status FROM sessions WHERE rowid<=?1 AND (created,id)<(?2,?3) AND (id LIKE ?4 ESCAPE '\\' OR title LIKE ?4 ESCAPE '\\') ORDER BY created DESC,id DESC LIMIT ?5"
            };
            let mut statement = db.prepare(sql)?;
            let mut sessions = statement.query_map(params![upper_rowid, before_created, before_id, pattern, i64::try_from(limit + 1)?], row_session)?.collect::<rusqlite::Result<Vec<_>>>()?;
            let has_more = sessions.len() > limit;
            sessions.truncate(limit);
            let next = if has_more {
                sessions.last().map(|session| SessionCursor { created_at_ms: session.created_at_ms, id: session.id.clone(), upper_rowid, query: search })
            } else { None };
            Ok(SessionPage { sessions, next })
        }).await
    }

    pub async fn resolve_session(&self, prefix: &str) -> Result<Session> {
        ensure!(
            !prefix.is_empty() && prefix.len() <= 256,
            "session identity must contain 1 to 256 UTF-8 bytes"
        );
        let prefix = prefix.to_owned();
        self.call(move |db| {
            let exact = db.query_row("SELECT id,title,cwd,created,status FROM sessions WHERE id=?1", [&prefix], row_session).optional()?;
            if let Some(session) = exact { return Ok(session) }
            let pattern = format!("{}%", escape_like(&prefix));
            let mut statement = db.prepare("SELECT id,title,cwd,created,status FROM sessions WHERE id LIKE ?1 ESCAPE '\\' ORDER BY id LIMIT 2")?;
            let mut sessions = statement.query_map([pattern], row_session)?.collect::<rusqlite::Result<Vec<_>>>()?;
            ensure!(sessions.len() <= 1, "session prefix is ambiguous; use the complete ID");
            sessions.pop().context("unknown session")
        }).await
    }

    pub async fn subscribe_session(&self, session_id: &str) -> Result<SessionSubscription> {
        self.subscribe_session_inner(
            session_id,
            #[cfg(feature = "test-support")]
            false,
        )
        .await
    }

    #[cfg(feature = "test-support")]
    pub async fn subscribe_session_traced(&self, session_id: &str) -> Result<SessionSubscription> {
        self.subscribe_session_inner(session_id, true).await
    }

    async fn subscribe_session_inner(
        &self,
        session_id: &str,
        #[cfg(feature = "test-support")] traced: bool,
    ) -> Result<SessionSubscription> {
        let id = session_id.to_owned();
        let selected = Arc::clone(&self.selected);
        self.call(move |db| {
            ensure!(
                db.query_row(
                    "SELECT EXISTS(SELECT 1 FROM sessions WHERE id=?1)",
                    [&id],
                    |row| row.get::<_, bool>(0)
                )?,
                "unknown session"
            );
            #[cfg(feature = "test-support")]
            if traced {
                return selected.subscribe_traced(&id);
            }
            selected.subscribe(&id)
        })
        .await
    }

    pub async fn session_view(
        &self,
        session_id: &str,
        options: SessionTailOptions,
    ) -> Result<SessionView> {
        let id = session_id.to_owned();
        let options = options.bounded();
        self.call(move |db| {
            let tx = db.transaction()?;
            let session = tx
                .query_row(
                    "SELECT id,title,cwd,created,status FROM sessions WHERE id=?1",
                    [&id],
                    row_session,
                )
                .optional()?
                .context("unknown session")?;
            let durable_cursor =
                tx.query_row("SELECT COALESCE(MAX(sequence),0) FROM events", [], |row| {
                    unsigned_column(row, 0)
                })?;
            let selected_cursor = tx.query_row(
                "SELECT COALESCE(MAX(sequence),0) FROM events WHERE session_id=?1",
                [&id],
                |row| unsigned_column(row, 0),
            )?;
            let last_transition = load_transition(&tx, &session)?;
            let run_id = if session.status == SessionStatus::Running {
                last_transition
                    .as_ref()
                    .and_then(|transition| transition.run_id.clone())
            } else {
                None
            };
            let (events, omitted) = load_tail(&tx, &id, options)?;
            tx.commit()?;
            Ok(SessionView {
                session,
                run_id,
                last_transition,
                durable_cursor,
                selected_cursor,
                events,
                omitted,
            })
        })
        .await
    }

    pub async fn events_page(
        &self,
        session_id: &str,
        after: u64,
        through: u64,
        options: SessionTailOptions,
    ) -> Result<EventPage> {
        let id = session_id.to_owned();
        let options = options.bounded();
        ensure!(
            options.max_events > 0 && options.max_bytes > 0,
            "event page bounds must be positive"
        );
        self.call(move |db| {
            ensure!(
                db.query_row(
                    "SELECT EXISTS(SELECT 1 FROM sessions WHERE id=?1)",
                    [&id],
                    |row| row.get::<_, bool>(0)
                )?,
                "unknown session"
            );
            load_event_page(db, &id, after, through, options)
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

    pub async fn append(&self, event: Event) -> Result<Event> {
        let (reply, result) = oneshot::channel();
        self.jobs
            .send(Job::Append(Box::new(AppendJob {
                event,
                queued: Instant::now(),
                reply,
            })))
            .await
            .map_err(|_| anyhow::anyhow!("journal stopped"))?;
        result.await.context("journal writer stopped")?
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

    /// Read one already accepted call after bounded dispatch admission. Waiting
    /// drivers retain its identity rather than the assistant's argument trees.
    pub async fn tool_call(
        &self,
        session_id: &str,
        assistant_sequence: i64,
        call_id: &str,
    ) -> Result<crate::ToolCall> {
        let session = session_id.to_owned();
        let call = call_id.to_owned();
        self.call(move |db| {
            let payload: Option<String> = db.query_row(
                "SELECT substr(value,1,98305) FROM json_each((SELECT payload FROM messages WHERE session_id=?1 AND sequence=?2),'$.tool_calls') WHERE json_extract(value,'$.id')=?3 LIMIT 1",
                params![session, assistant_sequence, call],
                |row| row.get(0),
            ).optional()?;
            let payload = payload.context("accepted tool call is missing from the journal")?;
            ensure!(payload.len() <= 96 * 1024, "accepted tool call exceeds its byte budget");
            Ok(serde_json::from_str(&payload)?)
        }).await
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

fn journal_worker(
    connection: &mut Connection,
    mut receiver: mpsc::Receiver<Job>,
    selected: &SelectedSubscriptions,
    timings: &JournalTimings,
) {
    let mut next = None;
    while let Some(job) = next.take().or_else(|| receiver.blocking_recv()) {
        match job {
            Job::Call(job) => job(connection),
            Job::Append(first) => {
                let (mut batch, following) = collect_appends(*first, &mut receiver);
                next = following;
                for job in &batch {
                    timings.queue.record(job.queued);
                }
                let started = Instant::now();
                let result = append_batch(connection, &mut batch, selected, timings);
                timings.work.record(started);
                for job in batch {
                    let result = match &result {
                        Ok(()) => Ok(job.event),
                        Err(error) => Err(anyhow::anyhow!("journal append failed: {error:#}")),
                    };
                    let _ = job.reply.send(result);
                }
            }
        }
    }
}

fn collect_appends(
    first: AppendJob,
    receiver: &mut mpsc::Receiver<Job>,
) -> (Vec<AppendJob>, Option<Job>) {
    let mut bytes = text_delta_bytes(&first.event);
    let mut batch = vec![first];
    if bytes.is_none() {
        return (batch, None);
    }
    let started = Instant::now();
    while batch.len() < APPEND_BATCH_EVENTS && started.elapsed() < APPEND_COLLECT_TIME {
        match receiver.try_recv() {
            Ok(Job::Append(job)) => {
                let total = text_delta_bytes(&job.event)
                    .and_then(|size| bytes.and_then(|used| used.checked_add(size)));
                if total.is_none_or(|size| size > APPEND_BATCH_BYTES) {
                    return (batch, Some(Job::Append(job)));
                }
                bytes = total;
                batch.push(*job);
            }
            Ok(barrier) => return (batch, Some(barrier)),
            Err(_) => break,
        }
    }
    (batch, None)
}

fn text_delta_bytes(event: &Event) -> Option<usize> {
    if let EventKind::TextDelta { text } = &event.kind {
        Some(text.len())
    } else {
        None
    }
}

fn append_batch(
    connection: &mut Connection,
    batch: &mut [AppendJob],
    selected: &SelectedSubscriptions,
    timings: &JournalTimings,
) -> Result<()> {
    let transaction = connection.transaction()?;
    let mut publications = Vec::with_capacity(batch.len());
    for job in batch.iter_mut() {
        let event = &mut job.event;
        let interest = selected.interest(&event.session_id);
        let previous = if interest.active {
            transaction.query_row(
                "SELECT COALESCE(MAX(sequence),0) FROM events WHERE session_id=?1",
                [&event.session_id],
                |row| unsigned_column(row, 0),
            )?
        } else {
            0
        };
        let bytes = insert_event(&transaction, event)?;
        let status = match &event.kind {
            EventKind::RunStarted { .. } => Some("running"),
            EventKind::RunFinished { .. } => Some("idle"),
            _ => None,
        };
        if let Some(status) = status {
            transaction.execute(
                "UPDATE sessions SET status=?2 WHERE id=?1",
                params![event.session_id, status],
            )?;
        }
        publications.push((interest, previous, bytes));
    }
    #[cfg(feature = "test-support")]
    let traced = publications.iter().any(|(interest, _, _)| interest.traced);
    let committing = Instant::now();
    transaction.commit()?;
    #[cfg(feature = "test-support")]
    let committed_at = traced.then(Instant::now);
    timings.commit.record(committing);
    for (job, (interest, previous, bytes)) in batch.iter().zip(publications) {
        if interest.active {
            selected.publish(
                &job.event,
                previous,
                bytes,
                #[cfg(feature = "test-support")]
                committed_at,
            );
        }
    }
    Ok(())
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

fn escape_like(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

fn load_transition(db: &Connection, session: &Session) -> Result<Option<SessionRunTransition>> {
    let mut transition = db
        .query_row(
            "SELECT sequence,run_id,outcome FROM session_authority WHERE session_id=?1",
            [&session.id],
            |row| {
                Ok((
                    unsigned_column(row, 0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            },
        )
        .optional()?;
    if transition.is_none() {
        // Old journals need only one indexed transition, never a history scan.
        transition = db.query_row(
            "SELECT sequence,run_id,json_extract(payload,'$.summary.status') FROM events WHERE session_id=?1 AND json_extract(payload,'$.type') IN ('run_started','run_finished') ORDER BY sequence DESC LIMIT 1",
            [&session.id],
            |row| Ok((unsigned_column(row, 0)?, row.get::<_, Option<String>>(1)?, row.get::<_, Option<String>>(2)?)),
        ).optional()?;
        if let Some((sequence, run_id, outcome)) = &transition {
            db.execute(
                "INSERT INTO session_authority VALUES(?1,?2,?3,?4)",
                params![session.id, i64::try_from(*sequence)?, run_id, outcome],
            )?;
        }
    }
    transition
        .map(|(sequence, run_id, outcome)| {
            Ok(SessionRunTransition {
                sequence,
                run_id,
                status: session.status.clone(),
                outcome: outcome
                    .map(|outcome| serde_json::from_value(serde_json::Value::String(outcome)))
                    .transpose()?,
            })
        })
        .transpose()
}

fn read_event(row: &rusqlite::Row<'_>, session_id: &str) -> Result<Event> {
    Ok(Event {
        sequence: unsigned_column(row, 0)?,
        session_id: session_id.to_owned(),
        run_id: row.get(1)?,
        timestamp_ms: unsigned_column(row, 2)?,
        kind: serde_json::from_str(&row.get::<_, String>(4)?)?,
    })
}

fn load_tail(db: &Connection, id: &str, options: SessionTailOptions) -> Result<(Vec<Event>, bool)> {
    if options.max_events == 0 || options.max_bytes == 0 {
        return Ok((
            vec![],
            db.query_row(
                "SELECT EXISTS(SELECT 1 FROM events WHERE session_id=?1)",
                [id],
                |row| row.get(0),
            )?,
        ));
    }
    let mut statement = db.prepare("SELECT sequence,run_id,created,length(CAST(payload AS BLOB)),payload FROM events WHERE session_id=?1 ORDER BY sequence DESC LIMIT ?2")?;
    let mut rows = statement.query(params![id, i64::try_from(options.max_events + 1)?])?;
    let mut events = vec![];
    let mut bytes: usize = 0;
    let mut omitted = false;
    while let Some(row) = rows.next()? {
        let payload_bytes = usize::try_from(row.get::<_, i64>(3)?)?;
        if events.len() == options.max_events
            || bytes.saturating_add(payload_bytes) > options.max_bytes
        {
            omitted = true;
            break;
        }
        bytes += payload_bytes;
        events.push(read_event(row, id)?);
    }
    events.reverse();
    Ok((events, omitted))
}

fn load_event_page(
    db: &Connection,
    id: &str,
    after: u64,
    through: u64,
    options: SessionTailOptions,
) -> Result<EventPage> {
    let durable = db.query_row("SELECT COALESCE(MAX(sequence),0) FROM events", [], |row| {
        unsigned_column(row, 0)
    })?;
    let through = through.min(durable);
    ensure!(after <= through, "event cursor exceeds durable boundary");
    let mut statement = db.prepare("SELECT sequence,run_id,created,length(CAST(payload AS BLOB)),payload FROM events WHERE session_id=?1 AND sequence>?2 AND sequence<=?3 ORDER BY sequence LIMIT ?4")?;
    let mut rows = statement.query(params![
        id,
        i64::try_from(after)?,
        i64::try_from(through)?,
        i64::try_from(options.max_events + 1)?
    ])?;
    let mut events = vec![];
    let mut bytes: usize = 0;
    let mut complete = true;
    let mut omitted = false;
    while let Some(row) = rows.next()? {
        if events.len() == options.max_events {
            complete = false;
            break;
        }
        let payload_bytes = usize::try_from(row.get::<_, i64>(3)?)?;
        if bytes.saturating_add(payload_bytes) > options.max_bytes {
            complete = false;
            omitted = payload_bytes > options.max_bytes;
            break;
        }
        bytes += payload_bytes;
        events.push(read_event(row, id)?);
    }
    let selected_cursor = events.last().map(|event| event.sequence);
    let cursor = if complete {
        through
    } else {
        selected_cursor.unwrap_or(after)
    };
    Ok(EventPage {
        events,
        cursor,
        selected_cursor,
        through,
        complete,
        omitted,
    })
}

fn insert_event(db: &Connection, event: &mut Event) -> Result<usize> {
    let payload = serde_json::to_string(&event.kind)?;
    db.execute(
        "INSERT INTO events(session_id,run_id,created,payload) VALUES(?1,?2,?3,?4)",
        params![
            event.session_id,
            event.run_id,
            i64::try_from(event.timestamp_ms)?,
            payload
        ],
    )?;
    event.sequence = u64::try_from(db.last_insert_rowid())?;
    let transition = match &event.kind {
        EventKind::RunStarted { .. } => Some(None),
        EventKind::RunFinished { summary } => Some(Some(
            serde_json::to_value(&summary.status)?
                .as_str()
                .context("run status is not a string")?
                .to_owned(),
        )),
        _ => None,
    };
    if let Some(outcome) = transition {
        db.execute("INSERT INTO session_authority VALUES(?1,?2,?3,?4) ON CONFLICT(session_id) DO UPDATE SET sequence=excluded.sequence,run_id=excluded.run_id,outcome=excluded.outcome", params![event.session_id, i64::try_from(event.sequence)?, event.run_id, outcome])?;
    }
    Ok(payload.len())
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

    async fn view_store() -> Result<(tempfile::TempDir, Store)> {
        let root = tempfile::tempdir()?;
        let store = Store::open(root.path(), 8 * 1024 * 1024)?;
        for id in ["selected", "other"] {
            store
                .create_session(Session {
                    id: id.into(),
                    title: id.into(),
                    cwd: root.path().into(),
                    created_at_ms: 0,
                    status: SessionStatus::Idle,
                })
                .await?;
        }
        Ok((root, store))
    }

    async fn view_event(store: &Store, id: &str, kind: EventKind) -> Result<Event> {
        store
            .append(Event {
                sequence: 0,
                session_id: id.into(),
                run_id: Some("run".into()),
                timestamp_ms: 1,
                kind,
            })
            .await
    }

    async fn pause_writer(store: &Store) -> Result<std::sync::mpsc::SyncSender<()>> {
        let (release, wait) = std::sync::mpsc::sync_channel(1);
        let (entered, ready) = oneshot::channel();
        queue_job(
            store,
            Job::Call(Box::new(move |_| {
                let _ = entered.send(());
                let _ = wait.recv();
            })),
        )
        .await?;
        ready.await?;
        Ok(release)
    }

    async fn queue_append(
        store: &Store,
        id: &str,
        kind: EventKind,
    ) -> Result<oneshot::Receiver<Result<Event>>> {
        let (reply, received) = oneshot::channel();
        queue_job(
            store,
            Job::Append(Box::new(AppendJob {
                event: Event {
                    sequence: 0,
                    session_id: id.into(),
                    run_id: Some("run".into()),
                    timestamp_ms: 1,
                    kind,
                },
                queued: Instant::now(),
                reply,
            })),
        )
        .await?;
        Ok(received)
    }

    async fn queue_job(store: &Store, job: Job) -> Result<()> {
        store
            .jobs
            .send(job)
            .await
            .map_err(|_| anyhow::anyhow!("fixture journal stopped"))
    }

    fn selected_is_pending(selected: &mut SessionSubscription) -> bool {
        let mut receive = Box::pin(selected.recv());
        let mut context = std::task::Context::from_waker(std::task::Waker::noop());
        std::future::Future::poll(receive.as_mut(), &mut context).is_pending()
    }

    #[cfg(feature = "test-support")]
    #[tokio::test]
    async fn traced_selected_batch_stamps_are_opt_in_and_precede_receipt() -> Result<()> {
        tokio::time::timeout(Duration::from_secs(5), async {
            let (_root, store) = view_store().await?;
            let mut ordinary = store.subscribe_session("selected").await?;
            let baseline = view_event(
                &store,
                "selected",
                EventKind::TextDelta {
                    text: "ordinary".into(),
                },
            )
            .await?;
            let (live, stamp) = ordinary.recv_traced().await?;
            assert_eq!(live.event.sequence, baseline.sequence);
            assert!(stamp.is_none());

            let mut traced = store.subscribe_session_traced("selected").await?;
            let peer = store.subscribe_session_traced("selected").await?;
            drop(peer);
            let mut other = store.subscribe_session("other").await?;
            let release = pause_writer(&store).await?;
            let mut replies = vec![];
            for (id, text) in [("selected", "a"), ("other", "b"), ("selected", "c")] {
                replies.push(
                    queue_append(&store, id, EventKind::TextDelta { text: text.into() }).await?,
                );
            }
            release.send(())?;
            let mut committed = vec![];
            for reply in replies {
                committed.push(reply.await??);
            }
            // Take this later writer barrier before receiving any live event.
            // A receipt-time stamp would fall after it and fail this check.
            let barrier = store.call(|_| Ok(Instant::now())).await?;
            let (first, stamp) = traced.recv_traced().await?;
            let stamp = stamp.context("traced batch is missing its commit stamp")?;
            let (second, second_stamp) = traced.recv_traced().await?;
            let (interleaved, other_stamp) = other.recv_traced().await?;
            assert!(stamp <= barrier);
            assert_eq!(second_stamp, Some(stamp));
            assert_eq!(other_stamp, Some(stamp));
            assert_eq!(first.event.sequence, committed[0].sequence);
            assert_eq!(interleaved.event.sequence, committed[1].sequence);
            assert_eq!(second.event.sequence, committed[2].sequence);
            assert_eq!(first.previous_sequence, baseline.sequence);
            assert_eq!(second.previous_sequence, first.event.sequence);
            assert_eq!(interleaved.previous_sequence, 0);
            assert_eq!(ordinary.recv().await?.event.sequence, first.event.sequence);
            assert_eq!(ordinary.recv().await?.event.sequence, second.event.sequence);

            drop(traced);
            let untraced = view_event(
                &store,
                "selected",
                EventKind::TextDelta {
                    text: "after trace".into(),
                },
            )
            .await?;
            let (live, stamp) = ordinary.recv_traced().await?;
            assert!(stamp.is_none());
            assert_eq!(live.event.sequence, untraced.sequence);
            assert_eq!(live.previous_sequence, second.event.sequence);
            Ok(())
        })
        .await?
    }

    #[cfg(feature = "test-support")]
    #[tokio::test]
    async fn traced_selected_reset_preserves_the_original_pending_stamp() -> Result<()> {
        tokio::time::timeout(Duration::from_secs(5), async {
            let (_root, store) = view_store().await?;
            let mut pending = store.subscribe_session_traced("selected").await?;
            let mut direct = store.subscribe_session_traced("selected").await?;
            let oversized = view_event(
                &store,
                "selected",
                EventKind::TextDelta {
                    text: "x".repeat(crate::engine::views::SELECTED_BYTES + 1),
                },
            )
            .await?;
            assert!(matches!(
                direct.recv_traced().await,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_))
            ));
            let marker = view_event(
                &store,
                "selected",
                EventKind::TextDelta {
                    text: "marker".into(),
                },
            )
            .await?;
            let barrier = store.call(|_| Ok(Instant::now())).await?;
            let (original, stamp) = direct.recv_traced().await?;
            let stamp = stamp.context("marker is missing its commit stamp")?;
            assert!(stamp <= barrier);

            // The first ordinary receive observes ring overflow; the second
            // drains reset signals and retains the marker for the next call.
            for _ in 0..2 {
                assert!(matches!(
                    pending.recv().await,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_))
                ));
            }
            let (preserved, preserved_stamp) = pending.recv_traced().await?;
            assert_eq!(preserved_stamp, Some(stamp));
            assert_eq!(preserved.event.sequence, marker.sequence);
            assert_eq!(preserved.event.sequence, original.event.sequence);
            assert_eq!(preserved.previous_sequence, oversized.sequence);
            assert_eq!(preserved.previous_sequence, original.previous_sequence);
            Ok(())
        })
        .await?
    }

    #[tokio::test]
    async fn text_group_commit_preserves_barriers_durable_order_and_dropped_replies() -> Result<()>
    {
        let (_root, store) = view_store().await?;
        let mut selected = store.subscribe_session("selected").await?;
        let release = pause_writer(&store).await?;
        let mut replies = vec![];
        for (id, text) in [
            ("selected", "a"),
            ("other", "b"),
            ("selected", "c"),
            ("selected", "d"),
        ] {
            replies
                .push(queue_append(&store, id, EventKind::TextDelta { text: text.into() }).await?);
        }
        assert!(matches!(
            replies[0].try_recv(),
            Err(oneshot::error::TryRecvError::Empty)
        ));
        assert!(selected_is_pending(&mut selected));
        // Losing a caller after queue acceptance must not erase its durable event.
        drop(replies.pop());
        let (barrier, observed) = oneshot::channel();
        let timings = Arc::clone(&store.timings);
        queue_job(
            &store,
            Job::Call(Box::new(move |db| {
                let count = db.query_row("SELECT COUNT(*) FROM events", [], |row| {
                    unsigned_column(row, 0)
                });
                let _ = barrier.send((count, timings.commit.samples.load(Ordering::Relaxed)));
            })),
        )
        .await?;
        let kinds = [
            EventKind::RunStarted {
                prompt: "intent".into(),
            },
            EventKind::TextDelta { text: "e".into() },
            EventKind::TextDelta { text: "f".into() },
            EventKind::ToolStarted {
                call: ToolCall {
                    id: "call".into(),
                    name: "read".into(),
                    arguments: serde_json::json!({}),
                },
            },
            EventKind::TextDelta { text: "g".into() },
            EventKind::RunFinished {
                summary: RunSummary {
                    run_id: "run".into(),
                    status: RunStatus::Completed,
                    steps: 1,
                    text: "acdefg".into(),
                    usage: Usage::default(),
                    error: None,
                },
            },
        ];
        for kind in kinds {
            replies.push(queue_append(&store, "selected", kind).await?);
        }
        release.send(())?;
        for reply in replies {
            reply.await??;
        }
        let (count, commits) = observed.await?;
        assert_eq!(count?, 4);
        assert_eq!(commits, 1);
        assert_eq!(store.journal_status().append_commits, 6);
        let events = store.events("selected", 0, 100).await?;
        assert_eq!(events.len(), 9);
        let mut previous = 0;
        let mut text = String::new();
        for event in &events {
            let live = tokio::time::timeout(Duration::from_secs(1), selected.recv()).await??;
            assert_eq!(live.previous_sequence, previous);
            assert_eq!(live.event.sequence, event.sequence);
            previous = event.sequence;
            if let EventKind::TextDelta { text: delta } = &event.kind {
                text.push_str(delta);
            }
        }
        assert_eq!(text, "acdefg");
        let view = store
            .session_view("selected", SessionTailOptions::default())
            .await?;
        assert_eq!(view.session.status, SessionStatus::Idle);
        assert_eq!(
            view.last_transition.context("missing finish")?.outcome,
            Some(RunStatus::Completed)
        );
        Ok(())
    }

    #[tokio::test]
    async fn failed_text_group_rolls_back_every_reply_and_publishes_nothing() -> Result<()> {
        let (_root, store) = view_store().await?;
        store.call(|db| {
            db.execute_batch("CREATE TEMP TRIGGER reject_text BEFORE INSERT ON events WHEN json_extract(NEW.payload,'$.text')='reject' BEGIN SELECT RAISE(ABORT,'fixture rejection'); END;")?;
            Ok(())
        }).await?;
        let mut selected = store.subscribe_session("selected").await?;
        let release = pause_writer(&store).await?;
        let mut replies = vec![];
        for text in ["before", "reject", "after"] {
            replies.push(
                queue_append(
                    &store,
                    "selected",
                    EventKind::TextDelta { text: text.into() },
                )
                .await?,
            );
        }
        release.send(())?;
        for reply in replies {
            assert!(reply.await?.is_err());
        }
        assert!(store.events("selected", 0, 100).await?.is_empty());
        assert_eq!(store.journal_status().append_commits, 0);
        assert!(selected_is_pending(&mut selected));
        store
            .call(|db| {
                db.execute_batch("DROP TRIGGER reject_text")?;
                Ok(())
            })
            .await?;
        let event = view_event(
            &store,
            "selected",
            EventKind::TextDelta {
                text: "recovered".into(),
            },
        )
        .await?;
        let live = tokio::time::timeout(Duration::from_secs(1), selected.recv()).await??;
        assert_eq!(live.previous_sequence, 0);
        assert_eq!(live.event.sequence, event.sequence);
        assert_eq!(store.journal_status().append_commits, 1);
        Ok(())
    }

    #[tokio::test]
    async fn text_groups_bound_count_and_payload_without_losing_accepted_events() -> Result<()> {
        let (_root, store) = view_store().await?;
        let release = pause_writer(&store).await?;
        let mut replies = vec![];
        for _ in 0..65 {
            replies.push(
                queue_append(
                    &store,
                    "selected",
                    EventKind::TextDelta {
                        text: "x".repeat(2048),
                    },
                )
                .await?,
            );
        }
        queue_job(&store, Job::Call(Box::new(|_| {}))).await?;
        for _ in 0..2 {
            replies.push(
                queue_append(
                    &store,
                    "other",
                    EventKind::TextDelta {
                        text: "y".repeat(80 * 1024),
                    },
                )
                .await?,
            );
        }
        release.send(())?;
        for reply in replies {
            reply.await??;
        }
        assert_eq!(store.journal_status().append_commits, 5);
        assert_eq!(store.events("selected", 0, 100).await?.len(), 65);
        assert_eq!(store.events("other", 0, 100).await?.len(), 2);
        Ok(())
    }

    #[tokio::test]
    async fn session_pages_keep_ties_new_inserts_and_full_set_prefixes_correct() -> Result<()> {
        let (_root, store) = view_store().await?;
        store.call(|db| {
            let tx = db.transaction()?;
            for index in 0..1200 {
                tx.execute("INSERT INTO sessions VALUES(?1,?2,'.',1,'idle')", params![format!("paged-{index:04}"), format!("Group {index}")])?;
            }
            tx.execute("INSERT INTO sessions VALUES('prefix-old-a','100% literal','.',0,'idle'),('prefix-old-b','100x other','.',0,'idle')", [])?;
            tx.commit()?;
            Ok(())
        }).await?;
        let page = store
            .sessions_page(SessionQuery {
                limit: 17,
                ..Default::default()
            })
            .await?;
        assert_eq!(page.sessions.len(), 17);
        let mut seen: HashSet<_> = page
            .sessions
            .into_iter()
            .map(|session| session.id)
            .collect();
        let mut cursor = page.next;
        store
            .create_session(Session {
                id: "paged-new".into(),
                title: "Group new".into(),
                cwd: ".".into(),
                created_at_ms: 1,
                status: SessionStatus::Idle,
            })
            .await?;
        while let Some(next) = cursor {
            let page = store
                .sessions_page(SessionQuery {
                    cursor: Some(next),
                    limit: 17,
                    ..Default::default()
                })
                .await?;
            for session in page.sessions {
                assert!(seen.insert(session.id));
            }
            cursor = page.next;
        }
        assert_eq!(seen.len(), 1204);
        assert!(!seen.contains("paged-new"));
        assert!(store.resolve_session("prefix-old-").await.is_err());
        assert_eq!(
            store.resolve_session("prefix-old-a").await?.title,
            "100% literal"
        );
        let literal = store
            .sessions_page(SessionQuery {
                query: "100%".into(),
                ..Default::default()
            })
            .await?;
        assert_eq!(literal.sessions.len(), 1);
        let first = store
            .sessions_page(SessionQuery {
                query: "gRoUp ".into(),
                limit: 5,
                ..Default::default()
            })
            .await?;
        assert_eq!(first.sessions.len(), 5);
        assert!(
            store
                .sessions_page(SessionQuery {
                    query: "another".into(),
                    cursor: first.next,
                    ..Default::default()
                })
                .await
                .is_err()
        );
        Ok(())
    }

    #[tokio::test]
    async fn bounded_authority_snapshot_and_filtered_continuity_share_durable_order() -> Result<()>
    {
        let (_root, store) = view_store().await?;
        let mut selected = store.subscribe_session("selected").await?;
        let started = view_event(
            &store,
            "selected",
            EventKind::RunStarted {
                prompt: "intent".into(),
            },
        )
        .await?;
        view_event(
            &store,
            "other",
            EventKind::Queued {
                text: "interleaved".into(),
            },
        )
        .await?;
        let text = view_event(
            &store,
            "selected",
            EventKind::TextDelta {
                text: "x".repeat(4096),
            },
        )
        .await?;
        let view = store
            .session_view(
                "selected",
                SessionTailOptions {
                    max_events: 1,
                    max_bytes: 64,
                },
            )
            .await?;
        assert_eq!(view.run_id.as_deref(), Some("run"));
        assert_eq!(
            view.last_transition
                .as_ref()
                .map(|transition| transition.sequence),
            Some(started.sequence)
        );
        assert_eq!(view.selected_cursor, text.sequence);
        assert_eq!(view.durable_cursor, text.sequence);
        assert!(view.omitted && view.events.is_empty());
        let one =
            tokio::time::timeout(std::time::Duration::from_secs(1), selected.recv()).await??;
        let two =
            tokio::time::timeout(std::time::Duration::from_secs(1), selected.recv()).await??;
        assert_eq!(one.previous_sequence, 0);
        assert_eq!(two.previous_sequence, started.sequence);
        assert!(two.event.sequence > one.event.sequence + 1);
        let page = store
            .events_page(
                "selected",
                0,
                view.durable_cursor,
                SessionTailOptions {
                    max_events: 10,
                    max_bytes: 64,
                },
            )
            .await?;
        assert_eq!(page.events.len(), 1);
        assert_eq!(page.cursor, started.sequence);
        assert!(page.omitted && !page.complete);
        let through_text = store
            .events_page(
                "selected",
                page.cursor,
                view.durable_cursor,
                SessionTailOptions::default(),
            )
            .await?;
        assert!(through_text.complete);
        assert_eq!(through_text.cursor, view.durable_cursor);
        assert_eq!(through_text.selected_cursor, Some(text.sequence));
        Ok(())
    }

    #[tokio::test]
    async fn committed_finish_survives_an_authority_only_view() -> Result<()> {
        let (_root, store) = view_store().await?;
        let mut selected = store.subscribe_session("selected").await?;
        view_event(
            &store,
            "selected",
            EventKind::RunStarted {
                prompt: "intent".into(),
            },
        )
        .await?;
        tokio::time::timeout(std::time::Duration::from_secs(1), selected.recv()).await??;
        let view = store
            .session_view("selected", SessionTailOptions::default())
            .await?;
        let finished = view_event(
            &store,
            "selected",
            EventKind::RunFinished {
                summary: RunSummary {
                    run_id: "run".into(),
                    status: RunStatus::Completed,
                    steps: 1,
                    text: String::new(),
                    usage: Usage::default(),
                    error: None,
                },
            },
        )
        .await?;
        let authority = store
            .session_view(
                "selected",
                SessionTailOptions {
                    max_events: 0,
                    max_bytes: 0,
                },
            )
            .await?;
        assert!(authority.run_id.is_none() && authority.events.is_empty() && authority.omitted);
        let transition = authority
            .last_transition
            .context("missing final transition")?;
        assert_eq!(transition.sequence, finished.sequence);
        assert_eq!(transition.run_id.as_deref(), Some("run"));
        assert_eq!(transition.outcome, Some(RunStatus::Completed));
        assert_eq!(transition.status, SessionStatus::Idle);
        let live =
            tokio::time::timeout(std::time::Duration::from_secs(1), selected.recv()).await??;
        assert_eq!(live.previous_sequence, view.selected_cursor);
        assert_eq!(live.event.sequence, finished.sequence);
        store.call(|db| {
            let plan: String = db.query_row("EXPLAIN QUERY PLAN SELECT sequence,run_id,json_extract(payload,'$.summary.status') FROM events WHERE session_id='selected' AND json_extract(payload,'$.type') IN ('run_started','run_finished') ORDER BY sequence DESC LIMIT 1", [], |row| row.get(3))?;
            assert!(plan.contains("events_by_transition"), "{plan}");
            Ok(())
        }).await?;
        Ok(())
    }

    #[tokio::test]
    async fn selected_channel_count_bytes_drop_and_lag_are_bounded() -> Result<()> {
        let (_root, store) = view_store().await?;
        let mut subscriptions = vec![];
        for _ in 0..crate::engine::views::MAX_SUBSCRIPTIONS {
            subscriptions.push(store.subscribe_session("selected").await?);
        }
        assert!(store.subscribe_session("selected").await.is_err());
        subscriptions.pop();
        let mut selected = store.subscribe_session("selected").await?;
        subscriptions.clear();
        for index in 0..70 {
            view_event(
                &store,
                "selected",
                EventKind::TextDelta {
                    text: format!("{index}"),
                },
            )
            .await?;
        }
        assert!(matches!(
            tokio::time::timeout(std::time::Duration::from_secs(1), selected.recv()).await?,
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_))
        ));
        let snapshot = store
            .session_view("selected", SessionTailOptions::default())
            .await?;
        view_event(
            &store,
            "selected",
            EventKind::TextDelta {
                text: "x".repeat(crate::engine::views::SELECTED_BYTES + 1),
            },
        )
        .await?;
        assert!(matches!(
            tokio::time::timeout(std::time::Duration::from_secs(1), selected.recv()).await?,
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_))
        ));
        let bounded = store
            .session_view(
                "selected",
                SessionTailOptions {
                    max_events: 1000,
                    max_bytes: 1024,
                },
            )
            .await?;
        assert!(bounded.omitted && bounded.events.is_empty());
        assert!(bounded.selected_cursor > snapshot.selected_cursor);
        let mut closed = store.subscribe_session("other").await?;
        drop(store);
        assert!(matches!(
            tokio::time::timeout(std::time::Duration::from_secs(1), closed.recv()).await?,
            Err(tokio::sync::broadcast::error::RecvError::Closed)
        ));
        Ok(())
    }

    #[test]
    fn journal_migration_respects_the_existing_size_budget() -> Result<()> {
        let root = tempfile::tempdir()?;
        let files = StoreFiles::open(root.path())?;
        let mut db = Connection::open(&files.database)?;
        db.execute_batch("CREATE TABLE sessions(id TEXT PRIMARY KEY,title TEXT NOT NULL,cwd TEXT NOT NULL,created INTEGER NOT NULL,status TEXT NOT NULL)")?;
        let tx = db.transaction()?;
        for index in 0..1500 {
            tx.execute(
                "INSERT INTO sessions VALUES(?1,?2,'.',1,'idle')",
                params![format!("old-{index:04}"), "x".repeat(400)],
            )?;
        }
        tx.commit()?;
        let before: i64 = db.query_row("PRAGMA page_count", [], |row| row.get(0))?;
        assert!(before < 256);
        drop(db);
        drop(files);
        assert!(Store::open(root.path(), 1024 * 1024).is_err());
        let db = Connection::open(root.path().join("sessions.sqlite3"))?;
        let after: i64 = db.query_row("PRAGMA page_count", [], |row| row.get(0))?;
        assert!(after <= 256, "migration exceeded 1 MiB: {after} pages");
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM sessions", [], |row| row
                .get::<_, i64>(0))?,
            1500
        );
        Ok(())
    }

    #[tokio::test]
    async fn old_journal_builds_transition_index_and_recovers_authority_without_tail_replay()
    -> Result<()> {
        let root = tempfile::tempdir()?;
        let files = StoreFiles::open(root.path())?;
        let mut db = Connection::open(&files.database)?;
        db.execute_batch("CREATE TABLE sessions(id TEXT PRIMARY KEY,title TEXT NOT NULL,cwd TEXT NOT NULL,created INTEGER NOT NULL,status TEXT NOT NULL);
            CREATE TABLE messages(sequence INTEGER PRIMARY KEY,session_id TEXT NOT NULL REFERENCES sessions(id),payload TEXT NOT NULL,bytes INTEGER NOT NULL);
            CREATE TABLE tool_views(sequence INTEGER PRIMARY KEY REFERENCES messages(sequence),original_output TEXT NOT NULL,payload TEXT,bytes INTEGER);
            CREATE TABLE events(sequence INTEGER PRIMARY KEY,session_id TEXT NOT NULL REFERENCES sessions(id),run_id TEXT,created INTEGER NOT NULL,payload TEXT NOT NULL);
            CREATE TABLE settings(key TEXT PRIMARY KEY,value TEXT NOT NULL);
            INSERT INTO sessions VALUES('old','Old running session','.',1,'running');")?;
        let tx = db.transaction()?;
        tx.execute(
            "INSERT INTO events(session_id,run_id,created,payload) VALUES('old','old-run',1,?1)",
            [serde_json::to_string(&EventKind::RunStarted {
                prompt: "old intent".into(),
            })?],
        )?;
        let payload = serde_json::to_string(&EventKind::TextDelta {
            text: "older chunk".into(),
        })?;
        for _ in 0..5000 {
            tx.execute("INSERT INTO events(session_id,run_id,created,payload) VALUES('old','old-run',1,?1)", [&payload])?;
        }
        tx.commit()?;
        drop(db);
        drop(files);
        let store = Store::open(root.path(), 8 * 1024 * 1024)?;
        let view = store
            .session_view(
                "old",
                SessionTailOptions {
                    max_events: 0,
                    max_bytes: 0,
                },
            )
            .await?;
        assert!(view.events.is_empty() && view.omitted && view.run_id.is_none());
        assert_eq!(view.session.status, SessionStatus::Interrupted);
        assert_eq!(view.durable_cursor, 5002);
        assert_eq!(view.selected_cursor, view.durable_cursor);
        let transition = view
            .last_transition
            .context("missing recovered transition")?;
        assert_eq!(transition.run_id.as_deref(), Some("old-run"));
        assert_eq!(transition.outcome, Some(RunStatus::Interrupted));
        assert_eq!(transition.sequence, view.durable_cursor);
        Ok(())
    }

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

    #[cfg(target_os = "macos")]
    #[test]
    fn journal_rejects_inherited_and_existing_allow_acls() -> Result<()> {
        let root = tempfile::tempdir()?;
        let parent = root.path().join("parent");
        std::fs::create_dir(&parent)?;
        let state = parent.join("state");
        assert!(
            std::process::Command::new("/bin/chmod")
                .args([
                    "+a",
                    "everyone allow read,write,file_inherit,directory_inherit"
                ])
                .arg(&parent)
                .status()?
                .success()
        );
        assert!(super::StoreFiles::open(&state).is_err());
        assert!(!state.exists());
        assert!(
            std::process::Command::new("/bin/chmod")
                .arg("-N")
                .arg(&parent)
                .status()?
                .success()
        );
        drop(super::StoreFiles::open(&state)?);
        assert!(
            std::process::Command::new("/bin/chmod")
                .args(["+a", "everyone allow read"])
                .arg(state.join("sessions.sqlite3"))
                .status()?
                .success()
        );
        assert!(super::StoreFiles::open(&state).is_err());
        Ok(())
    }

    #[tokio::test]
    async fn journals_replay_and_locks_ownership() -> Result<()> {
        let root = tempfile::tempdir()?;
        let state = root.path().join("state with space 雪");
        let store = Store::open(&state, 4 * 1024 * 1024)?;
        assert!(Store::open(&state, 4 * 1024 * 1024).is_err());
        #[cfg(windows)]
        {
            assert!(std::fs::rename(&state, root.path().join("replaced")).is_err());
            assert!(std::fs::remove_file(state.join("owner.lock")).is_err());
            assert!(std::fs::remove_file(state.join("sessions.sqlite3")).is_err());
            for path in [
                &state,
                &state.join("owner.lock"),
                &state.join("sessions.sqlite3"),
            ] {
                rightkit_fs::private::verify_owner_only(path)?;
            }
        }
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
        drop(store);
        let lock = OpenOptions::new()
            .read(true)
            .open(state.join("owner.lock"))?;
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                match lock.try_lock() {
                    Ok(()) => return Ok::<(), anyhow::Error>(()),
                    Err(std::fs::TryLockError::WouldBlock) => tokio::task::yield_now().await,
                    Err(std::fs::TryLockError::Error(error)) => return Err(error.into()),
                }
            }
        })
        .await??;
        drop(lock);
        let reopened = Store::open(&state, 4 * 1024 * 1024)?;
        assert_eq!(reopened.session("s").await?.title, "test");
        assert_eq!(reopened.events("s", 0, 100).await?.len(), 1);
        Ok(())
    }
}
