# Native platform support

The target matrix is macOS arm64 and x64, Windows x64 MSVC, and Linux x64 GNU.
Source portability and a green gate on one host do not prove another target's
native behavior. Prebuilt workers and host applications are not published by
this change.

## Private state and workspace files

Windows journal and package state use
[`ejm-rightkit-fs` 0.4.4-ejm.1](https://github.com/ejmlabs/ejm-rightkit-fs), licensed under
MIT OR Apache-2.0, with default
features disabled and its `fs` feature. `StateDir` refuses reparse traversal and
retains a directory handle without delete sharing. Stable read-only lock handles
prevent lock replacement while a store is open. Journal database handles permit
SQLite reads and writes but deny deletion and refuse reparse points or multiple
hard links. The writer retains this ownership through connection shutdown.

Existing Windows state directories must already pass the package's owner-only
ACL check. The adapter then sets and verifies an inheritable owner-only DACL so
new SQLite WAL/SHM and package staging files start private. A wider ACL is an
error. Use a private application-data directory or deliberately repair that
directory's access before opening it. Parent-directory creation uses the same
maintained native API. There is no shell ACL command or plaintext fallback.
Windows administrators, SYSTEM, and code running as the same user remain within
the native trust boundary. SQLite still owns its VFS, journals, and sync policy.

The workspace selects an exact-commit [maintained fork](../vendor/README.md)
of upstream 0.4.4. Native permanent locks need direct exclusive final-name
creation and opt-in same-handle owner-only reopening, which the reviewed
upstream releases do not expose. The repair preserves the upstream dependency
manifest and licenses. Ordinary state writes keep their existing durability
contract. Source and native builds use the public fork commit. Cargo registry publication
waits for a verified registry version; the Git pin does not satisfy that gate.

Package lockfiles publish through `StateDir`'s relative atomic write. On Windows
the package flushes files and requests write-through publication; it does not
flush directory handles and has no power-loss proof. No stronger Windows
durability claim is made. Unix journal modes and package ownership checks stay
in place. Bounded package reads check the opened file and refuse a final link.

On macOS, the same maintained file package also rejects extended allow ACLs
for journal and package state. Creation refuses inheritable parent allow ACLs;
existing protected entries must pass the owner-only check before use. Mode 0600
or 0700 alone cannot establish macOS privacy. Local ACL fixtures cover creation
and existing-file refusal. This does not isolate other code running as the owner.

Coding tools keep `cap-std` as their workspace traversal boundary. Windows input
components use the maintained device-name, alternate-stream, and trailing-dot
validator. Opened native handle paths are checked against the workspace and
protected `.git` and `.ejm-harness` components, covering normalized aliases.
Writes refuse reparse components. Optimistic hashes do not make external editor
races transactional. Ordinary Unicode and spaces are valid workspace names;
package archive entry names keep their separate portable ASCII contract.

These filesystem adapters run in the caller process. Windows file-handle counts,
startup cost, and filesystem latency have not been measured. Native ACL,
junction, short-name, replacement-race, crash/restart, and interrupted-save
acceptance remains required. The Windows symlink fixture requires Developer
Mode or the native symlink privilege; unavailable fixture prerequisites are an
unverified gate.

## Ownership contract

`ejm_harness_exec::process::spawn_owned` consumes a Tokio command and returns an
`OwnedChild`. The caller configures its cwd, argv, environment, and stdio.
The adapter supplies native process ownership:

| Platform        | Mechanism                              | Cleanup                                                                                            |
| --------------- | -------------------------------------- | -------------------------------------------------------------------------------------------------- |
| macOS and Linux | A fresh Unix process group before exec | Signal the group and reap the root. The executor also kills identity-checked sampled descendants.  |
| Windows         | A kill-on-close Job Object             | Spawn suspended, assign to the Job, then resume. Confirm empty Job membership, then reap the root. |

`wait()` and `try_wait()` report the direct child's status. Always finish with
`kill().await`, including after a successful root exit, to stop remaining
descendants. A cached exit status remains available after that cleanup.
`start_kill()` and `Drop` only initiate termination. Keep the owned child, pipe
tasks, and workspace or admission leases alive until cleanup finishes. Use the
host's existing Tokio runtime rather than creating a runtime or thread per
child.

Darwin can briefly report permission denied while a terminated Unix group is
disappearing. After reaping the root, cleanup retries that specific error up to
five times at 10 ms intervals. A persistent failure is returned. A completed
cleanup retires the stored group identity so a later drop cannot signal a reused
group number.

A failed Windows membership query does not mean the Job is empty. The adapter
retains ownership, repeats termination, and retries after 25 ms. A shutdown
deadline may report cleanup pending, but the host must keep its runtime alive.
Termination and wait errors are returned after confirmed cleanup. There is no
automatic command retry.

The executor and package language hosts share this adapter. Authentication has
the same small Windows Job adapter within its own crate, because auth does not
depend on command execution. Interactive Claude Code sign-in inherits terminal
stdio. On Unix it also retains the terminal's foreground process group so
canonical input still works; cancellation reaps that direct client. Captured
status and sign-out commands use their own Unix group. On Windows all three
operations use a Job.

The low-level Windows adapter owns creation flags for suspended assignment.
Arbitrary creation flags already set on a raw Tokio command are overwritten.
Callers must not rely on them. A rejected Job assignment fails launch and
terminates the suspended child before it can run user code. An outer Job with
incompatible policy can cause this failure; the adapter never retries without
containment or enables breakaway.

Process lifecycle control is not a filesystem or network sandbox. Unix groups
and sampling can miss a child that changes groups and is reparented between
samples. The stable executor capability still reports process-tree cancellation
as `monitored`. Required hard containment continues to fail before launch.

## Shared execution owner

Coordinated command and resident package-host execution uses the public
`native::NativeOwner`, `NativeCommand`, and `NativeChild` contracts. The owner
uses the same low-level process adapter. Call `native::dispatch_helper()` before
ordinary host startup, then pass the prepared capacity context's owner to the
executor and package runtime. A minimal helper is in
[`native_owner_helper.rs`](../crates/ejm-harness-exec/examples/native_owner_helper.rs).
The helper reexecutes an existing host binary, creates one small shared runtime,
and exits two seconds after all connections and native scopes drain.

One permanent OS lock establishes ownership within the protected user/resource
domain. The owner publishes a random epoch and capability with its ephemeral
IPv4 loopback endpoint. Requests validate protocol, epoch, capability, and fresh
visible execution constraints before effects. A paused owner retains its lock;
contact failure or elapsed time never permits a second owner. Std locks,
`rightkit-fs`, Tokio networking, and `tokio-util` framing provide these mechanics.
No unsafe project code or new separately installed daemon is added.

Permanent native lock files use the pinned directory's direct empty-file
creation. Existing names pass regular-file, single-link and private-access
checks on the same open handle. Failed creation checks retain the visible name;
unsafe entries are never removed or replaced. Windows creator handles omit
DELETE access so another client can open the same file. No sync or power-loss
guarantee is added. The [lock repair evidence](host-capacity.md#stable-lock-creation-repair)
covers bounded creator death, concurrent access and actual privacy failure on
this Mac. It does not establish native Windows or Linux acceptance.

Linux placement compares the exact Rustix affinity CPU set, namespace device and
inode identities from a fixed bounded list, and visible cgroup ancestry. Missing
critical identity, incomplete cgroup placement, or different thread/process
cgroup membership rejects the launch. Placement must remain stable across host
sampling. These checks do not set affinity or move a process into another domain.
The owner checks the launch's current placement against its prepared request and
fresh owner observations before spawning. Original-epoch recovery queries remain
available after a caller moves because they have no native effects.

The inspected `processkit` 3.3.4 public `ProcessGroup` API creates and controls
Jobs that it owns. It does not expose the current process's enclosing Job or
affinity identity. Its internal Job adapter cannot supply that public contract.
Shared Windows `NativeOwner::spawn` therefore rejects as unsupported before
helper startup. This leaves ordinary local Windows Job execution available.
`NativeSnapshot::domain_placement` reports that unsupported state. A maintained
safe placement adapter and native evidence are needed before enabling this path.

Every active native scope has one private lifetime connection. EOF initiates
supported cleanup in the surviving owner. Scope state, pipes, output queues, and
completed operation metadata are bounded in memory. The
[executor contract](executor.md#shared-native-owner) gives the concrete limits,
environment rules, cleanup states, and recovery behavior. Waiting logical agents
do not create a scope or helper process. Caller-local admission and opaque Rust
mutation guards remain in their supervisor until cleanup or confirmed owner
death; they cannot survive total caller-process death as Rust values.

| Platform | Supported cleanup in a surviving owner                                                                                                     | Consequence of owner death                                                                                             |
| -------- | ------------------------------------------------------------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------- |
| macOS    | Process group plus identity-checked sampled descendants                                                                                    | Escaped or unobserved descendants can survive. Effects and workspace safety remain uncertain.                          |
| Linux    | The same Unix group and sampled descendant path                                                                                            | This change adds no delegated cgroup or parent-death link. Survivors remain possible; native owner fixtures are unrun. |
| Windows  | Shared ownership rejects unsupported placement before launch. Local execution retains suspended Job assignment and empty-membership query. | Shared owner-crash and enclosing Job placement evidence remain unavailable.                                            |

After an IPC failure, the caller retains charged ownership while the original
owner still holds its lifetime lock and no `Finished` reply is available. A
released lock, or valid protected publication of a replacement epoch under that
same lock protocol, establishes owner death. This retires scheduling authority;
it does not establish that arbitrary native descendants ended. Executor and
package supervisors park new admission before releasing local scope charges.
Core recovery takes fresh measurements and installs a conservative grant through
an atomic admission revision. Native survivors contribute to observed external
load and may grow between samples.

`NativeSnapshot` is cached. Reading status creates no helper, probe, filesystem
read, or connection. `owner_pid` is the last authenticated PID and is not a
liveness claim. Owner deaths are counted only when live recovery observes proof.
Uncertain operations retain their original epoch and monotonic ID. Query and
replacement never replay them, and expired IDs remain expired. There is no
durable native per-job ledger. Authentication terminal interaction and SDK worker
bootstrap keep their separate, explicit host scope contracts.

## Package choice

The standard Tokio child API owns one process. It does not provide suspended
Windows Job assignment or whole-Job membership queries. Native project code
must remain safe Rust, so the Windows adapter uses the maintained MIT-licensed
[`processkit` 3.3.4](https://docs.rs/crate/processkit/3.3.4), pinned with default
features disabled and only `process-control` enabled. Its minimum Rust version
is 1.88, below this workspace's pinned compiler. Its `windows-sys` 0.61 line
matches the existing dependency family.

The inspected
[Windows spawn implementation](https://docs.rs/crate/processkit/3.3.4/source/src/sys/windows.rs)
starts suspended, guards failure before assignment, assigns to the Job, and
resumes only after containment. The public `ProcessGroup::members()` reports
actual Windows Job membership. The adapter retains that handle through cleanup.

Alternatives were checked against those requirements:

| Package                 | Unmet requirement                                                                                                                                                      |
| ----------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `process-wrap` 10.0.1   | Its Job wait accepts a completion-port message without checking `JOB_OBJECT_MSG_ACTIVE_PROCESS_ZERO`, and its wrappers hide the Job handle needed to check membership. |
| `command-group` 5.0.1   | The older implementation has the same completion-message gap.                                                                                                          |
| `process_control` 5.2.0 | Attaching an already running child leaves a spawn-to-assignment window for escaping descendants.                                                                       |
| `win32job` 2.0.3        | Safe Job queries alone do not provide suspended spawn and resume. Combining packages would need more ownership glue and another Windows binding family.                |

No vendored package or custom unsafe Windows code is added. `processkit` is
Windows-only, so its Unix dependency cost stays unchanged. Shared native
supervision adds one lazy helper process and runtime per compatible domain,
without a per-child monitoring thread. Windows owns
one Job per supervised root; membership polling runs only during cleanup.
Release binary size, idle cost, and spawn/cleanup latency have not been measured
on Windows. This choice makes no speed or footprint improvement claim. Native
measurements and lifecycle evidence remain release requirements.

## Native verification

The portable Rust fixtures exercise noisy dual pipes, private stdin, spawn
failure, cancellation, deadline expiry, dropped callers with retained host
guards, root exit with live descendants, and sampled process-budget abuse.
They launch the compiled native test executable rather than a Unix shell.
The required gate is `python scripts/check.py`, with thin Unix and PowerShell
wrappers. The SDK default worker name has `.exe` on Windows; explicit paths and
environment overrides remain authoritative.

Run the gate on each matrix target. Windows evidence must include ordinary and
nested-Job launch, assignment failure cleanup, root exit with live descendants,
and host guard retention until Job membership is empty. Linux needs its real
procfs path; macOS fixtures do not establish that behavior. Native shell,
terminal restoration, desktop sidecars, packaging, signing, and installer
acceptance belong to each host's gate. Check the OpenSpec tasks for current
evidence and unfinished native checks.

The [local implementation evidence](../openspec/changes/cross-platform-support/implementation-evidence.md)
records the macOS 26.5.2 arm64 focused suites, local worker archive, and bounded
offline worker profile. Artifact hashes identify the measured binary from a
dirty source checkout; they do not establish all later source edits. The final
required source gate and native Windows, Linux, and Intel macOS acceptance are
still required. No signing or publication follows from that local record.

## Shared CI and dependency pin evidence

The consumers currently pin the shared workflow repository to
`af4ee77e9ee8da606c4ae3a0838bc3b46fd9d7f3`. Inspection of that revision found
Harness quality jobs on Ubuntu 24.04, macOS 15, and Windows 2025; CLI on Ubuntu
24.04 and macOS 15; and IDE on macOS 15. There is no four-target acceptance
evidence at that pin.

The shared owner still needs a reviewed immutable revision covering these native
runner/target pairs for all three Rust products:

| Runner           | Rust target                |
| ---------------- | -------------------------- |
| `macos-15`       | `aarch64-apple-darwin`     |
| `macos-15-intel` | `x86_64-apple-darwin`      |
| `windows-2025`   | `x86_64-pc-windows-msvc`   |
| `ubuntu-24.04`   | `x86_64-unknown-linux-gnu` |

That change must preserve the `quality` job identity, exact-commit human
admission, maximum parallelism of two, source artifact flow, aggregation, and
empty job permissions. Setup needs a pinned Python 3.13 action before safe
archive extraction, an actual Rust host-architecture check, and Linux IDE
WebKit/GTK prerequisites. Generator fixtures must reject a missing target.
The registry retains its separate Linux gate.

Host dependency export takes `.github/harness-revision` from the trusted base
branch, not the proposed source. A reviewed harness main revision containing
the new public vault/process/resolver APIs must therefore be pinned by both
hosts before consumer CI can prove integration. Do not substitute an untrusted
pull-request harness ref. This local change neither edits shared workflows nor
produces their reviewed SHA, native jobs, package artifacts, or signing proof.

## Local endpoint authentication

Native protocol 3 authenticates the server before Hello or any launch data.
The RustCrypto HMAC-SHA256 proof covers a fresh client nonce, protocol, epoch,
domain and independently observed TCP endpoint ports. A forwarding listener
cannot reuse a proof for another socket. Existing frame, time and connection
limits apply. A free owner lock makes a discovered endpoint stale without
contacting it. This local authentication is not encrypted remote transport.

Relative working directories are resolved in the requesting process before
serialization. The shared helper does not reinterpret them against its own cwd.

The focused macOS authentication and lifecycle run passed 17 tests, with the
separate startup and saturation fixtures left unrun. A two-process relative-cwd
fixture shared the original helper and preserved cwd, environment and streams.
Strict executor Clippy passed. Native Windows verification remains pending CI.
The earlier 1,000-agent measurement predates this authentication exchange; it
must not be described as measuring protocol 3 overhead.

[Filesystem review source](../review/dependency-sources/ejm-rightkit-fs/README.md)
supplies the exact pinned lock/privacy implementation for offline review.
