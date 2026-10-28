# Changelog

Changes follow [Semantic Versioning](https://semver.org/). Each repository is versioned independently.

## [Unreleased]

## [0.1.1] - 2026-10-26

First public release of the worker and SDK sources. The `0.1.0` source notes below
stay as written. Nothing is signed or notarized, and no package is published by
these notes; see [RELEASING.md](RELEASING.md) and the known limits under
"Verification and limits" in the `0.1.0` section.

### Added

- A read-only CI workflow that builds and smoke-tests the `ejm-harness-worker`
  archive for Linux x86_64 and builds the TypeScript and Python SDK packages, each
  with SHA-256 values. A macOS Apple silicon build runs for evidence only; no
  macOS file is offered until it is notarized. The Linux archive is not signed or
  notarized, and its glibc baseline is recorded in the release notes after a hosted
  build. Registry publication of the SDKs is not set up yet.

### Changed

- **Breaking:** packages require `ejm.package.json` with no legacy alias. Rename
  earlier manifests and repack archives; their sizes and hashes change.
- **Breaking:** remove the default registry export. `install_source` now accepts
  `Option<&RegistryClient>`: registry sources need `Some`, while local, npm, PyPI
  and Git sources accept `None`. Hosts must coordinate the API update and select
  registry origins explicitly. Credentials remain isolated by exact origin.
- Give registry source its own identity and resource placeholders. No registry,
  package, worker binary or site is published by this source change.
- Retain immutable UI install metadata for static dependency review. This
  preparation selects no UI dependency and changes no product behavior.

### Security

- Override the registry's transitive undici to 7.30.1 (development and test
  tooling) and zod to 4.6.7 (also used by the auth worker through better-auth)
  for GHSA-4qv8-2p9j-2685, GHSA-f72v-6w45-q472, GHSA-xjj6-f33q-929c and
  GHSA-mrhx-c7xc-m3w5. No package, worker or site is published.
- Update the locked TLS dependencies to rustls-webpki 0.103.16 and
  webpki-root-certs 1.0.10 for RUSTSEC-2026-0382 and RUSTSEC-2026-0383.
  Source builds use the corrected certificate trust data. Public APIs and the
  existing install workflow are unchanged; no package or binary is published.

## [0.1.0] - 2026-10-19

Prepared development source for the first `0.1.0` release. These notes do not
announce a GitHub draft, package registry release, prebuilt worker or deployed
service. Follow [Releases](RELEASING.md) before creating or publishing artifacts.

### Added

- Shared Rust agent engine, bounded tool execution, coding tools, package runtime,
  worker protocol, and TypeScript and Python SDK source.
- Exact current-run completion subscriptions and clonable run handles, including
  internally queued successors and the atomic local-successor decision. Summary
  waits remain compatible, and detailed waits include bounded run-end observers.
- An encrypted credential vault, controlled registry-session migration, official
  Claude Code authentication and supervised structured inference, optional
  provider readiness, and idle provider replacement with durable continuation
  binding.
- Uniform repository guidance, explicit human admission, protected merge gates,
  two complete Codex reviews, and independent SemVer source-draft preparation.

### Fixed

- Align registry device login with the registered `ejm-cli` client. Deny protected
  repository and harness-state reads through file-tool paths and symlink aliases.
- Include the MIT license in both SDK distributions and keep owned Rust, registry
  and SDK versions aligned.
- Retire a Unix auth process-group identity after successful termination and
  reaping, so a later drop cannot signal a reused identifier. Cleanup errors and
  Windows Job ownership keep their existing behavior.
- Preserve original document and license bytes, including CRLF line endings,
  when validating source archives.
- Provider base URLs require `https`. Plain `http` is accepted only for exact
  `localhost`, an IPv4 address in `127.0.0.0/8`, or `::1`. `0.0.0.0`, LAN
  addresses, and other private addresses are rejected when the provider is
  constructed, before a connection or an API key is sent. The error does not
  include the API key. There is no opt-out. Worker protocol 1 and the generated
  schema are unchanged. Plain `http` loopback requests do not use `HTTP_PROXY`
  or `ALL_PROXY`. `https` still uses the process proxy. `VERSION` stays `0.1.0`.
  Everett recreates the
  unpublished `v0.1.0` tag at the protected `main` commit after this change
  and the registry move land.

### Verification and limits

- The installed shared caller passed Linux, macOS, Windows, registry and standards
  checks in [PR #16](https://github.com/ejmlabs/ejm-harness/pull/16), with two
  complete Codex Sol 6.1 medium reviews. This establishes the checked source and
  caller path; it does not certify native key services, live sign-in or inference.
- Host-auth API and process cleanup verification still has open work in the
  [verification record](docs/host-auth-verification.md). Native platform and host
  acceptance remain separate from fixture and hosted caller results.
- The SDKs require a separately built compatible `ejm-harness-worker`; they do not
  bundle or fetch prebuilt executables. Package registry publication, worker
  distribution and registry deployment each need their own release gates.
- `v0.1.0` was re-pointed from `8d3dc39` to the protected main commit recorded in provenance.json
  before publication. Discard archives from the withdrawn draft
  `untagged-d41ae33f280d24c8daa7`.
