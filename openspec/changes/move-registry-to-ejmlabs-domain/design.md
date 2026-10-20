# Design

## Context

See proposal.md for why the default registry must leave `meetgerry.ai`. This design covers how the harness and the registry Workers change together, and what stays put.

`DEFAULT_REGISTRY` is `https://registry.ejmlabs.com`. `ejm-cli` passes that constant to `RegistryClient::new`. A host may pass another origin. The client allows HTTPS, or HTTP only for `localhost`, `127.0.0.1`, and `[::1]`. It rejects credentials, a query, a fragment, and any non-root path. A host that is `meetgerry.ai` or a subdomain is rejected before those generic checks, including when the URL also has a path, userinfo, or an `http` scheme. Plain HTTP loopback clients disable proxies. HTTPS clients keep the process proxy environment. The HTTP client uses `redirect::Policy::none`. A version record whose `download_url` has a different origin is rejected before download.

The public Worker and the auth Worker both take `REGISTRY_ORIGIN` `https://registry.ejmlabs.com`. The public Worker builds download and archive URLs from that value. The auth Worker uses it as Better Auth `baseURL`, the only `trustedOrigins` entry, and the device verification URI. Cookies are secure, HttpOnly, SameSite=Lax, and host-only: `defaultCookieAttributes` sets no `Domain`. The auth Worker has no public route, `workers.dev` name, or preview URL. The public Worker has `workers_dev` and `preview_urls` false, and a root custom domain route for `registry.ejmlabs.com`. That route is configuration only. This change does not deploy it. GitHub's callback is `https://registry.ejmlabs.com/api/auth/callback/github`. The allowlist path is `/api/auth/callback/github`. There is no CORS allow-list. A foreign `Origin` is rejected.

Wrangler names the public Worker `ejm-registry`, the auth Worker `ejm-registry-auth`, the D1 databases `ejm-registry` and `ejm-registry-auth`, and the R2 bucket `ejm-registry-packages`. Both Wrangler files still use placeholder D1 ids. `https://harness.ejmlabs.com` stays the root custom domain of the static Worker `ejmlabs-harness` (`site/wrangler.jsonc`). `ejm-cli` and `ejm-ide` deploy notes still tell operators not to touch existing account Workers named `gerry-*`, `gerry-marketing`, and `gerry-coming-soon`.

Saved registry sessions are keyed by the exact origin. Before this change, a legacy `registry-session.json` blocked save and load for every origin until that file was migrated, and the error named a CLI command that `ejm-cli` does not implement. A retired-origin legacy file now does not block sign-in to `https://registry.ejmlabs.com`. The host discards it locally. `migrate_registry_token` still refuses a legacy file whose origin does not match the requested origin.

The first published harness release stays `0.1.0`. Everett will delete and recreate that version at the fixed commit. Notes for this change belong in the `0.1.0` section of `CHANGELOG.md`. Tags and GitHub releases stay untouched.

## Goals / Non-Goals

**Goals:**

- One confirmed `ejmlabs.com` origin for `DEFAULT_REGISTRY`, both Workers' `REGISTRY_ORIGIN`, schema generation, download URLs, and the GitHub OAuth callback.
- Reject `meetgerry.ai` and its subdomains before any request, with no fallback and no transfer of old sessions.
- Unblock sign-in to the new origin when a legacy file belongs to the retired origin, and discard that local session without a network call.
- Stop telling users to run a CLI command that does not exist.
- Keep same-origin downloads, redirect refusal, host-only cookies, and the private auth Worker.

**Non-Goals:**

- Choosing a different registry hostname. Decision 1 records `https://registry.ejmlabs.com`.
- Renaming `gerry.package.json`, the Cargo package `gerry-registry`, the npm package `@gerry/registry-development`, registry UI and auth branding, the reserved handle `gerry`, or `gerry-cli` test fixtures.
- Editing the Gerry wording in `README.md`, `AGENTS.md`, `site/public/index.html`, or `docs/contributing.md`.
- An override flag or environment variable in the harness. Host overrides belong to the `ejm-cli` change of the same name.
- Deploying DNS, Workers, OAuth, or packages. Changing the loopback set. Package signing. Moving packages published on the old host. Creating, moving, or deleting tags or GitHub releases.
- Editing `ejm-cli`, `ejm-ide`, `ejm-ui`, or `ejmlabs` in this change.

## Decisions

### 1. Everett chose `https://registry.ejmlabs.com`

On 2026-10-19 Everett approved `https://registry.ejmlabs.com` as the default registry origin. The spec scenarios already use that name. `https://harness.ejmlabs.com` was the other candidate and is not the registry host.

`https://registry.ejmlabs.com` is a new bare origin. Nothing in these repos binds it. The public Worker can take it as a root custom domain, in the same form as `site/wrangler.jsonc`, on the same Cloudflare account. The auth Worker stays unrouted. Host-only cookies, the exact `Origin` check, `trustedOrigins`, CSP `connect-src 'self'`, download URLs, and the GitHub callback then all name that one host. `parse_origin` already requires a bare origin, so this name needs no path support. It adds a fifth `ejmlabs.com` hostname beside harness, cli, ide, and the apex.

`https://harness.ejmlabs.com` is the hostname already published for the harness. It is also the root custom domain of the assets-only Worker `ejmlabs-harness`. The registry Worker needs the whole origin: `registry/public` pages, `/api/auth/*`, and `/v1/*`. One hostname is one Worker custom domain. Serving the registry there means the static site gives up that hostname first, or the site is folded into the registry Worker. A path such as `https://harness.ejmlabs.com/registry` is rejected by `parse_origin` and would place registry cookies on the marketing origin. Those cookies are host-only and SameSite=Lax, so the browser would send them to every path on that host. The registry grants no CORS and treats any other origin as foreign. Sharing the host makes the marketing site same-origin with account mutations, and it puts the GitHub callback on `https://harness.ejmlabs.com/api/auth/callback/github`.

`cli.ejmlabs.com`, `ide.ejmlabs.com`, and `ejmlabs.com` have the same collision with their own Workers. They are worse fits because they are not the registry product.

The public Worker route in `registry/wrangler.jsonc` names `registry.ejmlabs.com`. It is not deployed by this change. The auth Worker has no route. `site/` stays on `harness.ejmlabs.com`.

### 2. Move the Worker origin with the client default

The client refuses a download URL from any other origin, and it does not follow redirects. Changing only `DEFAULT_REGISTRY` would make every version record from the old Worker fail. Set the same origin in:

- `registry/wrangler.jsonc` `REGISTRY_ORIGIN`
- `registry/auth/wrangler.jsonc` `REGISTRY_ORIGIN`
- `registry/auth/scripts/generate-schema.ts`

Leave Better Auth `baseURL`, `trustedOrigins`, and `verificationUri` derived from `REGISTRY_ORIGIN`. Leave the callback path `/api/auth/callback/github`. Update the README so the registered callback and the custom domain use the confirmed origin. Add `routes: [{ pattern: "<confirmed-host>", custom_domain: true }]` on the public Worker only. Keep `workers_dev: false` and `preview_urls: false` on both Workers. Do not add a route, `workers.dev` name, or preview URL to the auth Worker.

Do not set a cookie `Domain`. A parent domain of `ejmlabs.com` would send registry cookies to the other product hosts. Do not add a CORS grant for the marketing sites.

`generate-schema.ts` passes the origin into Better Auth only so the migration compiler can run. `registry/auth/migrations/0001_auth.sql` does not contain the hostname today. After the constant changes, run the existing schema generator and commit SQL only if the generator emits a difference. Do not hand-edit that migration.

Worker names in Wrangler are `ejm-registry` and `ejm-registry-auth`. D1 database names are `ejm-registry` and `ejm-registry-auth`. The R2 bucket name is `ejm-registry-packages`. Placeholder D1 ids stay until an authorized deployment. The Cargo package stays `gerry-registry` and the npm package stays `@gerry/registry-development`. Sibling deploy notes still say not to touch live `gerry-*` Workers such as `gerry-marketing` and `gerry-coming-soon`. This change does not deploy, so it does not rename a live resource.

### 3. Reject the retired origin, and do not redirect it

`validate_registry_origin`, which `RegistryClient::new` calls, rejects a host that is `meetgerry.ai` or a subdomain of it, including `harness.meetgerry.ai`, before the generic path, userinfo, and scheme checks. A retired host with a path, userinfo, or `http` scheme still gets the retired-registry message. The error names the confirmed default and `DISCARD_RETIRED_REGISTRY_COMMAND`. The check is on the parsed host, so `notmeetgerry.ai` remains a normal HTTPS origin. `normalize_origin` still accepts a retired host so a stored vault identity can be deleted. No second origin is tried when the default connection fails.

The old host is not kept as an HTTP redirect. The client treats a 3xx as failure and would not send a credential to the `Location`. A temporary reverse proxy on the old host would also be a second origin, which splits host-only cookies and the exact `Origin` check. The README in this tree says this registry has not been deployed. If Everett later finds that `harness.meetgerry.ai` did issue sessions, those sessions stay unused. Users sign in on the new origin. The old tokens are not copied and not replayed.

A plain HTTP loopback client disables proxies, so a bearer is not sent to an HTTP proxy. An HTTPS client keeps the process proxy environment.

### 4. Discard a retired session locally, and stop citing a missing command

`ejm-cli` had no discard command at `c8e2239392f71df82cbef6166b1827c8e5f5f1c5`. On 2026-10-20 the sibling branch `openspec/move-registry-to-ejmlabs-domain` is commit `65525726fa71d5857174fce969c947249d238ae2` and is still a plan. Its tasks still say to add `package migrate-credentials` with a required `--old-clients-stopped` flag, because they were written against harness main, where the error named that missing command. Everett required this harness change to stop naming that command. The command this crate names, and that the CLI change should implement, is `ejm-cli package discard-retired-session` (`DISCARD_RETIRED_REGISTRY_COMMAND`). The CLI task that says to stop and update its change when the harness API differs is the coordination point.

- A legacy file or vault record for `meetgerry.ai` or a subdomain does not block `save_registry_token` or `load_registry_token` for another origin, and it is not imported or re-keyed. `retired_registry_session_notice` returns the discard instruction when one is present.
- `load_registry_token` and `save_registry_token` for a retired origin fail with that same instruction. The token is not returned and not sent.
- `locate_retired_registry_sessions` lists those origins without token values. `discard_retired_registry_sessions` deletes the legacy file and the vault records locally, with no HTTP request. A legacy file for any other origin is left alone.
- An unsafe or unreadable `registry-session.json` (a symlink, a mode other than `0600`, or JSON that does not parse) does not yield a token. Sign-in names the full path and `DISCARD_RETIRED_REGISTRY_COMMAND`. `discard_retired_registry_sessions` and `force_discard_legacy_registry_session` unlink that directory entry without following it and without a network request. A missing file makes force discard return false. A private well-formed file is left in place. `migrate_registry_token` does not delete an unsafe or unreadable file.
- A same-origin legacy file still uses `migrate_registry_token`. The error names that function. It does not name `ejm-cli package migrate-credentials`.
- `validate_registry_origin` and `registry_origin_is_default` are the harness API for the CLI's `--registry` and `EJM_CLI_REGISTRY_URL`. The flag, the environment variable, and doctor output stay in the CLI change.

The sentence in `docs/ejm-cli.md` that names `https://harness.meetgerry.ai` belongs to that CLI change. This repo does not edit that repository. `ejm-cli` builds against the sibling harness and uses `DEFAULT_REGISTRY`, so its default follows this crate once that checkout is updated.

### 5. Leave the loopback set alone

Development HTTP stays limited to `localhost`, `127.0.0.1`, and `[::1]`. Widening it to `127.0.0.0/8` is the `require-https-provider-endpoints` decision. This change should match that result when it exists, and it does not invent a second loopback list first. Plain HTTP loopback disables proxies. HTTPS keeps them.

### 6. Record the move under changelog `0.1.0`

Add the note to `## [0.1.0]`, not `## [Unreleased]` and not a `0.2.0` section. When Everett names the host, update that same bullet. Do not create, delete, move, or republish a tag or GitHub release from this work. `v0.1.0` is recreated later, at the fixed commit, by Everett.

## Risks / Trade-offs

- [`AGENTS.md` names an undecided registry origin] → The names table says `https://registry.ejmlabs.com`. The `gerry.package.json` clause stays until the user decides otherwise.
- [A legacy `registry-session.json` for the retired origin blocks every login] → Sign-in to `https://registry.ejmlabs.com` proceeds. The notice names `ejm-cli package discard-retired-session`. The token is not imported. Same-origin legacy files still require `migrate_registry_token`.
- [The CLI implements a different discard command name] → This crate publishes `DISCARD_RETIRED_REGISTRY_COMMAND`. The CLI change should use that string. The old `migrate-credentials` command is not named.
- [Someone sets cookie `Domain` to `ejmlabs.com`, or adds a redirect on the old host] → Cookies stay host-only. `validate_registry_origin` rejects the old host. The client already ignores redirects. Tests cover both.
- [Schema regeneration rewrites auth SQL] → Commit the generator output only. Review the diff. The hostname is not expected to appear in `0001_auth.sql`.
- [Wrangler resource names] → Config names the Workers `ejm-registry` and `ejm-registry-auth`, the D1 databases `ejm-registry` and `ejm-registry-auth`, and the R2 bucket `ejm-registry-packages`. Placeholder D1 ids stay. Nothing is deployed from this change. Cargo, npm, and product branding stay.
- [The old registry was actually live] → Still no token copy and no redirect. Confirm with Everett before any DNS for `harness.meetgerry.ai`. This design does not probe or change Cloudflare.

## Migration Plan

1. Everett confirms the origin. If it is `harness.ejmlabs.com`, move the static site off that hostname under its own authorization before the registry route exists.
2. Change the client default, rejection rule, both `REGISTRY_ORIGIN` values, the schema-generator input, the README callback and custom domain, and `docs/packages.md` to that origin.
3. Add the public Worker custom domain in Wrangler. Leave the auth Worker private. Set the Worker, D1, and R2 names to `ejm-registry`, `ejm-registry-auth`, and `ejm-registry-packages` in config only.
4. Replace the missing-command error, add local discard for a retired origin, and keep same-origin `migrate_registry_token` for every other legacy file.
5. Regenerate the auth schema check. Run the focused package tests and `./scripts/check-registry`. Do not deploy.
6. Keep the changelog note in the `0.1.0` section. Leave tags and GitHub releases alone.
7. The `ejm-cli` change updates its docs and any user-facing command after this harness revision is available. Rollback of this change is reverting the origin constant and the rejection. Stored retired-origin tokens are not restored onto the new origin.

## Open Questions

1. Whether `harness.meetgerry.ai` ever issued real sessions. The source shows placeholder D1 ids. Either answer uses the same rejection and local discard. Tokens are not copied.
2. Whether `require-https-provider-endpoints` changes the loopback set before a later edit. Until it does, the current three hosts stay.
