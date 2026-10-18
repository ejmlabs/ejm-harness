---
name: harness-performance
description: Measure or improve ejm-harness latency, resident memory, output, process count, or concurrency without weakening its execution guarantees.
---

# ejm-harness performance

Use deterministic replay or local fixtures. Choose a workload that exercises the
behavior being claimed, and record the compiler profile, OS, hardware, limits,
workload, and command with each result.

- Separate cold startup, time to first event, turn latency, cancellation latency,
  idle memory, active memory, and tool execution. Remote inference time is separate.
- Distinguish managed sessions from concurrent active tools. A hundred idle
  sessions do not prove a hundred builds or model streams can run at once.
- Compare the same workload and limits before and after the change. Include tail
  latency or maximums when queues, stalls, or cancellation are affected.
- Keep queues, output, history, and callback state bounded under a slow consumer.
  Do not gain speed by dropping required persistence or effect boundaries.
- Prefer less work, fewer allocations, bounded concurrency, and shared resources
  before adding threads, caches, or native bindings.
- Keep native sampling and all agents on one governor. A lighter admission
  estimate must follow measurement rather than merely raising throughput numbers.
- A local Mac result does not establish low-end hardware or other OS capacity.
  Report measurements as measurements and targets as targets.

Use the repository benchmark when it covers the claim. Add a small workload to
that benchmark when needed instead of creating another performance framework.
Confirm the relevant functional tests, then run `./scripts/check`.

For a lasting resource tradeoff, follow
[harness-maintenance](../harness-maintenance/SKILL.md) and record the evidence.
