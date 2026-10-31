# Python tooling

The house form for a Python project's `pyproject.toml`, ruff, mypy, and check commands.

## pyproject.toml

```toml
[project]
requires-python = ">=3.12"
dependencies = []

[dependency-groups]
dev = ["ruff==<pinned>", "mypy==<pinned>"]

[tool.mypy]
python_version = "3.12"
strict = true
warn_unreachable = true
files = ["scripts", "tests"]
```

- Use the ruff and mypy versions that the shared lint layer pins, so every repository runs the
  same rules. Tools take exact pins (`==`). The committed `uv.lock` fixes every resolved version,
  and every install and run uses `--locked`.
- Put development tools in `[dependency-groups] dev`, once. Do not repeat them in
  `[project.optional-dependencies]`.
- A project that only locks tools (no package to build) sets `[tool.uv] package = false`.
- A package that ships types includes a `py.typed` marker in its package data.
- `requires-python` is the source of truth. `python_version` (mypy) and `target-version` (ruff)
  match its floor.

## Ruff

The repository's `ruff.toml` (or `[tool.ruff]`) extends the synced strict layer:

```toml
extend = "ruff.strict.toml"
target-version = "py312"
```

- The layer selects `ALL` and ignores only rules that are style, or reviewed by other means (for
  example `S603` and `S607`: every subprocess call's inputs are reviewed). `BLE001` and `TRY400`
  are off because the house error-logging form catches `Exception` and logs only the exception
  type. A repository may add rules and excludes. It never removes a rule from the layer.
- Never edit `ruff.strict.toml` in a consumer repository; change it in the shared lint layer.
- A file that runs on a different interpreter than the project (a git hook on the system
  `python3`, a CI step on the runner's Python) gets its own entry in
  `[tool.ruff.per-file-target-version]`, and the reason in a comment.
- Generated files go in `exclude`. Do not lint or edit them by hand.
- Tests may use `assert`, private access, `Any`, and placeholder credentials (`S101`, `SLF001`,
  `ANN401`, `S105`, `S106`, `S107` are ignored for test paths). Every other rule applies to tests.

## Suppressions

One rule, one line, one reason. The reason says why the rule does not apply here.

```python
import litellm  # type: ignore[import-not-found]  # optional package, absent from the dev group


def level(value: Any) -> int:  # noqa: ANN401 - TOML values are untyped until checked here
    return LEVELS[value["level"]]
```

- Never `# noqa` with no code, never `# type: ignore` with no code, never a file-wide disable.
- A per-file ignore in the config is for a rule that cannot apply to a whole path. A temporary
  ignore for existing violations says so in a comment and names the fix.

## Commands

Run the tools from the locked environment, from the repository root:

```sh
uv run --locked --group dev ruff check .
uv run --locked --group dev mypy
uv run --locked python -m unittest discover -s tests
```

- Formatting is not gated: no gate runs `ruff format --check`. Run `ruff format` on a file you
  are writing anyway, never on files you did not change.
- `uv sync --locked` fails when `uv.lock` is stale. Fix it with `uv lock`, never by hand.
- Add a dependency with `uv add <name>` (or `uv add --group dev <name>==<version>` for a tool), then
  commit `pyproject.toml` and `uv.lock` together.
- The repository's gate may wrap these in its own command. Use the gate's command when it exists.

## Where repositories differ

- Python floors range from 3.11 to 3.15. Read `requires-python`; never copy a version from another
  repository.
- Some repositories still select a subset of ruff rules or pin older tool versions. Moving them to
  the strict layer is a separate change with its own fixes; do not loosen the layer to match them.

## Scripts and untrusted input

- `python -I` skips the script's directory and the working directory, user site packages, and
  `PYTHON*` variables, so a planted `json.py` cannot load. Use it for any Python that reads
  downloads, archives or PR checkouts, keep the script outside the untrusted directory, and pass
  paths as arguments.
- Untrusted CPU work runs in a child: `[sys.executable, "-I", ...]`, `env={}`, a timeout, and a
  kill in `finally`. Pass subprocess arguments as a list, never `shell=True`, and check the
  status.
- Every network call has a timeout and reads at most its byte limit plus one byte.
- Never use `functools.cache` or `maxsize=None` on user-chosen keys.
- A mutable default (`items=[]`) is shared between calls. Use `None` or an immutable value, and
  test absence with `is None`, so a `None` never skips a check.
