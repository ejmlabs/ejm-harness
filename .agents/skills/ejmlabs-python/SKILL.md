---
name: ejmlabs-python
description: Use before you write, edit, or review Python, including .py files, pyproject.toml, uv.lock, ruff or mypy config, a new dependency, asyncio code, SQL sent from Python, Python tests, or a script that reads untrusted input. Holds the ejmlabs rules for tooling, types, errors, async, and tests.
---

# Python at ejmlabs

Python here is typed, strict-linted, and plain: functions and built-in types, bounded work, and
errors that keep their cause. A repository skill may add stricter rules, never weaker ones.

## Tooling

- Use `uv`. Declare dependencies in `pyproject.toml`, commit `uv.lock`, never edit it by hand, and
  run tools with `uv run --locked`. Pin development tools exactly in `[dependency-groups] dev`.
- Support the versions in `requires-python`, and set ruff `target-version` and mypy
  `python_version` to its floor. Repositories differ (3.11 to 3.15): read it, never assume one.
- Ruff and mypy are strict. The ruff config sets `extend = "ruff.strict.toml"` (the synced shared
  layer) and may add rules, never remove them; `[tool.mypy]` sets `strict = true` and
  `warn_unreachable = true`. Ruff owns formatting and imports; add no black, isort, or flake8.
- Suppress one rule on one line, with a reason: `# noqa: TRY301 - reported by the handler below`,
  `# type: ignore[import-not-found]  # optional package`. No file-wide or bare suppressions.

## Design and types

- Write plain functions over built-in types. Add a class only when it owns state, enforces an
  invariant, or models a real concept. No pass-through wrappers or `utils`/`helpers` modules.
- Share code by composition; subclass only for strictly hierarchical data or a class you do not
  own. An interface is a small `typing.Protocol`, not `abc.ABC`, never gated by `@runtime_checkable`.
- Type every boundary: accept `Sequence` or `Mapping`, return concrete types, write `X | None`,
  use `Literal` for small fixed sets. Avoid `Any` and union returns. Data that is untyped until
  checked takes `Any` once, with `# noqa: ANN401 - <reason>`, and is narrowed right there.
- Use Pydantic models for app-owned request and response contracts, one per endpoint, with no
  generic envelope; keep SDK objects native unless a conversion has a concrete need. Code with no
  dependencies (CI scripts, SDKs) uses `TypedDict` or `dataclass`. Never hand-edit generated types.
- Validate untrusted input at the boundary and pass validated values inside. Fail clearly on
  missing configuration or data; never hide it behind an empty value or a silent default.
- No mutable defaults (`items=()`). Test absence with `is None`. A `None` never skips a check.

## Errors and logging

- Catch an exception only where you recover or add context. Catch `Exception`, never a bare
  `except:`. Re-raise with `from error` or `from None`. Raise a specific type, never `Exception`.
- A handler that does nothing says why in a comment. A plain `except T` misses a `T` inside an
  `ExceptionGroup`, so code around a `TaskGroup` handles the group.
- Log a failure once, where it is handled, with `logging.getLogger(__name__)` and `%s` arguments,
  never f-strings. Log `type(error).__name__` and safe identifiers, never tokens, secrets, row
  values, or an exception message that can carry them.
- A command-line script reports a failure on stderr and exits nonzero (`raise SystemExit(...)`).

## Asyncio

- Every task has an owner that awaits it: a `TaskGroup`, a variable awaited later, or a capped set
  with `task.add_done_callback(tasks.discard)`. Name every task you keep.
- Siblings that succeed or fail together run in a `TaskGroup`. Independent jobs use
  `gather(..., return_exceptions=True)` and check each result. Stop a task with `task.cancel()`
  then `await asyncio.gather(task, return_exceptions=True)`; never `suppress(CancelledError)`.
- Every outbound await has a finite bound: `asyncio.timeout`, one deadline across a multi-step
  operation, or an idle timeout plus a byte cap for a stream. Reject a timeout setting that is not
  finite and positive.
- Let cancellation propagate: an `except BaseException` or `except CancelledError` block cleans
  up, then runs `raise`. Never block the loop: run blocking or CPU work with
  `await asyncio.to_thread(...)`. A thread does not stop when its task is cancelled.
- Every `asyncio.Queue` has a `maxsize` and a stated full policy (drop, close, or block). Bound
  fan-out with a `Semaphore`. Open clients and pools once in a lifecycle owner, close them on
  shutdown or partial startup failure, and do no network I/O at import.

## SQL

- Pass every value as a driver parameter (`"... WHERE name = %s", (name,)`). An f-string may join
  only fixed SQL text that the code owns, never a value.
- One transaction per unit of work, with no network call inside it. Use `SET LOCAL`, never `SET`,
  on a pooled connection. Read a growing table with `LIMIT` and a keyset cursor, never `OFFSET`.

## Tests

- House default: standard library `unittest`, run as `python -m unittest discover -s tests`, with
  `IsolatedAsyncioTestCase` for async code. A repository already on pytest keeps pytest and
  follows the style of the file you edit. Add no second runner and no `pytest-asyncio`.
- Tests meet the same strict ruff and mypy settings. Fake at the boundary (`unittest.mock`, a mock
  HTTP transport). No network, paid calls, or real secrets. Wait on an event, never a fixed sleep.

## Scripts and untrusted input

- A script has `#!/usr/bin/env python3`, its docstring as the `argparse` description, and `main()`.
- Run Python that reads untrusted files (downloads, archives, PR checkouts) with `python -I`. It
  skips the script and working directories, user site packages, and `PYTHON*` variables, so a
  planted `json.py` cannot load. Keep the script outside that directory; pass paths as arguments.
- Run untrusted CPU work in a child: `[sys.executable, "-I", ...]`, `env={}`, a timeout, and a
  kill in `finally`. Pass subprocess arguments as a list, never `shell=True`; check the status.
  Every network call has a timeout and reads at most its byte limit plus one byte.

## Standard library first

- Use `pathlib`, `tomllib`, `json`, `argparse`, `subprocess`, `urllib.request`, and `unittest`
  first. Add a package only for a capability they and current packages lack, after you check its
  name and publisher. Scripts that run before dependencies install use only the standard library.
- Every cache has a maximum size: never `functools.cache` or `maxsize=None` on user-chosen keys.

## Read next

- [references/tooling.md](references/tooling.md): when you set up or change `pyproject.toml`, ruff, mypy, or check commands.
- [references/asyncio.md](references/asyncio.md): when you add a task, timeout, queue, thread hop, or shutdown path.
- [references/sql-and-durable-work.md](references/sql-and-durable-work.md): when you write SQL, a claim, a migration, a job, or a retry.
- [references/testing.md](references/testing.md): when you add Python tests, fakes, or database-backed tests.
