---
name: ejmlabs-python
description: Use before you write, edit, or review Python, including .py files, pyproject.toml, uv.lock, ruff or mypy config, a new dependency, asyncio code, SQL sent from Python, Python tests, or a script that reads untrusted input.
---

# Python at ejmlabs

Python here is typed, strict-linted, and plain: functions and built-in types, bounded work, and
errors that keep their cause. A repository skill may add stricter rules, never weaker ones.

## Tooling

- Use `uv`. Declare dependencies in `pyproject.toml`, commit `uv.lock`, never edit it by hand, and
  run tools with `uv run --locked`. Pin development tools exactly in `[dependency-groups] dev`.
- Support the versions in `requires-python`, and set ruff `target-version` and mypy
  `python_version` to its floor. Read it; never assume a version.
- Ruff and mypy are strict. The ruff config sets `extend = "ruff.strict.toml"` (the synced shared
  layer) and may add rules, never remove them; `[tool.mypy]` sets `strict = true` and
  `warn_unreachable = true`. Add no black, isort, or flake8. Formatting is not gated: leave the
  layout of files you did not change alone.
- Suppress one rule on one line, with a reason: `# noqa: TRY301 - reported by the handler below`.
  No file-wide or bare suppressions.

## Design and types

- Write plain functions over built-in types. Add a class only when it owns state, enforces an
  invariant, or models a real concept. No pass-through wrappers or `utils`/`helpers` modules.
- Share code by composition. An interface is a small `typing.Protocol`, not `abc.ABC`.
- Type every boundary: accept `Sequence` or `Mapping`, return concrete types, write `X | None`.
  Avoid `Any` and union returns. Data that is untyped until checked takes `Any` once, with
  `# noqa: ANN401 - <reason>`, and is narrowed right there.
- Use Pydantic models for app-owned request and response contracts, one per endpoint, with no
  generic envelope. Code with no dependencies uses `TypedDict` or `dataclass`.
- Validate untrusted input at the boundary and pass validated values inside. Fail clearly on
  missing configuration or data; never hide it behind an empty value or a silent default.
- No mutable defaults. Test absence with `is None`.

## Errors and logging

- Catch an exception only where you recover or add context. Catch `Exception`, never a bare
  `except:`. Re-raise with `from error` or `from None`. Raise a specific type, never `Exception`.
- A handler that does nothing says why. A plain `except T` misses a `T` inside an
  `ExceptionGroup`, so code around a `TaskGroup` handles the group.
- Log a failure once, where it is handled, with `logging.getLogger(__name__)` and `%s` arguments.
  Log `type(error).__name__` and safe identifiers, never tokens, secrets, row values, or an
  exception message that can carry them. The layer ignores BLE001 and TRY400 for this form.
- A command-line script reports a failure on stderr and exits nonzero (`raise SystemExit(...)`).

## Asyncio, SQL and tests

- Every task has an owner that awaits it, every outbound await has a finite bound, every
  `asyncio.Queue` has a `maxsize` and a full policy, and cancellation propagates. Never block the
  loop: use `asyncio.to_thread`. Forms: [references/asyncio.md](references/asyncio.md).
- Pass every SQL value as a driver parameter; an f-string joins only fixed SQL text. One short
  transaction per unit of work, no network call inside it, `SET LOCAL` never `SET`, keyset
  cursors never `OFFSET`. Forms: [references/sql-and-durable-work.md](references/sql-and-durable-work.md).
- The house test runner is `unittest` (`python -m unittest discover -s tests`); a repository on
  pytest keeps it. No network, paid calls, or real secrets, and wait on an event, never a fixed
  sleep. Forms: [references/testing.md](references/testing.md).

## Scripts and untrusted input

- A script has `#!/usr/bin/env python3`, its docstring as the `argparse` description, and `main()`.
  Use the standard library first; add a package only for a capability it lacks. Scripts that run
  before dependencies install use only the standard library. Every cache has a maximum size.
- Run Python that reads untrusted files (downloads, archives, PR checkouts) with `python -I`,
  with the script outside that directory and paths as arguments. Run untrusted CPU work in a
  child: `[sys.executable, "-I", ...]`, `env={}`, a timeout, a kill in `finally`. Pass subprocess
  arguments as a list, never `shell=True`; check the status. Every network call has a timeout
  and reads at most its byte limit plus one byte.

## Read next

- [references/tooling.md](references/tooling.md): `pyproject.toml`, ruff, mypy, check commands.
- [references/asyncio.md](references/asyncio.md): a task, timeout, queue, thread hop, shutdown.
- [references/sql-and-durable-work.md](references/sql-and-durable-work.md): SQL, claims, jobs.
- [references/testing.md](references/testing.md): Python tests, fakes, database-backed tests.
