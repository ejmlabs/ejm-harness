# Tasks

Everett confirmed `https://registry.ejmlabs.com` on 2026-10-20. The `AGENTS.md` names table records that origin. Only `gerry.package.json` stays unchanged until the user decides. Do not deploy, and do not create, delete, or move a tag or GitHub release. The CLI command for a leftover retired session is `ejm-cli package discard-retired-session`.

## 1. ejm-harness client origin

- [x] 1.1 Record Everett's confirmed origin in `specs/package-registry-origin/spec.md` and in the `0.1.0` changelog bullet. If he chooses `https://harness.ejmlabs.com`, record that the static site must give up that custom domain before the registry route is added, and leave `site/` unchanged until that separate authorization exists. Verify `openspec validate --strict` passes and the spec names only the confirmed origin.
- [x] 1.2 Set `DEFAULT_REGISTRY` to the confirmed origin and reject `meetgerry.ai` and its subdomains in `parse_origin`, with an error that names the confirmed default. Verify a unit test accepts the confirmed origin and `http://127.0.0.1:8787`, and rejects `https://harness.meetgerry.ai`, `https://other.meetgerry.ai`, a non-root path, credentials in the URL, and public `http`.
- [x] 1.3 Point the origin literals in `src/registry/credentials/tests.rs`, `src/source/tests.rs`, `registry/src/policy.rs:115`, `registry/tests/api.test.ts`, and `registry/tests/auth.test.ts` at the confirmed origin. Verify those files no longer contain `meetgerry`. Leave `gerry.package.json` fixtures and the `gerry-cli` client id unchanged.

## 2. ejm-harness retired sessions

- [x] 2.1 When `registry-session.json` belongs to `meetgerry.ai` or a subdomain, stop blocking other origins with the missing `ejm-cli package migrate-credentials` command. Return a message that the session is for the retired registry, that the token is not sent, and that the host can discard it locally. Verify a fixture with that legacy file performs no HTTP request and does not import the token into the new origin.
- [x] 2.2 Add a local discard for that retired legacy file and its vault record. Verify the file and vault record are removed with no network request, a later save for the confirmed origin succeeds, and a legacy file for any other origin is refused.
- [x] 2.3 Keep `migrate_registry_token` for a legacy file whose origin matches the requested origin. Remove `ejm-cli package migrate-credentials --old-clients-stopped` from `credentials.rs` and `docs/packages.md`. Verify the repository no longer contains that command string, and the same-origin handoff still names `migrate_registry_token`.

## 3. ejm-harness registry configuration and docs

- [x] 3.1 Set `REGISTRY_ORIGIN` in `registry/wrangler.jsonc`, `registry/auth/wrangler.jsonc`, and `registry/auth/scripts/generate-schema.ts` to the confirmed origin. Add that host as a custom domain on the public Worker only. Keep `workers_dev` and `preview_urls` false, and leave the auth Worker without a route. Verify the three literals equal `DEFAULT_REGISTRY` and the auth Wrangler file has no `routes` entry.
- [x] 3.2 Update the origin, GitHub callback, and custom-domain sentences in `registry/README.md`, and the origin sentence in `docs/packages.md`, to the confirmed origin. Verify the callback is `https://<confirmed-host>/api/auth/callback/github`. Leave Worker, D1, R2, and npm names, `registry/public` branding, `appName`, the consent plugin id, and the reserved handle `gerry` unchanged.
- [x] 3.3 Run the auth schema check. Verify `registry/auth/migrations/0001_auth.sql` changes only when the generator emits a difference, and that the committed SQL matches the generator.
- [x] 3.4 Update the origin sentence in `.agents/skills/harness-hosts/SKILL.md`. Keep the `gerry.package.json` clause. Leave `AGENTS.md`, `README.md` Gerry wording, `site/public/index.html`, and `docs/contributing.md` unchanged. Verify the skill names the confirmed origin and those four files have no new diff beyond what this change already required in `docs/packages.md` and `registry/README.md`.
- [x] 3.5 Add a dated decision-record row in `docs/contributing.md` for the confirmed registry origin, pointing at the same-origin download test and the retired-origin rejection test. Verify the new row does not rename `gerry.package.json` or edit the existing Gerry wording.

## 4. ejm-cli dependency

- [x] 4.1 Do not edit `ejm-cli` in this change. The sibling OpenSpec change `move-registry-to-ejmlabs-domain` owns `docs/ejm-cli.md` and any user-facing discard command. `packages.rs` keeps calling `RegistryClient::new(DEFAULT_REGISTRY)`. Verify this diff contains no `ejm-cli` files.

## 5. ejm-ide, ejm-ui, and ejmlabs

- [x] 5.1 Leave `ejm-ide`, `ejm-ui`, and `ejmlabs` unchanged, including the `gerry-marketing`, `gerry-coming-soon`, and `gerry-*` deploy warnings. Verify this diff contains none of those repositories.

## 6. Release notes and checks

- [x] 6.1 Keep this change's note in the `## [0.1.0]` section of `CHANGELOG.md`. Leave `## [Unreleased]` empty of this work, and do not add `0.2.0`. Do not create, delete, move, or republish a tag or GitHub release. Verify the `0.1.0` section names the confirmed origin and `git status` shows no tag or release workflow edits.
- [x] 6.2 Run the focused `ejm-harness-packages` tests, `./scripts/check-registry`, and `openspec validate --strict`. Verify the same-origin download rejection and the redirect refusal still pass. Do not deploy Cloudflare resources.

## Workflow follow-up

- After the tasks above pass and the change is reviewed, sync the delta spec and archive through the OpenSpec workflow.
- DNS, the custom domain, the GitHub OAuth app, real D1 ids, and registry deployment stay unauthorized until Everett asks.
- The `ejm-cli` change of the same name updates its docs after this harness revision is the one that checkout builds.
