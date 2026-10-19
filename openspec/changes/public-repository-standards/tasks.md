# Tasks

## 1. Harness readiness fixes

- [x] 1.1 Align the device client, polling, auth service, consent UI, and docs on ejm-cli; verify client request fixtures and real auth rejection/race tests.
- [x] 1.2 Deny protected reads/listings through direct and symlink paths; verify focused coding-tool regressions and ordinary file operations.
- [x] 1.3 Include MIT text and repository metadata in SDK distributions and align Rust metadata; verify archive contents and package/version metadata.
- [x] 1.4 Preserve harness policy enforcement while accepting the six initialized OpenSpec skills; verify policy self-tests and routing.

## 2. Shared pipeline adoption

- [x] 2.1 Add uniform community docs, version/changelog, immutable shared workflow callers, and a separate registry gate; verify standards/version/workflow checks.
- [x] 2.2 Apply available GitHub branch, approval, Actions, and dependency protections; verify API readback.
- [x] 2.3 Fill missing registry npm dependencies before root npm checks with a locked install and disabled lifecycle scripts; update setup docs and verify fresh, existing, and failed-install paths without repeating native gates.
- [x] 2.4 Remove the package-install deadline test race by waiting for governor idle within five seconds before checking persistence and admission cleanup; verify repeated focused runs and the package crate tests with the existing isolated cache. This test-only correction has no public product documentation impact. Hosted acceptance remains pending.
- [x] 2.5 Guard the Unix-only first-paint benchmark before its imports on Windows, document the existing Linux/macOS runner requirement, and verify strict script typing for win32, Linux, and macOS, scoped Ruff checks, the runtime guard, shared standards, and OpenSpec. Hosted acceptance remains pending.

## 3. Verification

- [x] 3.1 Run focused tests, workspace gate and registry gate with at most two Cargo jobs, plus strict OpenSpec validation; record any unavailable evidence.

- [ ] 3.2 Verify human admission, self approval, stale commits, revocation, failed checks, missing secrets, and release tag refusal in GitHub; record live evidence before declaring public readiness.

### Local verification evidence

- `./scripts/check` passed on macOS arm64 at `bcd01774bdfe1df6ec679e3d0e3ce70da2491364`: 157 Rust tests, 15 TypeScript SDK tests, 16 Python SDK tests, 11 runtime tests, and four policy self-tests passed. One Rust subprocess fixture remains intentionally ignored. The complete lint, build, documentation, dependency, protocol generation, and SDK packaging checks passed.
- `./scripts/check-registry` passed at `e774a319de1224a11dcfb8fa311eb0c2f38434af` with the import-order correction in `registry/src/edge.rs`: two native Rust tests, 10 schema tests, and 24 Worker integration tests passed. Native and WebAssembly Clippy, dependency checks, types, schema generation, lint, formatting, audit, and both Worker dry-run builds passed. The separate auth-only run also passed nine tests.
- Both gates used Rust 1.99.1, Node 24.19.0, `CARGO_BUILD_JOBS=2`, and `CARGO_INCREMENTAL=0`. Registry builds used worker-build 0.8.7. Root and registry locked npm dependencies were installed before the gates.
- The registry setup fallback passed three focused paths on Node 24.19.0: an actual offline locked npm install before root checks, reuse of an existing dependency tree, and install failure propagation without running root checks. Other gate commands were stubbed for this fixture. Shell syntax, scoped Prettier, strict OpenSpec validation, and whitespace checks passed. Full native gates were not repeated for this setup-only change; fresh hosted acceptance remains required.
- The package-install deadline regression passed one focused Cargo run and 20 repeated compiled runs on macOS arm64 after adding the bounded governor-idle barrier. `cargo test --offline --locked -p ejm-harness-packages` passed all 32 package tests with Rust 1.99.1, Node 24.19.0, Python 3.15.0, `CARGO_BUILD_JOBS=2`, and `CARGO_INCREMENTAL=0`. The build used an isolated APFS clone of the existing cache. Rustfmt, scoped Prettier, and whitespace checks passed. The sandbox first blocked the local Wiremock port before the test ran; the local fixture passed with sandbox relief. Full native gates were not repeated for this test-only correction; hosted acceptance remains pending.
- Strict script mypy targeting win32 and Python 3.12 reproduced the eight hosted benchmark errors before the platform guard and passed afterward. The same three script files also passed strict Linux and macOS typing with Python 3.12. A simulated Windows call on macOS rejected before Unix imports or child creation. Installed project mypy 1.18.2 and Ruff 0.13.2 ran on Python 3.14.2; scoped Ruff lint/format, shared repository standards, scoped Prettier, strict OpenSpec validation, and whitespace checks passed. The uv launcher was blocked by the sandbox cache and then failed in macOS system configuration with a scoped offline cache, so the installed project executables ran directly. These focused checks do not constitute the full uv or native gate. `docs/benchmarks.md` now states the existing Linux/macOS runner limit; no new measurements or platform support are claimed. Hosted acceptance remains pending.
- Strict OpenSpec validation passed. Linux and Windows execution, live GitHub acceptance, deployment, and publication remain unverified. The import-order correction changes no public behavior, so it requires no public site documentation change.

## Workflow follow-up

- Sync completed specs and archive after all tracked work and required verification succeed.
