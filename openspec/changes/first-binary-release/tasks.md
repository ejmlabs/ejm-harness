# Tasks

## 1. Version

- [x] 1.1 Set `0.1.1` in `VERSION`, `Cargo.toml` and pins, `registry/Cargo.toml`, both `Cargo.lock` files, both SDK manifests, `sdk/python/uv.lock`, and date a changelog section. Verify `grep` finds no stale owned version and the worker handshake reports `0.1.1`.

## 2. Build, smoke and package jobs

- [x] 2.1 Add `scripts/smoke-worker.py` and a build and smoke workflow for Linux x86_64 and macOS Apple silicon with `contents: read`, no secrets, `--locked`, archives and SHA-256 files. Verify the workflow has no write permission and no secret, and that every action is pinned by SHA.
- [x] 2.2 Add the SDK build workflow and the disabled publish workflow. Verify the publish workflow runs no project code and is disabled.
- [x] 2.3 Build the Linux worker locally, smoke the handshake, and run both SDKs against it through `EJM_HARNESS_WORKER_PATH`. Build both SDK packages locally and record SHA-256 values.

## 3. Notes and routes

- [x] 3.1 Draft the `0.1.1` release notes with the unsigned statement, the Linux baseline, the issue links and the open native evidence as known limits.
- [x] 3.2 Confirm the issue forms and the private advisory link exist and add only what is missing.

## 4. Acceptance

- [x] 4.1 `openspec validate first-binary-release --strict` and `./scripts/check` pass, or each skipped step is named.
