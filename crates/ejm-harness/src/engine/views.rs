//! Bounded host views over durable session state.

use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
};

use anyhow::{Context, Result, ensure};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

#[cfg(feature = "test-support")]
use std::{sync::Weak, time::Instant};

use crate::{Event, RunStatus, Session, SessionStatus};

use super::Harness;

pub(crate) const MAX_SUBSCRIPTIONS: usize = 128;
pub(crate) const SELECTED_EVENTS: usize = 64;
pub(crate) const SELECTED_BYTES: usize = 256 * 1024;

/// Stable continuation for a session query. Keep it with the same query.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct SessionCursor {
    pub(crate) created_at_ms: u64,
    pub(crate) id: String,
    pub(crate) upper_rowid: i64,
    pub(crate) query: String,
}

/// Case-insensitive title or identity prefix search, with bounded pages.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct SessionQuery {
    pub query: String,
    pub cursor: Option<SessionCursor>,
    pub limit: usize,
}

impl Default for SessionQuery {
    fn default() -> Self {
        Self {
            query: String::new(),
            cursor: None,
            limit: 50,
        }
    }
}

/// A page in descending creation-time and identity order.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct SessionPage {
    pub sessions: Vec<Session>,
    pub next: Option<SessionCursor>,
}

/// Hard payload bounds for a presentation tail or replay page.
///
/// Zero in either field requests an authority-only tail. Replay pages require
/// positive bounds.
/// At most 1000 events and 2 MiB of serialized event payload are returned.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct SessionTailOptions {
    pub max_events: usize,
    pub max_bytes: usize,
}

impl Default for SessionTailOptions {
    fn default() -> Self {
        Self {
            max_events: 200,
            max_bytes: 256 * 1024,
        }
    }
}

impl SessionTailOptions {
    pub(crate) fn bounded(self) -> Self {
        Self {
            max_events: self.max_events.min(1000),
            max_bytes: self.max_bytes.min(2 * 1024 * 1024),
        }
    }
}

/// The latest committed run transition, independent of the visible tail.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct SessionRunTransition {
    pub sequence: u64,
    pub run_id: Option<String>,
    pub status: SessionStatus,
    pub outcome: Option<RunStatus>,
}

/// One consistent journal boundary for authority and bounded presentation.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct SessionView {
    pub session: Session,
    /// The active run only. Finished identity is in `last_transition`.
    pub run_id: Option<String>,
    pub last_transition: Option<SessionRunTransition>,
    /// Global durable high-water sequence at this snapshot.
    pub durable_cursor: u64,
    /// Last selected-session sequence, matching live predecessor tokens.
    pub selected_cursor: u64,
    pub events: Vec<Event>,
    pub omitted: bool,
}

/// Bounded replay through a captured global high-water boundary.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct EventPage {
    pub events: Vec<Event>,
    /// Global boundary represented by this page, or `through` when complete.
    pub cursor: u64,
    /// Last represented selected-session sequence, if this page returned one.
    pub selected_cursor: Option<u64>,
    pub through: u64,
    pub complete: bool,
    /// The next single event exceeds the byte budget. Take a fresh snapshot.
    pub omitted: bool,
}

/// Bounded aggregate journal timings. No content or per-event traces are kept.
///
/// Histograms use powers of two nanoseconds: bucket zero includes 0 and 1,
/// bucket one includes 2 and 3, and the last bucket includes all larger values.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct JournalSnapshot {
    /// Writer dispatches. One text group is one dispatch.
    pub calls: u64,
    /// Successful append transactions. A text group may contain several events.
    pub append_commits: u64,
    pub queue_wait_ns: u64,
    pub work_ns: u64,
    pub commit_ns: u64,
    pub max_queue_wait_ns: u64,
    pub max_work_ns: u64,
    pub max_commit_ns: u64,
    /// One sample per accepted request, including each event in a text group.
    pub queue_wait_histogram: [u64; 32],
    /// One sample per writer dispatch, matching `calls`.
    pub work_histogram: [u64; 32],
    /// One sample per successful append transaction, matching `append_commits`.
    pub commit_histogram: [u64; 32],
}

/// A durable selected event and its preceding selected-session sequence.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct SessionEvent {
    pub event: Event,
    pub previous_sequence: u64,
}

pub(crate) struct SelectedChannel {
    sender: broadcast::Sender<SelectedMessage>,
    sizes: VecDeque<usize>,
    bytes: usize,
    #[cfg(feature = "test-support")]
    trace_interest: Weak<()>,
}

pub(crate) struct SelectedInterest {
    pub active: bool,
    #[cfg(feature = "test-support")]
    pub traced: bool,
}

#[derive(Clone)]
struct SelectedPublication {
    event: SessionEvent,
    #[cfg(feature = "test-support")]
    committed_at: Option<Instant>,
}

#[derive(Clone)]
enum SelectedMessage {
    Event(Arc<SelectedPublication>),
    Reset,
}

/// A bounded selected-session subscription. Drop it to release registration.
pub struct SessionSubscription {
    receiver: broadcast::Receiver<SelectedMessage>,
    pending: Option<Arc<SelectedPublication>>,
    #[cfg(feature = "test-support")]
    _trace_interest: Option<Arc<()>>,
}

impl SessionSubscription {
    /// Receive a committed event, or an explicit lag/closed signal.
    ///
    /// Subscribe before reading `session_view`. Discard live events at or below
    /// its `durable_cursor`. The next event's `previous_sequence` must equal its
    /// `selected_cursor`. Global numeric gaps are normal. After lag, take a new
    /// bounded snapshot or replay bounded pages through a captured high water.
    ///
    /// # Errors
    /// Returns `Lagged` when this receiver fell behind its 64-event channel,
    /// or `Closed` when the journal and all publishers stopped.
    pub async fn recv(&mut self) -> std::result::Result<SessionEvent, broadcast::error::RecvError> {
        self.recv_publication()
            .await
            .map(|publication| Arc::unwrap_or_clone(publication).event)
    }

    /// Receive an event and its optional monotonic journal commit timestamp.
    ///
    /// Available only with `test-support`. A batch receives one timestamp just
    /// after commit if its precommit interest snapshot includes a live
    /// subscription created by [`Harness::subscribe_session_traced`]. Otherwise
    /// it is `None`. A later subscription drop does not change that snapshot.
    /// The original stamp survives bounded reset handling; receiving does not
    /// take a new timestamp. This adds no durable or wire event field.
    ///
    /// # Errors
    /// Returns the same typed lag and closed signals as [`Self::recv`].
    #[cfg(feature = "test-support")]
    pub async fn recv_traced(
        &mut self,
    ) -> std::result::Result<(SessionEvent, Option<Instant>), broadcast::error::RecvError> {
        self.recv_publication().await.map(|publication| {
            let publication = Arc::unwrap_or_clone(publication);
            (publication.event, publication.committed_at)
        })
    }

    async fn recv_publication(
        &mut self,
    ) -> std::result::Result<Arc<SelectedPublication>, broadcast::error::RecvError> {
        if let Some(event) = self.pending.take() {
            return Ok(event);
        }
        match self.receiver.recv().await? {
            SelectedMessage::Event(event) => Ok(event),
            SelectedMessage::Reset => {
                for _ in 0..SELECTED_EVENTS {
                    match self.receiver.try_recv() {
                        Ok(SelectedMessage::Reset)
                        | Err(broadcast::error::TryRecvError::Lagged(_)) => {}
                        Ok(SelectedMessage::Event(event)) => {
                            self.pending = Some(event);
                            break;
                        }
                        Err(
                            broadcast::error::TryRecvError::Empty
                            | broadcast::error::TryRecvError::Closed,
                        ) => break,
                    }
                }
                Err(broadcast::error::RecvError::Lagged(1))
            }
        }
    }
}

#[derive(Default)]
pub(crate) struct SelectedSubscriptions {
    channels: Mutex<HashMap<String, SelectedChannel>>,
}

impl SelectedSubscriptions {
    pub fn interest(&self, session_id: &str) -> SelectedInterest {
        let channel = self.channels.lock().ok();
        let channel = channel
            .as_ref()
            .and_then(|channels| channels.get(session_id));
        let active = channel.is_some_and(|channel| channel.sender.receiver_count() > 0);
        SelectedInterest {
            active,
            #[cfg(feature = "test-support")]
            traced: active
                && channel.is_some_and(|channel| channel.trace_interest.strong_count() > 0),
        }
    }

    pub fn subscribe(&self, session_id: &str) -> Result<SessionSubscription> {
        self.subscribe_inner(
            session_id,
            #[cfg(feature = "test-support")]
            false,
        )
    }

    #[cfg(feature = "test-support")]
    pub fn subscribe_traced(&self, session_id: &str) -> Result<SessionSubscription> {
        self.subscribe_inner(session_id, true)
    }

    fn subscribe_inner(
        &self,
        session_id: &str,
        #[cfg(feature = "test-support")] traced: bool,
    ) -> Result<SessionSubscription> {
        let mut channels = self
            .channels
            .lock()
            .map_err(|_| anyhow::anyhow!("selected event registry stopped"))?;
        channels.retain(|_, channel| channel.sender.receiver_count() > 0);
        let registrations: usize = channels
            .values()
            .map(|channel| channel.sender.receiver_count())
            .sum();
        ensure!(
            registrations < MAX_SUBSCRIPTIONS,
            "at most 128 selected-session subscriptions may be open"
        );
        let channel = channels.entry(session_id.to_owned()).or_insert_with(|| {
            let (sender, _) = broadcast::channel(SELECTED_EVENTS);
            SelectedChannel {
                sender,
                sizes: VecDeque::new(),
                bytes: 0,
                #[cfg(feature = "test-support")]
                trace_interest: Weak::new(),
            }
        });
        #[cfg(feature = "test-support")]
        let trace_interest = traced.then(|| {
            channel.trace_interest.upgrade().unwrap_or_else(|| {
                let interest = Arc::new(());
                channel.trace_interest = Arc::downgrade(&interest);
                interest
            })
        });
        Ok(SessionSubscription {
            receiver: channel.sender.subscribe(),
            pending: None,
            #[cfg(feature = "test-support")]
            _trace_interest: trace_interest,
        })
    }

    pub fn publish(
        &self,
        event: &Event,
        previous_sequence: u64,
        bytes: usize,
        #[cfg(feature = "test-support")] committed_at: Option<Instant>,
    ) {
        let Ok(mut channels) = self.channels.lock() else {
            return;
        };
        if let Some(channel) = channels.get_mut(&event.session_id) {
            if channel.sender.receiver_count() == 0 {
                channels.remove(&event.session_id);
                return;
            }
            // Reset markers precede the event suffix, so their count only
            // makes retained payload accounting conservative.
            let retained = channel.sender.len();
            while channel.sizes.len() > retained {
                channel.bytes -= channel.sizes.pop_front().unwrap_or(0);
            }
            if channel.sizes.len() == SELECTED_EVENTS {
                channel.bytes -= channel.sizes.pop_front().unwrap_or(0);
            }
            if channel.bytes.saturating_add(bytes) > SELECTED_BYTES {
                // Overwrite the retained ring with tiny explicit loss signals.
                // A slow reader cannot make large payloads accumulate.
                for _ in 0..SELECTED_EVENTS {
                    let _ = channel.sender.send(SelectedMessage::Reset);
                }
                channel.sizes.clear();
                channel.bytes = 0;
            }
            if bytes <= SELECTED_BYTES {
                let _ =
                    channel
                        .sender
                        .send(SelectedMessage::Event(Arc::new(SelectedPublication {
                            event: SessionEvent {
                                event: event.clone(),
                                previous_sequence,
                            },
                            #[cfg(feature = "test-support")]
                            committed_at,
                        })));
                channel.sizes.push_back(bytes);
                channel.bytes += bytes;
            }
        }
    }
}

impl Harness {
    /// Read aggregate queue, writer-work, and committed-append sync timings.
    #[must_use]
    pub fn journal_status(&self) -> JournalSnapshot {
        self.inner.store.journal_status()
    }

    /// Search durable session metadata without materializing the full list.
    ///
    /// The query is a literal ASCII-case-insensitive title or identity prefix,
    /// at most 256 UTF-8 bytes. Pages contain at most 100 sessions. Continuation
    /// excludes sessions inserted after the first page; status may change.
    ///
    /// # Errors
    /// Rejects an oversized query, a cursor from another query, or store errors.
    pub async fn sessions_page(&self, query: SessionQuery) -> Result<SessionPage> {
        self.inner.store.sessions_page(query).await
    }

    /// Resolve an exact identity or unambiguous identity prefix in the full set.
    ///
    /// # Errors
    /// Rejects missing or ambiguous identities and unavailable journal state.
    pub async fn resolve_session(&self, prefix: &str) -> Result<Session> {
        self.inner.store.resolve_session(prefix).await
    }

    /// Read current authority and a bounded ordered durable tail together.
    ///
    /// The tail is a contiguous suffix. An event larger than its byte budget is
    /// omitted rather than copied beyond the limit. Authority remains available.
    /// Zero count or byte bounds read authority without fetching event payloads.
    ///
    /// # Errors
    /// Rejects unknown sessions or unavailable or invalid durable state.
    pub async fn session_view(
        &self,
        session_id: &str,
        options: SessionTailOptions,
    ) -> Result<SessionView> {
        self.inner.store.session_view(session_id, options).await
    }

    /// Subscribe to one session before obtaining its authority snapshot.
    ///
    /// # Errors
    /// Rejects an unknown session, unavailable journal, or registration overflow.
    pub async fn subscribe_session(&self, session_id: &str) -> Result<SessionSubscription> {
        self.inner.store.subscribe_session(session_id).await
    }

    /// Subscribe with opt-in monotonic journal commit stamps for measurements.
    ///
    /// Available only with `test-support`. Registration shares the journal's
    /// serialized subscription boundary. Batches sample traced interest before
    /// commit and take one timestamp immediately after a successful commit.
    /// Read it with [`SessionSubscription::recv_traced`]. Dropping the last
    /// traced subscription removes future interest; a batch that already
    /// sampled it may still take its timestamp.
    /// Default builds exclude this API and all trace fields and checks.
    ///
    /// # Errors
    /// Rejects the same states as [`Self::subscribe_session`].
    #[cfg(feature = "test-support")]
    pub async fn subscribe_session_traced(&self, session_id: &str) -> Result<SessionSubscription> {
        self.inner.store.subscribe_session_traced(session_id).await
    }

    /// Read a byte- and count-bounded durable page through a captured cursor.
    ///
    /// # Errors
    /// Rejects an unknown session, reversed range, zero page bounds, or an
    /// unavailable journal.
    pub async fn events_page(
        &self,
        session_id: &str,
        after: u64,
        through: u64,
        options: SessionTailOptions,
    ) -> Result<EventPage> {
        ensure!(after <= through, "event range is reversed");
        self.inner
            .store
            .events_page(session_id, after, through, options)
            .await
            .context("read bounded event page")
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{SELECTED_BYTES, SELECTED_EVENTS, SelectedSubscriptions};
    use crate::{Event, EventKind};

    #[tokio::test]
    async fn selected_channel_drained_publications_do_not_trigger_lag() -> anyhow::Result<()> {
        let selected = SelectedSubscriptions::default();
        let mut subscription = selected.subscribe("selected")?;
        let mut event = Event {
            sequence: 0,
            session_id: "selected".into(),
            run_id: None,
            timestamp_ms: 1,
            kind: EventKind::TextDelta {
                text: "x".repeat(8 * 1024),
            },
        };
        let bytes = serde_json::to_string(&event.kind)?.len();
        let publications = SELECTED_EVENTS * 2;
        assert!(bytes * publications > SELECTED_BYTES);
        for sequence in 1..=u64::try_from(publications)? {
            event.sequence = sequence;
            selected.publish(
                &event,
                sequence - 1,
                bytes,
                #[cfg(feature = "test-support")]
                None,
            );
            let live = tokio::time::timeout(Duration::from_secs(1), subscription.recv()).await??;
            assert_eq!(live.event.sequence, sequence);
            assert_eq!(live.previous_sequence, sequence - 1);
        }
        Ok(())
    }
}
