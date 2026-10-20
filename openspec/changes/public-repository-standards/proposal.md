# Proposal

## Why

The harness has no branch protection or common release process, and its registry client and server disagree on the device client identifier. Its registry tests and package license files must be part of public release checks.

## What Changes

- Adopt the shared workflows and standards in `ejmlabs/.github`, pinned to immutable commits.
- Require a human maintainer to admit each exact PR commit before quality checks or AI review can run, with bounded work and no automatic reruns.
- Require a PR, current maintainer approval, resolved conversations, and passing quality, standards, and two Codex review passes before merging.
- Allow a maintainer to approve their own current commit explicitly, without bypassing checks.
- Standardize contributor, security, support, license, documentation, and dependency maintenance guidance.
- Add independent semantic versions, release notes, version consistency checks, immutable tags, checksums, and a manual draft release pipeline that preserves original source and notice bytes.
- Preserve existing work and keep publication, package uploads, deployment, and paid security enrollment separate from setup.
- Align the registry device client and consent UI with ejm-cli and cover both device flow requests in tests.
- Include the registry gate in CI, preserve license text in SDK archives, align package metadata, and deny protected state reads through file tools.

## Capabilities

### New Capabilities

- `repository-governance`: Consistent contribution, maintainer review, security checks, and repository policy.
- `semantic-releases`: Independent SemVer releases with checked metadata and reproducible source artifacts.
- `registry-device-flow`: A registered ejm-cli device client is accepted consistently by the client, auth service, and consent UI.
- `coding-path-policy`: File tools reject protected repository and harness state, including symlink aliases.

### Modified Capabilities

None. OpenSpec was initialized for this work and has no existing capability specs.

## Impact

Registry client, auth Worker and fixtures, coding tool path validation, SDK package metadata, policy checks, workflow callers, docs, and GitHub settings.
