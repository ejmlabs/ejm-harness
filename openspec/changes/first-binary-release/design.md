# Design

## Context

Decisions come from the approved `first-public-releases` plan. This change covers
its tasks 2.1, 2.2, 3.2a, 3.4, 4.6 and the SDK packaging part of 5.6 for this
repository.

## Decisions

- **Version `0.1.1`.** A `v0.1.0` tag already exists on this repository and the
  draft flow refuses an existing tag. Versions are not lowered. `VERSION`,
  the workspace manifest and its internal pins, `registry/Cargo.toml` and its pin,
  both lock files that carry owned crates, both SDK manifests and `sdk/python/uv.lock`
  move together. The dated changelog section replaces `[Unreleased]`.
- **Trust split.** Build and smoke jobs hold `contents: read`, no secrets, use
  `--locked` and `npm ci --ignore-scripts`, and pin every action by full SHA. They
  upload workflow artifacts. The attach step is a separate shared workflow in the
  `.github` repository and is not added here. The build workflow runs on
  `workflow_dispatch` and on push to main, so its `head_sha` equals the commit the
  draft release tags.
- **Archives.** `ejm-harness-worker-<version>-<target>.tar.gz` holds the binary,
  `LICENSE` and `THIRD_PARTY_NOTICES.md`, with a `.sha256` file per archive and one
  `SHA256SUMS`. Targets use the `releases.json` names `linux-x86_64` and
  `darwin-aarch64`.
- **Linux baseline.** The Linux job runs on the oldest hosted image the project
  accepts (`ubuntu-22.04`, glibc 2.35) and installs `libdbus-1-dev`. The notes
  name `libdbus-1` and the glibc floor.
- **macOS held.** The macOS job builds and smokes the worker so evidence exists,
  but its artifact is named for evidence only. Notes and the future site list no
  macOS file until a notarization run is recorded. Worker command-line archives
  fetched with `curl` carry no quarantine flag, but the owner chose to wait.
- **SDK packaging.** One read-only build job runs the existing SDK tests and
  produces `npm pack` and `uv build` outputs with checksums. A separate publish
  workflow downloads those artifacts, runs no project code, and uses npm trusted
  publishing and PyPI Trusted Publishers through `id-token: write`. It has
  `if: false` until the owner confirms the names. No token appears in a file.
- **Smoke.** A small script starts the worker, sends `hello`, and checks the
  protocol and version. It is shared by the workflow and local runs. A second
  script step runs each SDK against the binary through `EJM_HARNESS_WORKER_PATH`.
- **Issue routes.** The bug form's `evidence` field already accepts a version and
  platform by query parameter, so no form field is added. Add the private advisory
  link to `feature.yml`-adjacent config only if missing.

## Risks

- Hosted runners may differ from local builds. Local builds record evidence;
  hosted results are unverified until a run.
- Open native evidence stays open and is named as known limits, pending owner
  approval.
