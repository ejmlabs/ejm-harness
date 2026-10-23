# Adaptive capacity release handoff

Snapshot: 2026-10-22. This is an engineering handoff, not a release receipt.

## Scope and owners

This handoff covers the capacity/release work in this chat. It does not include
unrelated dirty primary-checkout work for studio, branding, chess, terminal
redesign, relay deployment or orchestrator features.

- Harness owns host admission, native ownership, provider budgets, persistence,
  learning, session contracts, worker and SDK defaults.
- CLI consumes public harness contracts and owns terminal views and host adapters.
- Shared ejmlabs/.github owns deterministic PR guidance.
- Consumers must not import private harness source or add their own scheduler.

## Authoritative sources

Read proposal.md, design.md, tasks.md, specs/ and verification.md in this
change before editing. Use the release branches, not the dirty Desktop
checkouts: the latter contain older capacity plans and unrelated newer work.

Implementation snapshots before this documentation-only handoff:
- Harness: ad14b7ee228954b7621e5e3b7497de7e8d1deba1.
- CLI: 755f059d69b228706ac022c3f6b3f2192e3e78c4.
- Shared guidance: 5367de31289d952bbf358b0311eff6f906baad68.

These are provenance references. Read the live PR head before any admission,
approval or merge; never copy an old SHA into a new command.

## Open PRs and merge order

Harness: https://github.com/ejmlabs/ejm-harness/pull/32 is the final integration PR.
Its existing review stack contains all capacity implementation:
1. #33 foundation.
2. #35 then #36 core source preparation.
3. #37 then #38 documentation and evidence.
4. #39 activation.
5. #32 final integration and version 0.2.0.

CLI: https://github.com/ejmlabs/ejm-cli/pull/30 is the final integration PR.
Merge #31 inert source review first, then #30 activation. Before activation CI,
pin .github/harness-revision to the final protected harness commit.

Shared guidance: https://github.com/ejmlabs/.github/pull/24.
Merge implementation and validator support, adopt its reviewed pin, then install
the guided caller. Manual generated PR-body blocks already exist; automatic
refresh is not deployed.

Retarget successors to main after predecessors merge. Keep the existing review
limits and protected checks. Do not squash the entire stack into an oversized
replacement PR, close existing PRs, or overwrite unrelated checkout work.

## Implemented and verified

- Adaptive shared host budget, bounded learning and CLI session views are in
  the release branches. macOS automatic defaults and version 0.2.0 are prepared.
  Linux and Windows retain legacy defaults.
- User-facing replay/fake responses were removed; internal fixtures remain.
- Filesystem source fork uses the exact public commit
  ad6faff2442ffe6abccd1856f4c0df9a3e472035 at
  https://github.com/ejmlabs/ejm-rightkit-fs.
  Upstream contribution: https://github.com/bogusyogi/rightkit/pull/876.
- Protocol 3 authenticates the native endpoint before credentials/job data.
  Proofs bind fresh nonce, epoch, domain and socket endpoints.
- Relative cwd now resolves in the caller; Windows compilation repairs and
  operation-scoped package lock release are included.
- Earlier representative macOS acceptance used 1,000 agents across two processes:
  median automatic throughput loss -1.1594%, median terminal overhead 1.8826%.
  Individual second-trial overhead was 8.29%; the declared median gate passed.
- That acceptance PREDATES protocol 3. It does not verify handshake overhead.
- The repair passed 17 focused native tests, strict executor Clippy, foundation
  all-target checking and a real shared-helper relative-cwd fixture.
  Windows native execution still requires CI evidence.
- Shared guidance passed 118 shared tests and generated-workflow checks.
- Both capacity changes passed strict OpenSpec validation on 2026-10-22.
  CLI validation also reported the archive issue below.

## Outstanding critical path

1. Obtain fresh human admission for repaired #33, #31 and shared #24.
   Their generated body blocks carry current commands. Earlier admission
   covered older commits. Agents must not impersonate human approval.
2. Resolve required CI/review failures against each admitted head, including
   Windows compilation/execution evidence. Do not infer green CI from local tests.
3. Recheck the changed native authentication path with the existing bounded
   representative benchmark. Preserve the declared thresholds and raw failures.
   Do not rerun unrelated matrices. No current protocol-3 performance pass exists.
4. Merge the harness stack through protected flow, then update and verify the
   CLI harness pin and merge the CLI stack.
5. Adopt shared guidance in the documented order.
6. A human maintainer dispatches Draft source release for protected main and
   version 0.2.0. Verify draft assets/checksums before authorized publication.
   Source release, registry publication, installers and site deployment are
   distinct outcomes. None is claimed complete here.

## OpenSpec completion and archive constraints

Harness tasks: 51/52 checked. Task 3.2 remains open for real Linux cgroup-move
evidence; Windows ownership also gates Windows automatic activation.
Do not archive the full cross-platform change as complete.

CLI tasks: 39/39 checked, but release work above is outstanding. Strict validation
warns that interactive-commands/spec.md modifies a missing target spec.
Resolve that baseline/delta mismatch through the OpenSpec workflow before sync
or archive. Do not invent completion by checking more boxes.

After required evidence and review, use openspec-sync-specs for verified
requirements and openspec-archive-change only when its completion conditions hold.
Retain unverified platform work explicitly.

## Resume commands

Run in each release checkout:
~~~sh
git status --short
openspec status --change adaptive-agent-capacity --json
openspec instructions apply --change adaptive-agent-capacity --json
openspec validate adaptive-agent-capacity --strict --no-interactive
~~~

Use the project's OpenSpec skills for implementation and plan revisions.
Cargo: CARGO_INCREMENTAL=0, CARGO_BUILD_JOBS=2. Preserve source, state, evidence,
frozen measurements and unrelated work. No broad cleanup or paid-provider tests.

## Evidence locations and limits

Checked-in docs/evidence and verification.md are the portable evidence index.
Additional raw local evidence may not exist on another host:
- /private/tmp/ejm-capacity-critical-comparison-20261021-integration/20261022T104153Z-a69f5df4/report.json
- /private/tmp/ejm-native-auth-tests.log
- /private/tmp/ejm-native-auth-clippy-final.log
- /private/tmp/ejm-relative-cwd.log
- /private/tmp/ejm-foundation-auth-check.log

Inspect available receipts before reuse. Do not treat missing raw files as a
fresh pass. Full local hooks were skipped under the user's focused-check scope;
protected CI remains required. This handoff changes no runtime or public API,
so no additional public product guide change is needed.

