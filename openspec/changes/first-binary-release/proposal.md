# Proposal

## Why

The workspace `0.1.0` source tag predates ten commits and an `[Unreleased]`
changelog section, so the first public release of this repository is `0.1.1`.
Nothing builds the `ejm-harness-worker` binary or the SDK packages in CI, so a
visitor cannot install either. The release plan `first-public-releases`
(ejmlabs repository) approved Linux x86_64 and macOS Apple silicon worker
archives with SHA-256 values, publish-ready SDK builds, and prefilled issue
routes.

## What Changes

- Set every owned version to `0.1.1` and date a changelog section.
- Add a read-only build and smoke workflow for the worker (Linux x86_64 and macOS
  Apple silicon), producing archives and a SHA-256 list as workflow artifacts.
- Add a read-only SDK build workflow (npm tarball, Python sdist and wheel) and a
  separate publish workflow that runs no project code and stays disabled until
  the owner confirms package names and registry trust settings.
- Hold macOS downloads until notarization (owner choice, Decision 8 option b).
  The macOS build job runs for evidence only and the notes offer no macOS file.
- Draft `0.1.1` release notes with the unsigned statement, the Linux runtime
  baseline and the open native-evidence items as known limits.
- Add the issue form links and the private advisory link where missing.

## Out of scope

Tags, releases, registry publication, deployment, notarization, Windows builds,
and the attach workflow (owned by the shared `.github` repository).

## Impact

No public Rust API changes. The worker handshake reports `0.1.1`.
