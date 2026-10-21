---
name: harness-registry
description: Change the Cloudflare package registry, publisher accounts, device login, immutable uploads, moderation, dependency gates, or registry security tests.
---

# ejm-harness registry

Read [the registry contract](../../../registry/README.md) and
[the package contract](../../../docs/packages.md). Use Cloudflare Workers,
D1, R2, and private service bindings. Use GCP only for a demonstrated missing
capability after the host decision is reviewed.

- Better Auth owns login, sessions, bearer handling, device authorization,
  cookies, CSRF, expiry, and revocation. Do not write a second auth protocol.
- The auth Worker has no public route or preview URL. Derive identity and
  operator status through its private binding; never trust public identity headers.
- Check live account state and namespace ownership on every mutation. Keep
  names and versions immutable. Suspension never releases a namespace.
- Validate size before parsing. Match the uploaded archive's digest, size,
  identity, and full manifest to the reserved metadata before it becomes ready.
- R2 writes are create-once. A matching owner retry may repair an interrupted
  finalization; it may never replace another archive. Do not claim D1 and R2
  commit atomically.
- Bound anonymous traffic, per-account writes, pending versions, and storage.
  Keep pending, quarantined, and suspended packages out of search and downloads.
- Device approval stays disabled until server-verified client, scope, code,
  and pending state are visible. Explain that CLI tokens are revocable account
  sessions, not narrower permission-scoped tokens.
- Keep device decisions atomic and owner-matched while pending. Retain the
  real Better Auth approval-versus-denial race fixture when updating auth.
- Keep tokens, cookies, device codes, OAuth secrets, and archive contents out of
  logs. Set secrets through Cloudflare's secret store.
- A public package is unreviewed community code. Hash verification proves byte
  integrity, not that the package is safe. Preserve explicit local trust.

Verify unauthorized writes, namespace takeover, upload races, hostile archives,
quarantine, origin checks, and real Better Auth device/session behavior with
local Cloudflare bindings. Run strict checks and dependency audits. Deployment,
DNS, account provisioning, and publication require authorization.

## Browser presentation

Use public ejm-ui native recipes and the exact `.github/ui-source.json` revision.
`registry/scripts/build-web.mjs` checks the authored pages and assembles ignored
`.web-assets` from shared CSS, fonts, notices and project artwork. Extend ejm-ui
for missing visual or interaction contracts; do not add local styles or a second
theme system. Keep the existing frontend transfer budgets.

Use the fewest words that preserve meaning. Keep accessible labels, permanent
publisher-name consequences, server-verified consent details, full session
authority and recovery steps. Render untrusted publisher content as text.

The approved public target is `registry.ejmlabs.com`. Both Workers use that exact
origin; only the public Rust Worker has a route. The private auth binding,
placeholder database IDs and empty operator list remain deliberate launch
boundaries. Follow the README's resource, OAuth, migration and acceptance steps
after protected source acceptance. No static fixture stands in for the backend.

Current ejm-cli source has a fixed registry origin and no override. Do not show
bare CLI commands for the new address. Use the explicit `RegistryClient::new`
contract and retain the separate client/default migration gate before claiming
CLI availability.

## Registry account storage

Store origin-scoped account sessions through the shared vault APIs. Do not create or automatically load plaintext sessions. Legacy import requires older clients stopped, source privacy and origin validation, verified encrypted persistence and a permanent destination marker. Keep markers after logout and replacement so leftover legacy sources cannot restore revoked sessions. See [registry credentials](../../../docs/packages.md#registry-credentials).
