# Changelog

Changes follow [Semantic Versioning](https://semver.org/). Each repository is versioned independently.

## [Unreleased]

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

### Changed

- The default package registry is `https://registry.ejmlabs.com`. The client,
  both Workers' `REGISTRY_ORIGIN`, and the GitHub OAuth callback use that origin.
  `meetgerry.ai` and its subdomains are rejected, redirects are not followed, and
  sessions saved for the retired origin are not sent or reused. Discard a leftover
  session with `ejm-cli package discard-retired-session` and sign in again. These
  notes do not deploy a registry, and they do not create or replace a tag or
  GitHub release. Builds from the earlier v0.1.0 tag (commit 8d3dc39) default
  to a retired registry domain and must not be used for registry commands.
  Upgrade to the reissued v0.1.0.

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
