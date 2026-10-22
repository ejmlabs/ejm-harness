# Filesystem fork provenance

This directory preserves the original upstream archive, patch and source
manifest provenance. The reviewed source and its MIT OR Apache-2.0 licenses
are maintained in the public fork pinned by Cargo.toml.

## Source and scope

| Record                                                        | SHA256                                                             |
| ------------------------------------------------------------- | ------------------------------------------------------------------ |
| crates.io `rightkit-fs-0.4.4.crate`                           | `32e10ada6eb1323be113c4a7ee181a877cf3a1698bf66e1d278362e439f0501a` |
| [Exact patch](patches/rightkit-fs-0.4.4-direct-create.patch)  | `851acf4fdcfdb3b6c8f34a550893f9bcc3395e8bb7ad980700b23a8ba4fd9b7a` |
| [Baseline and patched file manifest](rightkit-fs.source.json) | `67788ced93a08f85550a4b1326263a7f91da30fb91c0312a3823e7e88f638c4e` |

The [provenance record](rightkit-fs.provenance.json) states the origin, scope,
dependencies, publication gate and removal condition. The manifest records all
76 baseline and patched files. The patched tree contains 1,255,377 bytes,
24,230 more than the archive's extracted files. Production `src/` grows by
8,301 bytes. These are source sizes, not binary or runtime overhead measurements.

The patch adds two public operations:

- `StateDir::create_new_empty(name)` exclusively creates a permanent empty file
  under its final name relative to the pinned directory and returns the verified
  handle. It uses no temporary hard link and never replaces an existing entry.
- `ReadOptions::require_owner_only(true)` checks ownership and private access
  on that open handle. Native lock reopening also requires one link. Default
  reads retain their previous behavior.

A failed post-create check retains the visible final name. Neither that name
nor one link proves private access, so later strict opens must check privacy
again. No repair, deletion, file sync, directory sync or power-loss guarantee
is added. Windows permanent-lock creator handles omit DELETE access. Unix mode,
owner and macOS ACL checks, plus the existing strict Windows DACL predicate,
remain enforced. These are snapshots and do not stop later changes by trusted
same-user code.

The upstream dependency manifest is unchanged. The production `fs` feature
still resolves only `libc` and `thiserror`; there is no new dependency or
production background task. The new creation hook uses the upstream debug-only
`testing` feature. Harness enables it only through the explicit
`ejm-harness-exec/native-test-seams` feature. Ordinary builds and release tests
do not select it. The dependency refuses fault injection in release builds.

## Why the local source is needed

The full harness gate reproduced a concurrent reader rejecting two hard links
while the old `StateDir::write_new` publication was incomplete. Creator death
could retain that state. The reviewed 0.4.4 and 0.4.6 APIs lack direct private
creation under a pinned `StateDir`; path-based creation and atomic writers do
not preserve the same permanent-name, directory and privacy contract.

The workspace now selects the maintained public fork
[`ejm-rightkit-fs`](https://github.com/ejmlabs/ejm-rightkit-fs) at full commit
`ad6faff2442ffe6abccd1856f4c0df9a3e472035`, version `0.4.4-ejm.1`.
The Rust dependency alias remains `rightkit-fs`. CLI and IDE path consumers
inherit this same pin. The fork preserves all 69 reviewed source and test files
byte for byte. The retained manifest records the original repair; the source is supplied
by the public fork rather than duplicated in this repository.

## Verification

The exact Git revision and Cargo locks bind the dependency source. The retained
manifest records the original imported files for comparison with fork history.

The [focused macOS receipt](../docs/evidence/native-stable-lock.macos-arm64.json)
records the exact commands and tested source hashes. It covers peer access at
first visibility, a real creator death at that boundary, retained identity and
lock reuse, actual mode/macOS ACL refusal, the original creator race and
hard-link refusal. It also checks ordinary release tests, strict executor
Clippy and worker compilation. Windows/Linux native behavior and broader
consumer, scale and automatic-default acceptance remain open.

Run the deterministic debug fixtures with:

```sh
cargo test --locked -p ejm-harness-exec --lib --features native-test-seams \
  native::tests::stable_locks:: -- --test-threads=1
```

Run ordinary release fixtures without `native-test-seams`. Do not use that
debug fault-injection feature in release measurement builds.

## Publication and upstream migration

Source and native builds resolve the exact public Git commit. Cargo registry
publication remains gated until `ejm-rightkit-fs` is published to crates.io or
an equivalent upstream registry release is verified. Cargo strips Git sources
from published dependency manifests, so a Git pin alone cannot satisfy that gate.

[Upstream PR #876](https://github.com/bogusyogi/rightkit/pull/876) tracks the
original API contribution. Switch back after equivalent concurrency,
creator-death, privacy and consumer checks pass. Retain licenses, provenance
and verification history through that migration.
