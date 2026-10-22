# Shared host capacity

## Availability

The capacity registry and additive executor APIs are implemented in this working
change. Automatic defaults remain gated on native lifecycle, recovery and scale
verification. Existing `Governor::new` and old host constructors keep their fixed,
local behavior. An uncoordinated legacy client is outside the shared budget.

The working implementation adds a separate coarse commitment for each shared
native helper. Targeted simultaneous-start and pool checks passed on this Mac;
the full crash and consumer checks remain in progress. Release CLI and worker
startup and control-state saturation windows now fit the provisional 32 MiB
estimate on this Mac, after reducing unused transport buffers. Other target and
embedding-image calibration, wider acceptance and automatic defaults stay gated.
See [the measured scope](#helper-commitments).

The scoped macOS sampler met the owner CPU target in short samples, but its
separate sustained idle test failed: one Python sample averaged 1.15% of one
core over a configured 60-second interval. See
[package measurements](packages.md#sustained-idle-measurement) for the result,
provenance and scope. It does not establish full CLI idle acceptance.

Managed agents hold bounded session state while they wait. Active model requests,
native jobs, resident interpreter processes and memory reservations have separate
limits. A thousand managed agents do not require a thousand running processes.
The registry supports at most 256 participating client processes, with many
managed agents per client. These bounds are validation limits, not throughput
or target-hardware claims.

### Cached scheduling status

`Harness::scheduling_snapshot()` returns a separate `SchedulingSnapshot` without
changing legacy `HarnessSnapshot` or `status()`. It reads coherent local counters
in constant time, without scanning agents, reading journals or sampling the OS.
It works with legacy and coordinated construction. It is not a host-wide total.

| Field                   | Meaning                                                                                        |
| ----------------------- | ---------------------------------------------------------------------------------------------- |
| `managed_runs`          | Runs retained by the engine, including waiting and finishing work.                             |
| `runnable_runs`         | Managed runs outside the explicit phases below, including hooks, journal writes and finishing. |
| `model_waiting`         | Waiting for a static model slot, coordinated model credit or context memory.                   |
| `model_preparing`       | Preparing admitted context, hooks and a provider request.                                      |
| `provider_waiting`      | Waiting for provider selection or coordinated provider quota.                                  |
| `model_running`         | Inside the provider callback, including streamed-output backpressure.                          |
| `tool_dispatch_waiting` | Waiting to enter bounded tool dispatch.                                                        |
| `workspace_waiting`     | Waiting for workspace mutation ownership.                                                      |
| `tool_resource_waiting` | Waiting for a governor tool reservation.                                                       |
| `tool_running`          | Inside the tool callback, including internal admission in self-admitting tools.                |

Each managed run occupies at most one explicit phase. `runnable_runs` is not an OS
runnable-thread count. Provider/tool callback counts do not prove remote inference
or uncertain native descendants have stopped. Retained execution leases and
cleanup status remain authoritative for those resources. Read the worker's
`scheduling.status` through TypeScript `schedulingStatus()` or Python
`scheduling_status()` when the `scheduling` capability is advertised.

## Scope and storage

The core opens `.ejm-harness-capacity` beneath `HOME` on Unix and `LOCALAPPDATA`
on Windows. It stores `capacity.sqlite3` separately from session journals. Using
a second journal directory does not create another budget. Production hosts must
use the public prepared capacity context and this common location. Isolated test
roots are available only through the explicit test-support contract.

Final harness composition must keep the resolved session, model, context-byte
and resource ceilings from preparation. A changed value is rejected before
tool registration, provider configuration or journal creation. Choose a new
prepared context when changing those ceilings. Choosing the final journal
directory after preparation is supported.

Preparation computes the runtime and bounded dispatch-memory minimum before
registering. The first shared plan includes that full resident floor even when
CPU, model and job credits are temporarily zero. The measured 512 MiB benchmark
configuration uses a 96 MiB runtime/dispatch minimum; this is separate from the
native helper's provisional 32 MiB commitment. A minimum above the configured
memory ceiling is rejected before registration. Insufficient shared memory can
still return the existing runtime-baseline preparation error.

The registry stores coarse participant grants, native-helper commitments, provider
rate charges and aggregate learning. It does not add a durable record for each
native job. Existing session history still records tool intent and recovery outcomes.

The registry holds one short SQLite write transaction for each coordinated
transition. Ordinary local admissions, stream tokens and executor output never
write the registry. A shared control window limits plan calculation to once per
500 ms. Busy waits are bounded at 100 ms. SQLite uses WAL and full synchronization.
Initial WAL conversion retries SQLite busy responses within that same 100 ms
budget and checks the returned journal mode.
No long-lived transaction spans a local admission lock or callback.

Demand wakes coalesce so coordination cycles stay at least 500 ms apart.
Without a wake, the coordinator waits up to two seconds. Each cycle renews
grants and samples host observations. Automatic target feedback uses elapsed
windows of at least two seconds; a late wake makes one decision without
catch-up steps.

State is private to the same OS user. Components, files, native identity and
permissions are checked before use. Symlinks, hard links, wide Unix modes,
unsafe macOS allow ACLs, unsafe Windows ACLs and known remote filesystems fail
closed. Directory handles remain retained. On Unix, SQLite owns the live database
descriptor. Core captures the no-follow device/inode identity before SQLite opens
the database, checks it again after open and during coordinated transitions, and
reads permissions and macOS ACLs by path. It never opens and closes live SQLite
files for validation: closing another descriptor can release SQLite's POSIX
locks. A process-local mutex covers only registry bootstrap, including private
file creation and SQLite initialization. Windows retains a separate database
anchor. Optional sidecar disappearance is accepted only after a typed missing-file
error and a fresh no-follow absence check. Other validation errors fail closed.

These are checks before and after use, not an atomic path snapshot. Code running
as the same OS user remains trusted. Parser or source-level platform coverage is
not evidence that native Windows or Linux filesystem behavior passed on those
targets.

This is cooperative coordination among participating processes of one user.
Containers with hidden ancestry and independent user namespaces can hide a
common ancestor. Unsupported or incomplete ancestry cannot prove an independent
capacity root. It fences new shared plans instead of creating another budget.

## Grants and useful work

Each participant reports bounded demand and current reservations. Native turns
reserve the resident floor plus the oldest queued request before filling spare
capacity beneath each owner's tuned ceilings. A large configured hard memory
ceiling does not become the minimum working allocation. Model slots have their
own fair distribution. Session FIFO and bounded backfill remain local to the
executor governor.

The root host budget reserves fixed OS headroom once. Visible child constraints
apply without subtracting this headroom again. Allocations charge each applicable
ancestor once. If an external resource-domain move happens, local admission is
fenced before the registry charges every new ancestor. Old ancestor charges stay
until native work and resident process reservations drain. A move or tighter
limit can create debt; it cannot erase an existing reservation.

An increase is committed before it is installed locally. A reduction installs a
smaller target first, then releases shared credit only after a fresh usage
witness fits and its revision still matches. A lost callback, failed install or
stale acknowledgement keeps the larger charge. A graceful exit installs a new
zero target before refund, so a retained governor handle cannot spend released
credit.

Automatic feedback halves model and tool targets, down to one, when a fresh CPU
sample reaches 95%, fresh available memory falls below 128 MiB, or a control
cycle takes more than four seconds. Existing reservations remain charged while
they drain. Missing or stale critical observations stop upward probes. Provider
quota waiting and remote latency alone do not establish local resource pressure.

Growth probes require more than 10% additional successful throughput and at most
25% higher service-time p95. Timing histograms use 16 subdivisions per power of
two and compare the probe's upper bound with the baseline's lower bound. This
can reject a marginal gain, but bucket rounding cannot hide a latency-budget
violation. Controller state stays below 128 KiB per participant, independent of
agent count. Learning version 2 ignores targets accepted by the earlier coarse
latency comparison; those participants start cold and learn again.

## Contact and recovery

Each committed local target has a monotonic contact deadline. After eight seconds
without renewed contact, it blocks new admissions while retaining existing
leases. Each participant also retains an exclusive OS lock on one of 256 stable
private witness files. The files are never unlinked during normal recovery.
A paused participant keeps its lock and allowance, even if its heartbeat expires
or a PID lookup fails. A timeout never permits takeover of a living owner.

After contact expires, acquiring the same recorded witness proves that its
scheduling authority has been released. Recovery can retire that participant's
future allowance and resume conservatively from fresh host measurements. Missing,
replaced or unsafe witness files fail closed. A retained capacity context holds
the lock if its coordinator stops. Dropping the last context parks its governor
before releasing that handle, so retained executor handles cannot spend an
allowance which recovery has retired.

This witness does not prove descendant cleanup. Any surviving native work becomes
observed external load. On macOS, arbitrary children can escape a process group;
losing both their owner and observer leaves no general ordinary-user method to
prove that every descendant ended. Measured recovery cannot prove workspace
safety or bound a survivor's future growth. An uncertain effect remains unknown
and is never replayed automatically.

A proved host boot change can reclaim former native ownership because native
processes cannot survive it. Provider request/token windows live separately and
survive boot cleanup. Their uncertain remote usage is not refunded by a native
lifecycle witness.

Registry protocol 2 adds participant lifetime witnesses. A protocol 1 registry
can migrate only after its participants drain. Other incompatible versions fail
before migration. Do not delete shared state to force admission. These protocols
are unreleased drafts; source changes do not establish a published upgrade path.

## Native supervision

Hosts call `native::dispatch_helper()` before argument parsing, terminal setup,
authentication or their normal async runtime. They prepare `CapacityContext`
before package imports, pass `context.native_owner()` to
`Executor::with_native_owner`, and use the same context with
`RuntimePool::with_capacity` and final harness construction. The legacy
constructors remain local and fixed.

The native owner starts on demand and exits when idle. Compatible participants
share one owner within their user/resource domain. Stable constraint identities
and CPU, memory, process and descriptor limits determine compatibility; changing
load does not create a new owner. A resource-domain change is rejected before a
new native launch when the existing owner cannot preserve placement.

Protected discovery state advertises an ephemeral loopback endpoint and random
capability. Failed contact or a stale advertisement cannot prove owner death.
A valid replacement generation in that protected state records succession of
the same lifetime lock. Authentication, protocol, generation and domain checks
precede effects. Scope connections act as
private lifetime links: client loss starts cleanup in the surviving owner. The
shared event loop bounds connections, active scopes, pipe buffers and completed
operation metadata. Launches carry the caller's bounded cwd and environment.
Operation IDs belong to one owner generation; a lost launch reply is uncertain,
and neither reconnect nor a replacement owner repeats it.

Before taking dependent job or package leases, the executor stages the full
request in its bounded local queue without charging a lease. Once eligible, it
calls `NativeOwner::ready()` and atomically activates the same ticket against
the current grant generation and recovery revision. Failed activation releases
the unused readiness guard before waiting again, while preserving queue age and
order. This also covers reuse of an idle helper by a larger job. Discovery can
retry before launch. Waiting for another process's startup lock follows the
command's overall deadline or package-startup cancellation. Direct native-owner
callers must bound readiness themselves; transport timeouts stay separate.
`NativeOwnerReady::spawn` uses the guard's generation and
cannot rediscover a replacement after admission. After submission, a lost
acknowledgement requires reconciliation with the original generation.

A broken lifetime connection keeps its local reservation until the same owner
confirms completion or lifetime-lock evidence proves that owner died. No timeout
refunds an uncertain native scope. Confirmed death parks admission before local
leases are released. The coordinator samples the host again before granting a
conservative first native target, and discards native learning from the lost
owner. Unknown survivors count as observed host load. Explicit package reload
can start a new host after that fresh grant; the failed callback is never replayed.

Windows Job Objects provide stronger inherited cleanup for contained children.
Unix process groups and descendant observation provide monitored cleanup.
Native execution does not provide a sandbox, filesystem isolation or network
isolation. No VM or container is required for this coordination design. See the
[native process contract](native-process-support.md) for exact limits and target
evidence.

### Helper commitments

Core implements the executor's public `NativeOwnerAdmission` boundary through
the existing registry. One helper generation has one commitment, independent of
its creating participant. Compatible clients share it. At most eight pending,
running, paused or draining generations occupy the pool. Unused slots consume
no memory or process allowance, and waiting agents create no helper each.

Each committed helper reserves a provisional 32 MiB and one PID. Linux process
constraints count its runtime threads, so the estimate reserves three task
slots there. Each applicable ancestor receives the charge once. Participant
baselines, interpreter charges and command reservations remain separate.
Pending helper demand lowers participant targets through the existing drain
protocol before any dependent execution or interpreter lease is held. Planning
selects a subset that leaves room for a useful dependent job after retained
resident floors. Demand and those floors travel in coarse participant renewals;
staging and helper reuse add no shared write per job. Existing leases retain
their charges until they drain, and a completed job does not refund a still-live
helper. Committing an already selected helper preserves sibling selections in
the same control generation. Each commitment still validates live demand,
grants and actual charges against every applicable ancestor. Cached
`CapacitySnapshot::native_helpers` reports occupied, pending and committed
generations and the committed memory and process estimates.

The launcher acquires the stable Unix `owner.lock` before admission and spawn.
The helper inherits that locked open file description through stdin, adopts it
before runtime startup, and excludes it from user children. It retains the
witness through runtime teardown. A creator crash, failed reply, pause or timeout
cannot refund the commitment. Recovery needs the supported released-lock or
verified-boot evidence. The permit has no refund on drop.

The registry uses protocol 4 and the private helper transport uses protocol 2.
Registry versions 1 through 3 must drain before migration. A live helper without
accounting is rejected for coordinated use. Stable registry and witness paths
remain shared across versions. Windows shared execution remains gated until its
supported lifetime transfer and native checks exist.

The baseline is an estimate, not enforced native memory containment. Before
automatic defaults, each claimed target needs cold-start and retained-state
peak measurements. The baseline must be at least the next power of two above
twice the largest validated startup peak, with a 32 MiB floor. Idle RSS alone
does not close this gate.

The [current-image report](evidence/native-helper-current-image-memory.macos-arm64.json.gz)
retains three startup and three control-state saturation trials per release CLI
and worker image after the stable-lock repair on this Mac. Startup lifetime RSS
maxima were 7.625 and 7.453125 MiB; saturated control-state maxima were 14.703125
and 14.421875 MiB. Both windows fit the unchanged estimate. A separate benchmark
pool charged eight helpers once, rejected a ninth and completed 6,400 commands
with verified helper exits and witness releases. It does not prove eight
saturated CLI or worker helpers. Read [the method and limits](benchmarks.md#real-embedding-helper-memory)
for exact image hashes, retained failures, earlier measurements and source scope.
This is not enforced containment or calibration of other images, platforms,
command descendants or full process trees.

The [local kernel-peak and pool evidence](evidence/native-helper-kernel-peaks-shared-pool.macos-arm64.json.gz)
records three startup/idle lifetime RSS bounds for an older smoke executable
and a separate sampler-checkpoint eight-helper pool run. The pool charged the host
ancestor exactly 256 MiB and eight processes, rejected a ninth helper before
spawn, and released witnesses only after the eight helpers exited. Its measured
helper RSS scan-sum high-water was 68.61 MiB. Registrations were staggered;
the preserved simultaneous attempt rejected a participant at runtime-baseline
admission before any helper started. This supports the pool accounting bound,
not simultaneous-start acceptance or complete embedding-image calibration.
The 32 MiB estimate and automatic-default gates remain unchanged.

A later [simultaneous-start check](evidence/native-helper-simultaneous-start.macos-arm64.json)
passed with eight participants in each of fixed and automatic mode, without
intentional launch staggering. Each mode completed 6,400 commands. All 16
client identities ended and were reaped; all 16 observed helper identities
ended and both helper and participant witnesses released without signals.
The frozen build includes the full runtime/dispatch startup floor, preserves
valid sibling helper selections during commitment and bounds initial WAL setup
retries. The report links the exact raw attempts, failures, source snapshot and
cleanup evidence. It supports this workload on this Mac, with sampled helper
discovery. It does not calibrate startup/retained-state peaks or close broader
consumer, platform, performance or automatic-default gates.

<a id="stable-lock-creation-blocker"></a>

### Stable lock creation repair

The earlier 2026-10-20 full gate reproduced a first-start race in permanent
lock creation. `StateDir::write_new` published a hard link before removing its
temporary name. A competing reader could see two links, and publisher death
could leave both names. The single-link check correctly rejected that state.
Earlier benchmark images retain this limitation in their recorded evidence.

The working source now uses `StateDir::create_new_empty` to create the final
name exclusively relative to the pinned private directory. It returns the
creator's validated handle. Only `AlreadyExists` leads to reopening through
that same `StateDir`, with both `require_single_link(true)` and
`require_owner_only(true)`. The open handle must describe a regular private
file with one link. Unix checks the effective owner and mode; macOS also
rejects extended allow ACLs. Windows checks the protected single-user DACL,
and permanent-lock creator handles omit DELETE access so peers can open them.
These checks describe the open file at that moment; they cannot prevent later
changes by trusted code running as the same user.

A post-create failure retains the final name. `HardLinked`, `TooPermissive`,
`UnsafeAcl` and other validation failures stop startup without deleting,
replacing or repairing the entry. Retaining the name avoids splitting lock
ownership across different files. Creation adds no file or directory sync and
makes no power-loss durability claim. A timeout still cannot authorize owner
replacement or release a live reservation.

The reviewed upstream 0.4.4 and 0.4.6 releases lack the required direct-create
and strict private-reopen APIs. Path-based creation, `open_confined` and
`AtomicWriter` do not meet the same pinned-directory, privacy and permanent-name
contract. The [maintained fork](../vendor/README.md) preserves the reviewed
0.4.4 source, licenses, patch and source hashes. All harness and path-host
consumers select `ejm-rightkit-fs` at full Git commit
`ad6faff2442ffe6abccd1856f4c0df9a3e472035`. Source and native builds can resolve
that public commit. Cargo registry publication still requires a verified
registry version, since Cargo removes Git sources from published manifests.
Return to upstream after equivalent APIs pass the same native contracts.

The [focused macOS evidence](evidence/native-stable-lock.macos-arm64.json)
covers a real creator killed after the final name became visible, concurrent
peer access to that same locked file, retained identity after death, actual
mode and macOS ACL failures, creator contention and hard-link refusal. It also
records normal release-test compatibility, strict executor Clippy and worker
compilation. Deterministic fault injection requires the explicit debug-only
`native-test-seams` feature; ordinary release tests omit it. This repairs the
publication gap in the working source. Native Windows/Linux, full consumer,
scale and automatic-default acceptance remain open. These checks do not prove
arbitrary descendant cleanup or workspace safety.

## Local learning

Learning stores disposable aggregate hints in the capacity registry, separate
from session history. Each profile contains a controller version, numeric
workload class, sample confidence, sustainable target, successful work rate and
service-time p95. Hashed keys scope it to the native environment and constraints;
model profiles also use the opaque provider quota identity. Prompts, command
text, session paths, credentials and raw HTTP traces never enter these profiles.
Loading or evicting learning does not scan session history.

A successful learning flush retains at most 256 profiles and 1 MiB of serialized
keys, scopes and profile data. An individual profile's JSON is limited to 4 KiB.
These are learning payload limits, not limits on the SQLite database, WAL or
other coordination tables. Oldest profiles are evicted when either aggregate
limit is exceeded. Each coordinator normally saves at most once per 60 seconds;
a final flush can save pending evidence sooner during shutdown.

Sample confidence halves after each seven days. Profiles older than 30 days
expire. Unknown versions, corrupt data and incompatible environments provide no
startup hint. A usable hint needs at least 32 samples after decay. Fresh healthy
observations must revalidate it, and current hard ceilings and shared grants
still limit admission. A learned target is not a reservation or proof of future
capacity.

`CapacityContext::snapshot().learning`, `capacityStatus()` and
`capacity_status()` report cold, learned or unavailable learning. An optional
learning-schema or write failure marks learning unavailable while valid work can
continue under working shared coordination. Failure of the protected registry
or its contact is separate: setup fails or new admission parks without releasing
live reservations. Do not delete shared state to repair optional learning.

The local host can call Rust's `reset_capacity_learning()`, TypeScript's
`resetCapacityLearning()` or Python's `reset_capacity_learning()`. Reset returns
the number of deleted aggregate profiles for this user's shared host registry.
Its epoch prevents an existing learner from saving pre-reset samples again.
Fresh evidence is collected on later control cycles. Reset does not change live
leases, current grants or targets, hard ceilings, provider charges, credentials,
session journals or artifacts. No model tool or provider feedback path exposes
reset or policy-setting authority.

## Verification and limits

The macOS-focused registry suite passed 29 tests, including two real clients
registering concurrently, private ACL/link/replacement rejection, charge-before-
install and stale-release faults, domain moves, missing ancestry, retained debt,
resident demand, graceful fencing, released-lock recovery after actual process
death, retained lifetime handles, invalid witness rejection, stable slot reuse
and boot recovery. Seven killed-subprocess transitions now cover uncommitted
and committed increases, installed targets, reduction before and after release
commit, and graceful removal after commit. A paused participant retains its
lifetime witness and charge. Real governor leases drain before release, lost
results reconcile once, and stale generations cannot release a later grant.
The [grant crash report](evidence/capacity-grant-crashes.macos-arm64.json) records
the exact source hashes, commands and local limits.

Real subprocesses also verify that validation, peer drop and
failed-open cleanup preserve writer exclusion in WAL and rollback-journal modes.
Captured-metadata fixtures exercise normal sidecar removal and unsafe vanished
sidecar rejection. Its synthetic fairness fixtures model drain and renewal.
The earlier synthetic fairness fixtures and later real commit-boundary faults
do not establish complete descendant cleanup, production throughput, a
256-process load result, or Linux/Windows acceptance.

A separate core fixture cancels queued model and tool waits within a 250 ms
assertion bound while a real SQLite write transaction blocks coordination and
while unsafe database permissions make validation fail. Retained leases keep
their exact charges, cached local memory admission still works, and renewal and
graceful owner release resume after each fault clears. This is local macOS
functional evidence. CLI input and Stop timing have their own measured gates.

The [native recovery report](evidence/native-owner-recovery.macos-arm64.json)
covers idle and busy client/owner crashes, retained cancellation/drop cleanup,
spawn failure and useful progress after a fresh grant on macOS arm64. Package
fixtures cover owner death during import and explicit reload in both languages.
These checks require no state deletion or reboot. They cannot prove cleanup of
arbitrary detached descendants or native behavior on Linux or Windows. The
[initial package measurement](packages.md#initial-package-host-measurement) also
missed the low-load sampler CPU target. The [typed native sampler follow-up](evidence/package-host-overhead.macos-arm64.native-sampler.json)
reduced idle-owner CPU to 1.626% to 1.933% of one core, still above the below-1%
target. Its binary hashes and partial source provenance are recorded separately.
The [scoped sampler measurements](packages.md#scoped-macos-memory-sampling)
then met the target in short intervals. The separate
[sustained idle measurement](packages.md#sustained-idle-measurement) failed it:
the Python case averaged 1.15% of one core over a configured 60-second interval.
These reports predate the separate helper commitment. Automatic-default
activation remains gated on platform, accounting and performance requirements.

See [executor admission](executor.md), [benchmark evidence](benchmarks.md) and the
tracked OpenSpec change for the remaining gates. No site publication or package
release is implied by source documentation.

The [bounded dispatch report](evidence/capacity-bursts.macos-arm64.json) records
completion and mass cancellation at 1000 and 4096 managed runs with one tool
dispatch slot, one job slot and an executor queue limit of four. Four ordinary
fixtures also cover hard count/UTF-8 byte bounds, deadlines in governor/dispatch
waits and canonical-workspace write exclusion through retained background work.
Accepted effects are checked exactly once and charges return to baseline. These
are local correctness checks, without a useful-throughput or native-platform
acceptance claim.

### Shared renewal failures

Capacity status distinguishes database contention, other database failures,
filesystem failures and admission failures. These categories omit raw errors,
paths and database contents. Failed renewals do not extend the eight-second
contact window; expired contact parks new work while existing charges remain.
A command can still reach its deadline while waiting for coordination recovery.

Current-protocol registry startup reads the schema version without acquiring a
write transaction. Creation and migration retain an immediate transaction and
locked version recheck. The 100 ms contention budget and admission deadlines
are unchanged.

Before any command submission, a verified clean helper exit during startup
permits one rediscovery under the existing caller deadline. Nonzero helper
exits fail. Each replacement reacquires stable ownership and generation
admission; this does not retry a command or refund a live reservation.
