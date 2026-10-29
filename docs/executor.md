# ejm-harness execution

`ejm-harness-exec` supplies one resource governor and a native command executor. Every
session, child agent, native tool, and executor in an ejm-harness instance must
use the same `Arc<Governor>`. Creating a governor per agent defeats the limits.

## Admission

The governor accounts for active jobs, heavy jobs, CPU estimates in millicpus,
memory estimates, process estimates, and retained output bytes. It rejects
reservations that cannot fit the configured aggregate limits. Zero-valued
dimensions are allowed for native tools that need no such resource. Commands
must reserve positive CPU, memory, processes, and output.

Defaults permit four active jobs and one heavy job, with aggregate reservations
of 2000 millicpus, 512 MiB memory, 32 processes, and 1 MiB output. The queue holds
at most 256 requests. These are conservative defaults, not measured capacity
claims. Host applications should classify builds and tests as heavy and choose
their reservations. A model cannot increase configured limits.

Requests rotate between queued sessions, with FIFO order within each session.
`Governor::new` retains the fixed policy: a session's head cannot be bypassed by
smaller reservations. This prevents starvation and accepts some head-of-line
blocking in exchange. A dropped waiter is removed immediately. A lease releases
every reserved dimension on drop.

### Capacity targets and memory ownership

`Governor::new_with_policy(limits, AdmissionPolicy::Automatic)` opts into bounded
backfill. It preserves FIFO within each session while allowing another session's
fitting head to use spare capacity. A blocked head can be bypassed by at most
eight grants or until it has waited one second. After either bound, conflicting
work drains so that head can progress. `AdmissionPolicy::Fixed` retains the old
head ordering when used through the new constructor.

The new constructor also charges retained output to the same memory account as
the tool's `Reservation::memory_bytes`. The combined checked sum must fit the
immutable memory limit. `acquire_memory` and `try_acquire_memory` reserve context,
model-buffer or persistent host memory in that account without consuming an
active-job, CPU, process or output slot. `acquire_resident` and
`try_acquire_resident` also charge an explicit resident process count. Keep these
leases until the interpreter has been reaped. `try_acquire_memory` returns `None`
instead of bypassing existing waiters or waiting for capacity. Existing
`Governor::new` accounting stays compatible.

`admission_demand` projects the resident floor plus the oldest queued request,
or current use when no request waits. This lets shared allocation reserve useful
work before filling spare capacity beneath tuned ceilings. It does no host I/O.
`admission_floor` exposes only retained resident resources, so transient work can
drain without being mistaken for a permanent minimum.
`admission_activity` returns a cached, saturating count of local job activations.
It includes short jobs between registry renewals; installing a target, reserving
resident memory or inspecting eligibility does not advance it. This counter
provides aggregate fairness feedback without a shared write for each job.
`wait_jobs_idle` excludes resident leases; final `wait_idle` includes them.

For work that needs native readiness, `stage` adds a bounded queue ticket without
charging a lease. `QueuedAdmission::wait_eligible` waits for that ticket's turn;
the caller then obtains readiness and calls `try_activate`. Activation checks
the ticket, grant generation, recovery revision, contact and available capacity
under the governor lock. If it returns `None`, drop the unused readiness guard
before waiting again. The ticket retains its position, age and aggregate demand
across retries; dropping it cancels the queued request. No launch effect has
occurred at this point, so retrying readiness does not repeat native work.

`Lease::split_resident` transfers memory and process charges from one admitted
job into a resident lease. It leaves aggregate usage unchanged and preserves
the remaining job's CPU, output and execution slot. Package startup uses this
transfer to admit its complete peak without a second admission wait.

`OperatingTarget` holds separate adjustable job, heavy-job, CPU, memory, process
and output budgets. An owner installs one with a strictly increasing generation
and an optional local monotonic contact deadline through `install_target`.
Installation rejects targets above the immutable `ResourceLimits`. Zero targets
are valid. A smaller target preserves every outstanding lease and exposes excess
reservations as drain debt through `operating_snapshot`; it cannot make shared
credits available by itself. A contact deadline freezes new admission on expiry
without revoking existing ownership. Queue admission checks cached state under
one mutex and performs no SQLite, filesystem or network I/O.

Recovery captures `admission_revision()` before fresh host sampling and calls
`install_target_for_revision`. `park_admission()` advances that revision and
zeros the target under the same governor lock. A grant based on a prior revision
cannot reopen admission after a concurrent owner death. Counter exhaustion keeps
admission parked. Existing `install_target` remains available for callers that
own their own coordination; coordinated recovery uses the revision check.

The harness core owns shared grants and must charge an increase before installing
it locally. For a reduction, it installs the smaller target, waits with
`wait_drained(generation, cancel)`, and commits the shared reduction only after
outstanding reservations fit. The wait rejects a replaced generation. The
executor constructor alone does not discover or join a host registry. A host
must install its charged grant before exposing a coordinated governor.

These are estimate budgets, not a native memory sandbox. Callers must reserve
baseline, retained context and persistent runtime state as well as transient
work. A sampled RSS value never releases a live reservation.

### Host observations

`HostSampler` uses existing `sysinfo`, `procfs-core` and Rustix APIs. Reuse it on a
bounded control interval outside Tokio's async worker threads. It starts no
background task, process or per-agent sampler. `HostObservations` keeps value,
source, availability, age and freshness distinct. Call `refresh_age` before using
a saved sample. The initial CPU-usage sample is unavailable until a second
sufficiently spaced observation. Missing and stale readings do not mean zero
pressure.

The sampler observes scheduler CPU availability, host memory and available
memory, CPU usage, and Unix soft process/descriptor limits. The descriptor limit
is per process. The process limit is per user and does not report remaining
process capacity. Linux cgroup v2 observations walk at most 64 visible ancestors,
read at most 256 KiB of mount data and 16 KiB per membership/constraint file, and
apply tighter CPU quota/cpuset, memory and process constraints. Missing controllers
remain absent; failed reads or malformed limits are unavailable. Native cgroup
device/inode IDs describe visible shared domains without retaining paths. A
visible `/` mount root still cannot prove that no ancestor is hidden by a
namespace, so visibility stays estimated. Cgroup v1 and inaccessible ancestry
remain unsupported. Parser fixtures do not prove native Linux behavior.

`HostSampler::boot_identity` uses the kernel boot UUID through the target-scoped
maintained `sysctl` package on macOS and bounded procfs on Linux. It creates no
child process. Windows has no supported boot UUID adapter here and returns
`None`, rather than using wall time minus uptime. `process_identity` adds one
native process-birth observation when available. Failure remains `None`; neither
a missing PID nor a matching birth value proves that descendants have ended or
allows shared credits to be reclaimed.

## Native execution and capabilities

Native execution inherits the host environment with explicit per-command
overrides and uses an explicit working directory. Ordinary command execution
closes stdin. `Executor::run_with_input` accepts at most 8 MiB of private bytes,
writes them while both output pipes drain, then closes stdin. Input stays out of
serialized requests and artifacts. Its environment removals apply before explicit
request overrides. It provides no interactive terminal.
It never changes the host process's cwd or environment. Direct argv execution
does not interpret a shell language. A shell tool must select and invoke the
actual shell.

Capabilities report three states:

| State         | Meaning                                                          |
| ------------- | ---------------------------------------------------------------- |
| `enforced`    | The executor implements the restriction within its stated scope  |
| `monitored`   | The executor observes or reacts, with sampling or OS limitations |
| `unsupported` | The backend supplies no such restriction                         |

Native execution enforces admission, retained output limits, and its shared
artifact accounting. Resident memory and process count are sampled when the OS
supports process inspection. Exceeding the command's reservation triggers
termination and an error. Sampling can miss short-lived processes or transient
spikes. CPU reservations govern admission; the native backend does not set CPU
quotas. A single lazy sampler serves active commands across the governor. It
does not start a monitor process, shell, or thread for each managed agent and
does not inspect process environments or command-line arguments.

Linux sampling reads bounded `/proc/<pid>/stat` records and closes each file
immediately. `procfs-core` parses those records; our adapter retains the 16 KiB
read cap and uses raw start ticks to check process identities. It does not change
the host's resource limits or retain a descriptor for every host process.
macOS uses `libproc` to enumerate full-host BSD process topology every 100 ms
while native work exists. It reads `TaskAllInfo` for owned roots and selected
descendants, pairing birth identity and RSS without loading arguments,
environments or thread details. An ambiguous failed owned-process read gets one
bounded fresh identity and memory observation. Only matching readable state,
confirmed exit, a matching zombie or a replacement identity resolves that race;
other persistent unreadability fails closed. A matching BSD identity with native
exit-progress metadata remains a separate exiting state when RSS is unreadable.
It retains ancestry and counts as an occupied process. The aggregate RSS is
incomplete: readable memory still supplies a lower bound for violation checks,
and the previous complete memory peak remains unchanged. This state proves
neither process death nor ended memory use. Job leases remain charged through
native wait and cleanup. Missing samples cannot justify smaller reservations.
The BSD identity lookup includes zombies, which can remain visible to
`kill(pid, 0)` after live-only process reads fail. It checks their birth identity
before treating them as exited. Internal cleanup compares the full microsecond
birth timestamp. Public `ProcessIdentity.start_time` keeps
whole-second units for compatibility with existing recovery records.
Enumeration is a point-in-time view. Identity validation and signal delivery are
separate native calls, so the PID-reuse check is not atomic.
Windows uses native process inspection through `sysinfo`.
Safe Rustix APIs supply Unix process signals, page size, and user identity.

A sampler failure outside that verified exit-progress state fails the affected
command and preserves cleanup ownership.
Its error can include one native diagnostic detail per affected job, capped at
512 UTF-8 bytes. Details identify the sampling stage, PID, expected birth
identity, and native probe errors. They exclude command arguments, environment
values, private input, and panic payloads. Successful samples create no detail;
there is no per-sample log or durable diagnostic ledger. A failed observation
does not establish process death or authorize a reservation refund.

The [macOS exit-transition evidence](evidence/native-sampler-exit-transitions.macos-arm64.json)
retains both observed kernel races, the bounded failures, source hashes and the
final 76 passing executor tests. The source analysis uses a nearby pinned XNU
revision, not the exact installed kernel. Native Linux and Windows behavior,
arbitrary escaped descendants and startup memory peaks need separate evidence.

The wall deadline includes queue time and is clamped to `max_job_ms`, which
defaults to 120 seconds. Cancellation or a deadline triggers process cleanup.
The deadline is reported as monitored because a native process in an
uninterruptible kernel operation may defer termination.

On Unix, commands enter their own process group before exec. Cleanup signals
that group, escalates to kill, and waits for the direct child. It also attempts
to kill sampled descendants after checking their recorded identities, including
children whose original parent has exited. This is best effort: signalling does
not confirm that every descendant has exited, and background processes can
outlive cleanup, including after an otherwise successful command.

Windows commands start suspended, join a kill-on-close Job Object, and resume
only after assignment. Cleanup kills the Job, reaps the root, and retains the
Job handle until a successful membership query reports no processes. Failed
queries keep cleanup pending. A Job assignment failure terminates the suspended
child and fails launch; the executor never falls back to an uncontained child.

Process groups and sampling are not escape-proof containment. A child can change
its group and become reparented between samples. The common capability contract
therefore keeps process-tree cancellation at `monitored`. See
[native process support](native-process-support.md) for the package choice,
low-level child API, nested-Job behavior, and native verification limits.
The native executor supplies no filesystem or network isolation and is never
called a security sandbox.

Every true field in `RequiredCapabilities` requires an enforced guarantee.
An unsupported or monitored requested restriction fails before admission and
launch. Linux cgroups, isolated helpers, and external
container/VM executors can be added behind explicit backend capability checks.
No backend is silently substituted for a requested isolation mode.

## Cancellation ownership

### Shared native owner

Coordinated hosts use `ejm_harness_exec::native::dispatch_helper()` at the start
of their synchronous `main`, before argument parsing, authentication, terminal
setup, or the ordinary async runtime. It returns `None` for an ordinary launch,
or the helper's `Result<ExitCode>` for the host to return. See
[`native_owner_helper.rs`](../crates/ejm-harness-exec/examples/native_owner_helper.rs)
for a minimal embedder. A helper invocation creates one current-thread Tokio
runtime with at most two blocking threads. It reexecutes the host binary and
needs no separately installed daemon binary.

Prepare the public `CapacityContext` before package imports. Pass its
`native_owner()` to `Executor::with_native_owner`, and pass the same context to
`RuntimePool::with_capacity` and final harness construction. The context owns
admission and the executor owns native supervision. `NativeOwner` alone does not
reserve a shared grant. Existing `Executor::new` and low-level local process APIs
keep their local behavior.

`NativeOwner::new(state_root, domain_key)` creates a cached handle without I/O,
threads, sockets, or processes. Its first readiness request discovers or starts
one shared execution owner for that compatible user/resource domain. Embedders may
select an existing helper-capable host with `with_helper_executable`. The caller
and owner must have matching visible resource-domain identities and CPU, memory,
process, and descriptor constraints. The owner checks these again before native
effects. Load changes do not change the domain. Unsupported placement changes
fail rather than silently running in the first caller's environment.

Linux also compares the exact Rustix CPU affinity set and a fixed set of native
namespace device/inode identities. It rejects missing critical namespace/cgroup
evidence and a calling thread whose cgroup differs from the process leader used
by host observations. Placement is checked before and after sampling, and again
immediately before owner launch. Read-only recovery queries need no new placement
grant. The Linux target type-check passed, but native Linux acceptance remains
unrun. Shared placement on
Windows is explicitly unsupported: the current maintained adapter cannot inspect
the caller's enclosing Job and affinity identity. Windows `NativeOwner::spawn`
rejects before helper startup. Legacy local execution keeps its existing Job
contract.

Discovery uses `native/<domain>/endpoint.json` within protected capacity state.
An ephemeral IPv4 loopback endpoint and random capability authenticate the
private connection. Other code running as the same OS user remains trusted.
The permanent `startup.lock` serializes discovery, while `owner.lock` establishes
exclusive lifetime ownership. Neither lock file is unlinked or replaced.
Failed contact, a timeout, and a paused process do not authorize takeover. A
replacement publishes its epoch only after acquiring that same lifetime lock.

Core attaches `NativeOwnerAdmission` through `with_admission` to obtain the
helper's separate coarse registry commitment. The executor holds `owner.lock`
before calling this boundary or creating the process. The Unix helper inherits
the locked open file description through stdin, adopts it before runtime
startup and retains it through runtime teardown. User children do not inherit
that witness. A dropped permit never refunds a commitment. Read
[helper commitments](host-capacity.md#helper-commitments) for the provisional
baseline, eight-generation pool, protocol migration and unfinished checks.

`NativeOwner::ready()` returns a generation-bound `NativeOwnerReady`. Executors
and package pools stage their full demand before requesting it, then atomically
activate the staged ticket. They release an unused guard whenever activation
must wait, including reuse of an idle helper. Readiness connections are shared
per client; they must not pin an otherwise idle helper across a resource wait.
`NativeOwnerReady::spawn` cannot rediscover another generation. Legacy direct
`NativeOwner` construction remains available without coordinated accounting.

Waiting for another process's startup lock follows the caller's cancellation or
deadline. The executor applies the command's overall deadline. Package startup
listens for management cancellation and runtime shutdown. Direct callers of
`NativeOwner::ready` or `NativeOwner::spawn` must bound the future themselves.
Startup contention does not use the five-second transport timeout or authorize
takeover of a live owner.

Each scope has its own lifetime connection. Client EOF starts cancellation and
cleanup in the surviving owner. Children, pipes, sampled descendant identities,
and cleanup metadata stay in that owner's bounded memory. Waiting logical agents
create no owner scope, child process, or watchdog. All remote scopes share the
owner's event loop and lazy process monitor.

`NativeCommand` snapshots the requesting caller's cwd and lossless OS environment.
Its explicit overrides, removals, and clear operation preserve native command
semantics. The owner installs that final environment for each launch. Private
stdin uses separate bounded stream frames. Arguments, environment values, stdin,
and completed command output are not written to discovery or a recovery ledger.
The executor's normal bounded output artifacts still apply.

| Bound                                         | Current value                                  |
| --------------------------------------------- | ---------------------------------------------- |
| Concurrent handshakes                         | 32                                             |
| Active native scopes per owner                | 1024                                           |
| Completed operation IDs retained              | 256                                            |
| Initial handshake frame                       | 32 KiB                                         |
| Authenticated launch frame                    | 2 MiB                                          |
| Raw program, argv, cwd, and environment bytes | 256 KiB                                        |
| Arguments and environment entries             | 4096 each                                      |
| Native scope lifetime                         | At most 24 hours                               |
| Stream chunk and client pipe buffer           | 16 KiB each                                    |
| Owner output queue                            | Four frames per scope                          |
| Idle exit                                     | Two seconds after connections and scopes drain |

Handshake and control send/reply operations have five-second limits. Each stdin
chunk waits for acknowledgment; a blocked stdin does not prevent cancellation or
EOF detection. Launch-sized framing allocations are released before streaming.
These bounds limit live state. They are not a tested 1024-job capacity claim.

An operation has a random owner epoch and a checked monotonic ID. `reconcile`
queries only its original live epoch and never starts a helper. A lost launch or
completion reply remains an error even when later cleanup is confirmed. It is
never resubmitted. A replacement rejects an earlier epoch. Expired completed IDs
cannot become new operations.

After transport loss, both connection halves close. Admission remains charged
until the same owner reports `Finished` or a stable-lock probe establishes owner
death. A valid replacement epoch also records prior lock release under this
protocol. Repeated failures and timeouts while the old lock is held keep cleanup
pending. Death probes are shared per handle and cache negative observations for
500 ms; confirmed epochs stay in a bounded in-memory cache.

`NativeChild::cleanup_state()` reads cached evidence:

| State       | Meaning                                                                                                                                              |
| ----------- | ---------------------------------------------------------------------------------------------------------------------------------------------------- |
| `Pending`   | Retain ownership while cleanup or owner-death proof is missing. Local children also report this; successful local `kill()` is their cleanup receipt. |
| `Complete`  | The original owner finished supported cleanup. A lost exit status can still produce an error.                                                        |
| `OwnerDied` | The lifetime owner ended. Descendant cleanup and effects remain uncertain.                                                                           |

The executor and governed package transport park their governor before releasing
retained admission on `OwnerDied`. Core recovery needs a fresh sample, a fresh
admission revision, and a conservative grant before useful work resumes. Cached
`NativeSnapshot` includes death count, last dead epoch, owner PID, operation,
scope count, domain placement, and restriction states. Reading it performs no I/O or liveness
probe and starts no helper. Its owner PID is the last authenticated PID.

Unix descendants can escape process groups and observation, and can survive
owner death. Retiring scheduling authority does not prove a workspace safe for
writes. Their observed use becomes external load during recovery. There is no
durable per-job native execution ledger or automatic replay of uncertain effects.
Auth terminal interaction and SDK worker bootstrap retain their explicit host
ownership contracts. Arbitrary package code or external services can create
processes beyond the supported native scope.

### Caller supervisors

After admission, a Rust supervisor task owns the command and its lease. Dropping
`Executor::run`, including an engine cancelling a tool future, cancels that
supervisor. The supervisor retains the lease while it attempts child cleanup and
finishes or aborts output drainers. On Unix, a descendant may still be alive when
the lease is released, including when cleanup reports an error. It does not retry
a command after cancellation or failure.
Host applications must keep the Tokio runtime alive to complete cleanup during
shutdown. After preventing new submissions and signalling every run's
cancellation token, call `Governor::wait_jobs_idle()` to await queue removals and
execution leases while resident memory remains charged. Shut down persistent
package hosts and drop their memory leases and the core baseline lease, then call
`Governor::wait_idle()` before releasing the shared process grant. The final
barrier includes memory-only leases, supervisors whose caller future was dropped,
and native blocking tools holding leases. Legacy hosts without resident memory
leases can continue to use `wait_idle()` directly.

Hosts that serialize workspace effects can use `Executor::run_with_retained`
to move an owned guard into the supervisor. The guard stays alive through the
supervisor's cleanup attempts even when the caller future is dropped. On Unix,
that does not guarantee all descendants have stopped before the guard is released.
Ordinary `run` uses the same path with an empty retained value.

Rust hosts can register the coding tools with
`tools_with_shell_process_tree_cancellation(config, true)` for the shell. A
native executor currently reports this capability as monitored, so strict shell
calls fail before admission and launch. The existing `tools` registration keeps
its default requirements. Worker and SDK hosts opt in with
`require_shell_process_tree_cancellation: true`. This is a fail-closed policy,
not a new native containment backend.

This generic guard remains in the caller process. It cannot be serialized into
the shared helper. Dropped futures retain it through the caller supervisor's
cleanup; total caller death loses that Rust value. A surviving native owner still
cleans its scopes after EOF. Losing the owner reports uncertainty and cannot
certify workspace mutation safety.

Private-input execution uses the same supervisor. A child that ignores stdin
cannot block cancellation or output draining; its input writer is reaped with
the other pipe tasks. Hosts should pass credentials through their dedicated
auth interfaces rather than model input.

## Output and artifacts

Stdout and stderr drain concurrently in fixed-size chunks. Each receives half
the command's output reservation as a retained tail. Invalid UTF-8 is replaced
without allowing the returned strings to exceed that reservation. Noisy output
cannot grow the resident transcript without bound. `truncated` reports loss in
either retained tails, artifact output, or final pipe draining.

Output also spools into a private, newly created `exec-<uuid>.log` file. Records
identify the stream and original byte count. The last record may be incomplete
when the shared quota is reached. Raw output can contain binary data; consumers
must render it safely rather than executing terminal control sequences.

The retained artifact quota is shared across all executors using the governor,
including different artifact roots. Accounting charges actual record bytes and
a 4 KiB allowance per file so tiny artifacts cannot create unlimited files.
Existing regular files are counted when each root is first registered. Roots
must be dedicated directories owned by the host; external writes are outside
this accounting contract. Reopening an already registered root does not count
its files twice. Removing files externally does not reclaim credits until a
fresh governor scans the directory. Failed writes retain charged credits
conservatively. There is no automatic artifact deletion.

Once the artifact quota is exhausted, the executor keeps draining pipes and
retaining bounded tails. It does not block the child on a full output buffer.
Disk write errors fail the command after process cleanup. The quota bounds
accounted file bytes, not filesystem-specific physical disk allocation.

`peak_memory_bytes` and `peak_processes` are observed peaks for attributed live
processes. Zero means no sample was available. Fast commands may finish between
samples, and shared memory can be counted in more than one process. These values
must not be described as kernel-enforced limits or exact resource usage.

## Verification

The focused test suite checks session rotation, aggregate reservation dimensions,
queue limits, cancellation/drop release, one heavy job, 100 managed admissions,
dual-pipe noisy output, invalid UTF-8, shared artifact quotas, prelaunch rejection,
spawn errors, queue deadlines, ordinary descendant cleanup, abandoned
caller cleanup, parent exit with inherited pipes, and sampled process abuse.
Linux stat adapter fixtures cover parentheses and non-UTF-8 process names,
raw start ticks, malformed resident-page counts, and incomplete records. They run on
every host; actual Linux execution still needs Linux platform verification.
The lifecycle fixtures also compile for Windows, but local macOS runs do not
prove Windows Job cleanup or native Windows terminal behavior.
No hosted model or external service is used. OS isolation and old-machine
capacity claims need separate platform evidence.

The native-owner fixtures additionally cover a lost preparation reply before
submission, a lost launch reply after a real file effect, original-epoch queries,
expired IDs, rejection before effects for protocol/capability/domain mismatches,
and ownership retained while an unavailable owner still holds its lifetime lock.
The client can rediscover once when preparation fails before any launch was sent.
An explicit rejection is returned directly. Once submitted, the operation is
queried in its original epoch and is never submitted again automatically.

The helper smoke exercises real idle and busy client/owner deaths, paused-owner
exclusion, cancellation and dropped callers while cleanup is paused, spawn
failure, an escaped Unix descendant, and useful executor progress after a fresh
grant. Guards and reservations remain charged until supported cleanup completes
or confirmed owner death parks admission. Package fixtures also kill the owner
during Node and Python import and explicit reload, preserve the failed operation,
and require an explicit reload after a fresh grant. Native Linux and Windows
owner lifecycle runs and representative large-scale native throughput remain
separate gates.

### Sampler traversal measurement

On 2026-10-19, the private release-profile fixture used 4096 synthetic host
processes in eight-process trees and 4, 32 or 128 active jobs. Each of 25 samples
started without previously attributed children. The same fixture, Rust 1.99.1
release profile and Darwin 25.5.0 arm64 host produced:

| Active jobs | Previous p50 / p95, us | Shared index p50 / p95, us |
| ----------- | ---------------------- | -------------------------- |
| 4           | 268 / 298              | 124 / 158                  |
| 32          | 3079 / 3176            | 130 / 137                  |
| 128         | 11260 / 11593          | 181 / 223                  |

The shared parent index is built once per sample. This fixture measures traversal
and accounting, excluding native enumeration, I/O and child cleanup. It establishes
neither 128 supported concurrent native jobs nor full-system throughput. Its
focused command is:

```sh
CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 cargo test -p ejm-harness-exec --release --lib sampler_traversal_cost --locked -- --nocapture
```

### Observation dependency footprint

The macOS process sampler uses `libproc` 0.14.11 (MIT). Its runtime dependencies,
`libc` and `errno`, were already in the graph. The target-specific addition brings
eleven lockfile packages, including its build-time `bindgen` toolchain. Building
on macOS requires an Apple SDK and a discoverable `libclang`; the measured host
already had both through Xcode and Command Line Tools. No build tools were
installed for this change. Other native targets do not build this adapter.

Upstream metadata checked on 2026-10-20 still lists 0.14.11 as the latest
`libproc`. It requires `bindgen ^0.72.1`, whose build tools require `shlex ^1`
and `itertools >=0.10,<0.14`. The current `cc` requires `shlex ^2.0.1`; CLI's
terminal libraries also require `itertools ^0.14`. These ranges cannot converge.
The harness allows only `shlex =1.3.0` through its duplicate-version check;
consumers with the terminal libraries also need the exact `itertools =0.13.0`
exception. `bindgen` 0.73 uses `shlex` 2 and removes `itertools`, but cannot
satisfy the current `libproc` constraint. Remove these exceptions when `libproc`
permits that newer build toolchain and no other path requires the older versions.

The existing `sysinfo` macOS refresh read BSD/thread metadata and an
argument/environment buffer even with its minimal refresh configuration. Keeping
that enumeration and adding selected RSS reads would retain those calls and
split identity from memory observation. `libproc` supplies typed enumeration
and process information in one maintained package. `darwin-libproc` 0.2.0 was
experimental with older maintenance and dependency constraints;
`proc_pidinfo` 0.1.4 lacked enumeration. Rustix and `sysctl` did not supply the
complete typed interface. Project code adds no unsafe block or custom native
parser.

`libproc` accepts a positive native byte count without exposing the exact returned
size. The adapter validates PID, nonzero birth seconds, microsecond range and
checked timestamp arithmetic. It cannot independently verify the returned
structure size. Native identity and stale-token cleanup fixtures protect the
adapter contract. The initial idle-owner CPU miss and replacement measurements
are kept separately under [package-host measurements](packages.md#initial-package-host-measurement).

The macOS boot-identity adapter adds `sysctl` 0.7.1 (MIT), with `enum-as-inner`
0.6.1 and `heck` 0.5.0. It reads `kern.bootsessionuuid` without an unadmitted
child process or project unsafe code. Existing `sysinfo` process start times
alone do not supply a kernel boot identity.

On the same Darwin arm64 release target, the capacity integration-test executable
was 1,127,648 bytes before this dependency and 1,161,392 bytes after the final
resident-process and demand fixtures, an increase of 33,744 bytes (2.99%). The
fixture changes confound attribution, so this is the combined test-binary delta,
not an isolated package-cost claim or a shipping CLI measurement. The final
release fixture passed seven capacity tests. At that stage the executor suite
passed 41 tests with two intentionally ignored fixtures, and strict all-target
Clippy passed. The native-owner work below adds later checks. Linux parser tests
ran on macOS and do not prove native Linux
behavior. No container execution support is established by these checks.

Permanent native locks now use the temporary pinned
[filesystem repair](host-capacity.md#stable-lock-creation-repair): exclusive
final-name creation and strict single-link/private checks on the open handle.
Failed checks retain the name without repair or replacement. The focused macOS
crash and privacy fixtures pass; native Windows/Linux and broader acceptance
remain open. Publishing waits for an equivalent maintained upstream release,
and automatic-default activation remains gated.

### Native owner cost and recovery evidence

The [current-image report](evidence/native-helper-current-image-memory.macos-arm64.json.gz)
adds three startup and three control-state saturation trials per release CLI
and worker image after the stable-lock repair on this Mac. Saturated helper
lifetime RSS maxima were 14.703125 MiB for CLI and 14.421875 MiB for worker;
startup maxima were 7.625 and 7.453125 MiB. The estimate remains 32 MiB.
A separate benchmark pool completed 6,400 commands with eight charged helpers,
refused a ninth and verified all helper exits and witness releases. These
results do not establish command or process-tree peaks, eight saturated CLI or
worker helpers, other images or platforms, full recovery, or automatic defaults.
Read the
[benchmark method and limits](benchmarks.md#real-embedding-helper-memory)
for startup timing, bounded occupancy, exact identity and witness cleanup, and source
scope. Earlier measurements below retain their original executable scope.

The 2026-10-20 release
[`native-helper-startup.macos-arm64.json`](evidence/native-helper-startup.macos-arm64.json)
retains three staged-helper startup observations, workspace input hashes, the
frozen executable hash and the raw-report hash. This checkpoint predates the
bounded sampler diagnostics and later exit-transition fix.

| Observation                           | Range across three runs |
| ------------------------------------- | ----------------------- |
| Cold helper readiness                 | 56.897 to 57.550 ms     |
| Helper RSS at readiness               | 6.703 to 6.719 MiB      |
| Helper RSS after 21 native operations | 7.875 to 7.953 MiB      |
| Open descriptors at both observations | 13                      |

The point RSS observations fit the provisional 32 MiB helper commitment. The
kernel's lifetime peak physical footprint was 1.657 to 1.672 MiB at readiness
and 2.766 to 2.844 MiB after those operations. Physical footprint is not RSS,
and point RSS is not peak RSS. This fixture covers only the example helper
process, with unsaturated retained state. It does not calibrate descendants,
all eight occupied generations, other host executables or other platforms.
The helper startup-peak and automatic-default gates remain open.

The later [kernel-peak and shared-pool report](evidence/native-helper-kernel-peaks-shared-pool.macos-arm64.json.gz)
adds three kernel lifetime RSS maxima of 6.70 to 6.77 MiB through command-free
startup and idle windows of that older smoke executable. These are RSS maxima,
not physical-footprint counters or exact Ready-boundary timings. The sampler
checkpoint's frozen benchmark separately reached eight committed helpers in one registry,
rejected a ninth at actual pool admission, completed 6,400 commands and verified
all helper exits and witness releases. Twenty sequential scans over 4.752127
seconds reached 68.61 MiB summed helper RSS; individual helper lifetime RSS
maxima were 8.50 to 8.66 MiB. Native-command descendants are outside those sums.
See [the benchmark scope](benchmarks.md#kernel-rss-peaks-and-the-shared-helper-pool)
for the preserved simultaneous-registration rejection, separate executable
checkpoints and remaining calibration and consumer gates.

A later [simultaneous-start report](evidence/native-helper-simultaneous-start.macos-arm64.json)
uses the corrected full participant startup floor and helper selection behavior.
Eight participants completed 6,400 commands in each of fixed and automatic
mode without intentional launch staggering. Each mode verified eight client
exits/reaps, eight observed helper identity exits and all helper/participant
witness releases without signals. The exact 130-input checkpoint and prior
failed attempts are retained. This closes the reproduced startup stall for
that workload on this Mac, with sampled helper discovery. It does not close
startup calibration, full crash recovery, consumer resource or platform gates.

The 2026-10-19 macOS 26.5.2 arm64 debug fixture recorded
[`native-owner-smoke.macos-arm64.json`](evidence/native-owner-smoke.macos-arm64.json).
It used one owner, four warm 2 MiB stdin/stdout round trips, and sixteen short
exit commands. Cold timing includes helper startup and the first round trip.
The local path used the same child binary and stream workload.

| Measurement                 | Shared owner     | Local path             |
| --------------------------- | ---------------- | ---------------------- |
| Cold 2 MiB round trip       | 74.7 ms          | Not separately sampled |
| Warm 2 MiB round trip, mean | 15.1 ms          | 5.3 ms                 |
| Short exit command, mean    | 4.0 ms           | 2.9 ms                 |
| Owner idle resident memory  | 11,534,336 bytes | No extra owner         |
| Owner idle open descriptors | 12               | No extra owner         |
| Extra owner process count   | One              | Zero                   |
| Client EOF cleanup          | 0.8 ms           | Not separately sampled |
| Idle exit after final scope | 2.004 s          | Not applicable         |

The shared owner adds transport and startup cost in this fixture. This design
trades that cost for shared lifetime ownership and bounded cleanup after client
death. The fixture also passed paused-owner exclusion, stale-epoch rejection,
private environment removal/cwd preservation, and executor recovery with a fresh
grant. A detached descendant survived owner death and was killed explicitly by
the fixture. No crash outcome was converted into a successful command.

That executor suite passed 48 tests with two intentionally ignored fixtures.
Strict all-target Clippy passed for executor and package crates. Linux all-target
type checking passed with the installed x64 GNU target; it is not native Linux
execution evidence. The evidence file includes source hashes for the executor
crate and native owner examples used by this fixture.

These are small debug measurements on one host, not release latency, peak memory,
CPU/I/O overhead, or useful-throughput evidence for hundreds of active jobs.
The documented aggregate-capacity and native-platform gates still apply.

The later 2026-10-20
[`native-owner-recovery.macos-arm64.json`](evidence/native-owner-recovery.macos-arm64.json)
records two successful runs of the expanded crash fixture, its 45-second bound,
and exact source/binary hashes. The executor suite passed 49 active tests and
strict all-target Clippy. The new preparation-disconnect regression failed
before the client fix and passed afterward; the existing lost-launch fixture
still confirms one effect. These are correctness runs overlapping development
work, so their timing fields are not a matched cost comparison. An earlier
unlabelled connection reset was not conclusively attributed to that defect.

The separate [package-host measurement](packages.md#initial-package-host-measurement)
found 9.01% to 12.60% of one core in the idle owner while a resident interpreter
kept the 100 ms full-host process sampler active. This misses the proposed
less-than-1% low-load target. The initial report is retained; passing cleanup
checks does not establish acceptable idle overhead.

The later [typed native sampler report](evidence/package-host-overhead.macos-arm64.native-sampler.json)
retains six new release samples. Idle-owner CPU fell to 1.626% to 1.933% of one
core, still above that target. The sample still enumerates all host PIDs every
100 ms; the adapter reduces the work per PID. The release helper grew by 176
bytes and the package fixture by 384 bytes relative to the initial report;
these are combined binary deltas, without an isolated dependency-size claim.
One core source hash is unavailable because report post-processing lost its
pre-run snapshot and that source later changed. The report records the exact
binary hashes and remaining post-run source hashes, with that provenance limit.
Both failed-target reports remain available. Automatic defaults stay gated.

```sh
CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 cargo build -p ejm-harness-exec --example native_owner_smoke --offline --locked
target/debug/examples/native_owner_smoke
```
