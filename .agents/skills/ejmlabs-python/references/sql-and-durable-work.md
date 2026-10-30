# SQL and durable work

Forms are shown for Postgres with psycopg (`%s` placeholders). Use your driver's placeholder; the
rules hold for any SQL database.

## Parameters

```python
await cursor.execute(
    "SELECT id FROM items WHERE owner = %s AND id = ANY(%s)",
    (owner, ids),
)
```

- Every value is a bound parameter. Never format a request value, an id, or a name into SQL.
- An f-string may join fixed SQL fragments the code owns. Build dynamic filters as a pair of SQL
  text and a parameter list, and join both together.

## Connections and transactions

- One pool per process, owned by its lifecycle context. A store takes a connection from it; it
  never opens its own pool or connection.
- One transaction per unit of work. Two transactions for one change leave half of it on a crash.
  A helper takes the connection as an argument and never commits.
- Do not catch an error inside a transaction and continue; the transaction is aborted.
- Keep each transaction short, with no network, model, or tool call inside it. Release the
  connection before slow awaited work.
- A per-request setting uses `SET LOCAL` or `set_config(name, %s, true)`. A plain `SET` stays on
  the pooled connection and the next request inherits it.
- Set client-side connect timeouts and TCP keepalives on every connection. A server-side
  `statement_timeout` cannot see a dead peer.
- Count every connection holder (request handlers, workers, background loops) when you size a pool.

## Bounded reads

- A read on a table that grows has `LIMIT` and a keyset cursor. `OFFSET` rescans every skipped row
  and shifts when rows change.

```python
rows = await fetch("... AND name > %s ORDER BY name LIMIT %s", (after, limit + 1))
next_after = rows[limit - 1]["name"] if len(rows) > limit else None
```

- Order by a unique key, ask for `limit + 1` rows, and return a cursor only when the extra row
  exists. Validate the limit and the cursor at the top. A cursor from another filter is rejected.
- Load large values one row at a time: select the metadata first, then the bytes by id.

## Claims and concurrency

- Claim a row in the statement that selects it. A `SELECT` followed by an `UPDATE` lets two
  workers both win.

```sql
UPDATE jobs SET state = 'running', attempts = attempts + 1, lease_until = now() + %s
WHERE id IN (
  SELECT id FROM jobs WHERE state = 'pending' ORDER BY id LIMIT %s FOR UPDATE SKIP LOCKED
)
RETURNING id, attempts
```

- Fence every later write with the claim (`AND attempts = %s AND state = 'running'`) and check
  `rowcount`. A worker that lost its claim must not write, send, or log success.
- Use `SKIP LOCKED` for claims only; it gives an inconsistent view for anything else.
- Optimistic concurrency: put the version in the `WHERE` clause and check `rowcount == 1`. A stale
  version goes back to the caller; never retry silently with the new version.
- A rule with no row to lock (a count, a unique name) takes a transaction-level advisory lock
  first: `pg_advisory_xact_lock(hashtextextended(%s, 0))`.
- Retry a serialization failure with a cap. A unique key, lock, lease, or atomic reservation needs
  a test against a real database, never a fake.

## Schema changes

- A schema change is a new migration file. Never edit a migration after it merged; add another.
- Expand first: new tables, nullable or defaulted columns, indexes. An older release must still run
  on the new schema, because rollbacks do. Drop or rename only once no serving code uses the old
  shape.
- The application role does not run DDL and has only the grants it needs.
- Test migrations on a disposable fresh database with synthetic data only.

## Durable work and side effects

- Commit accepted work before you acknowledge it. A queue that is full says so to the caller; never
  claim the request was saved.
- Dedupe a redelivered event with the database: `INSERT ... ON CONFLICT (event_id) DO NOTHING`,
  not an in-memory set.
- Renew a lease while long work runs, and stop the work when renewal fails. Never catch a lost
  lease to retry.
- Record intent before each external effect (`sending`), and confirmation after it (`sent`), in
  separate fenced writes. A crash between them leaves `sending`.
- Never replay an uncertain effect. A send, payment, or tool call found in `sending` after a crash
  goes to a human or a reconciliation step, not an automatic retry.
- Reserve spend before a paid request and settle it after, with the real cost. If the cost is
  unknown, keep the reservation.

## Retries and overload

- Retry only transient failures that are safe to repeat: timeouts, connection resets, rate
  limits, serialization failures. Never retry a malformed request or a permanent error.
- Retry at one layer only. Retries at several layers multiply load.
- Back off exponentially with a cap and jitter, honor `Retry-After`, and cap attempts per request.
- Reject excess work early and cheaply, with a status that tells the caller when to come back.
- NOTIFY or a message is a wake-up; the row is the truth. Payloads carry ids only, never text.

## Review checklist

- Is every value a bound parameter?
- Is each unit of work one short transaction with no network call inside?
- Does every read on a growing table have `LIMIT` and a keyset cursor?
- Does each claim select and update in one statement, and each later write check `rowcount`?
- Does an effect record intent first, and does an uncertain effect stop instead of repeating?
- Is each retry at one layer, capped, with backoff and jitter, for a transient error only?
- Does a database test cover the failure path (a lost lease, a stale version, a second claimer)?
