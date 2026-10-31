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
  root (`enable=all`, `external-sources=true`; `SC2250`, `SC2310`, `SC2311` and `SC2312` off). The
  file is synced from the shared lint layer; never edit it in a consumer repository.
- `enable=all` turns on the optional checks. Test with `[[ ]]`, not `[ ]` (SC2292).
- Suppress one code on the line before, with a reason:
  `# shellcheck disable=SC2329 # called through need()`. Never a file-wide disable.
- Fix, do not suppress, a code that names a real hazard: SC2086 (word splitting), SC2012 (parsing
  `ls`), SC2064 (trap text expanded too early).

## Quoting, temp files and traps

- Quote every expansion: `"$var"`, `"$(cmd)"`, `"${list[@]}"`. A list of paths or arguments is an
  array, never a space-separated string. Declare function variables `local`. Print data with
  `printf '%s\n' "$value"`, not `echo`.
- Read lines with `while IFS= read -r line; do ...; done <"$file"`. Paths from `find` travel with
  `-print0` and `read -r -d ''` or `xargs -0`. Loop over a glob and guard the no-match case.
  Compare ages with `-nt`, never by parsing `ls`.
- `tmp=$(mktemp -d)`, then `trap cleanup EXIT` on the next line. Never a fixed name in `/tmp`.
  One EXIT trap per script: a second replaces the first, so put all cleanup in one function.
- Build secrets under `umask 077` in a private directory you own and have checked is not a
  symlink. Write a file that others read to a temporary name in the same directory, then `mv` it
  over the target. Refuse to write through a symlink.

## Safe to run twice

- The second run changes nothing. Each step checks the current state, then acts, and prints
  `ok`, `changed`, or `skipped`.
- A script that changes a machine offers `--dry-run` (report, change nothing) and `--check` (exit 1
  on drift). Never overwrite a live file that differs from the managed copy; keep and report it.
- Prefer idempotent commands: `mkdir -p`, `install -D -m 644`, `ln -sfn`, and a marker check
  (`grep -qxF "$marker" file ||`) before you append.
- Download a pinned version with `curl -fsSL --max-time 60`, check its `sha256sum` before use.

## Processes, output and tests

- `pkill -f` and `pgrep -f` match whole command lines, including the calling shell's own. Bracket
  one character: `pkill -f '[w]atch\.sh'`. Use `pkill -x name` for an exact process name.
- Bound every wait: `curl -m 3`, `timeout 30 cmd`, and a retry loop with a fixed count.
- Diagnostics go to stderr, results to stdout. `|| true` only where a failure is expected.
- Test a script in a scratch directory, never on the real machine, and cover the rerun: zero
  changes.

## When to switch to Python

Write Python instead (the `ejmlabs-python` skill, when the repository has it) when the job parses
JSON, TOML, or other structure beyond one `jq` or `sed` expression; needs maps, nested data, or
per-item error handling and retries; calls an HTTP API beyond a single `curl`; has logic that
needs unit tests; or embeds another language in a heredoc.

## Read next

- [references/forms.md](references/forms.md): option parsing, an idempotent step, a temp
  directory, an atomic publish, a process match, a regression check.
