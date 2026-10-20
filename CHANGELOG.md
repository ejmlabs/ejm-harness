# Changelog

Changes follow [Semantic Versioning](https://semver.org/). Each repository is versioned independently.

## [Unreleased]

### Changed

- Provider base URLs require `https`. Plain `http` is accepted only for exact
  `localhost`, an IPv4 address in `127.0.0.0/8`, or `::1`. `0.0.0.0`, LAN
  addresses, and other private addresses are rejected when the provider is
  constructed, before a connection or an API key is sent. The error does not
  include the API key. There is no opt-out. Worker protocol 1 and the generated
  schema are unchanged.
- The published version for this break is not pinned as `0.2.0`. Everett is
  holding the unpublished `v0.1.0` draft until this change and the registry
  move land. Version assignment remains his decision.

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
  schema are unchanged. The published version for this break is not pinned as
  `0.2.0` and remains Everett's decision while the unpublished draft is held.

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
