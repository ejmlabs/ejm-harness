---
name: harness-dependencies
description: Change ejm-harness toolchain pins, Cargo, npm, or Python dependencies, cargo-deny exceptions, locks, CI tools, and local build environment problems.
---

# ejm-harness dependencies and tooling

Read [the contributor contract](../../../docs/contributing.md) and `deny.toml`. Use
[harness-maintenance](../harness-maintenance/SKILL.md) for the package-before-plumbing policy.

## Pins and installation

- Rust is pinned in `rust-toolchain.toml`. Quality tools are pinned in the contributor guide and
  `.github/workflows/check.yml`: `cargo-deny`, `cargo-machete`, `uv`, `pre-commit`. Change a pin
  in both places in one patch.
- Cargo, npm, and Python locks are committed. Install with `npm ci --ignore-scripts` and
  `uv sync --project sdk/python --group dev --locked`. A stale lock is a failed gate.
- Keep Cargo jobs at most two. Run `./scripts/install-hooks` once per clone.

## Change a dependency

1. Prefer upgrading to converge versions over adding an exception. The Windows duplicate
   crates went away by moving `sysinfo` to a series that shares `windows` 0.62 with the rest
   of the tree, not by skipping them.
2. Update one crate with `cargo update -p NAME --precise VERSION`, adapt the call sites, then
   run `cargo deny --locked --all-features check` and `cargo machete --with-metadata`.
3. A `deny.toml` skip is a last resort. It names the exact crate and version and cites the
   upstream constraint that forces the duplicate. Remove it as soon as the constraint lifts.
   Never add a broad tree exception or a permanent advisory ignore.
4. Check maintenance, license, platform support, and resource cost of any new package. The
   advisory check needs the RustSec database, so a network failure is unverified, not green.
5. A host that depends on these crates may carry its own lock. After a bump, tell the host
   session so it refreshes its lock, as in [harness-hosts](../harness-hosts/SKILL.md).

## Local environment traps

- **Disk full.** Build output is large. Set `CARGO_INCREMENTAL=0` for gate runs and delete
  `target/debug/incremental` before anything else. Never delete another project's files.
- **Moved folder.** A Python virtualenv records absolute paths. After moving or renaming the
  repo, run `uv sync --project sdk/python --group dev --locked --reinstall`.
- **Hooks.** The pre-commit hook runs the full gate. Fix the cause. Do not use `--no-verify`.
