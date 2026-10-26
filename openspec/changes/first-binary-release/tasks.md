# Tasks

## 1. Version

- [x] 1.1 Set `0.1.1` in `VERSION`, `Cargo.toml` and pins, `registry/Cargo.toml`, both `Cargo.lock` files, both SDK manifests, `sdk/python/uv.lock`, and date a changelog section. Verify `grep` finds no stale owned version and the worker handshake reports `0.1.1`.

## 2. Build, smoke and package jobs

- [x] 2.1 Add `scripts/smoke-worker.py`, `scripts/package-worker.py` and `build.yml` following the shared native build contract, plus the canonical native `release.yml`. Verify the shared `native_build` lint accepts `build.yml`, no write permission, secret or tag-pinned action exists, and only one `release-files` artifact is uploaded.
- [x] 2.2 Park a draft SDK publish workflow outside the repository, disabled, until names are confirmed and the shared standard allows it.
- [x] 2.3 Build the Linux worker locally, smoke the handshake, and run both SDKs against it through `EJM_HARNESS_WORKER_PATH`. Build both SDK packages locally and record SHA-256 values.

## 3. Notes and routes

- [x] 3.1 Draft the `0.1.1` release notes with the unsigned statement, the Linux baseline, the issue links and the open native evidence as known limits.
- [x] 3.2 Confirm the issue forms and the private advisory link exist and add only what is missing.

## 4. Acceptance

- [x] 4.1 `openspec validate first-binary-release --strict` and `./scripts/check` pass, or each skipped step is named.
