# Proposal

## Why

The default package registry was hard-coded to `https://harness.meetgerry.ai`. Everett does not operate `meetgerry.ai`. Every ejmlabs host is on `ejmlabs.com`. A default that points at a domain outside his control is a supply-chain risk. The audit (`security-baseline-2026-10-19.md`, Info 2) flags that a lapsed or third-party registry domain would let someone else serve packages, receive registry account tokens, and run the device-login page.

The registry is the trust root for package installs. The client fetches the version record, size, SHA-256 and download URL from it, and the download must come from the same origin. So the host has to be one Everett owns. The public registry is `https://registry.ejmlabs.com`. The `AGENTS.md` names table records that origin. Main's separate package contract requires `ejm.package.json` and an explicitly configured registry client, with no default export.

## What Changes

- Preserve main's removal of `DEFAULT_REGISTRY`. Hosts explicitly choose an origin through `RegistryClient::new(origin)`. `PUBLIC_REGISTRY_ORIGIN` identifies `https://registry.ejmlabs.com` without selecting it automatically, and `registry_origin_is_public` identifies that explicit choice.
- Both registry Workers explicitly configure `REGISTRY_ORIGIN` as `https://registry.ejmlabs.com`. Schema generation and automated fixtures retain main's reserved example origin. So must the deployment steps in `registry/README.md` (custom domain, GitHub OAuth callback). The client rejects any version record whose `download_url` has a different origin (`registry.rs:131-135`), so client and server must change together.
- **BREAKING:** the harness rejects the retired `meetgerry.ai` origin, including subdomains. It never falls back to that host, never sends it a credential, and never carries its sessions over to the new origin.
- Registry sessions for the retired origin are never saved, loaded, imported or sent. Nothing shipped against `meetgerry.ai`, so no user holds one and no discard path is added.
- No weakening of package integrity: exact size, SHA-256, manifest identity, same-origin download, no redirects, no credentials on downloads, and reinstall clearing trust all stay as they are.
- No host-side override is added in the harness. `RegistryClient::new(origin)` already takes any origin, so overrides belong to each host (see the ejm-cli change of the same name). The origin rules stay: HTTPS, a plain origin, and HTTP only for `localhost`, `127.0.0.1`, and `[::1]` (design decision 5).

### Out of scope

- Reverting main's accepted package and registry separation. Keep `ejm.package.json`, the `ejm-registry` Cargo package, `@ejm/registry-development`, registry/auth branding, reserved handles and the `ejm-cli` client identity. The new Worker host configuration does not deploy resources.
- Actually deploying the registry. That needs Everett's explicit authorization, per `registry/README.md`.
- Package signing, which does not exist today.
- Moving packages published on the old registry.

## Capabilities

### New Capabilities

- `package-registry-origin`: how hosts explicitly select the public registry origin, what origins are acceptable, how the retired origin and its saved sessions are handled, and the integrity guarantees that hold across the move.

### Modified Capabilities

None. `openspec/specs/` has no main specs. Related registry capabilities exist only in unarchived changes and are not changed here: `registry-credential-storage` (in `publish-host-auth-apis`) and `registry-device-flow` (in `public-repository-standards`).

## Impact

- **Code:**
    - `crates/ejm-harness-packages/src/registry.rs` (`PUBLIC_REGISTRY_ORIGIN`, `validate_registry_origin`)
    - `src/registry/credentials.rs` (handling of a session for the retired origin)
    - tests that hard-code `https://harness.meetgerry.ai`: `src/source/tests.rs:84`, `src/registry/credentials/tests.rs:30`, `registry/tests/api.test.ts:12`, `registry/tests/auth.test.ts:12`, `registry/src/policy.rs:115`
- **Registry server config and docs:** the two `wrangler.jsonc` files, `generate-schema.ts`, `registry/README.md:3-4,248,256`, `docs/packages.md:5-6`.
- **Downstream:** ejm-cli needs a coordinated update to explicitly configure `RegistryClient::new(origin)` and pass `Some(&client)` only for registry sources. The harness supplies validation and retired-session APIs; no host is edited here. ejm-ide does not use the package registry.
- **Operations:**
    - DNS and a Cloudflare custom domain for the new host on the `ejmlabs.com` zone, on the same account as `site/wrangler.jsonc`.
    - A new GitHub OAuth app callback (`https://<new-origin>/api/auth/callback/github`).
    - The real D1 database IDs, which are still placeholder UUIDs in both `wrangler.jsonc` files.
- **Coordination:** retain main's independent package identity, exact-origin credentials, renamed registry resources and current branding. This change adds the public host configuration and retired-origin handling on those contracts; sibling repositories remain untouched.

## Open Questions

1. **Public registry origin.** Resolved. Hosts explicitly select `https://registry.ejmlabs.com`. Design decision 1 and the requirement "The public registry origin is selected explicitly" record that choice. `harness.ejmlabs.com` stays the static site.
2. **Retired origin: reject, redirect or support?** Resolved. Reject `meetgerry.ai` and every subdomain, with no redirect and no fallback. Design decision 3 and the requirement "The retired meetgerry registry is never trusted" settle this.
3. **Saved sessions for the retired origin.** Resolved. Never send them and never re-key them. Nothing shipped against that origin, so there is nothing to discard. Design decision 4 and the requirement "Saved sessions for the retired origin are never sent" settle this.
4. **Was the old registry ever live?** Resolved for this change. The source still has placeholder D1 ids and this change does not deploy. Either history uses the same rule: old sessions are not sent, not copied, and not redirected. Design decision 3 and the requirement "Saved sessions for the retired origin are never sent" settle this.
5. **Loopback set** for development origins. Resolved. HTTP loopback is only `localhost`, `127.0.0.1`, and `[::1]`. `http://127.0.0.2` is rejected. Design decision 5 and the requirement "Registry origins are plain HTTPS origins" settle this.

## Proxy transport review follow-up

`ejm-harness-packages` owns the public `RegistryClient::new` transport contract. Accepted HTTP loopback origins must bypass every proxy before an account bearer request is sent. The existing reqwest `no_proxy()` setting supplies that behavior without a new dependency or a host transport copy. Strengthen the existing local route fixture to send a synthetic account bearer and prove the registry receives it while the proxy receives nothing.

The CLI consumes this contract through public path packages. Its current immutable harness pin predates the fix, so its separate change adds a fail-closed configuration adapter. Verify the harness contract first, merge through the existing human gates, then adopt a reviewed full harness commit in the CLI and remove that adapter after its route fixtures pass. No pin, release, CI approval, or deployment changes are part of this follow-up.
