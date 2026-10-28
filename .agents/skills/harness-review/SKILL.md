---
name: harness-review
description: Review an ejm-harness patch for correctness, resource bounds, cancellation, recovery, crate boundaries, host compatibility, and honest claims.
---

# ejm-harness review

Read `AGENTS.md` and [the contributor contract](../../../docs/contributing.md). Establish the exact
diff or revision first, then read the callers and tests of every changed public item before
judging the patch.

Trace one complete action through the changed code: a host call, admission, the model or tool
work, events and journaling, cancellation, shutdown, and recovery after a crash. Look for a
concrete way each step can leave state inconsistent.

## Check

- **Boundaries.** The change respects `scripts/architecture-policy.toml`. The engine has no
  frontend, coding, or worker dependency. Hosts own UI, browser launch, and listeners.
- **Admission.** Work goes through the shared governor. A queue, buffer, retry, step count,
  artifact, or process lifetime has a bound, and the bound holds with a hundred sessions.
- **Cancellation.** Every wait and spawned task ends when its caller drops or cancels. A lease
  is released only after the resource it owns is cleaned up.
- **Recovery.** An effect boundary is persisted before execution. An uncertain effect after a
  crash is never replayed.
- **Trust.** Repository content and model output cannot add tools, grants, or registry entries.
  Credentials never appear in arguments, logs, events, or debug output.
- **Claims.** Docs and code state native restrictions as enforced, monitored, or unsupported.
  A performance or capability claim cites a measurement and its limits. A source release is
  not described as published.
- **Hosts.** A public change keeps ejm-cli and ejm-ide building. Use
  [harness-hosts](../harness-hosts/SKILL.md) for any removed, renamed, or retyped public item.
- **Dependencies.** A new package passes the maintained-package review. A new deny skip names
  the exact crate and version and the upstream constraint.
- **Lint and style.** No relaxed lint group or broad exception. No em dashes. Tests cover the
  changed boundary, not only a unit.
- **Consistency.** New code follows [the code layout](../../../docs/contributing.md#code-layout)
  and reuses its shared helpers. A new pattern is added there in the same patch.

Run the focused proof first, then `./scripts/check`. Report each finding with severity, file and
line, a reproducible condition, and the user-visible effect. Keep speculative style advice
separate from confirmed defects. Do not claim a gate you did not run.
