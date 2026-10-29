# Python tests

Python forms for the shared testing rules. Read the nearest existing test file before you write a
new one, and match it.

## Runner and style

- House default: standard library `unittest`. Tests live in `tests/test_<area>.py` and run with
  `python -m unittest discover -s tests`. Add to the file of the area you change; start a new file
  only for a new area.
- Async tests use `unittest.IsolatedAsyncioTestCase`. Use `self.enterContext(patch(...))` for
  patches and `self.subTest(...)` for tables.
- A repository that already runs pytest keeps it. There, a plain `def test_x()` with `assert` is
  the default, and async tests follow the form of the file you edit. Add no new async form and no
  `pytest-asyncio`.
- Tests meet the strict lint layer, except `assert`, private access, and `Any`, which test paths
  may use. Type-check tests with mypy when the repository lists them in `files`.
- A test that runs the strict tools (ruff and mypy from the locked group) lets the gate's unit-test
  step enforce them with no workflow change. It fails, not skips, when `uv` is missing.

## Fakes

- Fake at the edge of code you do not own: `unittest.mock.patch` where the code looks the name up
  (`patch("package.module.get_pool", ...)`), `AsyncMock` for awaitables, a mock HTTP transport for
  outbound calls, an in-process ASGI transport to call a web app.
- Use the real object when it is cheap and deterministic. A claim about a unique key, a lock, a
  lease, or a reservation needs a real database.
- No real network, no paid model calls, no real secrets. Use placeholders like
  `"test-key-not-used"`. Never read `.env`.
- Test a script as its user runs it: `subprocess.run([sys.executable, str(SCRIPT), ...],
capture_output=True, text=True, check=False)` inside a `tempfile.TemporaryDirectory()`, then
  assert on the exit status and the output.

## Database-backed tests

- Gate each one on its database URL and skip without it:
  `@unittest.skipUnless(os.getenv("TEST_DATABASE_URL"), "needs Postgres")`. In CI, a missing URL
  must fail the run, not skip it.
- Give each test its own schema or database and drop it in teardown. Use synthetic data on a
  disposable server only.
- Shrink pools in tests, so the test that leaks a connection fails, not a later one.

## Determinism

- Never wait a fixed time. Wait on the event, with a limit that catches a hang:
  `await asyncio.wait_for(done.wait(), 2)`.
- Pass the clock in, so a test can run at midnight or across a month end. Seed any random input
  and print the seed on failure.
- Two coroutines racing can each win: assert every valid outcome, not the one you saw first.
- A skipped test is not a pass. Report the skip count you saw.

## What to assert

- One focused regression test per bug, named for the behavior. Watch it fail before the fix.
- Assert what the caller sees: return values, rows written, calls made at the boundary. Do not
  assert on private helpers or a call order you do not need.
- For a failure, assert the error type (or message) and that no effect ran.
- For a cancel, cancel at the exact await, require `CancelledError` to propagate, and assert the
  cleanup ran.
- For a redelivered event, send it twice and send copies at once; assert one effect.
