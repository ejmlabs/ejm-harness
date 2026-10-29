# Asyncio forms

The bar: every task has an owner, a name, and a logged failure. Every cancel reaches its owner.
Every outbound wait has a bound. Nothing blocking or heavy runs on the event loop. Ruff `ASYNC`
and `RUF006` (part of the strict layer) catch some misses; review catches the rest.

## Task ownership

A background loop lives in a lifecycle context. Start it on enter; cancel and await it on exit.

```python
@asynccontextmanager
async def runtime() -> AsyncIterator[None]:
    task = asyncio.create_task(remove_expired(), name="files-expiry")
    try:
        yield
    finally:
        task.cancel()
        await asyncio.gather(task, return_exceptions=True)
```

- The loop body catches `Exception`, logs the error type, and goes on. It never swallows a cancel.
- Enter contexts on one `AsyncExitStack` in dependency order; they exit in reverse.
- A fire-and-forget task needs a strong reference, a cap, a name, and a wrapper that logs its own
  failure. The loop holds tasks only by weak reference. Check the cap and add the task with no
  `await` between, or concurrent callers overshoot it.

```python
if len(tasks) >= MAX_TASKS:
    return  # say what covers the skipped work
task = asyncio.create_task(dispatch_logged(item_id), name=f"dispatch-{item_id}")
tasks.add(task)
task.add_done_callback(tasks.discard)
```

- A loop-wide exception handler is a net for tasks an owner missed, not the plan.

## Siblings and gather

- Jobs that succeed or fail together run in a `TaskGroup`: the first failure cancels the rest and
  the group raises after all have stopped. Handle the `ExceptionGroup` it raises.
- A plain `gather(a, b)` returns on the first error and leaves the others running unwatched.
  Independent jobs use `return_exceptions=True`, then check each result: re-raise a
  `BaseException` that is not an `Exception` (a cancel or an exit), log each other failure once.
- Cleanup that gathers tasks it just cancelled uses `return_exceptions=True`, then logs any result
  that is a real error, so a loop that died before the cancel stays visible.
- Never `with suppress(CancelledError): await task`. It also swallows a cancel aimed at you.

## Timeouts

- A bounded operation gets one deadline. A fresh timeout per read does not bound a slow trickle.

```python
deadline = asyncio.get_running_loop().time() + TOTAL_SECONDS
for part in parts:
    async with asyncio.timeout_at(deadline):
        await read(part)
```

- A stream of unknown length (a model reply, a log tail) gets an idle timeout per read plus a
  byte cap. A total deadline would cut long, valid output.
- Put `except TimeoutError` outside the `async with asyncio.timeout(...)` block. The block turns
  the cancel into `TimeoutError` only as it exits.
- `asyncio.wait_for` waits until the inner awaitable has stopped, so it can overrun. Use it only
  for one awaitable that stops quickly on cancel; use `asyncio.timeout` for a block.
- A check that grants access fails closed on timeout. A hung check must not keep access open.
- A loop that must stop waits on its stop event, not a bare sleep:
  `with suppress(TimeoutError): await asyncio.wait_for(stopping.wait(), INTERVAL)`.
- Set a client-side bound on every network wait: HTTP client timeouts, database connect timeout
  and TCP keepalives. A server-side limit cannot see a dead peer.
- Validate timeout settings at startup: `math.isfinite(value) and value > 0`.

## Cancellation

- Re-raise `CancelledError`. `except Exception` does not catch it; `except BaseException` does,
  and may only clean up before `raise`.
- Cleanup in a cancel handler must not replace the cancel. Give a cleanup that can fail its own
  `try`, log the error type, then re-raise the cancel.
- A task that truly absorbs its own cancel calls `uncancel()` and compares `cancelling()` with the
  count it saw at the start, so an outer shutdown cancel still passes.
- A helper task that can cancel its owner (a lease renewer) stops before the owner's handlers
  await, or its cancel lands in the middle of cleanup.
- Work that must finish after its caller leaves (a token refresh that rotates the token, a lease
  release) runs in a kept task that the caller does not cancel, with its own bound. A bare
  `asyncio.shield(coro())` keeps no reference; keep the task.
- Cleanup that must run goes in `finally`. A long cleanup gets its own timeout. A reconnect loop
  uses capped backoff.
- Do not `yield` inside `async with asyncio.timeout(...)` or a `TaskGroup` in an async generator.
  Await inside the scope and yield after it.

## Blocking and CPU work

- No blocking call inside `async def`: file trees, archives, `fsync`, `subprocess.run`, `requests`,
  `time.sleep`. Use `await asyncio.to_thread(fn, arg)` or the async API.
- A thread runs on after its task is cancelled. Put a multi-step file operation and its cleanup in
  one sync function, and on cancel wait for the thread before cleaning up.
- `re`, the C `json` encoder, and other C calls hold the GIL and stall the loop even in a thread.
  Bound the input first or use a cheaper search.
- Untrusted CPU work (schema validation, parsing a hostile file) runs in a child process:
  `sys.executable -I`, `env={}`, a CPU limit, a timeout, a non-blocking semaphore, and a kill in
  `finally`.
- Limit heavy thread work with a semaphore. Batch many small writes into one hop.

## Backpressure

- Every `asyncio.Queue(maxsize=N)`. State the full policy in code: drop, close, or block.

```python
try:
    queue.put_nowait(event)
except asyncio.QueueFull:
    connection.close_for_resync()  # the client reloads; the server never buffers without bound
```

- After writing to a stream, `await writer.drain()`, so a fast writer waits for a slow reader.
- A semaphore only moves the wait. For work a client asked for, say busy at the door (HTTP 503
  with `Retry-After`). Use a semaphore for work the service started itself.
- Bound what is accepted: claim a fixed batch from a queue table, cap the task set, and count every
  connection holder when you size a pool.

## Shutdown

Stop accepting work, let running work drain for a bounded time, cancel, then wait again with a
bound. Each layer's waits fit inside the layer above. Stop with an `asyncio.Event`; add no wait
without a deadline. Check at startup that the drain and cancel budgets fit the total.

## Tests

- Under pytest, turn "coroutine was never awaited" and unraisable exceptions into failures:
  `filterwarnings = ["error:coroutine .* was never awaited:RuntimeWarning",
"error::pytest.PytestUnraisableExceptionWarning"]`. Under `unittest`, run the suite with
  `python -X dev` and treat each such warning as a failure.
- A cancel test cancels at the exact await (an `asyncio.Event` the fake waits on), then asserts
  the cancel propagates and the cleanup ran.
- Run a suspect async test with `PYTHONASYNCIODEBUG=1` to see slow callbacks and lost tasks.

## Review checklist

1. Does every `create_task` result have an owner and a `name=`? Is a fire-and-forget set capped?
2. Is every cancelled task awaited? Is any `suppress(CancelledError)` left?
3. Do siblings use a `TaskGroup`, and do independent jobs check each `gather` result?
4. Does every outbound await have a finite bound that matches what the caller waits on?
5. Does every cancel handler re-raise, with cleanup unable to replace the cancel?
6. Is each blocking or CPU call off the loop, and untrusted CPU work in a child process?
7. Is each queue bounded with a stated full policy, and each fan-out capped?
8. Do shutdown waits fit inside the total shutdown budget?
