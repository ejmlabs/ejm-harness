# Design

## Context

See proposal.md for why the default registry must leave `meetgerry.ai`. This design covers how the harness and the registry Workers change together, and what stays put.

`DEFAULT_REGISTRY` is `https://harness.meetgerry.ai` (`crates/ejm-harness-packages/src/registry.rs:21`). `ejm-cli` passes that constant to `RegistryClient::new` and has no registry flag or environment override. `RegistryClient` already accepts an explicit origin. It allows HTTPS, or HTTP only for `localhost`, `127.0.0.1`, and `[::1]`. It rejects credentials, a query, a fragment, and any non-root path. Its HTTP client uses `redirect::Policy::none`. A version record whose `download_url` has a different origin is rejected before download (`registry.rs:131-135`).

The public Worker and the auth Worker both take `REGISTRY_ORIGIN`. The public Worker builds download and archive URLs from that value (`registry/src/edge.rs:319,327`). The auth Worker uses it as Better Auth `baseURL`, the only `trustedOrigins` entry, and the device verification URI (`registry/auth/src/auth.ts:22,25,60`). Cookies are secure, HttpOnly, SameSite=Lax, and host-only: `defaultCookieAttributes` sets no `Domain`. The auth Worker has no public route, `workers.dev` name, or preview URL. The public Worker also has `workers_dev` and `preview_urls` false, and it has no custom-domain route yet. GitHub's callback is documented as `https://harness.meetgerry.ai/api/auth/callback/github`. The allowlist path is `/api/auth/callback/github`. There is no CORS allow-list. A foreign `Origin` is rejected.

`https://harness.ejmlabs.com` is already the root custom domain of the static Worker `ejmlabs-harness` (`site/wrangler.jsonc`). `cli.ejmlabs.com`, `ide.ejmlabs.com`, and `ejmlabs.com` are the same kind of binding on other Workers. The registry source has not been deployed from this tree: both Wrangler files still use placeholder D1 ids, and no route is configured. `ejm-cli` and `ejm-ide` deploy notes still tell operators not to touch existing account Workers named `gerry-*`, `gerry-marketing`, and `gerry-coming-soon`.

Saved registry sessions are keyed by the exact origin. A legacy `registry-session.json` blocks save and load for every origin until that file is migrated (`credentials.rs:49-53`). The error tells the user to run `ejm-cli package migrate-credentials --old-clients-stopped`. That command is not implemented in `ejm-cli`. The harness function `migrate_registry_token` exists and refuses a legacy file whose origin does not match the requested origin (`credentials.rs:125-128`). `docs/packages.md` documents the same missing command.

The first published harness release stays `0.1.0`. Everett will delete and recreate that version at the fixed commit. Notes for this change belong in the `0.1.0` section of `CHANGELOG.md`. Tags and GitHub releases stay untouched.

## Goals / Non-Goals

**Goals:**

- One confirmed `ejmlabs.com` origin for `DEFAULT_REGISTRY`, both Workers' `REGISTRY_ORIGIN`, schema generation, download URLs, and the GitHub OAuth callback.
- Reject `meetgerry.ai` and its subdomains before any request, with no fallback and no transfer of old sessions.
- Unblock sign-in to the new origin when a legacy file belongs to the retired origin, and discard that local session without a network call.
- Stop telling users to run a CLI command that does not exist.
- Keep same-origin downloads, redirect refusal, host-only cookies, and the private auth Worker.

**Non-Goals:**

- Choosing the hostname here. Everett decides. See Open Questions.
- Renaming `gerry.package.json`, the `gerry-registry*` Worker, D1, R2, Cargo, or npm names, registry UI and auth branding, the reserved handle `gerry`, or `gerry-cli` test fixtures.
- Editing the Gerry wording in `README.md`, `AGENTS.md`, `site/public/index.html`, or `docs/contributing.md`.
- An override flag or environment variable in the harness. Host overrides belong to the `ejm-cli` change of the same name.
- Deploying DNS, Workers, OAuth, or packages. Changing the loopback set. Package signing. Moving packages published on the old host. Creating, moving, or deleting tags or GitHub releases.
- Editing `ejm-cli`, `ejm-ide`, `ejm-ui`, or `ejmlabs` in this change.

## Decisions

### 1. Keep the host choice open, and present both candidates

The proposal recommends `https://registry.ejmlabs.com`. The spec scenarios use that name and say the choice is pending. This design does not close the question.

`https://registry.ejmlabs.com` is a new bare origin. Nothing in these repos binds it. The public Worker can take it as a root custom domain, in the same form as `site/wrangler.jsonc`, on the same Cloudflare account. The auth Worker stays unrouted. Host-only cookies, the exact `Origin` check, `trustedOrigins`, CSP `connect-src 'self'`, download URLs, and the GitHub callback then all name that one host. `parse_origin` already requires a bare origin, so this name needs no path support. It adds a fifth `ejmlabs.com` hostname beside harness, cli, ide, and the apex.

`https://harness.ejmlabs.com` is the hostname already published for the harness. It is also the root custom domain of the assets-only Worker `ejmlabs-harness`. The registry Worker needs the whole origin: `registry/public` pages, `/api/auth/*`, and `/v1/*`. One hostname is one Worker custom domain. Serving the registry there means the static site gives up that hostname first, or the site is folded into the registry Worker. A path such as `https://harness.ejmlabs.com/registry` is rejected by `parse_origin` and would place registry cookies on the marketing origin. Those cookies are host-only and SameSite=Lax, so the browser would send them to every path on that host. The registry grants no CORS and treats any other origin as foreign. Sharing the host makes the marketing site same-origin with account mutations, and it puts the GitHub callback on `https://harness.ejmlabs.com/api/auth/callback/github`.

`cli.ejmlabs.com`, `ide.ejmlabs.com`, and `ejmlabs.com` have the same collision with their own Workers. They are worse fits because they are not the registry product.

Implementation waits for Everett's answer. Tasks substitute that exact origin into the client, both Wrangler vars, `generate-schema.ts`, the README callback and custom domain, `docs/packages.md`, the spec scenarios, and the `0.1.0` changelog bullet. If he chooses `harness.ejmlabs.com`, moving the static site off that name is a separate authorized change and a prerequisite. This change does not edit `site/` unless he chooses that name and authorizes the site move.

### 2. Move the Worker origin with the client default

The client refuses a download URL from any other origin, and it does not follow redirects. Changing only `DEFAULT_REGISTRY` would make every version record from the old Worker fail. Set the same origin in:

- `registry/wrangler.jsonc` `REGISTRY_ORIGIN`
- `registry/auth/wrangler.jsonc` `REGISTRY_ORIGIN`
- `registry/auth/scripts/generate-schema.ts`

Leave Better Auth `baseURL`, `trustedOrigins`, and `verificationUri` derived from `REGISTRY_ORIGIN`. Leave the callback path `/api/auth/callback/github`. Update the README so the registered callback and the custom domain use the confirmed origin. Add `routes: [{ pattern: "<confirmed-host>", custom_domain: true }]` on the public Worker only. Keep `workers_dev: false` and `preview_urls: false` on both Workers. Do not add a route, `workers.dev` name, or preview URL to the auth Worker.

Do not set a cookie `Domain`. A parent domain of `ejmlabs.com` would send registry cookies to the other product hosts. Do not add a CORS grant for the marketing sites.

`generate-schema.ts` passes the origin into Better Auth only so the migration compiler can run. `registry/auth/migrations/0001_auth.sql` does not contain the hostname today. After the constant changes, run the existing schema generator and commit SQL only if the generator emits a difference. Do not hand-edit that migration.

Worker, D1, R2, Cargo, and npm names stay `gerry-registry`, `gerry-registry-auth`, `gerry-registry-packages`, and `@gerry/registry-development`. Renaming them is a different migration, and the sibling deploy notes treat `gerry-*` Workers as live account resources that site deploys must not touch. Placeholder D1 ids stay until an authorized deployment.

### 3. Reject the retired origin, and do not redirect it

`parse_origin` rejects a host that is `meetgerry.ai` or a subdomain of it, including `harness.meetgerry.ai`. The error names the confirmed default. The check is on the parsed host, so `notmeetgerry.ai` remains a normal HTTPS origin. No second origin is tried when the default connection fails.

The old host is not kept as an HTTP redirect. The client treats a 3xx as failure and would not send a credential to the `Location`. A temporary reverse proxy on the old host would also be a second origin, which splits host-only cookies and the exact `Origin` check. The README in this tree says this registry has not been deployed. If Everett later finds that `harness.meetgerry.ai` did issue sessions, those sessions stay unused. Users sign in on the new origin. The old tokens are not copied and not replayed.

### 4. Discard a retired session locally, and stop citing a missing command

`ejm-cli` `PackageCommand` has no `migrate-credentials` variant. Searched at `ejm-cli` `c8e2239392f71df82cbef6166b1827c8e5f5f1c5`. The harness still tells users to run `ejm-cli package migrate-credentials --old-clients-stopped` whenever any `registry-session.json` exists.

Split that gate:

- If the legacy file's origin host is `meetgerry.ai` or a subdomain, do not import it and do not mark the new origin migrated from it. `save_registry_token` and `load_registry_token` for any other origin return an error that the session belongs to the retired registry, that the token is not sent, and that the host can discard it locally. That replaces the silent block.
- Add a discard function that removes the legacy file and the vault record for that exact retired origin. It performs no HTTP request. It refuses a legacy file for any other origin, so a real same-origin handoff is preserved.
- A vault record whose id is the retired origin is never returned for the new origin. Discard deletes it. It is not re-keyed.
- When the legacy file's origin matches the requested origin, keep `migrate_registry_token` and the rule that older clients must be stopped. Change the error and `docs/packages.md` so they name that function. They must not name `ejm-cli package migrate-credentials`.

A user-facing CLI command that calls discard, and the sentence in `docs/ejm-cli.md` that names `https://harness.meetgerry.ai`, belong to the `ejm-cli` OpenSpec change `move-registry-to-ejmlabs-domain`. This repo does not edit that repository. `ejm-cli` builds against the sibling harness and uses `DEFAULT_REGISTRY`, so its default follows this crate once that checkout is updated.

### 5. Leave the loopback set alone

Development HTTP stays limited to `localhost`, `127.0.0.1`, and `[::1]`. Widening it to `127.0.0.0/8` is the `require-https-provider-endpoints` decision. This change should match that result when it exists, and it does not invent a second loopback list first.

### 6. Record the move under changelog `0.1.0`

Add the note to `## [0.1.0]`, not `## [Unreleased]` and not a `0.2.0` section. When Everett names the host, update that same bullet. Do not create, delete, move, or republish a tag or GitHub release from this work. `v0.1.0` is recreated later, at the fixed commit, by Everett.

## Risks / Trade-offs

- [Everett picks `harness.ejmlabs.com` without moving the site] → The registry route would fight the static site's custom domain. The hostname task stays blocked until the site move is authorized and done.
- [Spec scenarios name `registry.ejmlabs.com` before he decides] → Apply updates the scenarios to the confirmed origin before changing code. `openspec validate --strict` is run again.
- [`AGENTS.md` still says the registry origin is unchanged until the user decides] → That line is intentionally kept. The hosts skill is updated so the origin clause matches this decision, and the manifest-name clause stays. The stale `AGENTS.md` row remains until the wider wording sweep.
- [A legacy `registry-session.json` for the retired origin blocks every login] → The retired-origin branch returns a discard message and does not import the token. Same-origin legacy files still require `migrate_registry_token`.
- [Citing `migrate-credentials` for the same-origin path too] → Both error strings and `docs/packages.md` stop naming the missing command. The CLI change can add a command later. It is not required for the harness API.
- [Someone sets cookie `Domain` to `ejmlabs.com`, or adds a redirect on the old host] → Cookies stay host-only. `parse_origin` rejects the old host. The client already ignores redirects. Tests cover both.
- [Schema regeneration rewrites auth SQL] → Commit the generator output only. Review the diff. The hostname is not expected to appear in `0001_auth.sql`.
- [Renaming `gerry-registry` during the origin edit] → Out of scope. A rename would orphan a live Worker or bucket if one exists. Names stay.
- [The old registry was actually live] → Still no token copy and no redirect. Confirm with Everett before any DNS for `harness.meetgerry.ai`. This design does not probe or change Cloudflare.

## Migration Plan

1. Everett confirms the origin. If it is `harness.ejmlabs.com`, move the static site off that hostname under its own authorization before the registry route exists.
2. Change the client default, rejection rule, both `REGISTRY_ORIGIN` values, the schema-generator input, the README callback and custom domain, and `docs/packages.md` to that origin.
3. Add the public Worker custom domain in Wrangler. Leave the auth Worker private. Leave resource names unchanged.
4. Replace the missing-command error, add local discard for a retired origin, and keep same-origin `migrate_registry_token` for every other legacy file.
5. Regenerate the auth schema check. Run the focused package tests and `./scripts/check-registry`. Do not deploy.
6. Keep the changelog note in the `0.1.0` section. Leave tags and GitHub releases alone.
7. The `ejm-cli` change updates its docs and any user-facing command after this harness revision is available. Rollback of this change is reverting the origin constant and the rejection. Stored retired-origin tokens are not restored onto the new origin.

## Open Questions

1. Which origin Everett confirms: `https://registry.ejmlabs.com` or `https://harness.ejmlabs.com`. The proposal recommends the first. The facts for both are in Decision 1. This choice changes the spec scenarios, the changelog bullet, and whether the static site must move. It does not change the rest of the approach.
2. Whether `harness.meetgerry.ai` ever issued real sessions. The source shows placeholder D1 ids and no route. Either answer uses the same rejection and local discard.
3. Whether `require-https-provider-endpoints` changes the loopback set before this change is applied. Until it does, the current three hosts stay.
