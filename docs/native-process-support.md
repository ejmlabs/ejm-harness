# Native child ownership

This contract covers the child owner used by the executor and official Claude Code adapters in this source change. Native Windows and Linux acceptance is still required. No prebuilt worker, package, host application or native support certification is published by preparing these APIs.

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

The executor uses this adapter for private-input inference and ordinary commands. Authentication has
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
Windows-only, so Unix dependency cost stays unchanged. No new background
service, runtime, or per-child monitoring thread is introduced. Windows owns
one Job per supervised root; membership polling runs only during cleanup.
Release binary size, idle cost, and spawn/cleanup latency have not been measured
on Windows. This choice makes no speed or footprint improvement claim. Native
measurements and lifecycle evidence remain release requirements.

## Credential directory preparation

The Windows credential adapter resolves the nearest existing parent before
calling `rightkit-fs`'s `StateDir::create`. The package creates missing parents
with owner-only ACLs, refuses reparse traversal, and pins the directory without
delete sharing. This allows a fresh CLI state path to contain missing parents.
The portable nested-parent save-and-reopen regression passed in this
extraction's macOS harness gate using an injected key store. It does not execute
the Windows ACL or handle path; native Windows verification is still required.

## Verification requirements

The focused executor fixtures cover noisy dual pipes, private stdin, spawn failure, cancellation, deadline expiry, dropped callers with retained guards, root exit with live descendants and sampled process-budget abuse. They use the compiled test executable and local processes. Official-client fixtures use local programs and never sign in or call a paid model.

Run the required harness gate and the focused suites on the actual host. Windows acceptance must cover ordinary and nested-Job launch, assignment failure cleanup, root exit with descendants and retained host guards until Job membership is empty. Linux requires its real procfs path. macOS fixtures do not establish either target's behavior. Interactive terminal restoration belongs to each consuming host's gate.

The isolated extraction's required harness gate passed on macOS 26.5.2 arm64
with Rust 1.99.1: 198 Rust tests passed and 3 were ignored, along with the
required SDK, lint, dependency, documentation and build checks. The
[publication verification record](host-auth-verification.md) retains an earlier
sign-out fixture failure, its successful diagnostic repeats and the remaining
nonfatal toolchain warnings. This is local evidence, not native Windows/Linux
acceptance or platform certification. Binary size, application idle cost and
native spawn/cleanup latency remain unmeasured.
