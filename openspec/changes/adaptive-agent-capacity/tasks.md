# Tasks

## Critical path

The owner narrowed remaining verification on 2026-10-21. Reuse passed evidence
for unchanged runtime and dependency paths; no deferred measurement counts as a
pass. Keep existing checked tasks and safety contracts intact.

1. Close only uncovered host-budget and native recovery risks for the platform
   being activated (3.7 and the affected parts of 3.11).
2. Use one representative 1000-total-agent workload across two processes for
   useful-throughput, pressure/restart and responsiveness evidence (8.4).
3. Activate that platform's new SDK defaults through 8.1 and the paired CLI
   change, then run the normal gates for actual code changes.

The real cgroup move in 3.2 gates Linux activation; Windows shared ownership
gates Windows activation. Neither blocks a verified platform's default. The
original three-platform goal stays open until those native dependencies pass.
Full scale/hardware/provider/package-language matrices, precision idle/I/O
profiling and all-eight production-helper saturation are follow-up coverage
unless a concrete failure or changed path requires them. Existing focused
4096-run admission/cancellation evidence remains relevant. The exact-commit public fork permits source/native builds; Cargo registry
publication still requires a verified registry dependency.

## 1. Establish the measurement and compatibility baseline

- [x] 1.1 Extend the existing benchmark envelope to describe 100/1000/4096 managed runs, independent model/tool activity, and participating process counts; verify fixed-mode runs reproduce current admission and record current rejection points without presenting them as supported scale.
- [x] 1.2 Add bounded per-lane timing and outcome instrumentation for queue, preparation, provider, journal and execution time; verify local fixtures separate these intervals, exclude retries/failures from useful throughput, and measure tracing overhead.
- [x] 1.3 Record baseline commands, release profile, host/limits, binary/dependency size, process-tree memory, CPU, I/O, p50/p95 and raw samples in benchmark evidence and `docs/benchmarks.md`; verify repeated identical workloads and label missing target-hardware evidence.
- [x] 1.4 Add compile/contract fixtures for existing constructors, configuration literals, custom providers, status/event types and worker request shapes; verify old worker requests lacking capacity retain fixed behavior and check current CLI/IDE public consumers before changing runtime defaults.

## 2. Add effective targets and host observations in their owners

- [x] 2.1 Add additive public capacity policy/resolution, a prepared capacity context and a separate capacity snapshot without required fields in old public structs; verify fixed constructors retain behavior, explicit ceilings survive resolution, the context can precede provider/hooks construction, and old global subscriptions compile.
- [x] 2.2 Add source/freshness/availability-bearing host observations using existing maintained native dependencies, including applicable CPU/affinity, memory, process and descriptor limits; verify bounded fixtures for tighter and nested Linux constraints, failed probes, stale values and unsupported scopes.
- [x] 2.3 Add atomic local operating-target/grant generation installation and one memory account, including memory-only leases, beneath immutable executor limits; verify mixed model contexts/tools/output/package hosts cannot double-spend memory and concurrent target shrink, invalid updates, cancellation, drop and spawn failure preserve all charges.
- [x] 2.4 Implement bounded feedback windows, demand-based slow increases, prompt pressure reductions, hysteresis and cooldown per lane; verify deterministic clock/pressure fixtures avoid idle probes, reject false gains, separate provider delay from CPU pressure, and stop growth on missing critical observations.
- [x] 2.5 Update `docs/executor.md`, `docs/implementation-contract.md`, public developer guidance and affected execution/performance skills for caps, targets, observations and native limits; verify rustdoc/examples and existing documentation checks without claiming tested OS coverage from parser fixtures.
- [x] 2.6 Add governed package runtime setup using that same public context and preserve old `RuntimePool::new`/`RuntimeConfig` contracts; verify no lazy Node/Python spawn or import precedes startup admission, idle host memory stays charged without active job slots, reload accounts for overlap, and failed import/final construction, cancellation and shutdown release only after cleanup. Update `docs/packages.md` and public package-host examples, and measure startup/idle host overhead without adding a second controller or dependency cycle.
- [x] 2.7 Make package registration locks operation-scoped with `scopeguard` 1.2.0 and default features disabled, attempting explicit unlock before handle close on return, error or unwind. Verify exclusion while the guard lives and immediate public enable/update progress after guard drop with a retained duplicate. Preserve Windows pinned anchors, acquisition/contention and native lifetime witnesses without explicit unlock. Update canonical/public package docs, check the dependency and lockfiles, and rerun the CLI startup regression plus worker/IDE compatibility; retain best-effort unlock and abrupt-death limits.

## 3. Coordinate one host budget across processes

- [x] 3.1 Add the protected common local capacity registry in core using existing SQLite/private-state APIs, independent of journal directories; verify two real processes can register concurrently, unsafe path/ACL/link cases fail, live protocol versions cannot fork into parallel registries, and metadata stays bounded.
- [ ] 3.2 Implement hierarchical domain charging, environment-generation fencing, fair process demand and one controller claim per domain/window; verify separate journals cannot multiply budgets, tightening creates drain debt, and a live cgroup move charges the new ancestor scope before old charges are released without double-counting common ancestors. Native cgroup-move evidence gates Linux activation only; reuse unchanged hierarchical accounting and move-fencing fixtures.
- [x] 3.3 Implement charge-before-install increases and install-drain-CAS-release reductions with idempotent operations and stale-generation rejection; verify crash/lost-reply injection at each boundary never makes the same credits spendable twice.
- [x] 3.4 Implement admission contact deadlines, retained OS lifetime witnesses and verified-death monitored recovery; verify missed heartbeats and process suspension preserve live grants, actual owner death retires future allowance without claiming descendant cleanup, inspection failure stays unknown, and boot recovery preserves provider charges with no independent fallback budget.
- [x] 3.5 Keep ordinary local leases, per-job lifecycle and streamed events off the shared ledger; bound registry waits/demand updates and drain before graceful release. Verify no per-token or per-native-job recovery writes and responsive cancellation during contention or coordinator failure.
- [x] 3.6 Document stable user/resource-domain scope, version skew, private lifetime links, verified-death measured recovery and observed external load in canonical/public guides; verify docs distinguish scheduling allowance retirement from proven cleanup and describe macOS workspace/descendant limits.
- [x] 3.7 Close only uncovered native recovery risks before a platform activates its automatic default. Reuse relevant idle/busy client/helper crash, paused-owner, controlled command/interpreter cleanup, cancellation and OS ownership-release evidence when the exercised runtime/dependency paths remain unchanged. Verify bounded safe useful progress, retained charges for unproven survivors, no heartbeat takeover and no uncertain launch retry. Record descendant limits; native evidence establishes only its own platform.
- [x] 3.8 Implement the public native supervision contract: lazy exclusive execution owner, bounded private IPC/lifetime links, shared event loop, caller environment/cwd, generation-scoped operation IDs, no uncertain launch replay and idle exit. Route admitted commands and resident package generations through public scope APIs, preserve legacy constructors and measure startup/dispatch/memory/process/descriptor costs without per-agent helpers or mandatory VM/container setup.
- [x] 3.9 Retain native children, pipes, leases and workspace mutation ownership through supported cleanup in bounded live state. Verify owner/client EOF, cancellation, dropped callers, spawn failure, lost replies, stale epochs, owner crash during command/import/reload, and escaped macOS descendants preserve truthful outcomes without a durable per-job execution ledger. Reject protocol/domain mismatches and stale operation retries before effects.
- [x] 3.10 Expose public early helper dispatch and cached native capability/status contracts for CLI and worker, plus native lifecycle/recovery documentation for macOS, Windows and Linux. Verify no helper starts during ordinary doctor/status, compatible resource domains and private endpoints, explicit unsupported capabilities, stable lock identity, bounded idle exit and no surviving-owner takeover on timeout; label untested native paths honestly.
- [x] 3.11 Add separate coarse native-helper commitments through a public executor admission/permit boundary implemented by core’s existing registry. Bound occupied generations at eight; use a provisional 32 MiB baseline pending target-specific startup calibration; charge applicable ancestors once without eager deductions for unused slots or per-job writes. Stage bounded local governor demand before native readiness, preserve ticket/FIFO/age across retries, select a safe pending-helper subset that protects a useful claimant, and atomically activate under the expected grant generation. Drop unused readiness before every failed-activation or resource wait, including reused helpers. Transfer package startup’s 64 MiB resident plus 16 MiB scratch working set once into actual leases, preserving reload overlap and cleanup ownership. Acquire the Unix ownership witness before process creation, inherit its locked open file description, retain it through helper exit, and exclude it from user children. Verify saturated first-start and reused-idle-owner progress, distinct-domain contention, failed activation, generation changes, staged cancellation/drop, compatible clients sharing one charge, retained resident floors, pre-spawn cancellation releasing only after witness release, launcher crashes before/after creation, paused startup, failed replies, spawn failure, idle exit, descendant noninheritance, slot exhaustion, tighter ancestors, protocol skew and boot recovery. Preserve provider windows, legacy constructors and the Windows shared-native gate. Update canonical/public docs, measure startup peaks and pool overhead, and check CLI/worker/SDK/IDE public consumers. Adopt the checksum-pinned temporary `rightkit-fs` 0.4.4 vendor patch for direct final-name permanent lock creation and strict same-handle privacy checks through one version-and-path workspace dependency. Retain failed or unsafe lock entries without deletion/replacement, omit DELETE access from Windows permanent-lock handles, and add no sync or power-loss claim. Verify deterministic concurrent first visibility, publisher death and actual privacy refusal, preserve patch/source/license provenance, check public consumers, and gate Cargo registry publishing until a verified registry release of the approved ejmlabs-maintained fork or equivalent maintained upstream replaces the override; permit GitHub source/native builds with a verified exact-commit public fork and repository-specific dependency allowlist. Reuse relevant accounting, startup and retained-state evidence with a recorded source/path bridge; recalibrate only affected helper execution or dependency paths, retain original image labels and conservative charges, and require no all-eight production saturation run without a concrete uncovered risk.

## 4. Bound runnable work and retain fair progress

- [x] 4.1 Move memory/model admission before full context/request preparation and release model credits before unrelated tool waits; verify large queued contexts retain bounded metadata, cancellation during preparation releases all credits, and hook/steering/provider ordering remains valid.
- [x] 4.2 Add bounded count/byte pending dispatch to feed executor waiters without burst queue errors or retry loops; verify 1000- and 4096-run tool bursts, hard managed-envelope rejection, queue deadlines and mass cancellation preserve accepted work and terminate within their bounds.
- [x] 4.3 Add automatic-mode bounded backfill and aging while preserving within-session FIFO and old fixed behavior; verify heavy-head progress under continuous small jobs, fitting work uses spare capacity, impossible heads reject clearly, and shared process fairness holds.
- [x] 4.4 Measure queue lookup/cancellation and sampler descendant traversal at target scale; if either exceeds its budget, add indexed bookkeeping/shared sample indexes and verify the same identity, fairness, cleanup and cancellation fixtures plus before/after evidence; otherwise record why no rewrite is needed.
- [x] 4.5 Raise the managed-run validation envelope only after the bounded fixtures support the 4096 target; verify status distinguishes managed/runnable/waiting/executing work and same-workspace writes remain serialized.
- [x] 4.6 Update executor, engine and public guides with managed versus active capacity, backpressure outcomes and automatic fairness behavior; verify examples and owning performance skills match the implemented limits rather than the proposed targets.

## 5. Integrate provider capacity without unsafe retries

- [x] 5.1 Add a compatible optional typed feedback/quota-domain path to provider APIs and built-in transports; verify old custom providers compile and local HTTP fixtures report validated throttle/usage/retry metadata without debug capture or raw secrets.
- [x] 5.2 Add shared model-domain grants and supported request/token-rate allocation with conservative uncertain-usage handling; verify multiple processes sharing an opaque quota identity cannot independently spend its budget and unrelated domains remain available.
- [x] 5.3 Implement coordinated bounded cooldown/re-entry and cancellation within deadlines; verify retry hints, synchronized expiry and quota changes avoid retry storms, and restart/reboot retains unexpired provider charges without refunding uncertain usage or claiming remote inference ended.
- [x] 5.4 Update provider references, SDK guidance and public configuration/recovery docs with quota scope, unknown hints and locally tracked request limits; verify offline examples and existing provider/auth/continuation fixtures preserve account identity and effect contracts.

## 6. Add bounded local learning

- [x] 6.1 Add versioned aggregate profiles keyed by relevant environment/domain/workload, with confidence, decay, expiry, count/byte caps and bounded writes; verify 256-profile/1-MiB goals, concurrent updates, old versions and eviction without retaining content or scanning session history.
- [x] 6.2 Revalidate cached startup targets against current observations, shared grants and hard caps; verify cold/learned starts, tighter hardware/container limits, workload changes, stale/corrupt state and learning-write failure preserve safe admission and ordinary session data.
- [x] 6.3 Add public learning status and local reset through core-owned persistence; verify reset during active work preserves leases, credentials, sessions, artifacts and ceilings, and neither tools nor remote-controller input gains capacity authority.
- [x] 6.4 Document local profile contents, storage limits, expiry, reset and separate coordination-versus-learning failure behavior in canonical and public guides; verify documented recovery through isolated fixture state and review for private-content leakage.

## 7. Bound journal and host-view work

- [x] 7.1 Use baseline traces to evaluate bounded compatible group commit before changing stream persistence; if needed, implement measured batching and verify durable ordering, post-commit publication, final text, effect/run barriers, slow-store backpressure and cancellation; otherwise preserve current writes and record the evidence.
- [x] 7.2 Add indexed bounded session paging/search and a consistent authority/presentation-tail projection with run identity, last transition, durable cursor and omission marker; verify large old journals, stable continuation, full-set prefix ambiguity and transactional projection/cursor consistency.
- [x] 7.3 Add bounded selected-session live subscriptions and aggregate status, with matching selected-stream continuity/snapshot anchors; verify interleaved global sequences do not imply loss, actual loss recovers, snapshot races deduplicate correctly, subscriber state stays bounded and old subscribers still work.
- [x] 7.4 Update storage/session/API references, host integration, SDK developer guidance and public history/recovery docs; verify old-journal upgrade behavior, displayed omission semantics and the canonical cursor examples against deterministic fixtures.

## 8. Adapt the worker and SDKs and complete integration evidence

- [ ] 8.1 Add optional worker capacity configuration and public status/reset/session-view operations, preparing and reusing coordinated capacity before package imports through final harness construction; after that platform's critical recovery and representative-workload gates pass, make its new SDK defaults explicitly request auto while old worker omissions stay fixed; regenerate wire types and verify behavioral compatibility, startup failures, ceilings, shutdown and TypeScript/Python parity offline.
- [x] 8.2 Update Rust/worker/SDK examples and public user/developer guides with the implemented defaults, fixed mode, limits and recovery; verify every authored example and generated-contract freshness check without claiming package publication.
- [x] 8.3 Coordinate the paired CLI integration through public exports and run `cargo check --locked --tests` in CLI and IDE with Cargo jobs at most two; verify owner versus unrelated failures, preserve other work and report any unchecked downstream contract.
- [ ] 8.4 Use the existing offline benchmark and paired CLI fixture for one representative 1000-total-agent workload across two processes with separate stores, fixed executing work, bounded selected output, background events, one pressure step, one learned restart and cancellation. Reuse existing boundary/contract evidence. Compare automatic capacity with a three-point safe fixed sweep under identical constraints, retaining the existing 10% useful-throughput tolerance; consumer evidence retains 50 ms input/Stop/event-to-paint p95 and 5% active-terminal overhead targets. Use two alternating paired trials, at most one extra pair for noise, then investigate any unresolved concrete cause. Record available process-tree observations, bounded shared reservations, exact inputs, raw results, source/calibration bridges and cleanup. Leave full scale/hardware/provider matrices and precision profiling unclaimed follow-ups; update affected benchmark guidance when activation changes behavior.
- [x] 8.5 Run focused executor/core/provider/store/SDK checks, formatting, strict all-target Clippy and `./scripts/check` with `CARGO_INCREMENTAL=0` and `CARGO_BUILD_JOBS=2`; verify the full gate or report each actual failure without weakening policy or reverting concurrent edits.
- [x] 8.6 Run the existing site checks/budgets, review affected local pages and release notes, validate both changes with `openspec validate --strict`, and review the final public dependency graph; verify docs distinguish planned, implemented, tested, released and published behavior and no copied host controller or unmeasured claim entered the patch.

## Workflow follow-up

- After required verification and review, sync completed requirements through `openspec-sync-specs` and archive both coordinated changes through `openspec-archive-change`.
- Keep commits, pushes, package/site publication and deployment separate and perform them only when the user asks.

## Maintained filesystem fork (authorized 2026-10-21)

- [ ] 9.1 Package the reviewed filesystem patch as `ejm-rightkit-fs`, retain licenses and provenance, verify the focused native contracts and crate archive, publish its source, replace the harness path override with the exact full public fork commit and repository-specific dependency allowlist, and refresh consumer locks. Verify source/native consumers; keep Cargo registry publication gated until a registry version exists. Preserve upstream PR #876 and migrate back after equivalent upstream verification.

Fork progress on 2026-10-21: source published at
https://github.com/ejmlabs/ejm-rightkit-fs (initial source commit
`3348ce4d2ddac4e61380e86e6a461b3dbdb7ed64`). All 69 source/test files match the
reviewed patched import. The standalone 25 state and 10 macOS ACL tests passed;
Cargo verified the unpacked crate with `--features fs`. Registry upload stopped
before publication because no crates.io token is configured. Use the verified exact Git pin for source/native consumers; registry publication
remains gated until registry retrieval verifies the new package.

- [x] 9.2 Report a nonsecret renewal failure category through existing core capacity status; verify error-chain classification and redaction without changing admission or contact expiry.

- [x] 9.3 Remove unnecessary current-protocol schema writes during registry open; retain locked version recheck for creation and migration, privacy checks and existing contention budgets. Verify opening under an active WAL writer and existing protocol rejection.

- [x] 9.4 Permit one pre-command rediscovery after verified clean helper startup exit; preserve caller deadlines, generation admission and lifetime witnesses, reject nonzero exits, and verify the bounded retry contract.

- [ ] 9.5 Let comparable, latency-safe probes with insufficient gain widen their next bounded step up to eight slots. Preserve all grant, pressure, latency and cooldown guards; reset exploration when evidence changes. Verify plateau recovery and existing controller regressions, then rerun representative acceptance before enabling defaults.
