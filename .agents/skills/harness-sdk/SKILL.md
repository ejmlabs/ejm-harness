---
name: harness-sdk
description: Change ejm-harness JSONL transport, generated wire schema, TypeScript SDK, Python SDK, tool callbacks, or cross-language cancellation.
---

# ejm-harness worker and SDKs

Read [the worker contract](../../../docs/implementation-contract.md#worker-protocol-v1)
and the relevant SDK source. The worker adapts the public Rust harness.

- Keep one persistent worker per client, shared across sessions. Do not start a
  process for each run, tool call, or event.
- Keep stdout protocol-only and diagnostics on stderr. Bound frames, pending
  calls, event queues, and callback lifetimes on both sides of the boundary.
- Generate wire types from Rust's schema with `scripts/generate-protocol.py`.
  The maintained `json-schema-to-typescript` and `datamodel-code-generator`
  packages own type rendering. Never patch generated types or write a parallel
  schema renderer. Preserve public names, tagged unions, and optional fields.
- Keep facade APIs idiomatic: async iteration and typed events in TypeScript;
  async iteration, async context cleanup, and typed callbacks in Python.
- A late subscriber must observe an already-finished run through buffering or
  durable replay. A slow subscriber must not block worker command handling.
- Cancellation, disconnect, and timeout resolve pending callbacks explicitly.
  Never retry an uncertain tool effect automatically.
- Test worker exit while the host keeps stdin open. Await harness cleanup before
  runtime shutdown, and do not wait for Tokio's uncancellable stdin read to end.
- Validate the protocol handshake and worker compatibility before sessions.
  A missing or incompatible executable needs an actionable error.
- Keep credentials out of diagnostics, command arguments, event payloads, and
  checked-in fixtures. Configuration grants tools; model output does not.

Run schema `--check`, SDK type checks, and local callback/stream/disconnect tests.
Use `./scripts/check` for the required Rust, TypeScript, and Python gate.
Apply [harness-maintenance](../harness-maintenance/SKILL.md) to protocol decisions.
