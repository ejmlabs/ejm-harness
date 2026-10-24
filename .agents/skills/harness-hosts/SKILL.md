---
name: harness-hosts
description: Change or rename a public ejm-harness API, crate, or path that sibling hosts such as ejm-cli and ejm-ide depend on, and coordinate the change with them.
---

# ejm-harness and its hosts

ejm-cli and ejm-ide are separate repositories, usually beside this one, and depend on these
crates by path (`../ejm-harness/crates/ejm-harness-*`). Their sessions may be running now.
Read [the host integration guide](../../../docs/ide-integration.md) and
[the implementation contract](../../../docs/implementation-contract.md).

## What hosts use

- ejm-cli: `ejm-harness`, `-coding`, `-exec`, `-auth`, `-packages`, `-package-spec`.
- ejm-ide: `ejm-harness` and `ejm-harness-auth`, including `Harness`, `Tool`,
  `ResponsesProvider`, `ProviderContinuation`, `HttpObserver`, `REASONING_EFFORTS`,
  `CredentialSource`, `StaticCredential`, and `SecretString`.
- ejm-cli pins crate names and the `../ejm-harness/` prefix in its architecture check.

## Rules

- Prefer additive changes. Keep defaults when adding an option. Removing, renaming, or
  retyping a public item, a crate, a directory, an environment variable, or a wire field is a
  breaking change for hosts.
- Do not edit a sibling repository from here, and do not run its formatters or lock updates.
  Another session or the user owns it. Send the owner the exact old to new mapping, say
  whether the folder moves, and wait for confirmation before the rename lands.
- Before reporting a public change done, build each host against this tree with
  `cargo check --locked --tests` in `../ejm-cli` and `../ejm-ide`, and report failures that
  are yours versus theirs. A stale host lock is the host's to refresh.
- Hosts own terminal or window UI, browser launch, the OAuth callback listener, and account
  controls. The engine keeps grants, budgets, validation, and journaling.
- Never share secrets across the boundary. Tokens use the auth crate's protected storage.
- Hosts can clone a `RunHandle` or subscribe to an exact current session/run;
  `None` means absence, not success. Keep retained handles bounded. Revocable
  dependent work waits outside host authority through `wait_completion`,
  discards work if `queued_successor` is true, then rechecks target, revocation
  and expiry. A false flag or completed summary is not a later admission permit.
- Keep user-facing names consistent: the program is `ejm-harness`, the CLI is `ejm-cli`, and
  packages use `ejm.package.json`. Registry clients require an explicitly configured
  origin; preserve each origin's separate credential namespace.

When a rename or a breaking change is approved, record it with the contributor decision
record through [harness-maintenance](../harness-maintenance/SKILL.md).
