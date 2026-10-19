# Design

## Context

At the start of this audit, the projects were private and main had no branch protection. Product workflows ran on every PR with failing remote runs, and the two hosts required a sibling harness checkout. All products currently use version 0.1.0. See proposal.md for motivation.

## Goals / Non-Goals

**Goals:** Share maintenance policy and trusted automation, keep each product independently versioned, and require a human to admit and approve the exact PR head.

**Non-Goals:** Publishing product releases, package uploads, installer signing, deployment, history rewrites, and buying private-repository security products during setup.

## Decisions

1. Use the user-selected ejmlabs/.github repository for reusable workflows, executable standards, templates, and runbooks. Consumers pin a reviewed full SHA. Product checks keep their established gate commands.
2. Use trusted default-branch issue-comment or manual dispatch entry points. A human maintainer issues `/ready <full-head-sha>` before quality or AI runs. New commits have no inherited readiness. Outsider and bot events do not launch jobs. Restrict GitHub fork workflows to approval by all external contributors and keep untrusted jobs read-only.
3. Keep readiness separate from `/approve <full-head-sha>`. The latter requires a current human maintainer and records native bot approval for self review where GitHub forbids native author approval. Main requires at least one native approval and the maintainer gate, plus required checks, resolved discussions, linear history, and no force pushes or deletion. Administrators do not bypass the merge requirements.
4. Run exactly two independent Codex passes using `gpt-6.1-sol` with medium reasoning, trusted prompts and configuration, read-only permissions, no PR build or test execution, and isolated model credentials. Post results in a separate job. Missing credentials or invalid output fail closed. Bound the diff, output, time, and concurrent runs. Check replayed admission identifiers before spending budget.
5. Keep a root VERSION and CHANGELOG.md in each product. Validate all releasable package versions. A common manual pipeline accepts an exact checked main commit and matching version, refuses existing tags, and creates only a draft release with source archives, checksums, and provenance. Document native binary and registry publication gates explicitly.
6. Keep licenses and brand notices distinct. Preserve each product's MIT license and bundled font licenses; include license text in SDK packages. The studio's design assets retain their existing ownership terms.
7. Enable available free repository protections and dependency alerts now. Do not enroll private repos into billed secret protection. Record the required public-only activation checks before any visibility change.

## Risks / Trade-offs

- Shared workflow changes affect several products. Immutable caller pins and reviewed update PRs bound rollout.
- Human readiness costs a small metadata action and can delay outside contributions. The expensive checks remain unavailable until deliberate admission.
- Private CLI dependency checkout requires reviewed harness source and a dedicated, expiring Contents: read credential for this repository. The user authorized private API publication and provided `EJM_HARNESS_TOKEN`; successful CLI source preparation does not prove its scope or expiry. Validate those separately. IDE access and adoption require its owner's separate authorization. New API consumers still need their own trusted pin and exact-source gates.
- API review needs a scoped OPENAI_API_KEY and provider budget settings. Setup does not prove a successful paid review.
- Local disk space is constrained. Coordinate Cargo checks, use focused existing caches, and report missing full platform evidence honestly.
- Source and history scans reduce known risk; partial scan coverage and platform-specific tests remain explicit.

## Migration Plan

Create and test the shared repository first. Add pinned consumers and version metadata, then apply and read back repository settings. Run the project gates and first admitted PR through the new pipeline when the required credentials are present. Keep repositories private until blockers are resolved. Roll back a bad workflow update by restoring the previous immutable caller pin.

## Harness adapter

Use ejm-cli consistently in device code, polling, consent UI, and real auth fixtures. Add request-body and unregistered-client regressions. Run the registry gate in an isolated CI job alongside the workspace gate. Apply protected path checks to reads and listings before returning data, including canonical symlink aliases. Include MIT text and repository metadata in language SDK distributions.

The root lint gate checks registry TypeScript, but the registry is outside the root npm workspace. Install both locked npm trees during setup. Before root npm checks, `scripts/check` fills an absent `registry/node_modules` with `npm --prefix registry ci --ignore-scripts` and stops if installation fails. Existing dependency trees are reused; rerun the locked install after dependency changes. This keeps the previous trusted caller's root-only setup compatible while preserving all lint coverage. Hosted acceptance remains pending.

The package-install deadline regression waits for `Governor::wait_idle()` within a five-second timeout before checking the empty package store and zero running or queued admissions. A deadline can return before the blocking install task drops its lease. This test uses the existing cleanup barrier rather than requiring an immediate lease release. Production ownership and public behavior stay unchanged, so this test-only correction has no public product documentation impact. Hosted acceptance remains pending.

The RSS and PTY benchmark runner already rejects non-POSIX systems in `main`. Guard `first_paint` with `sys.platform == "win32"` before its Unix-only imports so strict Windows type checking respects that boundary. Preserve Linux and macOS measurement behavior and keep all strict checks enabled. Clarify the existing Linux/macOS requirement in `docs/benchmarks.md`; this does not add platform support or new measurements. Verify strict script typing for Windows, Linux, and macOS plus a focused Windows-guard smoke test. Hosted acceptance remains pending.

The hook deadline fixture allows one second for the deadline case to reach its pending `before_model` hook after durable journal preparation. Five-second outer waits bound hook entry and completion, and name the phase and `block_end` mode if they expire. Keep the pending hook and observer, run-status, empty-provider, durable-completion, and observer-token cancellation assertions. Reproduce the old 20 ms failure with a temporary 100 ms SQLite write-transaction hold, verify the repaired fixture with the same hold, then remove the probe. This changes test timing only, with no product behavior or public site impact. Hosted Windows acceptance remains pending.

The tool-hook cancellation fixture also includes journal preparation before hook entry and durable reconciliation before `RunHandle::wait` completes. Its two unlabelled two-second waits cannot distinguish those phases on a slow host. Audit all hook-suite waits and use five-second entry/completion waits with hook-mode diagnostics, matching the other pending-hook fixture. A temporary probe with two separate 1,200 ms SQLite write-transaction holds reproduces the old entry timeout without exceeding either operation's two-second busy timeout; both cancellation modes pass with the repaired phase bounds. Remove the probe after verification. Preserve the entry notification, pending hooks, exact tool-effect counts, cancellation status, resumed provider history, confirmed result and artifact, and no-replay assertions. Production cancellation, journal durability, run deadlines, and observer deadlines stay unchanged. This test-only correction has no public product or site impact; fresh hosted Windows acceptance remains required.

## Linux harness caller prerequisite

Prepare only the canonical PR/release caller pair and release `standards-ref`
on protected harness main `ee1d2e3df5d77027be226d20830f6c5ea84d0d1d`, using
reviewed central PR #11 implementation `ae903129564926a14287f75d2012610c59cccb65`.
This central revision preserves protected main
`d8b31f1debf095b104f6de76193bf531ec4ddd2c`, retains the scoped CLI hook and
adds a fixed Linux-only `ejm-harness` quality step for `pkg-config` and
`libdbus-1-dev` before Rust setup. It introduces no harness-local action,
validator permission or product gate change. Earlier central targets are
obsolete history.

The separately prepared `publish-host-auth-apis` change needs those build
prerequisites. Hosted Linux run `118036420366` failed at missing `dbus-1.pc`.
Keep that API source outside this prerequisite. Preserve every product,
manifest, lock, test and tool input from the protected harness base. Required
central checks and exact-head human approval passed; protected merge
`8c304c8cdfc8b6fe50f3e442507215e615944f2b` has the reviewed tree
`91b7f5c0990e27f2e0fdfcce26aac18afdcf6b81`. The exact implementation is
retained at
`refs/heads/workflow-pins/ae903129564926a14287f75d2012610c59cccb65` under
active update/delete rule `25369476`, with no bypass actors and unchanged main
protections. The central prerequisite is complete. Publish the three-site
caller update through normal human admission and protected checks; harness
caller adoption and its hosted acceptance remain pending. Rebase and readmit the API source only after caller adoption,
with fresh locked Linux, macOS and Windows checks. Neither local macOS results
nor a caller-only pass establishes native key-service acceptance.

The protected base has no `publish-host-auth-apis` plan. This follow-up stays in
its existing repository standards change; the separate API plan is context,
not source imported into this candidate. Keep task 2.8 open until its required
verification and adoption pass. See [contributor CI setup](../../../docs/contributing.md#shared-ci-caller-prerequisite).

## Caller review source evidence

Both admitted AI reports for harness caller head
`28401e93b79824d13bda697b805ceca05ea65d04` explicitly said the previous
and proposed external workflow implementations were absent and review was
incomplete. Successful wrappers and zero findings do not complete review.
Task 2.8 and exact-source hosted acceptance remain open.

Include [exact plaintext source evidence](../../../docs/ci/shared-workflow-review/README.md)
for the two called workflows and their generator inputs in the normal consumer
source archive. Retain original paths, full revisions, Git blob IDs, SHA-256
digests and the MIT notice; share only byte-identical blobs. Text suffixes and
ordinary data modes keep the copies outside executable configuration and
product code. No caller, runtime, manifest, lock, tool or gate input changes.

Require the normal hook, fresh exact-head admission, Linux/macOS/Windows
quality, two complete reviews and explicit approval before protected adoption.
The evidence is source context, not completed review or native certification.
