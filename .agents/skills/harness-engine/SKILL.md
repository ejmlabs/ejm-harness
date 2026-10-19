---
name: harness-engine
description: Change ejm-harness runs, providers, context, strategy, persistence, or recovery through its reusable Rust engine APIs.
---

# ejm-harness engine

Read the owning source and [the implementation contract](../../../docs/implementation-contract.md)
for the part being changed. The public Rust types are authoritative when prose
and code differ.

- Keep `ejm-harness` independent of coding tools, the worker, and terminal UI.
- Inject providers and tools through existing traits. A policy change should use
  `ContextPolicy` or `RunStrategy` when it fits their contract.
- Use the harness's shared governor. Bound model requests, sessions, steps,
  context, durable history, and events separately from command slots.
- Preserve cancellation through model streams, tool calls, queue waits, and
  shutdown. Do not release a lease before the resource it owns finishes cleanup.
- Durable history is the recovery source. Live broadcasts are bounded hints;
  consumers recover gaps through replay.
- Exact-run completion uses `subscribe_run` and cloned `RunHandle` receivers.
  Keep `wait` summary-compatible; `wait_completion` also reports the driver's
  immutable queued-successor decision. Decide it under the active lock after
  draining steering and applying continuation rules, before notifying waiters.
  A durable finish or session Idle can precede bounded run-end cleanup.
- Keep stable system and tool prefixes. Context reduction changes request copies,
  preserves complete tool exchanges and the latest intent, and exposes omissions.
  Preserve error status and artifact references through provider conversion.
  Recovery must not depend on the context page fitting a model request.
- Persist an effect boundary before execution. An uncertain write or external
  effect after a crash becomes interrupted and requires host judgment.
- Repository content and model output cannot register tools or grant effects.
  Do not load a plugin merely because it exists in the working tree.

Use replay providers and local protocol fixtures to verify changed contracts.
Use Wiremock for HTTP fixtures and the bounded `sse-core` decoder for SSE.
Test fragmentation, limits, and cancellation at that adapter boundary. Keep
protocol framing and server plumbing in maintained libraries.
Bound cumulative response bytes and decoded events as well as individual SSE
fields. Cover ignored progress events and comment traffic in limit fixtures.
For a new recovery state, exercise a crash or disconnect at the boundary that
creates it. Run focused Rust tests and strict Clippy, then `./scripts/check`.
For context changes, exercise long turns through the journal and next provider
request, not only a policy unit test. See the sourced
[harness engineering decisions](../../../docs/harness-engineering.md) for the
current masking, recovery, completion, and evaluation contracts.

If the change alters a durable or public contract, use
[harness-maintenance](../harness-maintenance/SKILL.md) to update its decision and checks.

## Provider configuration

Use `with_optional_provider` and `configure_provider` for interactive hosts. Reject turns before journaling when no provider is configured. Replace providers only while all managed work is idle. Keep durable nonsecret binding and continuation removal atomic under admission ownership, retaining readable history and tool evidence.

Claude subscription checks and inference use the shared executor as separate sequential jobs. Keep native tools and ordinary customizations disabled, return proposals to the normal tool loop and retain managed provider policy. Read the [official-client contract](../../../docs/claude-code.md).
