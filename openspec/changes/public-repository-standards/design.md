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

The root lint gate checks registry TypeScript, but the registry is outside the root npm workspace. Install both locked npm trees during setup. Before root npm checks, `scripts/check` fills an absent `registry/node_modules` with `npm --prefix registry ci --ignore-scripts` and stops if installation fails. Existing dependency trees are reused; rerun the locked install after dependency changes. This keeps the previous trusted caller's root-only setup compatible while preserving all lint coverage. Hosted non-AI acceptance is recorded below.

The package-install deadline regression waits for `Governor::wait_idle()` within a five-second timeout before checking the empty package store and zero running or queued admissions. A deadline can return before the blocking install task drops its lease. This test uses the existing cleanup barrier rather than requiring an immediate lease release. Production ownership and public behavior stay unchanged, so this test-only correction has no public product documentation impact. Hosted non-AI acceptance is recorded below.

The RSS and PTY benchmark runner already rejects non-POSIX systems in `main`. Guard `first_paint` with `sys.platform == "win32"` before its Unix-only imports so strict Windows type checking respects that boundary. Preserve Linux and macOS measurement behavior and keep all strict checks enabled. Clarify the existing Linux/macOS requirement in `docs/benchmarks.md`; this does not add platform support or new measurements. Verify strict script typing for Windows, Linux, and macOS plus a focused Windows-guard smoke test. Hosted non-AI acceptance is recorded below.

The hook deadline fixture allows one second for the deadline case to reach its pending `before_model` hook after durable journal preparation. Five-second outer waits bound hook entry and completion, and name the phase and `block_end` mode if they expire. Keep the pending hook and observer, run-status, empty-provider, durable-completion, and observer-token cancellation assertions. Reproduce the old 20 ms failure with a temporary 100 ms SQLite write-transaction hold, verify the repaired fixture with the same hold, then remove the probe. This changes test timing only, with no product behavior or public site impact. Hosted Windows acceptance is recorded below.

The tool-hook cancellation fixture also includes journal preparation before hook entry and durable reconciliation before `RunHandle::wait` completes. Its two unlabelled two-second waits cannot distinguish those phases on a slow host. Audit all hook-suite waits and use five-second entry/completion waits with hook-mode diagnostics, matching the other pending-hook fixture. A temporary probe with two separate 1,200 ms SQLite write-transaction holds reproduces the old entry timeout without exceeding either operation's two-second busy timeout; both cancellation modes pass with the repaired phase bounds. Remove the probe after verification. Preserve the entry notification, pending hooks, exact tool-effect counts, cancellation status, resumed provider history, confirmed result and artifact, and no-replay assertions. Production cancellation, journal durability, run deadlines, and observer deadlines stay unchanged. This test-only correction has no public product or site impact. Hosted Windows acceptance is recorded below.

## Hosted recovery and caller acceptance

Harness PR #1 at `6da46394f29b3d0d1f068175b1715e8c6b048d23` passed Quality on Linux, macOS, and Windows, plus registry and repository standards, in [run 39239206945](https://github.com/ejmlabs/ejm-harness/actions/runs/39239206945). All six Windows hook tests passed, including both repaired fixtures. Both AI jobs stopped at the old caller's duplicate `--skip-git-repo-check` argument before model execution. The owner approved an AI-only recovery for that exact head. It merged as `ee1d2e3df5d77027be226d20830f6c5ea84d0d1d`, with the reviewed tree preserved and full main protection restored and verified equal to its prior snapshot. This recovery did not provide two AI reviews of the patch.

The initial local refresh generated both normal consumer callers at retained shared implementation `965cd684713ae91a7804745eddd452cc392af11d`. It clears inherited `GH_TOKEN` and `GITHUB_TOKEN` in the model steps while retaining the trusted verification steps' scoped tokens. Its alternate missing-key route is restricted to the shared repository; the harness keeps the exact normal caller. No harness public API, runtime, package metadata, dependency, or lockfile changes. The existing contribution and release guides already describe this workflow; the public site's product guidance is unaffected, and no site publication is claimed.

A fresh admitted harness PR must complete both actual `gpt-6.1-sol` medium passes with all main protections enforced. Shared PR #12 separately verified missing-key failure and revocation during active Quality work without model calls; it does not replace the harness's positive acceptance. Genuine outsider and bot cases and live release tag refusal still need evidence. Keep task 3.2 open and do not archive or declare public readiness yet.

## Completion guard and review source context

The admitted review of [harness PR #12](https://github.com/ejmlabs/ejm-harness/pull/12)
at `7a409665b4259135c85ca55c83bc5815cba237aa` ran through
[run 39244757531](https://github.com/ejmlabs/ejm-harness/actions/runs/39244757531).
Its trusted default caller came from
`ee1d2e3df5d77027be226d20830f6c5ea84d0d1d` and executed shared revision
`5fae851502c2e3c31d4df9a44f8d56c2cb49e098`. The proposed
`965cd684713ae91a7804745eddd452cc392af11d` caller did not run. Both model
summaries reported incomplete reviews because the archive lacked the old and
new shared workflow implementations. The old validator accepted those outputs,
so their success statuses do not establish complete AI acceptance. The PR was
converted to draft at that stage. Existing human comments were preserved. The
later repair merged as protected harness main
`09b779ee0452dda07b7f52503be22e65fe238373`.

That admitted run finished with all three OS Quality jobs, registry, and
repository standards passing. Both AI jobs technically succeeded with the
incomplete outputs above. At 11:58:04 UTC, the final gate refused the draft PR
with `PR head changed, PR is a draft, or PR no longer targets main`. The deliberate
draft hold therefore blocked acceptance; it is not a product Quality failure.

Both normal callers adopted by protected main `09b779` select retained shared revision
`aa4144fa978d711a5e009b384718b4cf27cf87bf`. The shared schema requires
a boolean `complete`; it permits `false` so a reviewer can report a limit. The
trusted validator accepts only `complete: true` and rejects a missing, false, or
nonboolean value. Keep exact admitted-head and pass validation, two serial
`gpt-6.1-sol` medium passes, and all merge protections. Do not add a product
alternate route or treat the incomplete reviews as acceptance.

`ejmlabs/.github` owns `scripts/export_review_context.py`, the shared generator
for this source context. Export exact canonical Git blobs for the old and new
shared revisions into `.github/review-context/<oldsha>/` and
`.github/review-context/<newsha>/`, with the exporter manifest. Use its fixed
allowlist and enforce the combined 750,000-byte cap before writing. Verify the
manifest's commit, tree, file mode, blob hash, and size against the canonical
local Git objects. The exporter needs no network or private token. These
nonexecutable snapshots are data for offline static review only; never import
or execute them. Generate them only through the shared exporter, and remove the
obsolete pair during a later reviewed pin rotation.

The harness runs Ruff and Prettier over the repository root. Their discovery
includes the snapshot Python, JSON, Markdown, and YAML files, so exclude only
`.github/review-context` in `ruff.toml` and `/.github/review-context/` in
`.prettierignore`. Preserve checks for first-party scripts, SDKs, docs, and real
workflow callers. Verify file discovery before and after the exclusions and run
the affected static checks without formatting the snapshots. Other inspected
checks use named first-party paths and do not import or execute review data.

The shared repository owns the guard, exporter, and consumer templates. The
harness owns its normal pinned callers and review data; no runtime dependency
points from the shared repository back into a product. This repair changes no
product runtime, dependency, lockfile, site, or public API. It enforces the
existing governance requirement that incomplete reviews block merging. Product
guidance needs no public site change, and no publication is claimed. Verify exact
snapshots, pinned
standards and templates, actionlint, strict OpenSpec validation, and whitespace.
Local preparation exported 32 exact source blobs and a 9,067-byte manifest,
totaling 582,279 bytes. Canonical blob and manifest equality, inert file modes,
normal template and community standards, alternate-route rejection, actionlint,
strict OpenSpec, whitespace, and projected admission bounds passed. Ruff 0.13.2
discovery excludes all 12 snapshot Python files while retaining its 13 existing
first-party/config paths; its lint passed. Prettier 3.9.10 on Node 24.19.0 ignores
all 33 snapshot files, keeps the sampled product docs, manifests, and callers in
scope, and passes the changed callers and planning docs. The system Ruff 0.17.0
format check found an unrelated unchanged SDK README snippet; it is outside the
pinned check evidence and was not changed. No native or full product gate was
repeated. At that preparation stage, the replacement guard had not executed
through this product's default caller, and no new CI or model run had started.
That preparation did not establish hosted acceptance. Keep fresh human decisions,
complete hosted reviews, and the remaining acceptance cases under task 3.2.
Do not sync, archive, or declare public readiness yet.

## Combined caller reconciliation

PR #13 originally prepared Linux harness prerequisites from protected parent
`ee1d2e3df5d77027be226d20830f6c5ea84d0d1d` by selecting reviewed shared
implementation `ae903129564926a14287f75d2012610c59cccb65`. Protected shared
merge `8c304c8cdfc8b6fe50f3e442507215e615944f2b` preserves that reviewed
tree. Its fixed Linux `ejm-harness` quality step installs `pkg-config` and
`libdbus-1-dev` before Rust setup. The separate host-auth API candidate needs
these files: Linux job `118036420366` failed to find `dbus-1.pc`. Caller-only
results do not verify that API source or the native key service.

Both actual reviews of the earlier PR #13 head
`28401e93b79824d13bda697b805ceca05ea65d04` reported incomplete assessment
because the old and new shared workflow sources were absent. Successful
wrappers and zero findings did not complete review. Head
`8b29b6c0aa944ff51611a377a067180b8fd6c940` supplied those sources; both
actual reports in [run 39274768967](https://github.com/ejmlabs/ejm-harness/actions/runs/39274768967)
bind that head, report `complete: true`, and have no findings. Those reports
cover that old head, not this reconciliation. Fresh complete checks and
separate exact-head approval remain required.

Protected harness main advanced to `09b779` and adopted the newer completion
guard and canonical exporter at `aa4144fa978d711a5e009b384718b4cf27cf87bf`.
That revision lacks the earlier Linux setup step. Selecting `ae903` unchanged
would remove the newer completion guard. Preserve both through retained
combined implementation `c4f2873ef0eef5bc3c7d75cf71fa3489a911d134`, tree
`dc440b03159b377979b6b76dde119eec98aa75cb`. Its 16 canonical workflow,
script, review and template blobs match protected shared main
`d00d8bbc7231485eb918008c562d835f394e82ea`, tree
`41309fdb41562cfdae1a226ef1d52023f9cbfffe`. The builder equals `ae903`;
the prompt, schema, review validator and exporter equal `aa414`. The exact
workflow-pins ref is protected against update and deletion by active rule
`25369476`, with no bypass actors.

Preserve PR #13 ancestry with a normal merge of protected `09b779` before
changing the three caller pin sites together. Keep completed tasks 2.8 and 2.9
and their evidence. Add open task 2.10 for this adoption. Rotate only the
canonical `.github/review-context` pair from `5fae851`/`aa414` to the actual
`aa414`/`c4f2873` transition using the shared exporter. Remove the duplicate
`docs/ci/shared-workflow-review` files added only by the old unmerged caller;
Git history preserves that earlier review context. Keep the existing narrow
Prettier and Ruff exclusions and never execute or import snapshots.

Preserve all runtime, manifest, lock, tool, gate, permission, admission,
release and secret boundaries from protected harness main. No API source
belongs in this prerequisite. Reconcile and readmit the separate API and
vault patch only after caller adoption, with fresh locked platform checks.
This CI guidance has no public product-page impact and claims no site or
package publication. Preparation does not establish a full gate, hosted
acceptance or native certification for the new head.

## Byte-preserving release caller repair

Protected harness base `42e4f9cf33bd3525aec344cfdffabc251271f782` uses
`c4f2873ef0eef5bc3c7d75cf71fa3489a911d134` at both workflow callers and
the release `standards-ref`. Update those three sites together to retained
shared revision `6717b433c462e01abaed86e2118e4843827598cb`, tree
`c07344ae6f5d5f04408b5423857c5da3d41df896`. The shared owner supplies the
release validator and workflow implementation; the harness consumes its public
reusable workflows. Runtime code, SDK contracts, manifests, locks, product
gates, permissions and secret names remain unchanged. Dependencies still point
from consumers to the harness, never from the harness to a product.

The old release validator normalizes line endings when reading expected
documents. The repaired validator preserves bytes for archive comparison,
including original CRLF font notices. The shared positive
[draft pilot](https://github.com/ejmlabs/.github/actions/runs/39340565293)
and separate
[duplicate refusal](https://github.com/ejmlabs/.github/actions/runs/39341034364)
verified the retained implementation. They do not verify a harness release.

The target retains the fixed Linux harness D-Bus setup, the existing three-OS
quality and registry gate commands, and the complete two-pass review guard.
New CLI/IDE harness selection and UI-profile checks do not change this harness
profile. Optional UI acquisition reads only protected base metadata. This
harness has no `.github/ui-source.json`, so no UI checkout, archive or npm
install is added. Existing credentials and admission rules remain in place.

Rotate only `.github/review-context` with the shared exporter for the actual
`c4f2873` to `6717` transition. Verify its fixed allowlist, complete Git identity,
ordinary nonexecutable files and combined byte cap. Keep uniform root community
guides byte-identical to the target templates; put local release guidance in
`docs/contributing.md` and the changelog. No product API or page changes, so
the public site needs no content or build change. No package installation,
native/full product gate, model call, CI dispatch, tag, draft, package upload or
deployment is part of local preparation. Fresh exact-head human decisions and
all hosted merge gates remain required. Sync and archive only after the wider
change's remaining verification is complete.

## Prepare source version 0.1.0 notes

Consolidate the current `Unreleased` entries into dated `0.1.0` source notes,
with concise product changes and the remaining verification limits. The
harness owns these notes and consumes the shared release workflow; no product
or SDK interface, dependency direction, workflow pin, package version or
license bytes change. The SDKs still require a separately built compatible
worker. Registry packages, worker binaries and the live registry remain
separate publication and deployment work.

Reconcile installed-caller task 2.12 with PR #16 and its retained original
run. Keep the historical task 2.10 and host-auth API/native acceptance open
unless their own evidence satisfies them. Review source and contributor
guidance, pinned standards, local links, strict OpenSpec, whitespace and
unchanged non-document blobs. Full runtime suites need no local repetition for
these docs alone; fresh normal PR gates remain required. No public product
page or generated site output changes, and no live docs publication is claimed.

After normal review and merge, freeze the resulting main commit before asking
for an exact source-draft plan. Preparing dated notes creates no tag, draft,
binary, package or deployment. Do not sync or archive incomplete work.
