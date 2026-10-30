---
name: ejmlabs-shell
description: Use before you write, edit, or review a shell script, including .sh files, bash in bin/ or scripts/, git hooks, CI run steps, install, restore, or backup scripts, .shellcheckrc, or a pkill or pgrep line. Holds the ejmlabs bash rules and when to switch to Python.
---

# Shell scripts at ejmlabs

A shell script here is bash, strict, ShellCheck-clean, quoted, and safe to run twice. Past a small
job it becomes Python. A repository skill may add stricter rules, never weaker ones.

## Start every script the same way

```bash
#!/usr/bin/env bash
# What this does, who runs it, and how to call it.
set -euo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
```

- Write bash, not POSIX `sh`. A `#!/bin/sh` file is only a one-line `exec` wrapper, such as a git
  hook that runs one tool.
- Drop `-e` only in a script that records each step's failure and keeps going. Say so in a comment
  on the `set` line, and check every status yourself.
- Find files from the script's directory (`here`), never from the caller's working directory.
- Check required tools first: `command -v jq >/dev/null || { echo "Missing jq" >&2; exit 1; }`.
- Parse options with a `while` and `case` loop. Print usage on `-h`, exit 2 on an unknown option.
  A required argument uses `${1:?usage: restore.sh ARCHIVE}`.
- Keep pinned versions in one sourced file, with `# shellcheck source=versions.env` above it.

## ShellCheck

- Every script passes `bash -n` and `shellcheck` with the shared `.shellcheckrc` at the repository
  root (`enable=all`, `external-sources=true`, only `SC2250` off). The file is synced from the
  shared lint layer; never edit it in a consumer repository.
- `enable=all` turns on the optional checks. Test with `[[ ]]`, not `[ ]` (SC2292). Do not hide a
  command's status inside `$(...)` or `< <(...)` (SC2312): run it on its own line first.
- Suppress one code on the line before, with a reason:
  `# shellcheck disable=SC2329 # called through need()`. Never a file-wide disable.
- Fix, do not suppress, a code that names a real hazard: SC2086 (word splitting), SC2012 (parsing
  `ls`), SC2064 (trap text expanded too early).

## Quoting and arrays

- Quote every expansion: `"$var"`, `"$(cmd)"`, `"${list[@]}"`. Use `$*` only inside a message.
- A list of paths, packages, or arguments is an array, never a space-separated string:
  `args=(--locked --group dev)`, then `uv run "${args[@]}" ruff check .`.
- Read lines with `while IFS= read -r line; do ...; done <"$file"`. Always `read -r`. Paths from
  `find` travel with `-print0` and `read -r -d ''` or `xargs -0`.
- Declare function variables `local`. Print data with `printf '%s\n' "$value"`, not `echo`.

## Never parse ls

- Loop over a glob and guard the no-match case: `for f in "$dir"/*.age; do [[ -e "$f" ]] || continue`.
- Compare ages with `[[ "$a" -nt "$b" ]]` and sizes with `stat`, never by reading `ls -l` or `ls -t`.

## Temp files and traps

- `tmp=$(mktemp -d)`, then `trap cleanup EXIT` on the next line, where `cleanup` removes it.
  Never a fixed name in `/tmp`.
- One EXIT trap per script: a second `trap ... EXIT` replaces the first. Put all cleanup in one
  function. Single-quote any inline trap text so it expands when it runs.
- Build secrets under `umask 077` in a private directory you own and have checked is not a
  symlink. Remove a key file in the EXIT cleanup.
- Write a file that others read to a temporary name in the same directory, then `mv` it over the
  target, so no reader sees a partial file. Refuse to write through a symlink.

## Safe to run twice

- The second run changes nothing. Each step checks the current state, then acts, and prints
  `ok`, `changed`, or `skipped`.
- A script that changes a machine offers `--dry-run` (report, change nothing) and `--check` (exit 1
  on drift). Never overwrite a live file that differs from the managed copy; keep and report it.
- Prefer idempotent commands: `mkdir -p`, `install -D -m 644`, `ln -sfn`, and a marker check
  (`grep -qxF "$marker" file ||`) before you append.
- Download a pinned version with `curl -fsSL --max-time 60`, check its `sha256sum` before use, and
  check the installed version after.

## Processes and waits

- `pkill -f` and `pgrep -f` match whole command lines, including the shell that runs them when the
  pattern is in that shell's own arguments (`bash -c`, `ssh host '...'`, `sudo sh -c`). Bracket
  one character: `pkill -f '[w]atch\.sh'`. The pattern still matches `watch.sh`, but not the
  literal text `[w]atch\.sh` in the calling shell. Use `pkill -x name` for an exact process name.
- Bound every wait: `curl -m 3`, `timeout 30 cmd`, and a retry loop with a fixed count.

## Output and tests

- Diagnostics go to stderr, results to stdout. `|| true` only where a failure is expected.
- Test a script in a scratch directory (`mktemp -d` plus an EXIT trap), never on the real machine.
  Assert with `fail() { echo "FAIL: $*" >&2; exit 1; }` and cover the rerun: zero changes.

## When to switch to Python

Write Python instead (see the `ejmlabs-python` skill) when the job parses JSON, TOML, or other
structure beyond one `jq` or `sed` expression; needs maps, nested data, or per-item error handling
and retries; calls an HTTP API beyond a single `curl`; has logic that needs unit tests; or embeds
another language in a heredoc. A long script that is mostly a list of commands may stay in bash.

## Read next

- [references/forms.md](references/forms.md): when you write option parsing, an idempotent step, a temp directory, an atomic publish, or a process match.
