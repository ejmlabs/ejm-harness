---
name: harness-maintenance
description: Change ejm-harness architecture, quality gates, dependency policy, or project skills while keeping decisions and executable checks aligned.
---

# ejm-harness maintenance

Read [the contributor contract](../../../docs/contributing.md) and
[the checked crate policy](../../../scripts/architecture-policy.toml).
Use the owning code as evidence of current behavior.

## Make a durable change

1. Identify the public contract or hidden module decision that changes. State
   the intended benefit and the evidence needed to judge it.
2. Keep existing ports unless they cannot express the behavior. Use a small
   trait for a real boundary or alternative, and explicit constructor arguments
   for the dependencies used. Avoid a container or factory hierarchy by default.
3. Change the owning code and a meaningful test or executable gate. Preserve
   cancellation, bounds, failure evidence, and recovery ownership.
4. Update the relevant documentation and skill when their guidance changes.
   Record the decision, alternatives, and test or measurement under the contributor
   decision record. Remove superseded advice in the same patch.
5. Run focused checks, `python3 scripts/check-policy.py --self-test`, and the
   required `./scripts/check`. Report any unavailable platform or service honestly.

## Preserve useful constraints

- Inherit the workspace's denied lint groups. Fix findings in code. A necessary
  exception stays on the smallest item with a reason tied to a stable contract.
- Do not weaken a check, ignore a dependency tree, or allow a lint group to make
  a patch green. Keep native unsafe exceptions inside the reviewed file list.
- A new crate needs an explicit dependency policy and rationale. Include a
  negative checker fixture if a new rule changes what the gate can reject.
- Change dependencies deliberately, update locks, and run Machete and Deny.
  Keep exact version exceptions narrow and documented.
- Use trusted, maintained packages before writing standard plumbing. Check
  maintenance, correctness, license, compatibility, and resource cost. Custom
  code should implement ejm-harness's distinct behavior. Record a concrete
  reason and rejected alternatives for any exception; fewer dependencies alone
  does not justify owning a protocol parser or security primitive.
- Keep skill descriptions specific. Link maintained contracts instead of copying
  them. Add a rule for a demonstrated failure or a deliberate architecture choice.
- Do not turn an ordinary fix or one user's preference into a new global rule.
  No paid calls, publication, or deployment is implied by a quality change.

## Credential compatibility

Treat the [credential vault](../../../docs/credential-vault.md) format, stable legacy lock, migration barriers and permanent markers as compatibility contracts. Never reset an unavailable key, remove a barrier to permit downgrade, or reimport stale credentials after logout. Keep native key-service, filesystem, lifecycle and resource evidence separate from injected-key fixtures. Publication of the API does not certify another operating system or release a host.
