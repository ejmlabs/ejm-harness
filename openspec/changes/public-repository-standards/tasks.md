# Tasks

## 1. Harness readiness fixes

- [x] 1.1 Align the device client, polling, auth service, consent UI, and docs on ejm-cli; verify client request fixtures and real auth rejection/race tests.
- [x] 1.2 Deny protected reads/listings through direct and symlink paths; verify focused coding-tool regressions and ordinary file operations.
- [x] 1.3 Include MIT text and repository metadata in SDK distributions and align Rust metadata; verify archive contents and package/version metadata.
- [x] 1.4 Preserve harness policy enforcement while accepting the six initialized OpenSpec skills; verify policy self-tests and routing.

## 2. Shared pipeline adoption

- [x] 2.1 Add uniform community docs, version/changelog, immutable shared workflow callers, and a separate registry gate; verify standards/version/workflow checks.
- [x] 2.2 Apply available GitHub branch, approval, Actions, and dependency protections; verify API readback.

## 3. Verification

- [x] 3.1 Run focused tests, workspace gate and registry gate with at most two Cargo jobs, plus strict OpenSpec validation; record any unavailable evidence.

- [ ] 3.2 Verify human admission, self approval, stale commits, revocation, failed checks, missing secrets, and release tag refusal in GitHub; record live evidence before declaring public readiness.

### Local verification evidence

- `./scripts/check` passed on macOS arm64 at `bcd01774bdfe1df6ec679e3d0e3ce70da2491364`: 157 Rust tests, 15 TypeScript SDK tests, 16 Python SDK tests, 11 runtime tests, and four policy self-tests passed. One Rust subprocess fixture remains intentionally ignored. The complete lint, build, documentation, dependency, protocol generation, and SDK packaging checks passed.
- `./scripts/check-registry` passed at `e774a319de1224a11dcfb8fa311eb0c2f38434af` with the import-order correction in `registry/src/edge.rs`: two native Rust tests, 10 schema tests, and 24 Worker integration tests passed. Native and WebAssembly Clippy, dependency checks, types, schema generation, lint, formatting, audit, and both Worker dry-run builds passed. The separate auth-only run also passed nine tests.
- Both gates used Rust 1.99.1, Node 24.19.0, `CARGO_BUILD_JOBS=2`, and `CARGO_INCREMENTAL=0`. Registry builds used worker-build 0.8.7. Root and registry locked npm dependencies were installed before the gates.
- Strict OpenSpec validation passed. Linux and Windows execution, live GitHub acceptance, deployment, and publication remain unverified. The import-order correction changes no public behavior, so it requires no public site documentation change.

## Workflow follow-up

- Sync completed specs and archive after all tracked work and required verification succeed.
