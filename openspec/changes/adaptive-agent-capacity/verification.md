# Verification

Local source work on macOS arm64, through 2026-10-21 UTC (2026-10-20 locally).
Automatic-default activation and cross-platform acceptance remain gated.
No packages or sites were published.

This record retains earlier source checkpoints and failures. Their test counts,
timings and successful phases apply to the identified checkpoint, not to every
later edit. The refreshed required gate passed at 00:05:51Z, including both
positive coordinated SDK package cases. The sampler investigation and replacement
native matrix retain their earlier source scope on this Mac. Release CLI and
worker startup and control-state measurements now fit the provisional helper
charge here, as recorded below. Other targets and images and broader acceptance
remain open.
Task progress is 43/46 in the harness and 33/39 in the CLI.

## Current-source macOS native crash check

The existing `native_owner_smoke` example passed after an offline, locked,
two-job debug build on 2026-10-21 UTC. The build and fixture took 2.209 and
19.296 seconds; the complete wrapper took 22.090 seconds. This is correctness
evidence, not a performance comparison or helper-baseline calibration.

Report:
`/private/tmp/ejm-capacity-current-owner-recovery-20261021/20261021T042249Z-85ad0f2e/report.json`,
SHA-256 `ccceeaf1621139a8cea8094ebc834f74edfe6b9c604f690f382835c91d30da7e`.
Reviewed wrapper SHA-256:
`226ac401519d9c95846509818377155cbd277bced2e29761832b1fe9164b1647`.
The exclusive fixture copy matched the build output, SHA-256
`e7f54f89df101f567416328f8f915e97c46de1534604a18208f058a376bd6c45`,
15,007,840 bytes. The full Git-listed harness source manifest stayed unchanged
through the run; ignored dependencies and compiler bytes were not fully frozen.

The fixture asserted useful progress after idle/busy client and owner death,
paused-owner exclusion, fresh-grant progress, retained executor cancellation
and dropped-caller cleanup, spawn rejection, launcher death before/after helper
creation, inherited witness release and user-child noninheritance. It also
confirmed that a detached macOS descendant survived owner death. That result
preserves the monitored-recovery limit.

Workload and cleanup verdicts both passed. Both command children were reaped.
Across 796 observer samples, all 11 captured process birth identities ended,
all five retained original witness descriptors were released after their paths
were removed, and final discovery for the exclusive executable path was empty.
The wrapper sent no cleanup signals and recorded no observer errors or audit
requirement. Minimum sampled free space was 26,805,829,632 bytes.

Two processes exited with `ESRCH` before birth capture. The report records their
final absence from exact-path discovery and does not assert captured-identity
death for them. The external observer saw the before-spawn witness only after
release; the fixture directly checked its held/released states and reaped the
launcher. These sampling limits remain explicit. Neither empty discovery nor
direct-child reaping proves arbitrary descendant containment.

The fixture retains its internal 45-second timeout. Wrapper phase, cleanup and
180-second total guards are cooperative, not a hard wall-clock bound. A failed
or forced fixture with coverage gaps still requires audit; this successful run
does not validate those wrapper failure paths by fault injection. No task was
closed and no default was activated. Tasks 3.7 and 3.11 retain their remaining
platform, package and acceptance requirements. Existing public native-recovery
docs already state these limits, so this verification-only addition changes no
public runtime contract or site claim.

## Current release helper calibration and corrected pool

The separate [current-image index](../../../docs/evidence/native-helper-current-image-memory.macos-arm64.json.gz)
and [raw bundle](../../../docs/evidence/native-helper-current-image-memory.macos-arm64.tar.gz)
retain fresh 2026-10-21 UTC startup and control-state saturation trials after the
approved stable-lock repair. Three trials per release CLI and worker image
passed on macOS 26.5.2 arm64. The CLI image SHA-256 is
`79193b2d6de3032f6c74d74e8a4d4b466c9b4a4b4cbe0345ce91ef3bc1c1199e`;
the worker image SHA-256 is
`1ca0a09f388406318f66221c25b80be66bef4331c24fb85e46daa45d55376dcc`.

| Helper | Startup lifetime RSS maximum | Saturated control-state lifetime RSS maximum |
| ------ | ---------------------------- | -------------------------------------------- |
| CLI    | 7.625 MiB                    | 14.703125 MiB                                |
| Worker | 7.453125 MiB                 | 14.421875 MiB                                |

Startup held authenticated readiness for six command-free seconds. All six
saturation trials passed all 14 bounded stages, including 256 readiness clients,
256 completed entries plus eviction, 1,024 prepared scopes and 32 incomplete
handshakes. No native commands ran in those trials. Clients were reaped, exact
helper identities ended and original witnesses released. Saturation cleanup
retains its inode proof when the fixture removes the temporary witness path.
The 32 MiB estimate stays unchanged. These kernel lifetime RSS maxima do not
calibrate other images or targets, command descendants or full process trees.
They establish no controlled cold/warm speed comparison.

Startup report SHA-256:
`9ae5cccf1f43f9738d07e9eec09a38e7e03a7fdfda6e3b6d5d2eb70c1ef2f48b`.
Saturation report SHA-256:
`24bd6f3f26bc25297b764f490860e19c2595eb9459a5e085c3a551e258c1a3ac`.
The index maps both to their exact original reports, manifests, build receipts,
observers, bounded samples and logs in the bundle.

The corrected benchmark pool passed after confirming all eight intended helper
commitments, identities and held witnesses before starting the ninth claimant.
Eight participants completed 6,400 commands; the host ancestor held eight 32 MiB
commitments and eight processes, and the ninth failed actual pool admission.
Twenty sequential scans spanned 4.753665 seconds. Helper, client and combined
RSS scan-sum high-waters were 67.984375, 190.890625 and 258.875 MiB. These are not
simultaneous or complete process-tree peaks. All nine clients were reaped; all
eight helper identities ended and witnesses released, with no audit required.
The benchmark image SHA-256 is
`a7b83ef181af4e2fb045d3ce174d4c8a18cb8d9fa7b2932d66a5493489493744`;
this does not prove eight saturated CLI or worker helpers. Pool report SHA-256:
`ca520a09b6b91965e9ebc5ce8009878df2c9d8d0fed390ed8dcfc1c2e1a56514`.

The earlier failed pool allowed the ninth claimant to reserve a pending slot
before one intended client. Its failed verdict, cleanup audit, proved ordering
diagnosis and barrier correction remain retained. A separate retry stopped at
source preflight before loading the observer or launching fixtures. Fresh build
receipts then reproduced all four earlier executable hashes. That equality
links image identities; it is not another memory trial. The new pool build
provenance stays separate from the earlier startup/saturation build provenance.
The runtime source comparison passed; `docs/contributing.md` changed within the
pre-existing exclusion and its exact delta remains recorded. This is not a
full-source-stable claim.

No task closes from these bounded results. Tasks 3.11 and 8.4, native platform
and crash acceptance, safe static sweeps and automatic defaults remain open.
Earlier full-gate and browser results below retain their own source scope.
The final docs checks below cover these public-doc edits.

## Current-image CLI useful-progress checks

The fresh CLI V5 checkpoint at
`/private/tmp/ejm-cli-capacity-fixture-checks-20261020/20261021T033611Z-36545fb5/report.json`
passed strict CLI Clippy, release helper/fixture builds, configuration and
full-envelope checks and both 100-agent shared-root smoke modes. SHA-256:
`56e83e6347d9d655ceee60025d86f891435e3ff37a4e7d361affe403ddbbc028`.
All commands and observers were reaped. Both repository source checks passed
without excluded changes.

Separate fixed and automatic 120-second headless diagnostics then passed with
two processes managing 4096 agents each under the same complete hard envelope,
including two model requests and two native jobs per process. Fixed completed
1657/1857 turns and automatic completed 1363/2377 turns; each participant
completed 4096 tools with zero active or drain failures. No repeat turns
completed. Each participant's 4096 pending turns, including replacement turns,
were fully accounted for as drain cancellations. Completed initial-turn
latencies remain censored by the work window. These are useful-progress
checks, not full-population completion or throughput acceptance.

Each case used CLI helper SHA-256
`79193b2d6de3032f6c74d74e8a4d4b466c9b4a4b4cbe0345ce91ef3bc1c1199e`
and fixture SHA-256
`66ed13c51c7136a57d4021526dcf7fab538dcc05cf2bf09bc6a079caa26367c2`.
All exact client/helper identities ended, witnesses released and observers
were reaped, with no audit required. Source and runner/image checks passed
without excluded changes. The observer cap stayed at 120 seconds. Reports:

- Fixed: `/private/tmp/ejm-cli-capacity-comparison-20261020/20261021T033818Z-d6d10718/report.json`,
  SHA-256 `c7592e12d70681717629247b91381fd8ca59db9326189ec0ca48a7ff8d1e649d`.
- Automatic: `/private/tmp/ejm-cli-capacity-comparison-20261020/20261021T034125Z-2e364127/report.json`,
  SHA-256 `bd4a64e4f9c790832762b7e2b36681b600a029ba7ea606e9a777100ecf4dca63`.

The CLI's separate current-image evidence index and verification record retain
the exact limits, rate/latency samples, approximate overlap, build receipts
and raw bytes. One sequential case per mode establishes no causal speed
improvement, safe static optimum, UI overhead, steady state, settled idle CPU
or full process-tree peaks. Tasks 3.11 and 8.4, the full crash/platform matrix
and automatic defaults remain gated. No task closes.

## Final docs checks after current-image evidence

Both sites passed serialized `npm --prefix site run check` and
`npm --prefix site run budget` after the current-image helper and CLI evidence
updates. Full captured source, symlinks, HEAD and index stayed unchanged during
each gate; all direct children were reaped. Receipts:

- Harness: `/private/tmp/ejm-capacity-consumer-required-gates-20261020/20261021T034615Z-67fe5002-harness-site/report.json`,
  SHA-256 `eb1d3e0b0c49ebcafbce6d923dae2bf87603195a587bd435db196aa5218c77c6`.
- CLI: `/private/tmp/ejm-capacity-consumer-required-gates-20261020/20261021T035335Z-bcf5a23d-cli-site/report.json`,
  SHA-256 `f601bb2b53a46417a2ae97f5285dbf69fdd03dc133e46a6c4fde151b812b9f91`.

Fresh local browser QA passed all seven derived gates: 16 layout captures,
32 behavior contexts, no tab-chunk requests on plain docs, both performance
reports, source/dist stability and cleanup. Coverage was the harness developer
guide and CLI developer guide, reference and benchmarks, with phone/desktop,
light/dark and scripts enabled/disabled. No console errors/warnings, page errors
or unexpected request failures remained. The raw report retains 116 same-origin
JavaScript CSP blocks with scripts disabled and 16 cancelled home requests
whose final destination and heading rendered. Their internal cancellation cause
is unproved. V4 corrects the capture tool's integer failure-count interpretation;
the failed V3 summary and all raw bytes remain unchanged. Independent read-only
review found no hidden behavior or performance failure.

Three throttled mobile loads per route and mode retained the unchanged 95 KiB
(97,280-byte), header-inclusive JavaScript wire budget:

| Site    | Route                         | JavaScript wire bytes |
| ------- | ----------------------------- | --------------------: |
| Harness | `/docs/developer-guide/`      |                95,141 |
| CLI     | `/docs/developer-guide/`      |                96,861 |
| CLI     | `/docs/reference/cli/`        |                96,950 |
| CLI     | `/docs/reference/benchmarks/` |                96,876 |

The tightest route has 330 bytes of headroom. CSS, font and HTML budgets also
passed; sampled CLS and TBT were zero. All direct phase/server children were
reaped and Chrome closed. Minimum sampled free space was 24,919,728,128 bytes.
The V4 summary is
`/private/tmp/ejm-capacity-site-final-qa-20261020/20261021T035358Z-37217c58/qa-summary.v4.json`,
SHA-256 `b843d9902b6ca53851827ce755cad171c56870e2bbea33c1bf514e50eaa69688`.

The separate linkage verifier passed final log receipts, serialized chronology,
full gate-to-QA source equality, stable QA dist manifests and all six shared-UI
file pins from the earlier owner receipt. Linkage:
`/private/tmp/ejm-capacity-current-docs-linkage-20261021/linkage-20261021T035358Z-37217c58.json`,
SHA-256 `cdb396696b3fc8af66a4bcc01aa496c69594f7bb8e7731be227876c528374146`.
Dist was first captured at QA start, so the receipt cannot prove that built
files were untouched between build exit and that capture. Ignored dependency
trees are not fully frozen.

Root reviewed eight viewport content images covering all four routes, both
sizes and themes, with no actionable layout defect observed. All 16 tall
captures were truncated; omitted content, physical devices, screen readers,
deployed Worker headers and external links were not reviewed. These local lab
results establish neither field performance nor publication. Only this
verification record changes after the linked checks. Task progress stays
40/45 in the harness and 32/39 in the CLI; broader acceptance and automatic
defaults remain open.

## Integrated consumer and docs checks on 2026-10-20

The CLI's literal required gate passed after its approved TLS repair and npm
cache restoration. CLI, harness and shared-UI source, symlinks, HEAD and index
stayed unchanged; the direct child was reaped. Receipt:
`/private/tmp/ejm-capacity-consumer-required-gates-20261020/20261021T025242Z-63379e59-cli-gate/report.json`,
SHA-256 `9506e54e5b95ed15a7af374666518e2f445356bf29cfcb9ea575a5db91eaec35`.
The required harness gate below remains the passed owner checkpoint.

The IDE's post-adoption `cargo check --offline --locked --tests -p ejm-ide`
also passed. Its lockfile changed only the source identity of
`rightkit-fs 0.4.4` from registry to the inherited harness path. No version or
dependency edge changed. Metadata selects exactly the vendored manifest with
`fs`, libc and thiserror. Generated inputs stayed unchanged, all command
children were reaped, and the shared lock was released. Completion:
`/private/tmp/ejm-capacity-ide-post-adoption-20261020/run-500adae73d3748d99be77fef336d0ff9/completion.json`;
report SHA-256
`119b49a05b27c928ed05e095e8893d17aa9bc2df068593499acb81685459b6a5`.
This is macOS public-consumer compilation, not IDE runtime or full-gate proof.

The approved shared docs loading repair passed its focused `ejm-ui` checks,
including cold SSR, delayed hydration with complete server panels, keyboard,
shared selection, copy, TypeScript, source entries and tree shaking. Both
rebuilt product sites then passed `npm run check` and `npm run budget` with
fully stable source:

- Harness: `/private/tmp/ejm-capacity-consumer-required-gates-20261020/20261021T025519Z-74b91f50-harness-site/report.json`.
- CLI: `/private/tmp/ejm-capacity-consumer-required-gates-20261020/20261021T025550Z-5a7171e1-cli-site/report.json`.

Local browser QA retained the unchanged 95 KiB header-inclusive JavaScript
wire budget. Harness developer docs used 95,141 bytes; the three CLI routes
used 96,861, 96,950 and 96,876 bytes. All CSS/font/HTML budgets, 16 capture
checks, 32 behavior contexts and no-tab-chunk request checks passed. Complete
source and built files stayed unchanged, direct children were reaped and
Chrome closed. Expected disabled-script CSP observations and cancelled home
requests whose destination rendered remain in the raw report. A v4 summary
corrects the capture tool's integer failure-count parsing; v3 and raw evidence
remain unchanged. Summary:
`/private/tmp/ejm-capacity-site-final-qa-20261020/20261021T025609Z-6d6fef32/qa-summary.v4.json`,
SHA-256 `4a5076bfd596a0fb47bdfee8cb534543e4af2162695cefd7a46ca415a4a185b4`.
The source/build linkage record is
`/private/tmp/ejm-capacity-docs-build-qa-linkage-20261021.json`, SHA-256
`55b277173d42039be19a0f42d9b3684671dd393517c857c7fa9c19225392c413`.
Read-only image review covered eight original-detail harness-developer and
CLI-reference viewports plus eight reduced tall overviews, with no actionable
defect observed. Truncated lower content, devices and screen readers were not
reviewed. These results close CLI tasks 8.4 and 8.5 only. No site was published.
At that checkpoint, current-image helper calibration remained open. The separate
calibration above does not close native Linux/Windows, full crash and scale
acceptance or automatic defaults.

## Approved stable-lock repair and current gate

The owner approved the temporary filesystem patch. The adopted source now
creates the final permanent name directly and checks privacy on the same open
handle. It retains unsafe or failed entries without replacement. All five
harness consumers inherit the version-and-path dependency, including when a
host is the Cargo root. Publishing remains gated until maintained upstream
supplies equivalent APIs. The source record and exact patch are in
`vendor/README.md`; focused native evidence is in
`docs/evidence/native-stable-lock.macos-arm64.json`.

The source-stable focused macOS gate passed deterministic first-visibility,
actual creator-death, privacy, competing-creator and hard-link checks, ordinary
release-test compatibility, strict executor Clippy and worker compilation.
No ordinary release graph enables the debug fault-injection seam. The adoption
record is
`/private/tmp/ejm-capacity-stable-lock-adoption-20261020/adoption-completion.json`,
SHA-256 `8718ce322e16e2b981859885e752778deb795f329f4b1104e1785ba114762fdd`.
An independent read-only review verified all 76 vendor files and found no
additional runtime/privacy defect. It confirmed the IDE lock still needs the
registry-to-path source refresh; the CLI and harness locks already select the
vendored package.

The required gate after adoption passed Rust tests, strict linting/build,
rustdoc and the advisory/license/source check, then stopped at two formatting
findings in unchanged imported vendor files. Its unused-dependency tool also
reported an offline metadata error while treating the excluded vendor package
as a standalone root, although it returned zero. That stage is incomplete.
Receipt:
`/private/tmp/ejm-capacity-final-required-gate-20261020/20261021T023025Z-84a78526/summary.json`.
Runtime and policy inputs, HEAD, index and source symlinks stayed unchanged;
four unrelated archived publication records changed and were retained. The
direct gate child was reaped and the tracked SDK outputs were restored.

The gate integration now preserves the imported subtree's exact bytes through
an explicit manifest comparison, rejects symlinks, and keeps vendor guidance
and provenance in the formatter's normal scope. The unused-dependency checker
scans all eight authored workspace crates under `crates/`; Cargo and the
advisory check still resolve the vendored dependency. The imported upstream
optional/test graph is not a separate workspace root. Full-gate, IDE consumer,
site and current-image calibration results remain pending at this checkpoint.
Task 3.11, platform/scale acceptance and automatic defaults stay open.

### Required gate after vendor integration

The literal `./scripts/check` passed in 72.205 seconds on macOS arm64 after
the vendor gate integration. It passed the exact 76-file source comparison,
authored-workspace unused-dependency scan, Rust format/lint/test/build/doc and
advisory/license/source checks, protocol generation check, ESLint/Prettier,
TypeScript and Python SDK suites, strict Python checks and package build.
Receipt and retained logs:
`/private/tmp/ejm-capacity-final-required-gate-20261020/20261021T024018Z-fea0d8d0/summary.json`.

All 811 captured source files, symlinks, policy identities, HEAD and index
stayed unchanged. The gate child was reaped and tracked SDK outputs were
restored. The run used two Cargo jobs, disabled incremental compilation,
requested offline dependency access and kept the 10 GiB free-space guard.
This is a passed required-gate source checkpoint. Later evidence-only edits,
consumer/site checks and current-image calibration have their own scope;
the gate does not establish cross-platform or broad scale/default acceptance.

## Stable lock publication review and consumer check

The [lock creation blocker](../../../docs/host-capacity.md#stable-lock-creation-blocker)
records a separate publication gap confirmed by source review. The installed
`rightkit-fs` 0.4.4 and checksum-verified 0.4.6 sources use the same hard-link
publication sequence. Their reviewed public APIs do not supply direct private
exclusive creation relative to an existing pinned `StateDir`. The exact API
review and package hashes are retained in
`/private/tmp/ejm-capacity-stable-lock-dependency-gap-20261020.json`, SHA-256
`694a4ecc5355a8ea98bb99f8718340c680f82ee66a1c6079b7adfff10fe010e9`.
No dependency, privacy check or permanent-lock identity was changed.

The new `hard_linked_startup_lock_is_rejected_without_replacement` test creates
the interrupted two-link state and verifies typed rejection, retained file
identities and continued lock exclusion through both names. It does not force
an actual publisher crash or demonstrate recovery. The fresh executor gate at
`/private/tmp/ejm-capacity-buffer-gate-20261020/20261021T013452Z-64af1632/report.json`
passed formatting, all 78 executed tests and strict all-target Clippy; three
native fixtures remained intentionally ignored. Source hashes stayed unchanged
through those checks. The blocker and tasks 3.7 and 3.11 remain open.

The IDE consumer also passed `cargo check --offline --locked --tests -p ejm-ide`
with two Cargo jobs, incremental compilation off and the existing CLI target.
Its original dedicated target was absent, so the first attempt stopped before
Cargo. The second scratch runner changed only the target constant. The 318
captured source files and 77 generated inputs stayed unchanged; none was
restored or deleted. Evidence and the exact runner adaptation are retained in
`/private/tmp/ejm-capacity-ide-contract-final-20261020/attempt-02/provenance.json`.
This is local macOS arm64 consumer compilation, not IDE runtime, calibration or
full-gate acceptance. Automatic defaults remain gated.

## Full gate after the lock safety regression

The literal `./scripts/check` at
`/private/tmp/ejm-capacity-final-required-gate-20261020/20261021T013610Z-1fa89d84/summary.json`
failed in `competing_startup_clients_share_one_stable_lock` with
`startup.lock has more than one hard link`. This reproduces the concurrent-reader
publication failure. The isolated executor gate above had passed; that success
did not eliminate the race. The full run stopped at this test and establishes no
result for later gate stages. An actual publisher-death schedule remains unrun.

All 717 captured source hashes, source symlinks, policy identities, HEAD and index
stayed unchanged during this failed gate. The four tracked SDK outputs were
preserved. The raw failure is retained. No test was removed, ignored or weakened
to make the gate pass, and no dependency fork was adopted. Automatic defaults
and the remaining tasks stay gated.

## Release embedding helper memory

The [embedding-memory index](../../../docs/evidence/native-helper-embedding-memory.macos-arm64.json.gz)
and [raw bundle](../../../docs/evidence/native-helper-embedding-memory.macos-arm64.tar.gz)
retain both release checkpoints, exact image, build and source receipts, runners
and observer sources, all startup and saturation trials and cleanup evidence. Three
fresh trials per CLI and worker image ran in each series on macOS 26.5.2 arm64.
The original saturation series passed its behavior and cleanup checks but
missed the provisional 32 MiB charge: CLI peaked at 33.9375 MiB and worker at
33.75 MiB. That miss remains part of the evidence.

This calibration uses the buffer-reduced CLI image with SHA-256
`bbdce48e8517f9f55541d52f63495a240712e4b2310c5565a15e0e17e3ccb3a4`
and worker image with SHA-256
`66c32c353cb25226bdd52fbcc99093403d113508ffdd7b8da1652196992678d0`.
The later startup-lock-race correction produced a separate V5 CLI image,
SHA-256 `d4f538eb44e2a1c04d96ce2f3674eb8873bb4695993c521d581895a433a0d1e2`,
which has not been startup- or saturation-peak calibrated. Its
[CLI coordinated evidence](https://github.com/ejmlabs/ejm-cli/blob/main/openspec/changes/adaptive-agent-capacity/verification.md#startup-lock-race-and-1000-agent-retest)
retains the separate race reproduction, focused executor/consumer checks and
selected progress retest. Earlier memory claims retain their earlier hashes.

The private transport now uses `Framed::with_capacity` with 512 bytes for each
initial buffer, retaining the 8 KiB backpressure boundary, 32 KiB initial frame
limit and existing streaming compaction. Replacement saturated kernel lifetime
RSS maxima were 14.671875 MiB for CLI and 14.359375 MiB for worker. All six
replacement saturation trials passed 14 stages and had successful post-proof
kernel RSS reads. Readiness holds reached 256, completed retention reached 256
plus eviction, prepared scopes reached 1,024, and incomplete handshakes reached
32 with overflow rejection and slot reuse. No native commands ran.

Replacement startup maxima were 7.546875 MiB for CLI and 7.390625 MiB for worker
through six command-free seconds after observed authenticated readiness.
Readiness observations were 3.3338/0.0860/0.0901 seconds for CLI and
0.2781/0.0839/0.0833 seconds for worker. The first CLI trial was slower than the
original first trial; no startup-speed improvement is claimed. All clients were
reaped. Every helper's exact birth identity ended and its original lifetime
witness released. Saturation cleanup also checked the retained inode after
fixture path removal. Rejected pre-proof samples and teardown read errors are
retained, and no timeout substitutes for cleanup proof.

These windows fit the unchanged 32 MiB estimate on this Mac. They do not prove
command, interpreter or process-tree peaks, other embedding images or platforms,
eight production images under saturation, complete consumer acceptance, broader
crash recovery, scale and static-sweep acceptance or automatic defaults. Runtime
source receipts exclude the documented concurrent site-maintenance paths; their
exact scope and before/after observations are retained. The full required gate
below predates the buffer change. Tasks 3.11, 8.1 and 8.4 remain open. These
measurements did not activate automatic defaults or a release. Nothing was published.

## Refreshed required gate for consumer fixtures

The literal `./scripts/check` passed on `macOS-26.5.2-arm64-arm-64bit-Mach-O`
from 2026-10-21 00:04:41.400283Z to 00:05:51.709725Z in 70.308186 seconds.
Its direct child exited zero and was reaped. The runner held the shared workload
lock and the 10 GiB free-space guard, set `CARGO_BUILD_JOBS=2` and
`CARGO_INCREMENTAL=0`, requested offline Cargo and uv resolution, and disabled
uv environment sync. No install command was added by the runner.

The summary is
`/private/tmp/ejm-capacity-final-required-gate-20261020/20261021T000440Z-13cae371/summary.json`,
SHA-256 `bd301f352a442896fd6f60a10c75abf46aa49c0bd5f9492b2ec57e4216048530`.
The matching `provenance.json` has SHA-256
`e8fe25c8f72fd6470cdbc3e799028eaf12ec0595b3f1dc51222cffc9ad74ccea`;
`required-gate.stdout.log` has SHA-256
`326101bf847e6f56b968057923908d2dd32dcca6134473de8cbead285b1dfd58`.

The complete gate passed, including Rust builds and workspace tests, strict
all-target Clippy, warnings-denied Rustdoc, policy and dependency checks,
generated protocol freshness, formatting and language checks. TypeScript passed
23 SDK tests and Python passed 25, including one positive coordinated package
case in each SDK. Those cases execute real offline package composition through
the worker with explicit automatic capacity. The Python source archive and wheel
both built successfully with the complete cached setuptools 80.9.0 backend.
This is local package-build evidence, not publication.

All 714 source-file hashes, source symlinks, policy identities, HEAD and index
remained unchanged during the gate. Tracked TypeScript SDK outputs were restored.
The explicit 129-input historical runtime comparison records nine differences
from the earlier native benchmark checkpoint and none during this gate. This
pass therefore covers the later fixtures without changing the source scope of
the earlier performance measurements. The receipt excludes ignored build/cache
directories and does not freeze every installed dependency or sibling repository.

At this gate checkpoint the ignored control-state fixture compiled but did
not run. Actual-image startup and retained-state calibration, CLI shared-root
acceptance, native Linux/Windows coverage, broader performance acceptance and
automatic-default activation remained open. Later local image measurements are
recorded above; tasks 3.11, 8.1 and 8.4 remain open. This records verification and
corrects the canonical benchmark comparison; no public runtime contract or guide
behavior changed, and no site build or publication is claimed for this update.

## Consumer calibration fixtures after the required gate

One positive coordinated package case was added to each SDK. They explicitly
request automatic capacity with bounded ceilings, import a real offline package
through the worker, invoke its provider and inspect public capacity/governor
status before shutdown. The Python case also invokes a package command twice
and verifies retained interpreter identity and state. Both check resident
accounting after transient work drains. Helper counts remain host aggregates;
these cases cannot attribute a particular helper charge or prove final helper
witness release. Windows cases remain skipped under the shared-native gate.

Review moved the Python body timeout inside the harness context so it cannot
cancel shutdown. Its successful idle snapshot is retained for assertions.
The nearby provider-quota fixture now uses its generated public configuration
type. No SDK implementation or wire contract changed.

The test-only `native_owner_smoke` witness client now accepts an optional
helper executable through public `NativeOwner::with_helper_executable`.
The canonical benchmark reference documents command-free observation of a
real CLI or worker image. No image calibration has run through this addition,
and no product demo, fake-response or private helper mode was added.

The witness client also writes a bounded PID/generation JSON marker after
authenticated readiness. Its existing admission-pause marker remains a plain
PID. Independent review checked compatibility with the pre-dispatch crash
barrier and existing callers. The current example has SHA-256
`f9ee1cd7a9b9ccfa60ff9391cc724feb153846262616aea9caf5a8d22943829c`.

An ignored Unix control-state fixture now checks the real helper's 256
readiness holds, 256-entry completed FIFO and eviction, 1,024 prepared scopes
and 32 incomplete handshakes. The scope/handshake proof has a four-second
deadline within the normal five-second protocol waits. Overflow and slot reuse
are checked behaviorally. It launches no commands, emits bounded observer
stages, and requires witness release plus exact helper-identity exit. Unknown
startup or cleanup retains state and fails. Descriptor preflight requires the
current open count plus 1,440; it does not raise the limit. This measures
control-state occupancy, not command memory or shared registry accounting.

The fixture's final SHA-256 is
`b05e0a79dc79683cacecd673727f1a0e79c9603fb804f2ad48470aa879b0f875`.
Independent review and formatting passed. Its retained static receipt is
`/private/tmp/ejm-native-control-occupancy-static-receipt-20261020.json`,
SHA-256 `6b2b588c05bbf077ed84434d65c14ed4ee0003690d6a87f6ae5725444f3029c3`.
The refreshed gate compiled the ignored fixture. Its helper run, saturation
proof and peak measurement remain unverified.

Eight static commands passed with all four source hashes unchanged. The receipt
`/private/tmp/ejm-capacity-consumer-fixture-static-20261020.v2.json` has SHA-256
`5f46afbaf7625d48695b224b93b6bf56dd791ddb8da1d87f81bdd1a42f8a33f4`.
It retains exact arguments, explicit environment overrides, timestamps and
stdout/stderr for Rustfmt, ESLint, Prettier, Ruff check/format, strict mypy for
the Python test, Node syntax and no-bytecode Python compilation.

After the marker and occupancy additions, three more static commands passed:
Rustfmt for the example/test module/fixture, Prettier for the benchmark reference,
and scoped whitespace checks. All six recorded source hashes were unchanged.
The captured V3 receipt is
`/private/tmp/ejm-capacity-consumer-fixture-static-20261020.v3.json`, SHA-256
`f708fc154402c20dcad6f6a53e6a69dc4efd336480469ef2250637c8f9c7f659`.
These syntax/format checks do not type-check the Rust additions.

The refreshed required gate above covers native SDK execution and these later
fixture edits. Actual-image startup and retained-state measurements were still
unverified at that checkpoint; the later local image results are recorded above.
Tasks 3.11 and 8.1 and automatic defaults remain open. Public runtime
behavior is unchanged; the benchmark reference adds only the test-fixture
measurement procedure. No site build or publication is claimed for this follow-up.

## Required gate after approved cache cleanup

The literal `./scripts/check` passed from 23:19:12Z to 23:22:46Z in
213.317478 seconds. The reviewed V2 runner held the shared workload lock, used
two Cargo jobs with incremental compilation disabled and offline dependencies,
and retained the 10 GiB free-space guard. Its direct child exited zero and was
reaped without termination signals or cleanup uncertainty.

The receipt is
`/private/tmp/ejm-capacity-final-required-gate-20261020/20261020T231911Z-f7cacce7/provenance.json`,
SHA-256 `d0420feca7fec78bff76305ed7a848c012b6ab64c7cd2662075d6941b36a26ef`.
The complete gate log has SHA-256
`eab4afa0b4120c620efd354cbf3fe491c2ef4ddbb0032d2395f723a6add0485a`.
Independent review verified the command, phase coverage, counts, file hashes,
SDK outputs and the bridge to the native measurement checkpoint.

Passed phases include policy self-tests and crate boundaries, shell syntax,
Rustfmt, strict workspace/all-target Clippy, native helper and worker builds,
workspace tests, Python runtime-host tests, warnings-denied Rustdoc, Machete,
Cargo Deny, generated protocol freshness, ESLint, Prettier, TypeScript build
and SDK tests, Ruff, both mypy scopes, Python SDK tests and the Python source
and wheel package builds. Rust recorded 387 top-level passes and nine ignored
tests across 40 summaries. Two separately logged subprocess child passes are
excluded from that total. Policy tests passed four cases, runtime-host tests
11, TypeScript SDK tests 22 and Python SDK tests 24. The ignored measurement,
large-run and platform fixtures remain unverified by this gate.

All 713 source-file hashes, source symlinks, 24 policy inputs, Git status,
HEAD and index remained unchanged. All four tracked SDK output hashes were
preserved and verified against their backups. The cached setuptools backend
remained complete; Python package outputs were retained and hashed. No install,
environment sync, commit, publication or deployment was added by the runner.

The explicit 130-input bridge to the native build matches 129 inputs exactly.
Only `Cargo.lock` differs, as described below. The gate's separate historical
129-input comparison predates the startup fixes and is not that measurement
bridge. Native timings retain their archived dependency scope. This gate does
not establish helper-memory calibration, a successful coordinated SDK package
case, the CLI shared-root smoke, native Linux/Windows coverage or broader
performance acceptance. The subsequent SDK composition test additions require
their own execution and a refreshed gate.

The approved 9,013-file generated-cache cleanup completed before the gate.
Its exact-set receipt is
`/private/tmp/ejm-capacity-approved-cache-cleanup-completion-20261020.json`,
SHA-256 `0deb95cbdd53848bdc0aaa46f9828db21f1544296a9fbf531bcae7cad30afede`.
It verified 1,225 preserved outputs, reported no errors or unresolved unlink,
and measured 3,467,808,768 bytes of net free-space recovery. The listed inode
allocation was not used as the actual recovery figure. Later rebuilds changed
some retained output timestamps after cleanup had finished.

This adds verification records and planned coverage within the existing task.
No public runtime contract or performance claim changed. The public guides and
benchmark caveats still describe the implemented behavior and open gates.

## Latest source checkpoint

The [simultaneous-start index](../../../docs/evidence/native-helper-simultaneous-start.macos-arm64.json)
and [raw bundle](../../../docs/evidence/native-helper-simultaneous-start.macos-arm64.tar.gz)
retain the startup-floor, helper-selection and bounded WAL-setup follow-up.
The fresh release executable is 7,520,160 bytes, SHA-256
`245fb5547789043a33633624c727fa55cc2ed33e4158ed8e0df8bc74b0ee8521`.
All 130 recorded inputs matched before and after build and workload. The
retained deterministic bundle now contains 122 of those inputs; eight unrelated
package-distribution files were omitted during the independent open source
scope cleanup. It includes `SCOPE.md` and an updated receipt, and each retained
member matches its manifest. Original provenance records and input hashes
remain historical evidence. The revised subset is not byte-identical measured
source and does not establish reproduction of the original full build.

The final fixed8 and auto8 cases each completed eight participants and 6,400
commands without intentional launch staggering or a changed command deadline.
Fixed completed in 19.027691 seconds with 1.895799 seconds of helper cleanup;
automatic completed in 18.597322 seconds with 1.926593 seconds of cleanup.
Each mode verified eight client identities ended/reaped, eight observed helper
identities ended and all eight helper and eight participant witnesses released.
No termination signals, discovery errors or cleanup errors were recorded.
All 143 registry snapshots fit recorded ancestor envelopes; committed helper
charges reached 256 MiB for eight generations. This is sampled identity and
registry evidence, not a requirement that all eight helpers live together.

Earlier raw attempts are preserved. The first recorded one SQLite busy
rejection before journal creation, with the failing SQL call unproven; an
unrelated probe assertion aborted automatic launch at two clients. The
corrected observer then completed all fixed clients and seven automatic
clients, while one automatic client missed its five-second command deadline
before useful work. Removing the invalidation caused by a selected helper's
commit lets valid siblings commit in that generation, retaining live-demand,
minimum, epoch, grant and transactional ancestor-fit checks. Initial WAL
conversion retries only SQLite busy within the existing 100 ms budget and
verifies WAL mode, with no retry around grants or effects.

Nine focused tests passed: one WAL bootstrap regression, two cross-process
writer-exclusion checks and six staged-helper checks. The final formatting and
strict core lib/tests Clippy receipt is
`/private/tmp/ejm-capacity-selected-helper-gate-20261020/20261020T205509Z-7c22256d/provenance.json`,
SHA-256 `f588ce13d3738498525a436cf4ca15c6ec60eef45ea7ba5eb53026cc2e7f4544`.
It bridges to the actual nine passing tests in the preceding receipt; the
only intervening input change is a scoped test-only unwrap lint allowance.
Independent read-only review verified this bridge, the final 130 source
hashes, 42 native evidence paths, raw completions and cleanup identities.
These checks close the reproduced simultaneous-start miss on this Mac.
Task 3.11, startup/retained-state calibration, consumer costs, wider
scale/platform acceptance and automatic defaults remain open. The later
required-gate pass is recorded above.

The subsequent literal `./scripts/check` began at 21:02:44Z with the reviewed
runner, two Cargo jobs, incremental compilation disabled and offline resolution.
It passed policy checks and began strict workspace Clippy before free disk fell
below the retained 10 GiB guard. The runner sent SIGTERM to its new process group;
the later SIGKILL attempt returned `EPERM`, so the original receipt lacks a
reaped-child exit code. A separate expanded-access process scan then found no
remaining process with that PID, parent PID or group ID 7924. This is a failed
gate, not a Clippy pass. All four SDK outputs were restored; source, policy,
symlink, HEAD and index identities remained unchanged during the attempt.
The receipt is
`/private/tmp/ejm-capacity-final-required-gate-20261020/20261020T210243Z-1860d406/provenance.json`,
SHA-256 `664d41358e220cfc5eb83f966199b889ee3cb0d369e04877df6c134e272da642`.

Before that gate, the live lockfile changed from the measured SHA-256
`072a52712dd92466e59bd62c31a75bcfdaa430e4e7e66c1d45f33c0854df504a` to
`110ce8dfb31141c1bd65c4784c38082ff7df1c6b923951f69dc7ec182912f80c`.
Only `rustls-webpki` 0.103.15 to 0.103.16 and `webpki-root-certs` 1.0.9 to
1.0.10 differ, with their checksums. All other 129 native build inputs matched.
The archive preserves the measured lockfile and timings for that checkpoint;
the later required gate passed that dependency checkpoint. These native
timings were not remeasured. No concurrent lockfile edit was reverted.

### Earlier sampler checkpoint

The sampler checkpoint passed the replacement fixed and automatic
native matrix: 36 workloads at 100, 1000 and 4096 managed agents, with 93,528
completed native commands and no failed model or tool outcomes. External RSS
sampling stayed enabled. The frozen release executable is 7,520,160 bytes,
SHA-256 `66618eed04f6887b2c4eba21ed85401130936a47ef7891bf4c58a36458ad2f22`.
The [consolidated baseline](../../../docs/evidence/capacity-baseline.macos-arm64.json.gz)
retains exact commands, release inputs, host/limits, binary/dependency sizes,
sampled tree memory, available own-process CPU/I/O and all 36 raw samples.
Each cell has three whole-work-phase samples; its nearest-rank p95 is the
maximum, not a stable tail estimate or per-request latency. The 100-agent and
1000-agent automatic maxima exceeded fixed by 69.4% (3110 / 1836 ms) and 44.7%
(19764 / 13654 ms). Sequential mode order and uncontrolled host load prevent a
causal explanation or automatic-tuning acceptance claim. Target low-end hardware
and native Linux/Windows remain unmeasured. This closes baseline task 1.3 only.

The final macOS sampler includes zombie and confirmed entering-exit identities,
checks birth mismatch first, and keeps process occupancy and full leases while
RSS is unavailable. Incomplete RSS cannot replace the last complete peak with
zero. The focused executor suite passed 76 tests with two ignored measurement
fixtures, strict all-target Clippy and formatting. Independent review found no
actionable issue. The [exit-transition evidence](../../../docs/evidence/native-sampler-exit-transitions.macos-arm64.json)
records the observed races and nearby pinned XNU source, which is not the exact
installed kernel source. Ordinary unreadable live identity still fails closed.

The required `./scripts/check` ran on 2026-10-20 from 18:55:24Z to 18:57:22Z,
with `CARGO_INCREMENTAL=0`, two Cargo jobs and offline resolution. All 129 tracked
runtime inputs remained unchanged. Workspace Rust tests, package-host tests,
strict all-target Clippy, fresh worker build, warnings-denied Rustdoc, Machete,
Cargo Deny, generated protocol freshness and ESLint passed. Rust output contains
386 summed passing summaries, including two nested subprocess summaries; this
is not 386 unique workspace tests. The gate stopped at 33 Prettier findings:
five capacity evidence files and 28 review snapshot/archive/spec paths outside
this change. It did not reach later SDK/Python phases. Evidence and logs remain
under `/private/tmp/ejm-zombie-aware-required-gate-20261020/`. Capacity-owned
formatting reconciliation is recorded separately; the original gate failure is
retained. After preserving originals and proving parsed equality, the five
capacity reports pass formatting. The four historical reports have a portable
[hash bridge](../../../docs/evidence/capacity-historical-formatting.macos-arm64.json).
A fresh root formatting check at that checkpoint reported only the 28 outside
paths. Their owning session retained those bytes while checking snapshot
integrity. Those historical logs are
`/private/tmp/ejm-capacity-root-format-final-20261020.{stdout,stderr}.log`.

The user subsequently approved five exact redundant-source roots for removal.
Cross-Platform verified the retained archive and extracted mirror, updated
current links and removed exactly 191 files and 40 directories. Its
[completion receipt](../cross-platform-support/shared-ci-review/retained-source-archive-20261020/cleanup-completion.json)
is SHA-256 `aa8b0deebe0ccc709d053188c8d7ea71983d5f316889277514e6c98721eeefb3`.
The retained archive, mirror, newer manifest and five patches stayed unchanged;
23 current local links have no missing or removed-root destinations. Root's
fresh `npm run format:check` passed from 20:04:17.924642Z to 20:04:21.421976Z
with no warnings or stderr and no ignore or gate-policy change. Receipt and
logs are under `/private/tmp/ejm-capacity-post-archive-format-20261020/`.
This closes the frozen-payload formatting blocker at that checkpoint, not the
full required gate. The startup-floor correction and its focused regression
precede the next complete gate.

Separate SDK checks at that checkpoint used that gate's freshly built worker, SHA-256
`2fb750b83f57b3ec07c4a98be7dbbac1f395a4a1142014ac49cafbb7401683b7`, copied
to scratch. TypeScript build and 22 native-worker tests passed. Python passed
24 tests, focused Ruff checks/formatting and strict SDK mypy. All 129 overlapping
gate hashes, 154 scoped runtime/SDK inputs and both worker copies stayed unchanged.
Outputs and caches stayed in scratch; no install or checked-in dist write ran.
Receipts are in
`/private/tmp/ejm-capacity-consumer-refresh-20261020/sdk-final-gate-worker/`.
These passes do not turn the interrupted required gate green.

CLI's final direct sequence passed all-target locked/offline checking, Cargo
Deny, fixture builds, 177 unit and 18 integration tests, release build, the full
native PTY suite and relay checks. Five development fixtures remained ignored.
All 289 scoped CLI/runtime inputs stayed unchanged; their 129 shared harness
hashes match the required gate. Executed executor, registry and package tests
also cover the complete task 1.4 integration contract, so CLI task 1.4 is closed.
The full CLI `./scripts/check` is incomplete because its `npm ci` phase was not
run. No immutable harness pin was selected. Dirty-source consumer passes do not
prove compatibility with an existing immutable pin.

The coordinated IDE review also passed its literal `npm run verify` from the
frozen complete-source checkpoint plus three formatting-only IDE fixes:
635 web tests, 218 native unit tests and eight workspace integrations, with
strict all-target Clippy and production build. One unit and six installed-server
fixtures remained ignored. The reviewed runtime/pins were unchanged. This is
frozen dirty-source compatibility evidence, not a new immutable-pin result.
The record is
`/private/tmp/ejm-audit-verification-20261020/continuation-1847/primary-verify/attempt-04/gate-summary.json`.

The final harness site gate passed typechecking, prerendered build, headers and
all 39 tests; all five routes met the existing size budgets. All 1,132 scoped
inputs, including 876 shared UI files, stayed unchanged. Receipts remain under
`/private/tmp/ejm-capacity-harness-site-final-20261020/`. Local built-guide review
covered 1280-pixel desktop and 390-pixel phone layouts, light/dark themes,
visible keyboard focus and no document-width overflow. Fresh navigation to
`/docs/developer-guide/#ownership` had no warning/error logs. An initial
mistaken `/developer-guide/` request reached the expected 404 page and logged
React hydration error 418; that separate route issue was reported to the shared
UI owner. Static tests read the guide without scripts; a separate browser run
with scripts disabled was not repeated. Theme and viewport were restored, and
temporary tabs and the preview server were closed. No publication occurred.

Eight isolated ready helpers used 53.66 MiB sampled aggregate RSS, with 55.38 MiB
for fixture clients, over 20 settled samples. Cleanup verified all eight clients
reaped, helper PIDs absent and witnesses available. The
[idle-helper report](../../../docs/evidence/native-helper-eight-idle-ready.macos-arm64.json)
identifies its earlier frozen executable. It does not establish startup peak,
shared registry-pool saturation or consumer resource acceptance. Task 3.11 and
automatic defaults remain open.

The new [kernel-peak and shared-pool report](../../../docs/evidence/native-helper-kernel-peaks-shared-pool.macos-arm64.json.gz)
retains the valid startup components and failed simultaneous pool attempt from
19:38:25Z, its later cleanup audit, and the successful staggered pool retry from
19:46:54Z to 19:47:27Z. Three command-free six-second windows recorded kernel
lifetime RSS maxima of 7,094,272, 7,028,736 and 7,028,736 bytes. All counter reads
had matched birth identities, successful kernel queries and released Mach ports.
Those startup windows use the older frozen smoke executable, with six runtime
input differences explicitly listed. They do not become final-source startup
calibration merely because the fixture source is unchanged.

The sampler checkpoint's frozen benchmark reached eight committed running helpers in one
registry with distinct inherited descriptor domains. It completed eight
800-command workloads, rejected the ninth helper at actual pool admission and
charged the host ancestor exactly eight generations, 256 MiB and eight processes.
Twenty sequential scans over 4.752127 seconds reached 68.609375 MiB helper RSS,
114.765625 MiB fixture-client RSS and 183.28125 MiB combined. These scan sums
exclude command descendants and are not instantaneous aggregate peaks.
Individual helper lifetime RSS maxima were 8.50 to 8.66 MiB. All nine clients
were reaped and all eight helper identities ended and witnesses released
without termination signals in the successful run. All 129 benchmark
inputs and 26 focused executor-gate inputs match the evidence checkpoint.

The initial simultaneous attempt rejected one participant at its 96 MiB runtime
and dispatch baseline before any helper started. Initial requests were 2,000
CPU millis per participant against an observed 11,000-millis host budget. The
runner's unconditional eight-helper cleanup assertion masked that original
rejection; both failures and subsequent cleanup proof are preserved. The retry's
staggered registration does not close simultaneous-start acceptance. Task 3.11,
the provisional 32 MiB estimate and automatic defaults remain open. Canonical
benchmark, executor and host-capacity references now explain the evidence;
public guide links remain current and their rendering inputs are unchanged.

Independent read-only review accepted the supplement's input/source hashes,
kernel-counter semantics, exact generation/ancestor membership, ninth rejection
and cleanup proof. It also traced the first-start rejection to registration
planning only a 32 MiB runtime floor before construction needs 96 MiB with
dispatch memory. The existing resident-floor transition can carry that complete
minimum before the first grant, allowing a memory-only runtime baseline with
zero model/job/CPU credits. That correction fits the current first-start and
working-set tasks. It adds no schema, per-job ledger or synchronous constructor
wait. Its implementation and post-fix evidence are tracked separately from the
preserved earlier attempts.

## Ownership and contracts

The harness owns admission, host observations, provider quotas, learning,
session storage and native supervision. CLI composition consumes public
prepared-capacity, native-owner and bounded session-view APIs. The worker and
SDKs adapt the same core contracts. Runtime dependencies do not point back
from the harness into consuming products.

The earlier source/manifest/lockfile review found an acyclic runtime graph, no
private sibling imports or copied host controller, and unchanged SQLite
dependencies. It adds no consumer-build, advisory or platform evidence.

Existing library constructors retain fixed local behavior. Explicit coordinated
policies use the shared host registry. Old worker requests that omit capacity
keep their prior behavior. No mandatory VM, container or durable per-job native
execution ledger was added. Uncertain effects are never repeated automatically.

## Earlier verified local behavior

- An earlier required-gate run passed all Rust workspace tests, including 103 core
  tests, provider/auth integrations, executor accounting, package lifecycle,
  coding native-owner forwarding and 12 worker tests.
- The registry suite passed 25 tests, including real concurrent registration,
  private-state checks, released-lock recovery and actual subprocess writer
  exclusion. The Unix wrapper no longer opens and closes auxiliary descriptors
  for live SQLite files. Sidecar disappearance is accepted only through a typed
  missing-file result and a fresh absence check.
- Cancellation under registry write contention and unsafe-path failure passed
  its 250 ms fixture bound. Existing charges stayed exact, cached memory
  admission remained usable, and coordination resumed after the fault cleared.
- Fresh CPU saturation reduces targets; stale or missing critical observations
  block growth. Deterministic local accounting and fairness fixtures passed.
- Profile tests cover concurrent merges, independent count/byte eviction,
  confidence decay, expiry, incompatible data, reset races and preservation of
  active job and resident-memory leases during optional learning failures.
- The exact learning-flush test confirms no write at 59,999 ms, normal saving
  at 60,000 ms and early persistence through a forced shutdown flush.
- Two public preparation fixtures confirm bounded admitted context accounting,
  cancellation credit reuse and model-credit release during tool admission and
  execution waits. These assertions do not measure pending allocations or RSS.
- Three public contract fixtures protect complete legacy literals, preparation
  before provider/hooks construction and shared-governor identity. Changed
  session/context budgets reject before tool/provider/journal setup, while a
  later journal-directory choice remains supported. Two resolver tests cover
  every explicit policy ceiling in fixed/auto mode and tighter observations.
  The extended worker suite passed all 12 tests, including omitted-capacity
  fixed behavior and preservation of legacy explicit resource ceilings.
- Two additional learning fixtures passed through the actual coordinator.
  Optional-write failure and isolated reset preserve real sessions/events,
  artifact bytes, encrypted provider credentials, live job/memory leases,
  targets, hard caps and persisted grants; useful turns complete in both states.
  Compatible, changed-environment and corrupt startup profiles cannot override
  smaller caps or a live peer's fully charged model/CPU allowance. The public
  reset wrapper was reviewed, not invoked against the user's host registry.
- Journal tests cover group-commit durability, barriers, rollback, dropped
  replies, bounded publication and old-journal projection upgrades. Selected
  streams retain their own continuity anchor.
- CLI's real oversized-history fixture renders the omission notice while
  preserving durable history and authoritative cursor/state. Fresh CLI checks
  passed 164 unit tests and 12 integration tests, with five libtest fixture
  entries ignored; the full Unix PTY suite passed.
- TypeScript passed its build and 21 tests. Python passed 23 tests and strict
  SDK mypy. Authored Rust, TypeScript, Python, shell and PowerShell examples
  passed their applicable compile, type or syntax checks without paid calls.
- CLI and IDE passed their public consumer checks with at most two Cargo jobs.
  Rustdoc, strict workspace all-target Clippy, dependency checks, protocol
  freshness and ESLint passed. Public site typechecking, build, route budgets
  and all 39 tests passed. The final CLI site gate also passed typechecking,
  build budgets, headers and all 99 tests.

Local guide review covered desktop and 390-pixel phone layouts, light/dark
themes and keyboard focus. The changed recovery text was readable with no
document-width overflow or browser warning/error logs. Static site tests read
the built guides without running scripts; a separate browser session with
scripts disabled was not repeated for this prose update. Temporary preview
tabs and servers were closed, with theme and viewport overrides restored.

The native-owner macOS smoke and its measured limits are recorded in
`docs/evidence/native-owner-smoke.macos-arm64.json`. It does not prove Linux or
Windows native behavior, arbitrary escaped-descendant cleanup, or retained
Rust workspace guards surviving total caller death.

## Crash and package lifecycle follow-up

The registry suite now passes 29 tests. Four new fault fixtures exercise seven
actual killed-subprocess boundaries, a retained live OS witness, all grant
dimensions, real governor lease drain, writer contention, lost release/removal
results, duplicate reconciliation and stale generations. Strict core all-target
Clippy passed. Exact sources, commands and log hashes are recorded in
`docs/evidence/capacity-grant-crashes.macos-arm64.json`. This closes host-local
task 3.3 without claiming native cgroup movement or Linux/Windows acceptance.

The executor suite passes 49 active tests with two ignored measurement fixtures,
and strict executor all-target Clippy passes. The preparation-disconnect fixture
failed before the private client fix, then passed while the lost-launch fixture
still performed exactly one effect. The expanded smoke passed twice under its
45-second bound with idle/busy client and owner death, paused-owner exclusion,
retained cancellation/drop cleanup, spawn rejection and fresh-grant progress.
`docs/evidence/native-owner-recovery.macos-arm64.json` retains source/binary
hashes and both runs. Its timing fields overlap development workloads and are
not a matched cost comparison. An earlier unlabelled connection reset remains
unattributed; the distinct preparation retry defect has a deterministic red/green
regression.

Seven real package lifecycle fixtures pass for Node and Python: failed, blocked
and cancelled imports, reload failure, failed final composition, shutdown of both
initialized languages, paused-owner cleanup timeouts and owner death during
initial import or explicit reload. A cleanup error cannot hide the original
import failure. Reservations remain held while cleanup is uncertain. Confirmed
owner death permits scheduling recovery without asserting descendant cleanup.
The package crate passes 46 active tests, its Python host suite passes 11 tests,
and strict package all-target Clippy passes. The public shutdown snippet compiles.

The six-sample release package-host measurement completed in a quiet window.
`docs/evidence/package-host-overhead.macos-arm64.initial.json` retains exact
source/binary hashes and all raw samples. The idle native owner used 9.01% to
12.60% of one core, missing the proposed less-than-1% low-load target. Cleanup
correctness does not close that performance gate. Native Linux/Windows lifecycle
and the larger acceptance matrix remain open.

The typed `libproc` sampler passes 51 active executor tests with two ignored
measurements and strict all-target Clippy. The two focused native tests also
pass with system access: internal birth identity retains microsecond precision,
public recovery timestamps retain whole-second compatibility, and cleanup
rejects a stale birth token before terminating the correctly identified child.
The repeated six-sample release package measurement is saved separately in
`docs/evidence/package-host-overhead.macos-arm64.native-sampler.json`. Owner CPU
is 1.626% to 1.933% of one core, still missing the below-1% target. All lifecycle
assertions passed. The report retains exact binaries and all available source
hashes. A parser failure lost its pre-run hash snapshot, and a later core edit
prevents attributing that one current source hash to the measured binary. This
limits source provenance; the report says so without replacing the old evidence.

## Performance evidence

The recurring tracing comparison completed all 20 approved invocations, five
alternating pairs per workload, with exact useful-work and accounting parity.
`docs/evidence/capacity-telemetry.macos-arm64.json.gz` embeds frozen inputs, the
tracing-only patch, commands, source/binary hashes, raw samples and the preserved
zero-sample free-disk failure. Its companion summary records a pressure paired
median of -1.01% (-4.75% to +4.98%) and mixed paired median of +0.52% (-1.32%
to +10.76%). Both slower instrumented mixed samples remain visible. This
measures recurring tracing with fixed control choices on one Mac; five pairs do
not establish zero overhead, a speedup, stable tails or a budget pass. The
separate telemetry correctness report protects stage separation, cancellation
before admission and failed/skipped outcomes without observer-owned credit.
Together these satisfy task 1.2's instrumentation and measurement scope;
automatic tuning and broader acceptance stay open.

The bounded dispatch fixture passes four ordinary tests and both explicitly run
target-scale tests. The latter complete and cancel 1000 and 4096 managed runs
with one dispatch slot, one job slot and an executor queue limit of four. Accepted
effects occur exactly once, cancellation cannot start old work later, governor
accounting returns to baseline, and an explicit new run can progress. Boundary
fixtures reject managed-count and UTF-8 prompt overflows before effects and apply
run deadlines while governor/dispatch admission is blocked. Canonical-workspace
writes use two job/dispatch slots to verify workspace exclusion separately:
cancelled waiters cannot start, and retained background work keeps the write lock
after caller cancellation until it drains. Strict core all-target Clippy passes.
`docs/evidence/capacity-bursts.macos-arm64.json` retains commands, logs, fixture and
binary hashes. This closes task 4.2's local correctness scope. It does not close
throughput, idle resource, native platform or automatic-default gates. Final
combined-source checks remain required after scheduling and telemetry edits.

All six burst fixtures then passed on the combined scheduling/telemetry source,
including both target-scale cases and new cached-count/drain assertions. The
report preserves the earlier provenance and appends the current fixture, engine,
driver, capacity, executable and raw-log hashes. The 26.98-second suite overlaps
other development checks and is not a performance comparison.

The separate cached scheduling API passes three Rust phase/cancellation
fixtures, twelve worker tests, two actual-worker TypeScript tests and three
actual-worker Python tests, including the sync facade. Capability rejection
precedes method dispatch, and the old four-field status payload stays unchanged.
Core/worker strict all-target Clippy, generated-contract freshness, TypeScript
build, Python Ruff and strict mypy, and scoped formatting pass. The initial
worker sandbox run denied three loopback binds; the allowed rerun passed all
twelve. `docs/evidence/scheduling-status.macos-arm64.json` records the API,
source hashes and transcript-only log limit. Together with the 4096-run and
retained-workspace fixtures, this closes task 4.5's implemented local contract.

See `docs/benchmarks.md` and its raw evidence links. The controlled journal
comparison changed only the text group limit, with matched normalized source
hashes and alternating variant order. Pressure-workload median execution fell
from 644 ms to 333 ms. The mixed command median changed from 1184 ms to 1187 ms.
These are one-host development fixtures, not automatic-mode scale acceptance.

The matched release fixed/automatic matrix completed all 36 single-process
workloads at 100, 1000 and 4096 managed agents. Automatic median duration changed
by +7.6%, +4.6% and -3.8% respectively. Sampled process-tree RSS peaked at
51.2, 60.9 and 86.6 MB in automatic mode. Each 4096-agent mixed run completed
12,288 commands. A separate two-process comparison managed 2000 agents across
separate journals and one isolated host registry; all 6000 commands completed.
Raw reports and the derived input-hashed summary are linked from
`docs/benchmarks.md`. Three samples do not establish stable tails or the best
safe static setting. The matrix predates the final constructor envelope guard;
its configurations never change prepared ceilings. Binary provenance is retained.

The corrected 120-second CLI pair used the same frozen release-profile libtest
binary and an explicit 2 GiB test-only journal limit. Headless completed 8085
turns at 67.38 turns/s; interactive completed 8192 at 68.27 turns/s. Both reported
zero active failures and zero drain failures, with 4096 pending turns cancelled
at cutoff. Main database sizes were 460,468,224 and 504,303,616 bytes, with WALs
below 363 KiB. One pair and cutoff sensitivity do not establish a speed gain or
stable tails. These cases contain no selected-response trace. The earlier
100/1000 traces also cannot prove loss-free delivery: subscription receive
errors resubscribe without counting lag separately. Full trace acceptance stays
open.

The original long 4096-agent interactive run remains failed evidence: 8192 turns
completed and 4096 failed. Its main database exhausted exactly the inherited
512 MiB page budget, with zero free pages. Individual error text was not retained,
so storage exhaustion is strongly supported rather than directly captured. The
fixture now reports an explicit journal envelope and bounded failures, and
rejects workload failure even when libtest exits zero. The successful replacement
pair does not erase that result or change the production journal default.

Initial legacy 4096 rejection and the original fixed 1000 completion-deadline
miss also remain part of the evidence. No database engine migration was started.
Current evidence supports measuring write shape and coordination costs before
replacing SQLite; LMDB and redb have not been benchmarked for this workload.

## Earlier gate limits

The protected-main compatibility reconciliation restores cloned run observers,
`RunCompletion`, exact active subscriptions and retained successor receivers,
plus bounded/cancellable waits around native vault calls. Fourteen core tests
and 21 vault tests pass; three native/profile vault fixtures remain ignored.
`docs/evidence/protected-main-contracts.macos-arm64.json` records exact source
and log hashes against protected main `8d3dc39`. Current durable start ownership,
scheduling/telemetry and the optimized serialization/encryption path remain
intact. Strict core/auth Clippy first stopped in two executor draft findings;
their owner fixed both, but the rerun waits for build space. Final consumer
checks and immutable dependency pins remain open. The public contract and
developer guide now describe these restored semantics.

After the new telemetry/contract docs, the local harness site gate again passes
typechecking, build budgets and all 39 tests. The CLI site gate again passes
its corresponding checks and all 99 tests. Logs are retained in
`/private/tmp/ejm-capacity-harness-site-20261020.log` and
`/private/tmp/ejm-capacity-cli-site-20261020.log`. These are local checks and do
not establish publication.

The required full harness gate was run offline with `CARGO_INCREMENTAL=0` and
`CARGO_BUILD_JOBS=2`. It stopped at 28 Prettier findings in unrelated work:
stored cross-platform CI review snapshots, an earlier archived site verification
record and the main site-build specification. The gate is not green. Its Rust,
dependency, generated-protocol and ESLint phases passed before that stop.
Policies were not weakened and unrelated work was not reformatted.

The Python offline package build lacks pinned `setuptools==80.9.0`. A later CLI
gate found its tools and passed Python tests, architecture, formatting, strict
Clippy and fixture build, then stopped when the sandbox denied nine local-socket
tests. The separately allowed CLI suite passed 165 unit and 16 integration tests;
five development fixtures stayed ignored. Later full-gate stages remain unrun.
Root-wide Ruff includes unrelated stored snapshots. Focused owned-file checks
passed. No missing tool or package was installed. Worker debug archive and
extracted CLI macOS release smoke passed;
this does not establish a worker release archive, signing or publication.

## Remaining acceptance

The task list retains incomplete requirements. Material remaining work includes
native Linux/Windows acceptance, a live Linux cgroup move, the package-startup
and native-crash matrix on those platforms, safe static sweeps and
changing-pressure/learned-restart measurements,
automatic-tuning cost and automatic-default activation. Additional
preparation, compatibility, resolver and learning fixtures passed after the
full gate, followed by strict core/worker all-target Clippy. The final runtime
guard changes only construction validation and a private captured session
ceiling. The release benchmark binary stayed frozen; its reports identify it
separately from that later guard.

## Source-stable site snapshot on 2026-10-20

The separately serialized harness site `check` and `budget` commands both
passed. The captured harness and shared-UI sources, HEAD, index and symlink
receipts stayed fully stable throughout those commands. All direct children
were reaped. Receipt and logs:
`/private/tmp/ejm-capacity-consumer-required-gates-20261020/20261021T014655Z-a5bd48b5-harness-site/`.

Later local-preview preparation detected concurrent changes after this
snapshot, including the public developer guide and site lockfile, plus a
harness HEAD change from `6312657` to `e0e914d`. Those changes were retained.
The preview compares exact built files and records source deltas separately;
the passed snapshot alone does not establish a final current-source site gate.
This site result also does not resolve the reproduced native lock-publication
race or establish performance, native crash acceptance, another platform,
automatic-default activation, package release or site publication.

## Commit traces and package crash checkpoint on 2026-10-20

The following local macOS runs finished on 2026-10-21 UTC. Each retained report
records stable full source snapshots, the shared workload lock, the 10 GiB disk
floor and reaped direct children. Those receipts describe the tested snapshots;
this later evidence-only edit is outside their source hashes. No task is closed
by this checkpoint.

### Selected-event commit clock

With `test-support`, `Harness::subscribe_session_traced` and
`SessionSubscription::recv_traced` carry one monotonic stamp taken immediately
after successful transaction commit, before publication and timing aggregation.
The private publication envelope preserves that stamp through pending delivery
and reset handling. Each batch samples traced interest before commit. The final
traced subscription's drop ends future interest; an in-flight batch that already
sampled it may still take its timestamp. Ordinary subscriptions alone request no
new clock. Default builds exclude the trace fields, checks and APIs. Durable events,
the journal schema, worker wire and ordinary subscriptions retain their existing
contracts.

The focused core run passed two actual regressions for opt-in batch stamps and
pending-stamp preservation. The later full harness gate also passed both:

- `/private/tmp/ejm-capacity-core-trace-checks-20261021/20261021T045831Z-a2fce601-core-trace-focused/report.json`
- SHA-256 `2afeb0166dc440e8b7ee7a2357f125c311a13107fb5b2b9f6b276050c618a198`

The CLI consumes this public test-support contract. It retains receipt-based
samples and adds bounded commit-to-terminal-write samples, with both intervals
ending after `terminal.draw` returns. Missing commit stamps have a separate
counter and never fall back to receipt time. Missing or partial samples and
missing integrity accounting keep measurement incomplete, independently of
workload success. Selected-event latency acceptance stays unverified. Trace
overhead, first-write coverage, selection correlation and a representative
latency population remain acceptance work. The terminal observer still caps the
active window at 120 seconds. Canonical benchmarks and public developer guides
describe these limits.

### Package caller and idle-owner crashes

Two native macOS regressions now pass:

- `caller_crash_during_import_and_reload_cleans_roots_before_fresh_client_progress`
  kills the actual caller during Node and Python initial import and reload.
- `idle_owner_crash_with_both_languages_requires_fresh_grant_and_explicit_reload`
  kills an idle helper with both language hosts resident and checks stale
  callbacks before explicit reload.

The assertions retain precrash helper accounting, the original kernel witness,
captured process identities, interpreter/helper exit, witness release before a
newer helper-zero grant, a new owner epoch, the unchanged registry inode and
useful fresh-client work without effect replay. Failure cleanup cannot satisfy
the success assertions. The killed caller's local governor is unobservable;
these tests do not assert its counters after death.

The first caller fixture stopped the helper before killing its parent and then
expected a sustained helper-one plateau. A stopped orphaned POSIX process group
may receive SIGHUP and SIGCONT. The diagnostic showed helper-zero accounting,
but did not prove the helper's exit signal. The corrected fixture keeps the
helper runnable while the package import remains blocked. It removes that
invalid plateau and the explicit continuation, while keeping precrash witness
and accounting checks, actual exit proofs, original witness release, a newer
valid grant and useful recovery. Separate paused-owner coverage remains.

The corrected focused run passed both tests. Its caller-test command took
26.850 seconds and its idle-owner command took 4.487 seconds. These include
test-command overhead and are not startup or recovery latency measurements:

- `/private/tmp/ejm-capacity-package-crash-checks-20261021/20261021T050253Z-62936c09-package-crash-focused/report.json`
- SHA-256 `cc3961ac31357ee2b3b3180a2eea0579c3670fddcc0b782473c7e5621a6b06fd`

Both tests were then split into focused helpers to satisfy Clippy's line limit.
Static review found no lost assertions; the final full gate passed both tests
on the refactored source. Evidence covers captured roots on this Mac. Whole-second
macOS birth tokens, cooperative deadlines and arbitrary detached descendants
remain limits. This does not establish Linux or Windows behavior, arbitrary
descendant containment, package performance or automatic-default readiness.

### Required gates and retained failures

The literal harness `./scripts/check` passed in 86.930 seconds with 396 Rust
tests passed, 11 ignored and 42 successful Rust test groups including doctests.
It also passed the gate's lint, documentation, dependency, SDK and local package
assembly stages. Receipt:

- `/private/tmp/ejm-capacity-package-crash-checks-20261021/20261021T051634Z-9881afc2-harness-gate/report.json`
- SHA-256 `1b88770ce7e496b5649ddcc3266b3c507cb900d375044232744267e3567c9591`

Earlier failures remain unchanged:

| Failure                                     | Retained report                                                                                                       | SHA-256                                                            |
| ------------------------------------------- | --------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------ |
| Initial invalid helper plateau timeout      | `/private/tmp/ejm-capacity-package-crash-checks-20261021/20261021T044933Z-c75b4f0d-package-crash-focused/report.json` | `d32aeca46193f4a9c8b054ae7161fda66d33f6560df3bb25e4b3102c5a2068f4` |
| Diagnostic plateau timeout                  | `/private/tmp/ejm-capacity-package-crash-checks-20261021/20261021T045930Z-b64d583c-package-crash-focused/report.json` | `a882b7ad40bf41df943cf23d7687f6de8cc622d8cf4d5f49b1d360e4bffdbaee` |
| Registry skill line limit                   | `/private/tmp/ejm-capacity-package-crash-checks-20261021/20261021T050645Z-cd016f8b-harness-gate/report.json`          | `6afffe957a75c7a689c375a080b4b2b0b20068329faf604ced392e98babdf1a6` |
| Package regression line limits              | `/private/tmp/ejm-capacity-harness-focused-checks-20261021/20261021T050929Z-4f7dca4e-harness-focused/report.json`     | `905985ec4552a1922c22fc6369eb04049a3a78520bb6fcda43bd704265644aaa` |
| Registry skill formatting after Rust passed | `/private/tmp/ejm-capacity-package-crash-checks-20261021/20261021T051334Z-d0a259e0-harness-gate/report.json`          | `245be122fb96774732b36cbb0845ff455b5c87dc19c405b08fdcb9a86ae0daef` |

The registry skill now links to the existing root resource and public-docs rules
instead of repeating them; its unique registry, security and browser guidance
remains. This policy-only cleanup adds no public runtime behavior. The later
passing gate verifies its length and formatting without weakening either rule.

The separately rebuilt harness site passed `check` and `budget`, with the
unchanged 95 KiB wire budget. Receipt:

- `/private/tmp/ejm-capacity-consumer-required-gates-20261020/20261021T050054Z-d78bd218-harness-site/report.json`
- SHA-256 `c4eee5bff509b8fa2f3a26501ceacb4030648b8add70af41baf80c846135513a`

This is a local build of the current guide content, not a new browser interaction
run or site publication. Earlier shared-docs browser QA remains separate.

The consumer CLI gate still fails. It passed strict Clippy and 184 unit tests
with five ignored fixtures, then passed 21 of 22 integration tests. Package
enable immediately after install failed in
`startup::startup_failures_reap_all_loaded_package_hosts` with
`another package operation is running` and a would-block lock error:

- `/private/tmp/ejm-capacity-consumer-required-gates-20261020/20261021T050503Z-92ed1162-cli-gate/report.json`
- SHA-256 `e86f2b9280473c518487bc4eb1de4908e4fee37063aa0bd5ffdd8522b24a1c6d`

Review found that the package manager returns a bare locked file. Closing that
handle does not explicitly unlock an open file description retained by a
duplicate. This is a concrete lifetime gap; concurrent inheritance is a plausible
explanation for the gate failure, but the log does not prove that origin. The
proposed existing-`scopeguard` repair and duplicate-handle regression await the
owner's OpenSpec plan approval. No production repair or retry workaround is
included at this checkpoint. The consumer gate remains failed.

Host-wide activation stays gated. Linux live cgroup moves, native Linux/Windows
crashes, startup peaks, full process-tree and settled idle costs, package
performance, trace overhead/correlation, repeated fixed sweeps, changing pressure
and learned restarts remain open. No defaults, releases or publication changed.

## Frozen release and paired trace diagnostic on 2026-10-21 UTC

The new release build passed Clippy, helper and internal fixture builds,
configuration compatibility, hard-envelope equivalence, native observer build,
and fixed/automatic shared-root smoke cases. The fixture is a release-profile
libtest image; it adds no user-facing fake provider or demo mode. Receipt:

- `/private/tmp/ejm-cli-capacity-release-build-20261021-trace/20261021T053104Z-0f5729dd/report.json`
- SHA-256 `40aa2811b91b17f33aa174a5bb8267b715076e3205812c356e610e6dd8de8235`

The frozen helper has 18,109,872 bytes and SHA-256
`c386f2c07e9242284b3fff705d7b55a262e03cc6d02d2bf737587fb2fdc11d4a`.
The fixture has 19,637,392 bytes and SHA-256
`2ad9dd2ec4a35f454b201f3c22ea50e119ec39bcf402b136ab28692d7e01073f`.
These are tested build inputs, not evidence of package publication.

The subsequent diagnostic ran three alternating trace-off/trace-on pairs, each
with a 30-second active window, two participating processes, 100 managed agents
per participant, separate session directories and a common host-capacity root.
Both variants used the same frozen binaries, fixed hard envelopes, interactive
work and input without Stop. Only trace machinery differed. Receipt:

- `/private/tmp/ejm-cli-capacity-comparison-20261021-trace/20261021T053409Z-dc49b81a/report.json`
- SHA-256 `ade007cefd327919c5869b78ca08543850f876580d32e6edd3fa8d65acdf6d68`

| Pair | Trace off, completed turns/s | Trace on, completed turns/s | Lower traced rate |
| ---- | ---------------------------: | --------------------------: | ----------------: |
| 1    |                    39.695738 |                   36.666799 |           7.6304% |
| 2    |                    36.667732 |                   29.687817 |          19.0356% |
| 3    |                    44.400595 |                   39.999979 |           9.9112% |

The run spread was 19.4803% without tracing and 28.1240% with tracing. Every
participant had 100 pending turns cancelled at cutoff. Hypothetical pending-count
intervals are not confidence intervals or observed drain throughput. The lower
traced rates are observed results; these short noisy pairs do not isolate causal
trace cost or establish acceptable overhead.

All six useful workloads and their cleanup checks passed, with zero active/drain
failures. The six traced participant reports remain `diagnostic_only`, with 44
receipt and 44 commit samples in total. They report zero lag, closure, observer
errors, missing commit stamps or omissions; each participant made two selection
changes and three subscriptions. These counts do not establish first-write,
selection-correlation or latency acceptance.

Native discovery was not error-free: the six cases retained 4, 4, 3, 4, 1 and 4
`registry_snapshot` errors, respectively, each reporting `Process identity
unavailable`. Most had native result 1/errno 3; one had result 2/errno 0. Their
cause is not established. Every case has empty cleanup errors and unresolved
client/helper identity lists and passed its scoped cleanup check. That does not
prove universal native descendant containment.

Both reports record stable full CLI/harness sources, reaped commands and
observers, and the unchanged 10 GiB free-space floor. Minimum free space during
the diagnostic was 23,231,811,584 bytes. Reports and binaries remain immutable;
subsequent source edits need their own checkpoints. The terminal observer's
120-second active-window cap remains unchanged. No task is closed by this run.

The harness site also passed a later local `check` and `budget` after the commit
clock wording correction:

- `/private/tmp/ejm-capacity-consumer-required-gates-20261020/20261021T052718Z-769e01ab-harness-site/report.json`
- SHA-256 `b131edd2b492a825870700a8cfabd57371312fa09a5cef215126630151a17423`

Its harness/UI sources stayed stable and all direct children were reaped. This
local build adds no browser or publication claim. The latest CLI runtime gate
still fails on the package mutation lock described above. I/O, complete peak and
exited-process costs, stable latency tails, scale acceptance, automatic defaults,
release and publication remain unverified.

## Package mutation-lock repair on 2026-10-21 UTC

The owner approved the operation-lock amendment. `PackageManager` now wraps
each acquired mutation file in `scopeguard` 1.2.0 with default features disabled
and attempts `File::unlock()` before closing it on return, error or unwind.
Install, enable, disable and remove retain their acquisition and contention
behavior. Windows keeps its separate pinned directory and lock anchors.
Native participant/helper lifetime witnesses retain their close-only protocol.
Unlock is best effort; abrupt process death can bypass the destructor.

The new private regression retains a duplicate file description, verifies
exclusion while the operation guard lives, then exercises public enable and
update after guard drop. It would fail with the prior bare locked file.
The exact CLI startup regression also passed. This closes the concrete guard
lifetime gap; a successful rerun does not prove the cause of the earlier
install/enable failure or diagnose descriptor inheritance across exec.

All six guarded runs below passed on macOS arm64. Every recorded source root
stayed fully stable, and all direct children were reaped. Cargo used at most
two jobs, locked offline dependencies, no incremental output and the existing
10 GiB free-space floor. Receipts:

- Focused retained-description regression, exact CLI startup regression and
  locked feature graph:
  `/private/tmp/ejm-capacity-package-lock-checks-20261021/20261021T062910Z-6f75d1b6-package-lock-focused/report.json`,
  SHA-256 `22612d289f187f707a422ab94551e3f0d88e3d8613a8d5899569ea5f3423cb9a`.
- Full harness `./scripts/check`, including current worker and both SDK suites:
  `/private/tmp/ejm-capacity-package-crash-checks-20261021/20261021T063039Z-2d22bffe-harness-gate/report.json`,
  SHA-256 `dd35fad60a5d9fc06b9f6f197d97c11e2029d1ad88b03e55117b0aa99b0a5814`.
- Full CLI `./scripts/check`, including all 22 integration cases, native relay
  fixtures and nine browser cases:
  `/private/tmp/ejm-capacity-consumer-required-gates-20261020/20261021T063354Z-95d9549c-cli-gate/report.json`,
  SHA-256 `740521e487f956dfd3f48d84a41619a6b2bb6f6999be9116d707520969bdc37f`.
- IDE `cargo check --locked --offline --tests --jobs 2 -p ejm-ide`:
  `/private/tmp/ejm-capacity-package-lock-ide-check-20261021/20261021T063747Z-39910459-ide-compatibility/report.json`,
  SHA-256 `6db355b0ae76c6747354df0e59f3b85884e9aea6aa2d054ccc7fbc6b5b7b42cd`.
- Harness site `check` and `budget`:
  `/private/tmp/ejm-capacity-consumer-required-gates-20261020/20261021T063954Z-684e36e8-harness-site/report.json`,
  SHA-256 `2dc159ed72720c2a127489fe229ea9875f25e3ae017f94dd9b9751563a6b2480`.
- CLI site `check` and `budget`:
  `/private/tmp/ejm-capacity-consumer-required-gates-20261020/20261021T064057Z-09d67e98-cli-site/report.json`,
  SHA-256 `1525424e59d339269c87205d000078188e332d8fafff4db911f25a9db2b42a2e`.

The dependency adds one direct edge to already locked `scopeguard` 1.2.0,
with no new resolved package. Harness and CLI locks include that edge. The IDE
consumes core/auth/exec, has no package-manager dependency, and needs no source
or lock change. Its compile check covers current public consumers, not IDE
runtime acceptance. Canonical package docs, public developer guidance and the
dependency decision now describe operation-end unlock and its limits. The
latest site builds include those docs and the CLI's selected-view diagnostics;
their budgets remain unchanged. These local checks establish no publication.

Task 2.7 is complete. The broader change remains open. Native Windows shared
ownership, native Linux/Windows crash evidence, the real Linux cgroup-move
fixture, helper startup-peak calibration, full repeated performance acceptance
and automatic-default activation remain outstanding. The synthetic domain-move
test does not establish a real Linux move. Earlier frozen measurements retain
their original images and limits. No acceptance task beyond 2.7 is closed here.


## macOS recovery and helper source bridge on 2026-10-21 UTC

The owner narrowed activation to relevant native recovery and calibration,
then one representative workload. The source bridge now closes the macOS
arm64 recovery and helper-calibration scope. Linux and Windows still require
their own native evidence. Earlier open statements in this file retain their
checkpoint scope.

Machine-checked receipt:
`/private/tmp/ejm-capacity-critical-recovery-bridge-20261021.json`,
SHA-256 `0b9cd9030b581c8f46466884cf02ba7f0154551a62c73c5075290399955467b8`.
The retained Python standard-library adapter is
`/private/tmp/ejm-capacity-critical-recovery-bridge-20261021.py`.
It ran no build, test, native fixture or benchmark.

All 133 selected source identities match across the relevant calibration,
recovery, current required-gate and live-source checkpoints. They cover the
executor and native helper, core capacity registry, package-runtime admission,
vendored filesystem repair, worker early helper entry, and CLI entry and
extension composition. CLI and harness keep their public dependency direction.
The receipt retains every compared path and hash, complete manifest references,
report hashes and all changed dependency bytes used for the bridge. It makes
no claim that every linked source, compiler or ignored dependency input is
unchanged.

The dependency comparison removes only the new `scopeguard` workspace and
package declarations and the `ejm-harness-packages` edge in each lockfile in
memory. Each reconstructed file hashes exactly to its measured predecessor.
No resolved package, version or checksum changed. This adds an operation-end
unlock in the package manager only; helper and participant lifetime witnesses
keep their close-only protocol. The latest harness and CLI gates and focused
retained-description regression cover the amended package path.

The recovery receipt remains
`/private/tmp/ejm-capacity-current-owner-recovery-20261021/20261021T042249Z-85ad0f2e/report.json`,
SHA-256 `ccceeaf1621139a8cea8094ebc834f74edfe6b9c604f690f382835c91d30da7e`.
Its fixture image remains
`e7f54f89df101f567416328f8f915e97c46de1534604a18208f058a376bd6c45`.
It proves the recorded idle/busy client and owner recovery, paused-owner
exclusion, cancellation/drop ownership, launcher crashes, inherited witness
release, child noninheritance and useful fresh-grant progress. Its successful
cleanup and sampling limits remain those recorded above.

The current full harness gate is
`/private/tmp/ejm-capacity-package-crash-checks-20261021/20261021T063039Z-2d22bffe-harness-gate/report.json`,
SHA-256 `dd35fad60a5d9fc06b9f6f197d97c11e2029d1ad88b03e55117b0aa99b0a5814`.
The bridge verifies its passed witness, generation, boot, failed-activation,
uncertain-reply and package import/reload/caller-death cases. Existing CLI,
worker, SDK and IDE checks retain their original scope and hashes in the
receipt. The IDE result establishes consumer compilation only.

The calibration retains its exact historical CLI image
`79193b2d6de3032f6c74d74e8a4d4b466c9b4a4b4cbe0345ce91ef3bc1c1199e`
and worker image
`1ca0a09f388406318f66221c25b80be66bef4331c24fb85e46daa45d55376dcc`.
The largest startup lifetime RSS was 7.625 MiB and largest retained
control-state lifetime RSS was 14.703125 MiB. The conservative helper charge
stays 32 MiB. The source bridge reuses those measurements without relabelling
later binaries as measured. Exact-image samples and failed attempts remain in
the current-image evidence bundle. No eight-production-helper saturation or
full process-tree peak is claimed.

Detached macOS descendants can survive owner death, as the fixture showed.
Recovery remains monitored: uncertain survivors retain accounting, paused or
uninspectable owners keep their commitments, heartbeat expiry cannot authorize
takeover, and uncertain launches are never retried. This does not certify full
cleanup, total native-memory bounds or workspace safety. The real cgroup move
in harness task 3.2 and Linux recovery gate Linux activation. Windows shared
ownership and native recovery gate Windows activation. The temporary
`rightkit-fs` patch still blocks publication pending maintained upstream.

Representative performance acceptance, automatic defaults, wider platform
acceptance, release and publication remain pending. This verification-only
bridge changes no runtime or public contract. Existing public native limits
and gated-default guidance remain accurate, so no public guide or site build
changed for this closeout. The earlier local site checks retain their original
scope; no live publication is claimed.

Harness tasks 3.7 and 3.11 are complete for macOS arm64. Task 3.2,
representative acceptance in 8.4 and default activation in 8.1 remain open.

## Maintained fork and renewal diagnosis, 2026-10-21

The owner authorized an ejmlabs-maintained fork to remove the upstream-release
wait. Source is public at https://github.com/ejmlabs/ejm-rightkit-fs. Initial
commit: `3348ce4d2ddac4e61380e86e6a461b3dbdb7ed64`. Production source and tests
(all 69 files) match the reviewed patched 0.4.4 tree. Standalone macOS state and
ACL suites passed 35 tests; Cargo verified the packaged crate with `fs` enabled.
`cargo publish` stopped before upload because no crates.io credential is
configured. Consumer pins remain unchanged pending verified registry retrieval.

The corrected comparison run at
`/private/tmp/ejm-capacity-critical-comparison-20261021-v8/20261021T144819Z-9acba9af/report.json`
passed the three fixed sweep cases and first fixed trial, then failed automatic
inactive execution: five native command admission timeouts per participant.
The failure preceded the injected stall. Shared renewal failed near 14 seconds,
contact expired near 22 seconds, and queued commands reached their ten-second
deadlines. Cleanup passed without unresolved participant/helper identities.
No throughput acceptance or default activation is claimed.

Core now reports fixed nonsecret renewal failure categories without changing
grants, contact expiry, deadlines or scheduling. The focused release-profile
`capacity::tests::renewal_failure_categories_preserve_causes_without_exposing_error_text`
regression passed; formatting and strict OpenSpec validation passed. The full
gate was not repeated for this diagnostic addition.

Old v4 generated journals were losslessly compressed after checking they were
closed. Each archive was decompressed and SHA256-verified before replacing its
original. Restore manifests are `/private/tmp/ejm-capacity-evidence-compression-20261021.json`,
`/private/tmp/ejm-capacity-evidence-compression-20261021-part2.json`, and
`/private/tmp/ejm-capacity-evidence-compression-20261021-rest.json`.
Original report and binary evidence remain unchanged.

## 2026-10-21: current-protocol startup and public fork integration

The continuation at `/private/tmp/ejm-capacity-critical-comparison-continuation-20261021/20261021T151855Z-0675b6ed/report.json` failed startup with SQLite busy during schema validation. Its cleanup verdict remains failed: observer processes were reaped, recorded helper identities ended and their witnesses became available, but a failed participant omitted the fixture result. No acceptance is claimed.

Current-protocol open now reads the version without schema writes. Creation and migration retain their immediate transaction and locked recheck. Release-profile regressions `current_protocol_opens_while_a_wal_writer_is_active` and `protocol_mismatch_and_database_replacement_fail_closed` passed on this Mac. Busy budgets and deadlines remain unchanged.

Harness, CLI and IDE Cargo metadata resolve `ejm-rightkit-fs` version `0.4.4-ejm.1` at public commit `ad6faff2442ffe6abccd1856f4c0df9a3e472035`. Harness and CLI `cargo deny --offline check sources` passed with only that Git repository allowed. Source/native publication no longer requires crates.io credentials; Cargo registry publication remains gated. These results do not establish final workload acceptance or a product release.

## 2026-10-21: helper startup retirement

The v10 continuation completed 6,777 useful runs in its first automatic case but failed one run because its helper exited before startup. Cleanup passed with no unresolved helper or client identities. Renewal diagnostics also observed transient database contention that subsequently recovered. This does not establish workload acceptance.

The helper may retire after two idle seconds while its launching client is unscheduled. Startup now permits one rediscovery only after a verified successful child exit, before any command submission. Error exits remain failures and include exit status. Each attempt reacquires ownership and generation admission. The release-profile regression `startup_exit_retries_only_clean_exit_and_remains_bounded` passed on macOS, covering clean-exit retry count, immediate error-exit failure and released ownership. Full workload validation remains pending.

### 2026-10-22 representative acceptance and probe repair

The exact integration checkpoint passed. All nine planned cases completed with successful cleanup, and the one permitted extra triplet also completed. Median automatic throughput loss across all three pairs was 26.3257 percent, above the unchanged 10 percent limit. Latency and median terminal-overhead gates passed. Do not enable automatic defaults or claim throughput acceptance. The evidence is retained in `/private/tmp/ejm-capacity-critical-comparison-20261022-extra-pair/20261022T102724Z-75877202/report.json`.

Controller samples showed repeated rejected one-slot recovery probes. The bounded wider-probe repair passed all 18 controller regressions, including latency, grant refusal, unknown pressure and plateau recovery. Representative acceptance of this repair remains pending.

## Final macOS representative acceptance and default activation, 2026-10-22

The repaired candidate passed the predeclared representative gate. Exact report:
`/private/tmp/ejm-capacity-critical-comparison-20261021-integration/20261022T104153Z-a69f5df4/report.json`.
See `docs/capacity-integration-checkpoint.json` for its hash, host, inputs, paired
results, latency samples and source bridge. Median automatic throughput loss
was -1.1594%; median terminal overhead was 1.8826%. All commands and observers
were reaped. Pressure recovery, learned restart and reservation bounds passed.
The second terminal-overhead pair was 8.29%; the declared median gate passed.
Earlier failed trials remain evidence. Host fixed rates changed between runs,
so these results do not isolate the effect of the controller repair.

The CLI parser and TypeScript/Python SDK tests passed for macOS automatic
omission and explicit fixed mode against the actual release worker. SDK build,
ESLint and Python Ruff checks passed. Version 0.2.0 and docs changes do not alter
the measured explicit-auto path. No second workload matrix was run.

The public filesystem fork and consumer lockfiles satisfy source distribution.
Registry publication remains separate. Linux activation, other platform native
acceptance and broader hardware/idle claims remain open. Required protected CI,
human review, main merge and source-release dispatch are not complete.
