---
name: ejmlabs-rust
description: Use when writing, reviewing or building Rust in an ejmlabs repo, including .rs files, Cargo.toml, Cargo.lock, rust-toolchain.toml, deny.toml, Clippy findings, errors, async and cancellation, limits, Rust tests, benchmarks, dependency or toolchain bumps, or a target directory filling the disk.
---

# Rust at ejmlabs

Strict lints pass with no new exceptions, every failure is a `Result`, every wait can be
cancelled, every buffer has a bound, and the gate builds locked.

## Errors and panics

- Production code never uses `unwrap`, `expect`, `panic!`, `todo!`, `unimplemented!`,
  `unreachable!`, `dbg!`, `process::exit` or `mem::forget`. The lint layer denies them.
- Return `anyhow::Result`. Add `.context(...)` naming what failed; use `bail!` and `ensure!`.
- When a caller must branch on a failure, define a small type that implements `Display` and
  `std::error::Error`, and match it with `downcast_ref`. A public error enum is `#[non_exhaustive]`.
- Never drop an error's source, swallow an error, or turn an unknown failure into success.
  Convert to a framework's error shape only at that boundary.
- Recover a poisoned mutex in one crate-private `lock` helper that returns
  `mutex.lock().unwrap_or_else(PoisonError::into_inner)`, not at each call site.

## Design

- Make illegal states unrepresentable with ownership, enums and `Result`. Use an enum, not a
  boolean mode flag. Name repeated limits, timeouts and file names as constants.
- Add a trait only at an external boundary or for a real second implementation. No dependency
  injection container, factory hierarchy, or trait for a single type.
- Give each state transition one owner. Lock logic lives in the state type's methods, so
  callers never reach into `state.field.lock()`. Adding an option preserves the old default.
- Protocol parsing, HTTP, cryptography, file traversal and terminal rendering come from
  maintained crates; custom code is a thin adapter that applies our bounds and cancellation.
  Record any exception with alternatives and evidence.

## Code layout

- A `foo.rs` root with a `foo/` folder of submodules. Never `mod.rs`.
- Module root order: `mod` lines, `use` imports, `pub use` re-exports, then shared constants.
  Import explicit names; globs only in test modules.
- `pub(super)` for siblings, `pub(crate)` across the crate. A split never widens the public API,
  and the old module re-exports what moved only while a caller outside this change still uses it.
- Generated files are checked by their generator, never edited by hand.

## Async, cancellation and bounds

- Every wait and spawned task ends when its caller cancels or drops. Race a `CancellationToken`
  with `tokio::select! { biased; ... }`, cancellation first.
- Bound every queue, buffer, read, output, retry, step count and process lifetime. Use
  `mpsc::channel(n)`, never `unbounded_channel`. Cap bytes before you allocate them. A bound
  must hold with a hundred sessions, and a truncated result says it was truncated.

## Lints and unsafe

- Paste the org strict lint tables into `[workspace.lints]`; members set `workspace = true`
  under `[lints]`. A repo may be stricter, never looser. Fix the cause. An exception is
  `#[expect(lint, reason = "...")]` on the smallest item, tied to a stable contract or a measured
  tradeoff. Never `allow` a group, lower a level, or skip a tool to pass.
- No unsafe code. Use safe wrappers such as `rustix` for native calls. A needed exception names
  the package gap, carries a `// SAFETY:` comment and a platform test plan.

## The Rust gate

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
cargo machete --with-metadata
cargo deny --locked --all-features check
```

- Only on a constrained shared machine, run Cargo with `CARGO_BUILD_JOBS=2` (and
  `CARGO_INCREMENTAL=0` for full runs). Do not commit a jobs cap; CI uses every core.
- Disk full: delete this repo's `target/debug/incremental`, then `cargo clean` in this repo.
  Never delete another project's files or a shared target directory. With sibling path
  dependencies see the toolchain reference before `cargo fmt`.
- The repository's gate adds its own steps. Run the gate, not a subset, before done.

## Tests

- Tests make no network or paid model calls: use replay providers, Wiremock and local
  subprocess fixtures. Every wait has a timeout; timer tests use `start_paused = true`.

## Read next

- [references/async-and-bounds.md](references/async-and-bounds.md): tasks, channels, child
  processes, shutdown, timeouts, limits.
- [references/toolchain-and-dependencies.md](references/toolchain-and-dependencies.md):
  toolchain pins, dependencies, `deny.toml`, lint tables, unsafe code, forks.
- [references/testing-and-performance.md](references/testing-and-performance.md): Rust tests,
  fixtures, measuring and claiming performance.
