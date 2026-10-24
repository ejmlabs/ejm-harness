# Tasks

Everett confirmed `https://registry.ejmlabs.com` on 2026-10-20. The `AGENTS.md` names table records that origin. Main's `ejm.package.json` manifest and explicitly configured registry-client contract remain in force. Do not deploy, and do not create, delete, or move a tag or GitHub release. The CLI command for a leftover retired session is `ejm-cli package discard-retired-session`.

## 1. ejm-harness client origin

- [x] 1.1 Record Everett's confirmed origin in `specs/package-registry-origin/spec.md` and in the `Unreleased` changelog bullet. If he chooses `https://harness.ejmlabs.com`, record that the static site must give up that custom domain before the registry route is added, and leave `site/` unchanged until that separate authorization exists. Verify `openspec validate --strict` passes and the spec names only the confirmed origin.
- [x] 1.2 Keep main's removal of the default registry. Expose `PUBLIC_REGISTRY_ORIGIN` for explicit selection and reject retired hosts through `validate_registry_origin`, retaining malformed-origin and exact loopback checks.
- [x] 1.3 Preserve main's reserved-origin fixtures and the optional registry client in source tests. Keep retired-origin fixtures and exercise both reserved and public origins in the registry security policy test.

## 2. ejm-harness retired sessions

- [x] 2.1 When `registry-session.json` belongs to `meetgerry.ai` or a subdomain, stop blocking other origins with the missing `ejm-cli package migrate-credentials` command. Return a message that the session is for the retired registry, that the token is not sent, and that the host can discard it locally. Verify a fixture with that legacy file performs no HTTP request and does not import the token into the new origin.
- [x] 2.2 Add a local discard for that retired legacy file and its vault record. An unsafe or unreadable `registry-session.json` is unlinked without following it. Verify the file and vault record are removed with no network request, a later save for the confirmed origin succeeds, and a legacy file for any other origin is refused.
- [x] 2.3 Keep `migrate_registry_token` for a legacy file whose origin matches the requested origin. Remove `ejm-cli package migrate-credentials --old-clients-stopped` from `credentials.rs` and `docs/packages.md`. Verify the repository no longer contains that command string, and the same-origin handoff still names `migrate_registry_token`.

## 3. ejm-harness registry configuration and docs

- [x] 3.1 Explicitly configure both Workers with `https://registry.ejmlabs.com`, with a public custom domain only on the public Worker. Keep schema-generation and automated fixtures on main's reserved example origin. Keep both preview settings disabled and the auth Worker private.
- [x] 3.2 Document the selected public origin, callback and custom domain while preserving main's package identity, current resource names, registry/auth branding and client identity. Keep deployment unverified.
- [x] 3.3 Run the auth schema check. Verify `registry/auth/migrations/0001_auth.sql` changes only when the generator emits a difference, and that the committed SQL matches the generator.
- [x] 3.4 Merge the hosts skill and names table: `ejm.package.json`, explicit registry origins, exact-origin credentials and the public registry host all remain documented.
- [x] 3.5 Preserve main's package-identity decision and add the explicit public-origin decision, linking same-origin and retired-origin regression evidence.

## 4. ejm-cli dependency

- [x] 4.1 Do not edit ejm-cli. Its separate change must adopt explicit `RegistryClient::new(origin)` configuration, optional registry clients and the retired-session APIs together.

## 5. ejm-ide, ejm-ui, and ejmlabs

- [x] 5.1 Leave `ejm-ide`, `ejm-ui`, and `ejmlabs` unchanged, including the `gerry-marketing`, `gerry-coming-soon`, and `gerry-*` deploy warnings. Verify this diff contains none of those repositories.

## 6. Release notes and checks

- [x] 6.1 Preserve main's changelog entries exactly and place this PR's origin and proxy notes under `Unreleased` once. Preserve the older-tag warning. Do not modify tags or release workflows.
- [x] 6.2 Run the focused `ejm-harness-packages` tests, `./scripts/check-registry`, and `openspec validate --strict`. Verify the same-origin download rejection and the redirect refusal still pass. Do not deploy Cloudflare resources.

## Workflow follow-up

- After the tasks above pass and the change is reviewed, sync the delta spec and archive through the OpenSpec workflow.
- DNS, the custom domain, the GitHub OAuth app, real D1 ids, and registry deployment stay unauthorized until Everett asks.
- The `ejm-cli` change of the same name updates its docs after this harness revision is the one that checkout builds.

## 7. Proxy route regression follow-up

- [x] 7.1 Preserve the harness-owned reqwest direct route for accepted HTTP loopback origins and strengthen the existing subprocess Wiremock fixture to call `me` with a synthetic bearer, verify direct authentication and zero proxy requests, and bound child cleanup without changing HTTPS behavior or public signatures.
- [x] 7.2 Run the focused registry route fixture with Cargo jobs at most two and the approved existing cache. Record platform, exact command, result, and all unrun full gates. Do not install tools or run a full rebuild on the constrained disk. See `proxy-verification.md` for the successful authenticated fixture and sandbox limit.
- [x] 7.3 Review public registry docs and record the two owners, public transport contract, dependency direction, CLI compatibility adapter, integration order, and unchanged human gates. Keep source checks distinct from publication.
- [ ] 7.4 Run the normal full gate for the final revision and complete human review before syncing specs and archiving this change. These checks remain deferred on the constrained disk; focused fixture success does not close them.

## 8. Main integration verification

- [x] 8.1 Run formatting, offline workspace Clippy and package tests, offline registry Clippy and Rust tests, and local Vitest where supported. Record failures and sandbox limits; keep full hosted, native, deployment and sibling-host gates separate.

Local merge verification: formatting, workspace and registry Clippy, two registry Rust tests, TypeScript, schema freshness, policy checks, Prettier and strict OpenSpec validation pass. Package unit/integration tests report 25 passed and 19 failed: nine mock-server socket denials and ten language-host deadline/cancellation failures. The runtime and adapter sources/tests match main; their timeout cause remains unresolved. Default Vitest lacks the generated Wasm shim, worker-build and the Wasm target; the independent Better Auth pool is blocked by localhost listen EPERM. Full gates and deployment acceptance remain open.
