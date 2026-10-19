---
name: harness-execution
description: Change ejm-harness resource admission, native command supervision, output accounting, cancellation, or executor capability guarantees.
---

# ejm-harness execution

Read [the executor contract](../../../docs/executor.md) and the affected
`ejm-harness-exec` module before changing admission or process ownership.

- Every session and tool in one harness shares an `Arc<Governor>`.
- Admission uses estimates and RAII leases. Preserve aggregate accounting on
  success, rejection, cancellation, dropped futures, and spawn failure.
- Preserve session round robin and FIFO order. A smaller request cannot bypass
  the oldest session head indefinitely.
- The supervisor owns the process and lease through cleanup. Dropping a caller
  cancels its supervisor. Runtime shutdown first cancels and then waits for idle.
- Drain stdout and stderr together, retain bounded tails, and charge artifacts
  across every executor using the governor. Quota exhaustion still drains pipes.
- Keep native sampling lazy and shared. Do not create a monitor process or
  permanent shell per agent, inspect environments, or mutate host resource limits.
- A required restriction must be enforced before launch. Sampling and process
  groups are monitored behavior, not a security sandbox.
- Use safe Rustix APIs for native identity, page-size, and process signals.
  Project code has no unsafe exceptions. Document a package gap before proposing
  one, and keep capability claims specific to the OS evidence available.
- Use `procfs-core` for Linux stat records. Preserve the bounded read and exact
  raw start ticks used by our cleanup checks; wall-clock timestamps lose identity
  precision.

Use the subprocess fixture tests for noisy output, child cleanup, abandoned
callers, queue deadlines, and shared quotas. Linux parser fixtures run on all
hosts, but they do not prove Linux execution behavior. Report that distinction.
Run `cargo test -p ejm-harness-exec` and strict all-target Clippy before the full gate.

Update [harness-maintenance](../harness-maintenance/SKILL.md) when the guarantee changes.

## Private input and native ownership

Use `Executor::run_with_input` for bounded private stdin. Keep it outside serialized command requests and artifacts, write it while both output pipes drain, and retain supervisor ownership through cancellation and cleanup.

Use `process::spawn_owned` for shared native child ownership. Root `wait()` does not finish descendant cleanup. Retain children, pipes and guards through `kill().await`, including confirmed empty Windows Job membership. See the [native ownership contract](../../../docs/native-process-support.md) for maintained packages, creation flags and missing native evidence.
