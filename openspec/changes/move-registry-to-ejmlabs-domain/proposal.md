# Proposal

## Why

The default package registry was hard-coded to `https://harness.meetgerry.ai`. Everett does not operate `meetgerry.ai`. Every ejmlabs host is on `ejmlabs.com`. A default that points at a domain outside his control is a supply-chain risk. The audit (`security-baseline-2026-10-19.md`, Info 2) flags that a lapsed or third-party registry domain would let someone else serve packages, receive registry account tokens, and run the device-login page.

The registry is the trust root for package installs. The client fetches the version record, size, SHA-256 and download URL from it, and the download must come from the same origin. So the host has to be one Everett owns. The default is now `https://registry.ejmlabs.com`. The `AGENTS.md` names table records that origin. Only `gerry.package.json` stays unchanged until the user decides.

## What Changes

- **BREAKING:** `DEFAULT_REGISTRY` is `https://registry.ejmlabs.com`. Registry search, info, install, update, login, whoami, publisher claim, publish and logout all use that origin.
- The registry Workers' `REGISTRY_ORIGIN` must equal the new default: `registry/wrangler.jsonc:9`, `registry/auth/wrangler.jsonc:10` and `registry/auth/scripts/generate-schema.ts:9`. So must the deployment steps in `registry/README.md` (custom domain, GitHub OAuth callback). The client rejects any version record whose `download_url` has a different origin (`registry.rs:131-135`), so client and server must change together.
- **BREAKING:** the harness rejects the retired `meetgerry.ai` origin, including subdomains. It never falls back to that host, never sends it a credential, and never carries its sessions over to the new origin.
- Registry sessions saved for the retired origin are never sent anywhere. Users get a clear message and a supported way to discard them. Today a leftover legacy `registry-session.json` for the old origin blocks every new login (`registry/credentials.rs:50-53`), and migrating it to a different origin is refused (`credentials.rs:125-128`). That is a dead end unless this change handles it.
- No weakening of package integrity: exact size, SHA-256, manifest identity, same-origin download, no redirects, no credentials on downloads, and reinstall clearing trust all stay as they are.
- No host-side override is added in the harness. `RegistryClient::new(origin)` already takes any origin, so overrides belong to each host (see the ejm-cli change of the same name). The origin rules stay: HTTPS, a plain origin, and HTTP only for `localhost`, `127.0.0.1`, and `[::1]` (design decision 5).

### Out of scope

- Renaming `gerry.package.json`, the Cargo package `gerry-registry`, the npm package `@gerry/registry-development`, and the "Gerry" branding in `registry/public/*.html`. Wrangler names the Workers `ejm-registry` and `ejm-registry-auth`, the D1 databases `ejm-registry` and `ejm-registry-auth`, and the R2 bucket `ejm-registry-packages`. Placeholder D1 ids stay. This change does not deploy those resources.
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
    - Wrangler Worker, D1, and R2 names are `ejm-registry`, `ejm-registry-auth`, and `ejm-registry-packages`. The Cargo package stays `gerry-registry` (`registry/Cargo.toml`). The npm package stays `@gerry/registry-development`
    - branding: `registry/src/lib.rs:1`, `registry/README.md:1,3,241,244`, `registry/public/{index,device,account}.html` ("Gerry extensions"), `registry/auth/src/auth.ts:21` ("Gerry Registry"), `registry/auth/src/consent.ts:5`
    - `registry/src/policy.rs:44,169`: `gerry` as a reserved handle
    - the test client id `gerry-cli`: `registry/tests/auth.test.ts:88,206`
    - the explanatory "No Gerry Workspace" lines in `README.md:8`, `AGENTS.md:4`, `site/public/index.html:343`

## Open Questions

1. **New default origin.** Resolved. The default is `https://registry.ejmlabs.com`. Design decision 1 and the requirement "Default registry origin is on ejmlabs.com" record that choice. `harness.ejmlabs.com` stays the static site.
2. **Retired origin: reject, redirect or support?** Resolved. Reject `meetgerry.ai` and every subdomain, with no redirect and no fallback. Design decision 3 and the requirement "The retired meetgerry registry is never trusted" settle this.
3. **Saved sessions for the retired origin.** Resolved. Never send them and never re-key them. The host discards them locally. Design decision 4 and the requirement "Saved sessions for the retired origin are never sent" settle this.
4. **Was the old registry ever live?** Resolved for this change. The source still has placeholder D1 ids and this change does not deploy. Either history uses the same rule: old sessions are not sent, not copied, and not redirected. Design decision 3 and the requirement "Saved sessions for the retired origin are never sent" settle this.
5. **Loopback set** for development origins. Resolved. HTTP loopback is only `localhost`, `127.0.0.1`, and `[::1]`. `http://127.0.0.2` is rejected. Design decision 5 and the requirement "Registry origins are plain HTTPS origins" settle this.
