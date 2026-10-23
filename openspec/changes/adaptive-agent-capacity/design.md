# Design

## Context

See [the proposal](proposal.md) for the outcome and capability boundaries. This is a plan, not a runtime or performance claim.

The current source establishes these design seams:

| Source                                                                   | Current behavior                                                                                                        | Consequence                                                                                |
| ------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------ |
| `src/config.rs`, `src/engine.rs` in the core crate                       | Defaults of 100 managed runs and 8 model requests; hard maxima of 1000 and 128                                          | Several thousand managed runs need an explicit measured envelope, not just larger defaults |
| `ejm-harness-exec/src/governor.rs`                                       | One shared governor, immutable limits, multidimensional leases, session FIFO and rotation                               | Retain hard accounting and add separate operating targets                                  |
| `ejm-harness-exec/src/lib.rs`                                            | Four jobs, one heavy job, 512 MiB and 256 queue entries by default                                                      | Large bursts need upstream backpressure                                                    |
| `engine/driver.rs`                                                       | Full context is prepared before acquiring a model semaphore                                                             | Waiting runs retain unnecessary working sets                                               |
| `store.rs`, `engine/driver.rs`                                           | One bounded FULL-sync SQLite writer; each text delta awaits append                                                      | Measure journal wait separately; preserve durability when batching                         |
| `monitor.rs`                                                             | One lazy sampler; descendant discovery repeatedly scans the process set per job                                         | Raising job counts can raise monitor cost; profile and index shared sample data if needed  |
| `provider.rs`, `provider/responses.rs`                                   | HTTP failures lose ordinary scheduler quota feedback                                                                    | Add typed observations outside optional debug capture                                      |
| `ejm-harness-packages/src/runtime.rs`, CLI `extensions.rs` and `main.rs` | Package load can spawn a language host and import code before harness construction; the callback governor arrives later | Prepare shared capacity before loading packages and reuse it through construction          |
| `engine.rs`                                                              | Global live events and bounded pages, but no filtered subscription, tail snapshot, or paged session search              | Provide host contracts without putting presentation in the engine                          |

Existing evidence in `docs/benchmarks.md` covers 100 sessions and tiny local checks on one 24 GiB Mac. These source shapes are scaling risks; this exploration has not measured their throughput cost.

## Critical path

The owner narrowed remaining verification on 2026-10-21. Default activation needs three decisions: native recovery is safe, the representative workload meets its critical budgets, and the consuming CLI/SDK selects auto through the existing public contracts. Reuse passed focused evidence where the exercised runtime and dependency paths remain unchanged. Rerun only uncovered or affected risks, and retain failed or inconclusive results as such.

Each platform activates independently. Linux's real cgroup-move evidence and Windows' shared native ownership remain dependencies for their own defaults and support claims. They do not block another verified platform. Preserve the original three-platform goal, fixed legacy API behavior and honest capability status. The filesystem dependency must be publicly reproducible before source release. An exact-commit maintained fork may satisfy source and native builds; Cargo registry publication separately requires a verified registry dependency.

## Goals / Non-Goals

**Goals:**

- Apply OS scheduling ideas at the appropriate level: hierarchical budgets, lightweight waiting work, fair runnable queues, working-set admission, and feedback under pressure.
- Find the lowest concurrency near the best sustainable successful throughput, with headroom for the OS and foreground interaction.
- Coordinate participating processes for the same user and visible resource domain, independent of their journals, without a permanent product broker.
- Run native work through one lazy execution owner per compatible user/resource domain, with bounded live scopes and private lifetime links for controlled helpers. Use no VM or per-job recovery database.
- Keep local job, heavy job, model, provider-rate, journal, and presentation pressure distinguishable.
- Preserve existing fixed integrations and the harness's effect, grant, cancellation, and recovery contracts.

**Non-Goals:**

- A custom kernel, VMM or container runtime, a complete security sandbox, distributed agents across hosts, remote telemetry, or a promise that every agent can execute simultaneously. Native estimates, sampling and process groups do not provide enforced total memory ceilings or universal descendant cleanup.
- Automatic model, account, reasoning effort, context-quality, tool-grant, token-budget, or spending-policy changes. Learning cannot increase owner authority.
- Copying a general-purpose OS scheduler into the harness, adding a thread per agent, or replacing Tokio's executor.
- An IDE feature migration. Its current integration is a compatibility consumer; nonparticipating versions remain observed external load.

## Decisions

### 1. Keep ownership and public compatibility explicit

| Owner                            | Planned public contract                                                                                                                                                                      | Consumers and dependency direction                                                                          |
| -------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| `ejm-harness-exec`               | Effective admission target and generation installation beneath `ResourceLimits`; native observations, bounded execution-owner transport, private helper dispatch and process lifetime scopes | Core, coding tools, native providers and package execution consume one governor and public runtime contract |
| `ejm-harness`                    | Additive capacity-policy construction and prepared capacity context, automatic hard-envelope resolution, separate capacity snapshot, local learning/reset, coordinated grants                | CLI, worker and SDK facades consume public APIs; no reverse host dependency                                 |
| `ejm-harness-packages`           | Additive governed runtime setup, startup/import admission and persistent language-host memory ownership                                                                                      | Packages consume core/executor public contracts; core does not depend on packages                           |
| `ejm-harness` provider adapters  | Optional typed feedback and opaque quota-domain identity                                                                                                                                     | Old providers work through a default compatible path                                                        |
| `ejm-harness` session/store APIs | Paged search, consistent bounded tail and authority state, selected-session subscription                                                                                                     | CLI views and SDK clients; durable state stays core-owned                                                   |
| Worker and SDKs                  | Optional capacity configuration and status/reset methods, generated schemas                                                                                                                  | Translate the same core behavior without a second controller                                                |

Do not add required fields to existing public configuration or event structs merely to enable this feature. Keep current constructors, `Governor::limits()`, `Harness::status()`, global subscriptions and provider implementations usable. Add a constructor that accepts capacity policy alongside optional provider and hooks, plus a separate capacity snapshot. New automatic resolution accepts explicit optional ceilings and returns resolved hard limits before construction; it must never ignore an explicitly supplied limit.

Provide an additive public prepared capacity context that can exist before providers, hooks, package imports or heavy context allocation. CLI and worker composition prepare it first, pass it to governed `RuntimePool` setup, then reuse the same context and governor in final harness construction. This avoids a startup cycle when package providers/hooks must exist before the harness. Preserve old `RuntimePool::new`, `RuntimeConfig` literals and harness constructors; they do not become coordinated implicitly. The context must not require a consuming host or package-runtime dependency in core.

`RuntimePool::new` currently starts no process, but `load` can spawn a lazy language host and execute its import. Governed setup must reserve startup/import work and persistent host memory before that spawn. Keep at most one shared host per language, with one owned native scope per interpreter generation. Idle hosts retain their memory and PID charge without occupying active callback/job slots. Reload must account for overlap until the old host scope finishes cleanup. Failed imports, cancellation, failed final harness construction and shutdown retain ownership through the supported native cleanup procedure. Native capabilities describe the interpreter scope; package grants and effect policy still apply. Include startup, idle language-host and native-owner cost in the same benchmark envelope.

Package registration mutations use a separate operation-scoped store lock owned by `ejm-harness-packages`. Preserve its acquisition, contention behavior and Windows pinned directory and lock anchor. Wrap the acquired operation file in `scopeguard` 1.2.0 with default features disabled and attempt `File::unlock()` before closing that handle when the operation scope ends. This prevents a surviving duplicate from retaining a successfully released operation lock. Unlock is best effort, and abrupt process death can bypass it. Native participant and helper lifetime witnesses keep their existing handle-close protocol without explicit unlock. Verify exclusion while the guard lives and successful public mutations after it drops while a duplicate remains. Keep public APIs and dependency direction unchanged, and check the CLI startup regression plus worker and IDE compatibility.

After the crash-recovery activation gate passes, CLI and new SDK composition select automatic coordinated mode by default. CLI fixed mode selects coordinated admission with fixed local targets. Existing library constructors retain static behavior and do not open a registry. Worker requests omitting the new capacity field retain legacy fixed behavior; new SDKs explicitly send automatic policy. Additive decoding alone must not change old request semantics. Check CLI and IDE compilation after public changes.

### 2. Separate hard envelopes, grants, targets, and observations

Use four distinct concepts:

1. Hard envelopes cover explicit user ceilings and detected host/container restrictions.
2. Shared committed grants assign a portion of the applicable host envelopes to each process.
3. Local operating targets restrict new work beneath each process's grant.
4. Observations describe actual sampled use with source, age, availability, and confidence.

For each resource dimension and applicable domain, new grants and increases must fit the current hard envelope after all charged grants are counted once. Quarantine changes a grant's state, not its charge. Local outstanding reservations must not exceed their committed grant. An external envelope decrease can leave existing charges above the new envelope; represent that excess as drain debt, block conflicting increases, and request cooperative reductions. A local target can likewise fall below outstanding reservations during drain. Report both conditions without erasing accounting.

Derive a conservative hard envelope from effective CPU availability, supported affinity/quota restrictions, memory limits, process and descriptor limits, and explicit foreground headroom. Recheck when the environment changes. If an external constraint falls below existing allocations, preserve their charges and stop increases while grants drain. Owner-launched work must remain in a compatible execution domain. Reject unsupported cgroup, namespace, Job or affinity placement rather than inheriting the first client's tighter domain silently. Charge the native owner baseline once and distinguish sampled host use from native enforced controls.

Use one balanced memory account per process beneath its shared memory grant. Charge baseline core state, admitted contexts/model buffers, native-tool estimates, retained output, journal queues and package hosts to that account; reserve fixed OS/foreground headroom once before distributing grants. Model-count and quota credits constrain requests without granting memory independently. Extend executor admission with memory-only lease support where core preparation needs it, rather than charging remote waits as local jobs. Document estimated accounting and its allocation overhead; sampled RSS is not an exact allocation ceiling.

Avoid double-counting memory: available memory already reflects some live harness and child usage. Treat sampled incremental headroom and outstanding reservation accounting separately; do not repeatedly subtract a whole live reservation from an already reduced available-memory reading. Verify mixed large contexts and native jobs cannot each spend the whole memory grant.

`sysinfo` already supplies supported memory and CPU observations. Its cgroup memory helper does not establish nested CPU, cpuset, process, or ancestor limits. On Linux, use maintained native and procfs APIs and narrow bounded adapters for discovered constraint files. Unsupported ancestry or limits stay explicit. No environment, command-line, or secret inspection is needed.

### 3. Use coarse shared grants and cheap local run queues

Keep the capacity broker in core, using existing `rusqlite` and protected local-state APIs. Executor code receives grant updates through public contracts and does not depend on SQLite or consuming products.

Use one stable owner-only local path for the user and host, independent of `HarnessConfig::state_dir`. Do not put a protocol version in the filename and accidentally create parallel budgets after upgrades. Keep protocol/schema versions inside the registry. Session journals retain their current exclusive ownership; the capacity database permits concurrent participants and uses short migration locks and transactions. Reuse private-directory, ACL and file identity checks. Retain directory handles. On Unix, capture no-follow database device/inode identity before SQLite opens, recheck after open and during coordinated transitions, and let the live SQLite connection retain its descriptor. Read metadata and macOS ACLs by path without opening and closing live database or sidecar files, which would release process-wide POSIX locks. A narrow process-local bootstrap mutex prevents private file-creation closes racing another SQLite open. Windows retains a separate database anchor. Accept optional sidecar disappearance only for a typed missing-file error followed by a fresh no-follow absence check. These checks are not an atomic path snapshot; other code of the same OS user remains trusted. Do not apply the journal's lifetime-exclusive owner lock to the shared database.

Bound registry metadata by participant count and resource domains. Record boot identity, protocol features, random instance nonce, supported process-birth identity, demand summary, grant revision, requested reduction, cooldown state, and recovery state. No transcripts, commands, cwd values, tokens, or raw headers belong here. Represent known host/container ancestor domains explicitly; charge a child grant at every shared ancestor. Per-process descriptor limits remain local.

On a detected domain move, fence local admission with a new environment generation. Charge newly applicable domains before releasing old-domain charges, counting common ancestors once. Insufficient new capacity becomes drain debt rather than disappearing ownership. Unknown ancestry freezes the affected lane; it cannot register an independent root budget. Include a live move into a tighter cgroup in verification.

Grant protocol:

1. **Increase:** atomically debit all applicable ledger domains, commit a new revision, then install the larger local grant. A lost reply can strand credits temporarily but cannot authorize double spending.
2. **Reduce or transfer:** request a smaller target while the full old grant stays charged. The owner installs that target under its local admission lock and waits until outstanding leases fit it. Only then may an idempotent compare-and-swap transaction commit the smaller grant. The recipient can spend the released credits after that commit.
3. **Stale responses:** reject old generations and serialize each instance's grant changes. A failed release acknowledgement leaves the local target lower and the ledger charge higher until reconciliation.
4. **Loss of contact:** use a bounded local admission deadline after successful coordination. Freeze new affected admissions when contact expires; retain existing ownership and cleanup. Do not create a private replacement budget.
5. **Exit or crash:** graceful exit drains before releasing. Expiry alone does not release credits. Retain an OS-held lifetime witness for each active participant. Release of that witness, or a verified boot change, establishes loss of its future scheduling authority. Retire confirmed-dead scheduling grants under monitored recovery; preserve live or uninspectable owners. Possible survivors become sampled external load. This transition does not certify cleanup, undo effects, or justify replay.

One transaction claims each shared domain's control window so independent participants do not all probe the same capacity at once. Use bounded demand-driven updates, fair process rotation, and revocation of unused credits before adding new ones. Ordinary local lease acquisition/release and text/token events do not write the shared ledger. There are no durable per-job lifecycle transitions. Live native scopes stay in bounded execution-owner memory. Keep the existing session effect barriers and provider quota accounting, which serve different recovery contracts. Coarse provider-rate chunks are charged at allocation and cannot be refunded just because an owner stops heartbeating.

Local admissions read only the cached committed grant, its generation, and a local contact deadline. Shared validation happens during bounded grant/renewal/control updates, not on every tool acquisition. Keep provider-rate records separate from boot-scoped native owners: a reboot invalidates old native ownership but does not erase unexpired provider windows or prove remote inference ended.

A shared registry cannot reserve resources for unrelated apps or older binaries. Observe their pressure. Cross-user or inaccessible container coordination is unsupported unless a trusted shared scope can be established; status must never imply otherwise. Protocol mismatch, unsafe state, corruption, or persistent contention parks new coordinated work and preserves cancellation. Learning corruption has a separate, permissive fallback.

### 3a. Link native helpers through live ownership

Use a supervision tree, with exactly one lifetime owner per controlled helper. A private pipe or local connection supplies the owner-loss signal. Keep its owner endpoint out of unrelated descendants; EOF stops further work and cancels/reaps the helper's known scopes. Watch links in a bounded shared event loop, not one thread or watchdog per agent. The execution owner starts lazily, owns admitted native work, and exits after clients and scopes drain. A CLI attaches to that owner through a replaceable transport; its policy can cancel on disconnect without equating transient UI attachment with execution ownership.

**Charge the shared helper separately.** Use one bounded coarse registry commitment per native helper generation, independent of its creating client. Allow at most eight occupied helper slots per protected host/user registry, including pending startup, running, paused and draining generations. Compatible clients reuse one helper and one commitment. Ordinary agents, commands, streams and package callbacks create no additional helper records.

Start with a provisional baseline estimate of 32 MiB per helper. Charge one helper PID and, where native PID limits count threads, the current runtime’s maximum of three task slots. These are accounting estimates. Existing idle RSS measurements do not establish startup peaks. Before automatic-default activation, establish cold-startup and retained-helper-state evidence for the target. Reuse prior calibration when a recorded source comparison proves native-helper execution and dependency paths unchanged, even if unrelated linked code changes the binary hash. Keep those samples attached to their original images; recalibrate only affected paths. Do not require all eight production helpers under saturation without an uncovered accounting risk. Use a baseline of at least the next power of two above twice the largest validated startup peak, with a 32 MiB floor. Do not learn this estimate downward from low or missing samples.

Deduct only committed helper allowances from every applicable ancestor envelope before distributing participant grants. Count each commitment once per ancestor, independent of how many clients share it. Reserve no helper baseline before native demand. Keep OS/foreground headroom, participant runtime baselines, interpreter generations and command reservations separate. Do not subtract a helper allowance again from sampled available memory.

A pending helper request must obtain its shared commitment before holding execution or package reservations that would prevent that commitment from fitting. Stage each dependent request in the existing bounded local governor queue before establishing native readiness. Staging exposes the resident floor and next useful working set through the existing aggregate admission demand without granting a resource lease. It retains the original ticket, within-session FIFO, age, bypass bound, cancellation and overall deadline. A pending request creates no helper process and adds no per-job shared record or write.

Normal coarse renewals carry bounded queued presence, the current resident floor and a cached monotonic native activation count. Actual activation advances native service credit even when short work finishes between renewals. A grant issued while a staged ticket still waits earns no service credit. These are aggregate participant updates, not per-job registry writes.

Select a safe subset of pending helper allowances when proposing reductions. Merge staged turns backed by committed helpers into the same fair order, with no additional helper charge for reuse. Keep every committed helper charge, retained participant floor and old-ancestor charge. A pending helper that would prevent every useful claimant from receiving a next turn must remain pending without subtracting its allowance from every participant plan. Protect a feasible claimant through the existing reduction, actual drain and committed-release rules. Associate each pending generation with all staged participants in its current compatible native domain. The selected beneficiary may differ from the client that starts the helper. Bind selection to that beneficiary, the current control epoch and a bounded useful minimum. Count the pending helper allowance only once when several compatible participants receive turns. Commit the helper only after its beneficiary's actual committed grant covers that minimum and the additional helper charge fits every ancestor. Reject stale selections, canceled demand, unsafe or stale claimants, changed ancestry and larger current minima. Check every applicable ancestor; a tightened limit preserves existing charges as visible drain debt. Do not simulate refunding a shared helper when one job finishes, since other clients or resident work may still retain it.

Before obtaining a readiness guard, a staged caller waits until its local queue position and current grant make activation prospectively eligible. After readiness succeeds, atomically try to convert that same staged ticket under the unchanged grant generation. If it would wait, lose eligibility or cross a grant generation, drop the unused readiness guard immediately and wait with the staged ticket, holding neither an execution reservation nor a helper keepalive. Repeat only this preparation before effects. Never hold unused readiness across a resource wait. This rule applies to every wait interval, including reuse of an already committed helper. Thus idle helpers can exit and their witnesses can authorize recovery when their baselines prevent useful work. Existing ready work stays pinned to its original helper epoch and uncertain launches are never retried.

Package preparation stages one total startup working set: the new 64 MiB resident interpreter plus the 16 MiB management scratch, above any existing resident floor. Reload includes the old interpreter until cleanup. Convert the admitted total once into the actual resident and management leases without charging the same bytes twice. Idle interpreters retain their real resident leases; startup scratch ends through normal management cleanup. These are local lease transitions, not shared per-job lifecycle writes.

**Retain a kernel witness through startup and exit.** The executor acquires the stable native `owner.lock` with plain `File::try_lock()` before committing the helper allowance or creating its process. Pass a duplicate of that locked file through helper stdin. The helper validates and adopts that same open file description, sets `FD_CLOEXEC` through safe Rustix APIs before starting its runtime, and retains it through cleanup, idle exit and runtime teardown. It must not reopen and reacquire the ownership lock. Parent references close without calling `unlock()`. User commands never inherit the witness.

Release a coarse commitment only after supported witness-release evidence or a verified boot change. Launcher death, missed heartbeats, failed replies and elapsed time do not refund it. An interrupted startup before process creation can release once its witness is free. A helper created before launcher death retains the witness even if paused before early dispatch.

`ejm-harness-exec` owns additive staged-governor preparation and activation contracts, the public `NativeOwnerAdmission` and opaque `NativeOwnerPermit` boundary, helper startup and witness transfer. Core implements the admission boundary and safe-subset planning through the prepared capacity context and existing registry. Package runtime consumes these public contracts for its combined startup working set. The executor does not depend on SQLite, and no shared package depends on a consuming host. Keep existing host dispatch and legacy constructors compatible. Cache helper commitment and pool status for ordinary status reads.

Version the private helper launch and discovery protocol so an older helper without this accounting cannot be reused silently. Preserve stable ownership lock identities and reject incompatible live owners. Windows shared native execution remains gated until a supported handle-lifetime mechanism and native evidence exist.

Only one execution owner may hold the stable OS lifetime lock for a compatible execution domain. All installed protocol versions use the same lock identity. A paused holder retains ownership. Missed heartbeats park work and never authorize stealing the lock, unlinking a live endpoint, or starting an incompatible second owner. Validate the private state and IPC peer boundary before accepting requests. Reuse standard-library locks and maintained local IPC/framing/native process packages; custom code owns only harness-specific policy and bounded messages. Hosts call a public early dispatch entry in their existing executable, so helper packaging does not require a new user-installed daemon.

Create permanent native lock files directly under their final name with exclusive creation relative to the pinned private state directory. Do not publish them through a temporary hard link. Return the creator's verified handle; on `AlreadyExists`, reopen the same name and require both one link and owner-only privacy on that open handle. Retain a final name after any post-create validation failure and reject unsafe existing entries without deleting, replacing or repairing them. Windows permanent-lock creator handles must omit DELETE access so ordinary peers can open the same identity. This operation adds no file or directory sync and makes no power-loss durability claim.

The reviewed maintained `rightkit-fs` releases do not expose these operations. Temporarily vendor the checksum-pinned 0.4.4 archive with the reviewed direct-create and same-handle privacy patch. Record the archive, patch and source hashes, license, dependency graph, public API checks and removal condition. `ejm-harness-exec` consumes `StateDir::create_new_empty` and `ReadOptions::require_owner_only`; the other existing harness consumers share one workspace version-and-path dependency. CLI and IDE path consumers must resolve that same reviewed source without depending on a workspace-root patch override. Cargo registry publication requires a verified registry dependency, since Cargo strips local paths from published manifests. The configured GitHub source-release pipeline may instead consume the maintained public fork at an exact full commit SHA, with its exact repository allowlisted in dependency policy and public consumer builds verified. The owner authorized an ejmlabs-maintained fork on 2026-10-21: publish the reviewed patched 0.4.4 source as `ejm-rightkit-fs` with a distinct package identity, exact version pin, retained licenses and source provenance, and the existing `rightkit_fs` library name. Preserve the upstream contribution and migrate back when an upstream release passes the same contract checks. An exact Git pin satisfies the GitHub source/native dependency gate only; it does not permit Cargo registry publication. Remove the vendor override and patch when that upstream version passes the same checks. Preserve the dependency direction from hosts through public harness crates to the filesystem package.

Verify simultaneous access at the first visible final name, creator death at that boundary, and rejection of retained unsafe files without changing their identity. Use deterministic debug-only dependency seams and real bounded child crashes, including the actual platform privacy boundary. Record native target limits, source stability and downstream CLI/worker/SDK/IDE checks. These tests close the publication gap only; they do not establish native descendant cleanup, all-platform support or automatic-default readiness.

Keep request IDs scoped to a random owner epoch. Reconnect and query only that same live owner. An uncertain launch is never resubmitted to a replacement owner, and expired operation IDs are rejected rather than treated as new. Bound pending clients, frames, stdin, environment entries/bytes, commands, output, completed results and lifetime. Preserve caller cwd, environment overrides/removals and native host semantics instead of inheriting the first client's PATH or virtual environment. Never log or persist private launch input or environment values. Model streams and waiting-agent metadata do not pass through the native owner.

The owner retains children, pipes, leases and mutation ownership through cleanup; cover executor commands, provider CLI work and resident package hosts through public contracts. Package startup is admitted before import, resident hosts keep memory/PID charges while idle, and reload accounts for overlap. Auth terminal interaction and worker bootstrap remain narrow explicit host contracts. A provider CLI or callback requesting unrelated OS services cannot promise ownership of that service's descendants.

Core forwards the prepared native owner through additive `Harness::native_owner`
and `ToolContext::native_owner` accessors. Provider and shell executors attach that
handle as well as sharing the governor. Legacy constructors return no owner and
keep local execution. Passing only the governor accounts for resources but does
not select shared process supervision; test the composed shell path through an
actual native operation.

Windows uses a non-breakaway kill-on-close Job assigned before user code runs, with noninheritable job handles. Linux may use supported delegated cgroups and controlled direct-child parent-death links. macOS uses lifelines, process groups and sampled descendant identities. Its arbitrary detached descendants can outlive both owner and observer. Extra watchers do not turn this into a kernel guarantee. Do not claim that EOF, root exit, low pressure or elapsed time proves an arbitrary native scope empty.

After confirmed owner death, require fresh supported host observations, reset new targets conservatively and increase only through normal feedback. Retiring a scheduling allowance is explicitly measured recovery, not a cleanup receipt. Current use by survivors is already included in available-memory observations; do not subtract it twice. Native survivors can grow faster than sampling, so this policy cannot bound their future resource growth or certify a workspace safe for new writes. Existing interrupted-session semantics retain unknown effects without replay. Provider charges survive local death and reboot until their own windows expire.

Measure native-owner cold/warm startup, idle lifetime, command dispatch, cancellation, EOF cleanup, memory/process/descriptor cost and local transport overhead. Before automatic-default activation, exercise real idle and busy client/owner crashes, paused owners, uncertain launch replies, detached descendants and useful progress after confirmed death. Demonstrate each platform independently. No containers, VM lifecycle, image setup or durable execution ledger is required by this design.

### 4. Schedule runnable work and bound its working set

Use a few declared workload classes: context preparation, remote model request, lightweight native work, and heavy native work. The host/tool contract supplies class and reservation; model text does not. Do not learn a smaller command reservation from missed or low samples, because current reservations also define monitored violation thresholds.

Move model and context-memory admission before `prepare_context` and request cloning. Release model credits before unrelated tool waits, while retaining the required durable and hook ordering. Reuse immutable tool definitions where public ownership permits it; do not retain a full request for every model waiter.

Keep accepted runs in bounded count-and-byte pending state. Feed executor waiters as capacity permits rather than allowing a 4096-agent burst to overflow a 256-entry lower-level queue. No retry loop or queue expansion substitutes for backpressure. Existing overall deadlines include waits. Prompt cancellation must remove pending, context, model, provider-rate and local-job ownership.

Retain session FIFO. In automatic mode, allow bounded backfilling across sessions: initially at most eight bypass grants or one second of bypass age, then stop conflicting grants until the old fitting head can start. Count and age limits are policy parameters, not magic correctness proofs. Test finite progress under continuous small demand. Existing fixed admission retains its documented head-of-line behavior. Do not bypass workspace mutation serialization.

The current queue performs linear session/ticket searches and the monitor repeats process-set traversal. Profile their cost at the new envelopes. If they exceed budgets, use indexed tickets and one parent/identity index per native sample while preserving FIFO, drop removal, and descendant identity checks. This is product-specific accounting over maintained OS APIs, not another process parser.

### 5. Control useful throughput with bounded feedback

Use short bounded aggregate windows. Measure queue wait, context preparation, provider service, journal wait, native execution, successful completions, usage, and resource pressure separately. Never reward retries, failures, abandoned work, longer replies, or higher token consumption.

Begin with a conservative host-derived target. Increase one lane by a bounded step only after several healthy windows with demand and enough comparable successful samples. Change one main control variable per trial so attribution is possible. Back off promptly on observed memory/CPU pressure, long scheduler or journal stalls, or typed provider throttling. Use hysteresis and cooldown before probing again. If throughput stays within the acceptance tolerance at a smaller target, prefer the smaller target.

Host sharing and local fair queues constrain the controller, not vice versa. Low traffic does not establish a low capacity ceiling; absence of demand means hold the target and skip probes. Remote latency alone does not show local saturation. Cap control history and probe cost; learn from normal work without extra provider calls.

This bounded controller is a practical search for a sustainable range, not proof of a global optimum across all workloads. Static sweeps in offline fixtures provide a reference for its behavior.

### 6. Make quota feedback a provider contract

Add a compatible optional observation path to `Provider`, with a default implementation forwarding the existing `stream` behavior. Built-in adapters emit typed, redacted quota hints, throttles, timing, and usage; old custom providers report unavailable hints. Keep optional `HttpObserver` debug capture separate.

Use an opaque identity supplied by auth/provider APIs for quota sharing. A provider-selection binding includes model choice and is not sufficient evidence of an independent quota. Key sharing by transport origin, account quota identity and provider-declared family. Unknown sharing stays explicit; never hash raw credential text into a durable identifier.

Coordinate known request/token budgets as well as in-flight model grants. Reserve bounded estimated tokens, reconcile reported usage, and retain conservative charges when the outcome is uncertain. Parse supported retry hints through maintained HTTP/date APIs; invalid hints use bounded policy, and a valid hint beyond the supported wait horizon fails clearly rather than retrying early. Re-entry is coordinated and staggered. Tuning affects future work and adds no automatic retry of an uncertain request or tool effect.

Keep allocated request/token charges until their provider window/reset even after owner death or a host reboot. Never refund uncertainty from local heartbeat/boot recovery. In-flight counts describe locally tracked requests; provider-side inference lifetime requires provider evidence. Test a reboot/restart during an unfinished request with an unexpired rate window.

### 7. Persist small local summaries as disposable hints

Core owns profile persistence and exposes reset/status to hosts. Reuse the protected capacity database with separate bounded tables and failure handling. Keep profile persistence optional for progress; shared grant transactions remain mandatory for coordinated admission.

Key summaries by controller version, relevant environment and constraint fingerprint, workload class and opaque quota domain. Retain capped counts, latency/throughput aggregates, measured resource distributions, confidence and timestamps. No prompts, paths, command arguments or raw provider trace data are retained. Proposed initial bounds are 256 profiles, 1 MiB total serialized learning data, and at most one profile flush per minute plus clean shutdown.

Expire old profiles, decay confidence, and invalidate material version/environment changes. Cached targets must pass current host, grant and pressure checks. Missing or corrupt profiles use a cold start. Reset affects future estimates and leaves sessions, credentials, active leases and owner ceilings intact.

### 8. Remove proven serialization cost without weakening the journal

Instrument journal queue and commit wait before changing storage behavior. First evaluate bounded group commit for compatible append jobs in one FULL-sync transaction. Proposed collection bounds are 32 compatible jobs or four milliseconds, with per-session order and completion only after commit. Do not delay effect intent, completion, or run-transition barriers past dependent work.

If text coalescing is still required, constrain it by bytes and time, preserve final content and ordered durable events, and document that chunk boundaries are transport details. Do not drop persistence, disable FULL synchronization, hide replay gaps, or duplicate final streamed text. Add only the batching that measured fixtures justify.

### 9. Provide bounded consistent host contracts

Add indexed paged session search with stable cursors. Add a bounded presentation-tail read that returns authoritative current run identity, last run-state transition sequence, durable high-water cursor, ordered tail and omitted-history marker in one consistent read. Maintain any compact authority projection in the same durable transition as its source event; migrate or reconstruct old journals through a bounded startup task rather than scanning them inside UI input handling.

Support subscribe-before-snapshot with a bounded session-filtered live channel. The host discards already represented events, replays after the snapshot cursor, and restarts from a fresh snapshot on bounded-channel lag. Preserve the old global subscription and full pageable durable history. Aggregate status notifications coalesce; they do not replace durable effect events. Bound subscriber registrations and close them on drop.

Durable event sequences are global, so a selected stream legitimately skips numbers when other sessions emit. Add selected-stream continuity metadata to the new wrapper contract, or expose a precise channel-loss signal with a documented cursor handshake; do not redefine the old `Event` shape. Snapshots carry the matching selected-session continuity anchor. Numeric `sequence + 1` checks cannot detect loss in a filtered stream. Verify interleaved global events do not cause spurious recovery.

CLI authority refresh uses this public projection and revalidates exact run identity before a mutation. The engine does not acquire CLI remote-sharing policy or terminal state.

### 10. Establish budgets before claims

These remain engineering targets, not existing results. Platform activation uses the critical checks below; the broader matrix and precision profiling rows are follow-up coverage unless a concrete failure makes them relevant. An unmeasured target is not reported as passed:

| Path                      | Initial gate                                                                                                                                                                                                                                                                 |
| ------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Scale                     | Offline fixtures at 100, 1000 and 4096 simultaneously managed agents; separately vary model streams, local jobs and participating processes                                                                                                                                  |
| Productive throughput     | After stabilization, within 10% of the best safe static setting for the same stationary fixture and constraints; report variance and lower-concurrency ties                                                                                                                  |
| Pressure response         | Stop increases on stale critical samples; respond within two control windows to a fixture capacity drop; drain without limit violations or oscillation                                                                                                                       |
| Fixed-workload regression | At 100 agents, no unexplained regression over 10% in throughput, p95 queue/turn latency or peak process-tree RSS across repeated equivalent runs                                                                                                                             |
| Controller state          | At most 8 MiB added resident controller/coordination metadata per process; ledger metadata at most 4 MiB for the initial 64-participant envelope                                                                                                                             |
| Waiting state             | Incremental parked memory target at most 32 KiB per managed agent in the defined small-prompt fixture; measure hooks/contexts separately                                                                                                                                     |
| Shared-loop work          | Aggregate control/sampling overhead target below 1% of one logical CPU in the documented low-load fixture; no polling proportional to parked agent count                                                                                                                     |
| Shared contention         | At most two steady-state coordination transactions per active participant per second; bounded demand-change transactions; no ledger access per text delta, ordinary local lease or native-job lifecycle. Measure private transport cost per admitted native scope separately |
| Foreground budgets        | Existing warm first-paint p95 below 300 ms; input/Stop acknowledgement and selected received-text-to-paint p95 below 50 ms in the CLI fixture                                                                                                                                |
| Existing memory target    | Keep the below-256-MiB combined engine/TUI target for the defined 100-agent workload; publish separate measured envelopes for 1000 and 4096                                                                                                                                  |
| Learning                  | At most 256 profiles and 1 MiB learned state; bounded writes and no content telemetry                                                                                                                                                                                        |

Use the existing benchmark and CLI PTY/selected-path instrumentation for one representative workload: 1000 total managed agents across two participating processes with separate stores, fixed executing work, bounded selected output, background events and cancellation. Include one bounded pressure step and one learned restart in this fixture. Use the same provider fixture, useful-work definition, enabled packages, constraints and build inputs across modes. Record startup separately from steady work, available process-tree observations, reservation bounds and cleanup proof.

The blocking performance checks retain automatic throughput within 10% of the best safe point from a three-point fixed sweep, p95 input/Stop/selected-event-to-paint within 50 ms in consumer evidence, and at most 5% useful-work loss from active terminal interaction against equivalent inactive-terminal work. Native safety requires retained live charges, no double spending or heartbeat takeover, prompt cancellation and bounded safe useful progress after supported ownership release. Existing focused fixtures may establish these invariants; do not duplicate them in every performance run.

Use two alternating paired trials for comparisons and at most one extra pair if noise prevents a verdict. A remaining miss or inconclusive result needs investigation of its concrete cause, not an expanded benchmark matrix. Keep raw samples, missing-observation limits and source/calibration bridges. No paid providers or second benchmark framework.

Existing 4096-run admission/cancellation fixtures retain their boundary coverage. Full 100/1000/4096, low-resource hardware, provider and package-language matrices, separate cold/warm campaigns and precision idle/I/O profiling remain follow-up work. Reuse relevant binary/dependency receipts. Known safety or representative performance failures remain blocking; unrelated unmeasured targets do not. Update public guidance during activation and retain measured scope. This planning-only revision changes no runtime, API or current default and needs no docs rebuild or Rust suites. Later code changes retain required affected-owner gates.

## Risks / Trade-offs

- Native owner death can leave untracked work. Gate defaults on measured idle/busy recovery; preserve uncertainty about effects and cleanup while resuming only from fresh host observations. Native monitored recovery cannot promise hard total resource bounds or a safe workspace after escaped work.
- Coarse grants reduce shared-lock cost but can temporarily leave unused capacity in another process. Rebalance unused grants before disturbing active work and measure cold-grant latency and churn.
- A constraint decrease can leave charged work above a new target. Preserve accounting and drain; report both values rather than changing counters.
- Missing lifetime witnesses or unsupported boot identity can prevent verified death. Use retained OS locks; preserve uninspectable grants. Native PID birth tokens aid cleanup but cannot make a separate check-and-signal atomic.
- Shared SQLite contention or unsafe storage can pause progress. Keep transactions short and off UI threads, bound waits, and retain cancellation. Never split into an independent fallback ledger.
- The host cannot reserve unrelated applications' future demand. Maintain headroom and react to observations; sampling is not enforced isolation.
- Workload changes can invalidate apparent throughput gains. Use coarse workload classes, minimum evidence, decay and repeated comparable fixtures.
- Group commit changes stream timing. Bound delay and preserve durability, effect barriers and cancellation before claiming throughput improvement.
- More public APIs and wire fields increase compatibility risk. Keep additions optional, generated, and checked against existing custom providers, SDK fixtures, CLI and IDE.

## Migration Plan

1. Add measurement fixtures and additive contracts while old integrations retain fixed behavior.
2. Implement local operating targets, host/domain observations, fair pending admission and context gating.
3. Add shared ledger allocation and recovery, prove the native crash-recovery activation gate, then add provider-domain quotas and learned profiles.
4. Add bounded host views and only the measured journal/queue/sampler changes. Integrate CLI automatic defaults and explicit new-SDK automatic requests through public contracts while old worker omissions remain fixed.
5. Run focused invariants, deterministic control scenarios, the representative workload, affected SDK/host compatibility and required repository gates with Cargo jobs at most two.
6. Update `docs/executor.md`, `docs/implementation-contract.md`, `docs/ide-integration.md`, provider/SDK references, `docs/benchmarks.md`, public guides and relevant owning skills. Clearly mark measured limits and coordination/recovery scope. Run existing site checks; publish only through separately authorized release work.
7. After verification, sync specs and archive the paired changes through their skills. Until then, keep every implementation task open.

Rollback disables new automatic probing through the explicit fixed coordinated mode. Do not remove shared accounting while participants retain grants. A binary without compatible coordination must wait for a safe drained migration or run only within its explicitly documented uncoordinated legacy scope; it cannot silently join a live budget through a parallel registry. Profile data is disposable; live scheduling commitments and provider quota accounting must retain their defined ownership semantics.

## Renewal failure diagnosis (2026-10-21)

The representative run exposed command timeouts before the injected stall,
following failed shared renewals. Core-owned capacity status reports a fixed
nonsecret error category (database contention, database failure, filesystem
failure or admission failure). It never copies raw errors, SQL or paths into
status. Preserve contact expiry, grants, deadlines and admission behavior.
Verify classification and redaction with one focused contract regression.

## Current-protocol registry startup

The harness reads the committed protocol before requesting a schema write lock. A current-protocol registry opens without schema writes, including while a WAL writer is active. Creation and migration still acquire an immediate transaction and reread the protocol under that lock. Keep the 100 ms busy budget, private-file checks, migration fencing and admission deadlines unchanged. CLI and SDK consumers use this shared implementation. A focused regression holds a write transaction across current-protocol open; existing mismatch and migration tests protect rejection.

## Helper retirement during startup

Before readiness or any command submission, a clean helper exit may indicate idle retirement while the client was unscheduled. The executor permits one rediscovery after a verified successful exit, reacquiring the stable owner lock and a fresh accounted generation. Nonzero exits fail with exit status. The existing caller deadline still bounds readiness. No command is retried, no live witness is released and no shared row is refunded by timeout. Verify bounded clean-exit rediscovery and immediate failure on nonzero exit.

## Recovery after an inconclusive small probe

The representative workload on 2026-10-22 completed safely but missed automatic throughput acceptance. A rejected one-slot probe repeats unchanged after every cooldown. Permit a larger subsequent probe only when comparable work misses the throughput gain threshold while still meeting the existing latency guard. Double the probe distance up to the existing eight-slot maximum and keep the same hard ceiling, full-grant evidence, one-lane attribution, pressure backoff and cooldown. Reset the distance after acceptance, latency regression, class change, grant refusal or pressure. The extension adds one bounded integer per lane and no persisted state. Verify a flat small step followed by useful larger growth, plus existing pressure and latency regressions; measure before activation.

### Admitted review: native endpoint authentication

Native protocol 3 adds a bounded server proof before Hello. The maintained
RustCrypto HMAC-SHA256 implementation signs the fresh client nonce, role,
protocol, epoch, domain and independently observed TCP endpoint ports with the
private advertised capability. Constant-time verification precedes all client
capability and launch data. Cached and discovered endpoints use the same check;
a free stable owner lock makes discovery stale without contacting its port.
The existing handshake semaphore and frame deadlines bound this exchange.
This authenticates local endpoints, not an encrypted or general remote tunnel.
Rustls would require certificate provisioning and a larger transport change;
existing sha2 and transitive hmac meet this narrow local proof requirement.
Relative cwd is made absolute in the caller before serialization, preserving
symlink traversal. Existing admission and uncertain-effect rules remain intact.
