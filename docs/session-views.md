# Bounded session views

The Rust API provides bounded session discovery, current run authority, transcript
tails, and selected-session live events. These APIs are implemented in the source
tree. Publication and platform performance acceptance are separate release checks.
Full durable history remains available through `Harness::events` and the stricter
`Harness::events_page`. Hosts own presentation, not a second journal or run state.

## Find sessions

`Harness::sessions_page(SessionQuery)` returns at most 100 sessions. The default
page has 50. `query` is a literal prefix of the title or identity, with SQLite's
ASCII case-insensitive matching. `%`, `_`, and backslash are literal characters.
Queries are limited to 256 UTF-8 bytes. This is prefix search, not substring or
linguistic search.

Results use descending creation time, then descending identity for ties. Pass
`SessionPage::next` back unchanged with the same query. The cursor captures the
first page's session insertion boundary, so later inserts do not shift pages or
appear midway through the result set. Status can change between pages. A changed
query requires a fresh cursor.

`Harness::resolve_session(prefix)` checks the full durable set. An exact identity
wins. A prefix with several matches is rejected even when an older match falls
outside `list_sessions`' legacy 1000-session limit. The implementation uses the
session indexes instead of reconstructing journal history.

```rust,no_run
use ejm_harness::{Harness, SessionQuery};

async fn first_page(harness: &Harness) -> anyhow::Result<()> {
    let page = harness.sessions_page(SessionQuery {
        query: "build".into(),
        limit: 50,
        ..Default::default()
    }).await?;
    for session in page.sessions {
        println!("{} {}", session.id, session.title);
    }
    Ok(())
}
```

## Read current authority without full replay

`Harness::session_view(id, SessionTailOptions)` reads these fields in one SQLite
transaction:

| Field             | Meaning                                                                                                |
| ----------------- | ------------------------------------------------------------------------------------------------------ |
| `session`         | Durable identity, title, directory, creation time, and status                                          |
| `run_id`          | Current active run identity, or `None` after its finish                                                |
| `last_transition` | Latest committed start or finish, including run identity, sequence, status, and optional final outcome |
| `durable_cursor`  | Global committed event high water                                                                      |
| `selected_cursor` | Last committed event for this session                                                                  |
| `events`          | Ordered contiguous transcript suffix within both bounds                                                |
| `omitted`         | Older history or a too-large tail event was excluded                                                   |

The default tail is at most 200 events and 256 KiB of serialized event payload.
Absolute caps are 1000 events and 2 MiB. Payload bounds exclude the small event
envelope and Rust allocation overhead. A zero event or byte bound requests
authority only and fetches no transcript payload. Authority remains available
when the most recent event is too large for the requested tail. A host should
show the omission marker and offer separately paged history.

Run transitions update a compact authority row in the same transaction as their
source event. Old journals receive a partial transition index when opened. Index
construction is a one-time SQLite migration and can take time for a large old
journal. The configured `max_store_bytes` limit applies before index creation.
If the migration cannot fit, opening fails without dropping session history;
increase that explicit store budget before trying again. A legacy session's
first read fills its missing authority row from one
indexed transition; later reads use only the compact row. They do not scan or
deserialize the event history. An interrupted prior run still passes through
normal recovery before views open.

## Subscribe before the snapshot

1. Call `Harness::subscribe_session(id)`.
2. Read `session_view` and render its authority and bounded tail.
3. Discard buffered live events with `event.sequence <= durable_cursor`.
4. For each later event, require `previous_sequence == selected_cursor`, then
   update `selected_cursor` to `event.sequence`.
5. On lag or a predecessor mismatch, obtain a fresh consistent snapshot, or
   replay bounded pages through a captured high water before applying new events.

Event sequence numbers belong to the whole journal. If another session emits
sequence 101 between this session's events 100 and 102, the selected wrapper for
102 has predecessor 100. That is valid continuity. Do not check `sequence + 1`.

Selected channels retain at most 64 event payloads and 256 KiB per session.
They share retained payloads across receivers. A count overflow, byte overflow,
or single oversized event produces an explicit `RecvError::Lagged` signal.
The lag count is a recovery hint, not a durable event count. Reset hints coalesce
when received. At most 128 selected subscriptions may be open in one store.
Dropping receivers releases registration; dropping the store closes channels.
The existing global `subscribe` API is unchanged.

Committed selected events publish from the single journal writer after commit.
Concurrent run drivers cannot publish selected events out of durable order.
The channel is a hint stream. The journal remains authoritative.

## Replay bounded pages

`Harness::events_page(id, after, through, options)` reads forward after `after`
through a captured global boundary. `through` is clamped to current committed
high water. The same count and byte caps apply as for tails. Page bounds must
be positive so an accepted page makes progress or reports a single-event omission.

- `complete` means all selected events through that boundary were represented.
- `cursor` is the last returned event, or `through` when complete. This lets a
  caller finish when other sessions occupy the final global sequences.
- `selected_cursor` is the last selected sequence returned, or `None` when empty.
- `omitted` means the next single event cannot fit the requested byte budget.
  The page does not advance past it. Increase the budget within the hard cap or
  use a fresh snapshot and show its omission marker.

Keep replay asynchronous and bound work per input-loop iteration. Reaching a
page limit does not justify replaying the full history inside one input handler.

## Journal durability and measurements

`HarnessConfig::max_store_bytes` defaults to 512 MiB and accepts 1 MiB through
8 GiB. SQLite's page count enforces the main database budget; this is not a
combined cap on WAL, shared-memory files or tool artifacts. Persistent load
fixtures must report the chosen budget and actual file sizes.

If completion cannot be committed, the run handle reports failure and the
harness stops new effects. The last durable authority can still say running
because the failed terminal event could not be stored. Hosts must observe
`RunHandle::wait`, surface that failure and preserve the journal. Resolve the
storage issue or increase the explicit budget before reopening; normal recovery
then handles incomplete work without repeating uncertain effects.

The writer groups adjacent queued text deltas in one transaction. A group holds
at most 32 events and 128 KiB of text payload. A single larger event remains one
transaction. Collection scans for at most four milliseconds and never waits for
another event to arrive. Reads, messages, tool events, and run transitions end a
group and retain their queue order. Grouping adds no deliberate idle delay.

Acknowledgements and selected live publication follow a successful commit.
A failed group rolls back every event in that group and fails every waiting
reply. An accepted append remains durable if its caller drops its reply before
completion. The count and byte limits bound each transaction's extra accumulation;
they do not replace the existing 256-request journal queue bound.

`Harness::journal_status()` reads fixed-size aggregate counters and 32-bucket
histograms for queued journal wait, writer work, and successful append commit
time. Histograms use powers of two nanoseconds; the last bucket is open ended.
Counters include no prompt, command, provider content, or raw event trace.
They are approximate concurrent snapshots and reset with the store instance.

Queue time includes channel backpressure. Writer work includes SQLite work and
post-commit selected publication. Commit time covers `Transaction::commit` for
event appends. Message and other implicit SQLite writes remain included in writer
work. These measurements distinguish serialized storage wait from model time;
they do not measure disk I/O bytes or prove a batching gain.

`calls` and the writer-work histogram count writer dispatches, including one per
text group. Queue-wait samples count each accepted request, including every
grouped event. `append_commits` and the commit histogram count successful append
transactions. These three sample counts can differ. Use counter differences
between captured phase boundaries and compare the same workload and build.
