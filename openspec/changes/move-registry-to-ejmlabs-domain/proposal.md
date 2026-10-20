# Proposal

## Why

The default package registry is hard-coded to `https://harness.meetgerry.ai` (`crates/ejm-harness-packages/src/registry.rs:21`, `DEFAULT_REGISTRY`). Everett does not operate `meetgerry.ai`. Every ejmlabs host is on `ejmlabs.com`. A default that points at a domain outside his control is a supply-chain risk. The audit (`security-baseline-2026-10-19.md`, Info 2) flags that a lapsed or third-party registry domain would let someone else serve packages, receive registry account tokens, and run the device-login page.

The registry is the trust root for package installs. The client fetches the version record, size, SHA-256 and download URL from it, and the download must come from the same origin. So the host has to be one Everett owns. `AGENTS.md:57` records the registry origin as "unchanged on purpose until the user decides". This change is that decision.

## What Changes

- **BREAKING:** `DEFAULT_REGISTRY` moves to an `ejmlabs.com` origin. Recommended: `https://registry.ejmlabs.com` (decision pending, see Open Questions). Registry search, info, install, update, login, whoami, publisher claim, publish and logout all use the new origin.
- The registry Workers' `REGISTRY_ORIGIN` must equal the new default: `registry/wrangler.jsonc:9`, `registry/auth/wrangler.jsonc:10` and `registry/auth/scripts/generate-schema.ts:9`. So must the deployment steps in `registry/README.md` (custom domain, GitHub OAuth callback). The client rejects any version record whose `download_url` has a different origin (`registry.rs:131-135`), so client and server must change together.
- **BREAKING:** the harness rejects the retired `meetgerry.ai` origin, including subdomains. It never falls back to that host, never sends it a credential, and never carries its sessions over to the new origin.
- Registry sessions saved for the retired origin are never sent anywhere. Users get a clear message and a supported way to discard them. Today a leftover legacy `registry-session.json` for the old origin blocks every new login (`registry/credentials.rs:50-53`), and migrating it to a different origin is refused (`credentials.rs:125-128`). That is a dead end unless this change handles it.
- No weakening of package integrity: exact size, SHA-256, manifest identity, same-origin download, no redirects, no credentials on downloads, and reinstall clearing trust all stay as they are.
- No host-side override is added in the harness. `RegistryClient::new(origin)` already takes any origin, so overrides belong to each host (see the ejm-cli change of the same name). The origin rules stay: HTTPS, plain origin, loopback HTTP for local development. The loopback set should match whatever Everett decides in `require-https-provider-endpoints`.

### Out of scope

- Renaming the `gerry.package.json` manifest, the `gerry-registry*` Worker, D1 and R2 names, and the "Gerry" branding in `registry/public/*.html`. These are part of the Engineer's wider meetgerry/Gerry sweep.
- Actually deploying the registry. That needs Everett's explicit authorization, per `registry/README.md`.
- Package signing, which does not exist today.
- Moving packages published on the old registry.

## Capabilities

### New Capabilities

- `package-registry-origin`: which registry origin hosts use by default, what origins are acceptable, how the retired origin and its saved sessions are handled, and the integrity guarantees that hold across the move.

### Modified Capabilities

None. `openspec/specs/` has no main specs. Related registry capabilities exist only in unarchived changes and are not changed here: `registry-credential-storage` (in `publish-host-auth-apis`) and `registry-device-flow` (in `public-repository-standards`).

## Impact

- **Code:**
    - `crates/ejm-harness-packages/src/registry.rs` (`DEFAULT_REGISTRY`, `parse_origin`)
    - `src/registry/credentials.rs` (handling of a session for the retired origin)
    - tests that hard-code `https://harness.meetgerry.ai`: `src/source/tests.rs:84`, `src/registry/credentials/tests.rs:30`, `registry/tests/api.test.ts:12`, `registry/tests/auth.test.ts:12`, `registry/src/policy.rs:115`
- **Registry server config and docs:** the two `wrangler.jsonc` files, `generate-schema.ts`, `registry/README.md:3-4,248,256`, `docs/packages.md:5-6`.
- **Downstream:** ejm-cli uses `DEFAULT_REGISTRY` directly (`ejm-cli/crates/ejm-cli/src/packages.rs:85`) and builds against the sibling harness path, so its default follows automatically. Its flag/env/doc work is in the ejm-cli change `move-registry-to-ejmlabs-domain`. ejm-ide does not use the package registry.
- **Operations:**
    - DNS and a Cloudflare custom domain for the new host on the `ejmlabs.com` zone, on the same account as `site/wrangler.jsonc`.
    - A new GitHub OAuth app callback (`https://<new-origin>/api/auth/callback/github`).
    - The real D1 database IDs, which are still placeholder UUIDs in both `wrangler.jsonc` files.
- **Coordination:** the Engineer is sweeping every `meetgerry` reference across all five repos. This change covers only the registry default origin and what directly depends on it (server origin config, retired-origin handling, tests and docs that name the origin). Other `meetgerry`/Gerry references seen in ejm-harness, left to the sweep:
    - the manifest name `gerry.package.json`: `crates/ejm-harness-package-spec/src/lib.rs:12`, `archive.rs:109`, and many tests and SDK tests
    - Worker, database and bucket names `gerry-registry`, `gerry-registry-auth`, `gerry-registry-packages`: `registry/wrangler.jsonc:3,10,14,19`, `registry/auth/wrangler.jsonc:3,16`, `registry/Cargo.toml:2`, `registry/Cargo.lock:241`, `registry/package.json:2` and `package-lock.json` (`@gerry/registry-development`)
    - branding: `registry/src/lib.rs:1`, `registry/README.md:1,3,241,244`, `registry/public/{index,device,account}.html` ("Gerry extensions"), `registry/auth/src/auth.ts:21` ("Gerry Registry"), `registry/auth/src/consent.ts:5`
    - `registry/src/policy.rs:44,169`: `gerry` as a reserved handle
    - the test client id `gerry-cli`: `registry/tests/auth.test.ts:88,206`
    - the explanatory "No Gerry Workspace" lines in `README.md:8`, `AGENTS.md:4`, `site/public/index.html:343`

## Open Questions

1. **New default origin.** (Everett.) Recommendation: **`https://registry.ejmlabs.com`**.
    - The client accepts only a bare origin; `parse_origin` rejects any path (`registry.rs:338-347`).
    - The registry Worker owns the whole origin: its public pages (`registry/public/index.html`, `account.html`, `device.html`), `/api/auth/*` and `/v1/*`.
    - It builds download URLs as `{REGISTRY_ORIGIN}/v1/...` (`registry/src/edge.rs:319,327`).
    - Its cookie/Origin mutation policy assumes a dedicated origin (`policy.rs:84-101`).
    - `harness.ejmlabs.com` already serves the static site Worker as a root custom domain (`site/wrangler.jsonc:9`).
    - So `harness.ejmlabs.com/registry` or `ejmlabs.com/registry` would need path-prefix support in both client and server, and would put registry auth cookies on a marketing-site origin.
    - Taking over `harness.ejmlabs.com` itself would mean moving the site.
2. **Retired origin: reject, redirect or support?** Recommendation: **reject**, with a clear message and no fallback. Rewriting old sessions to the new origin is not possible safely: those tokens were issued by a server Everett doesn't run, so sending them to the new host would leak them and they wouldn't work anyway. Should the rejection cover all of `*.meetgerry.ai`, or only `harness.meetgerry.ai`? Recommendation: all of it.
3. **Saved sessions for the retired origin.** Recommendation: never send them; tell the user and offer a discard step. The alternative is to delete them automatically on first run. That is simpler, but it deletes user state without asking.
4. **Was the old registry ever live?** Both `wrangler.jsonc` files still have placeholder D1 IDs and no route, and `harness.meetgerry.ai` could not be reached from the box today (the box proxy masks DNS, so this is not proof). If nothing was ever published there, migration is mostly theoretical. Please confirm.
5. **Loopback set** for development origins. Today it is `localhost`, `127.0.0.1` and `[::1]`. It should match the decision in `require-https-provider-endpoints` (recommended there: `localhost`, `127.0.0.0/8`, `::1`).
