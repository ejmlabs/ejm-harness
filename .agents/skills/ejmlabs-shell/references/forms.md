# Shell forms

Short house forms. Each passes ShellCheck 0.11 with the shared `.shellcheckrc` (`enable=all`).

## Options and usage

```bash
usage() {
  cat <<'USAGE'
usage: restore.sh [--dry-run | --check]
       restore.sh --key KEYFILE
USAGE
}

MODE=run KEY=''
while [[ $# -gt 0 ]]; do
  case $1 in
    --dry-run) MODE=dry ;;
    --check) MODE=check ;;
    --key) KEY=${2:?key file}; shift ;;
    -h|--help) usage; exit 0 ;;
    *) usage >&2; exit 2 ;;
  esac
  shift
done
[[ -z "$KEY" || -f "$KEY" ]] || { echo "--key needs an existing file" >&2; exit 2; }
```

## An idempotent step

Each step reports `ok` when the state is already right. Otherwise `need` applies the change in a
run, or reports drift in `--dry-run` and `--check`. The summary line lets a rerun prove zero
changes.

```bash
CHANGED=0 DRIFT=0
ok() { echo "ok       $*"; }
# need <what> <command...>: apply and count in a run, report drift otherwise.
need() {
  local what=$1; shift
  if [[ "$MODE" = run ]]; then
    "$@"; echo "changed  $what"; CHANGED=$((CHANGED + 1))
  else
    echo "drift    $what"; DRIFT=$((DRIFT + 1))
  fi
}

step_link() {
  local current
  current=$(readlink "$HOME/.local/bin/tool" || true)  # no link yet is a normal state
  if [[ "$current" = "$TOOL_PATH" ]]; then ok "tool link"; return; fi
  need "tool link" ln -sfn "$TOOL_PATH" "$HOME/.local/bin/tool"
}

step_link
echo "summary: changed=$CHANGED drift=$DRIFT"
if [[ "$MODE" = check && "$DRIFT" -gt 0 ]]; then exit 1; fi
```

If ShellCheck flags a helper that exists only to be passed to `need` (SC2329, or SC2317 before
ShellCheck 0.11), suppress that code on the line before the helper:
`# shellcheck disable=SC2329 # called through need()`.

## Temp directory and one cleanup trap

```bash
tmp=$(mktemp -d)
cleanup() { rm -rf "$tmp"; }
trap cleanup EXIT
```

When a later step needs more cleanup (a key file to shred), add it to `cleanup`; do not set a
second EXIT trap.

## Publish a finished file

Readers never see a partial file, and the script never writes through a symlink.

```bash
publish() { # publish <file> <dir>
  local src=$1 dir=$2 name
  name=$(basename "$src")
  if [[ -L "$dir" || -L "$dir/$name" ]]; then
    echo "refusing to write through a symlink: $dir/$name" >&2; return 1
  fi
  mkdir -p "$dir"
  cp "$src" "$dir/$name.tmp"
  mv -f "$dir/$name.tmp" "$dir/$name"
}
```

## Files without ls

Write a command's output to a file first, so its failure stops the script. A `< <(...)` process
substitution hides the command's status from `set -e`.

```bash
newest=''
for f in "$dir"/*.tar.age; do
  [[ -e "$f" ]] || continue  # the glob matched nothing
  if [[ -z "$newest" || "$f" -nt "$newest" ]]; then newest=$f; fi
done

find "$root" -type f -name '*.md' -print0 >"$tmp/paths"
while IFS= read -r -d '' path; do
  printf 'found %s\n' "$path"
done <"$tmp/paths"
```

## Processes

```bash
# The bracket keeps the pattern from matching the shell that runs this line.
pkill -f '[w]atch\.sh' || true  # no running watcher is fine

if ! pgrep -f '[w]atch\.sh' >/dev/null; then
  setsid nohup "$here/watch.sh" >/dev/null 2>&1 </dev/null &
fi

healthy=0
for _ in $(seq 12); do
  if curl -fsS -m 3 -o /dev/null http://127.0.0.1:8080/health; then healthy=1; break; fi
  sleep 5
done
[[ "$healthy" = 1 ]] || { echo "service not healthy after 60 s" >&2; exit 1; }
```

`pkill -f` and `pgrep -f` match whole command lines, including the shell that runs them when the
pattern is in that shell's own arguments (`bash -c`, `ssh host '...'`, `sudo sh -c`). The pattern
`[w]atch\.sh` still matches `watch.sh`, but not its own literal text in the calling shell.

## A pinned, verified download

```bash
fetch_tool() { # fetch_tool <url> <sha256> <dest>, with $tmp from the cleanup trap
  local url=$1 sum=$2 dest=$3
  curl -fsSL --max-time 120 -o "$tmp/tool.tgz" "$url"
  echo "$sum  $tmp/tool.tgz" | sha256sum -c - >/dev/null
  tar -xzf "$tmp/tool.tgz" -C "$tmp"
  install -D -m 755 "$tmp/tool" "$dest"
}
```

## Keep going and report each failure

Only for a job whose steps are independent, such as a nightly backup. Say so on the `set` line.

```bash
set -uo pipefail  # no -e: each step records its own failure and the run goes on
FAILED=()
run_step() { # run_step <name> <command...>
  local name=$1; shift
  "$@" || FAILED+=("$name")
}
run_step "secret backup" backup_secrets
run_step "work snapshot" backup_work
for step in "${FAILED[@]}"; do echo "failed: $step" >&2; done
[[ "${#FAILED[@]}" -eq 0 ]]
```

## A regression check

```bash
#!/usr/bin/env bash
# Runs restore.sh in a scratch directory; nothing on the machine is touched.
set -euo pipefail
repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT
fail() { echo "FAIL: $*" >&2; exit 1; }

out=$(TARGET=$scratch "$repo/restore.sh")
grep -q 'changed=1' <<<"$out" || fail "first run: $out"
out=$(TARGET=$scratch "$repo/restore.sh")
grep -q 'changed=0' <<<"$out" || fail "rerun not idempotent: $out"
echo "ok   restore is idempotent"
```
