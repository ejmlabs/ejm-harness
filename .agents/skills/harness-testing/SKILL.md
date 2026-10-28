---
name: harness-testing
description: Choose, write, and run focused tests and fixtures for ejm-harness without paid models, and report what each run does and does not prove.
---

# ejm-harness testing

Read [the contributor contract](../../../docs/contributing.md) for the required gate. Write the
smallest test that fails when the changed behavior breaks, then run the focused command before
the full gate.

## Where tests live

| Area                                    | Location                                         | Focused command                                                                                    |
| --------------------------------------- | ------------------------------------------------ | -------------------------------------------------------------------------------------------------- |
| Engine runs, hooks, recovery, providers | `crates/ejm-harness/tests`                       | `cargo test -p ejm-harness`                                                                        |
| Admission and supervision               | `crates/ejm-harness-exec/tests`                  | `cargo test -p ejm-harness-exec`                                                                   |
| Coding tools                            | `crates/ejm-harness-coding/src` tool modules     | `cargo test -p ejm-harness-coding`                                                                 |
| Package install, registry client, hosts | `crates/ejm-harness-packages/tests`              | `cargo test -p ejm-harness-packages`                                                               |
| Manifest and archive validation         | `crates/ejm-harness-package-spec/tests`          | `cargo test -p ejm-harness-package-spec`                                                           |
| Credential store and renewal            | inline in `crates/ejm-harness-auth/src/tests.rs` | `cargo test -p ejm-harness-auth`                                                                   |
| Node and Python package hosts           | `crates/ejm-harness-packages/runtime`            | `python3 -B -m unittest discover -s crates/ejm-harness-packages/runtime -p test_hosts.py`          |
| TypeScript SDK                          | `sdk/typescript/test`                            | `npm run check`                                                                                    |
| Python SDK                              | `sdk/python/tests`                               | `uv run --project sdk/python --group dev --locked python -m unittest discover -s sdk/python/tests` |
| Policy checker itself                   | `scripts/check-policy.py`                        | `python3 scripts/check-policy.py --self-test`                                                      |
| Registry Workers                        | `registry/tests`                                 | `./scripts/check-registry`                                                                         |

## Rules

- No test calls a paid model or a live service. Use `ReplayProvider` for model behavior,
  Wiremock for HTTP and SSE, and local subprocess fixtures for execution. Do not write your
  own HTTP server or protocol parser for a test.
- Exercise the boundary that creates a state. A new recovery state needs a crash or
  disconnect at that boundary. A new limit needs a test at the exact limit and one past it.
  A new wait needs a cancellation test.
- Keep tests deterministic and bounded. Put a timeout on any wait, and clean up processes,
  temp directories, and tasks. Linux parser fixtures run on every host but do not prove Linux
  execution behavior.
- Test modules may allow `unwrap` and `expect`. Production code may not.
- SDK tests need the Rust worker. Build it with `cargo build -p ejm-harness-worker` and set
  `EJM_HARNESS_WORKER_PATH` when it is not on `PATH`.
- Generated wire types are checked, not edited: `python3 scripts/generate-protocol.py --check`.

## Report honestly

CI runs the gate on Linux, macOS, and Windows. A local run covers one platform. Say which
platform and which commands ran. A skipped or unavailable step is unverified, not green.
Never describe a partial run as the required gate.
