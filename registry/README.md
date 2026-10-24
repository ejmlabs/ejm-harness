# ejm extension registry

The registry source implements immutable ejm extension packages. No public
endpoint is configured or verified as deployed. The Rust Worker owns publication
and catalog policy. A separate Better Auth Worker handles GitHub sign-in and first-party CLI
device authorization through a private service binding. D1 stores metadata and
auth in separate databases. A private R2 bucket stores `.tar.gz` archives.

This source tree is prepared for local checks. No Worker, database, bucket,
domain, DNS record, OAuth registration, secret, or package has been deployed or
published by this change. Real GitHub sign-in remains a deployment acceptance
check.

## Local development and required checks

Use Node 24 or later and the repository's pinned Rust toolchain through Rustup.
The check selects that toolchain so its compiler and installed Wasm target agree.
Keep Cargo jobs at most two. Install maintained tooling without running package
lifecycle scripts:

```sh
npm --prefix registry ci --ignore-scripts
cargo install --locked --jobs 2 --version 0.8.7 worker-build
rustup target add wasm32-unknown-unknown
./scripts/check-registry
```

The check runs native policy tests, the local SQLite constraints, strict Clippy
for native and Wasm code, dependency audits, schema freshness, TypeScript,
ESLint, Prettier, local Workerd/D1/R2/auth fixtures, and dry-run bundles. Dry-run
bundles do not upload or deploy. No fixture calls GitHub, a paid model, or a live
registry. The API fixtures substitute only the private normalized identity
service. Separate auth fixtures run the real Better Auth device/session code
against local D1 with locally created fixture users.

`npm --prefix registry run schema:auth` uses Better Auth's maintained migration
generator with Node's in-memory SQLite driver. Commit the generated SQL. Never
handwrite or silently apply auth schema changes. `check:schema` checks exact
generator output. The auth and registry databases have separate migration
directories and bindings. Review each migration before any remote apply.

Local development uses both Wrangler configurations:

```sh
cd registry
npm run dev
```

Set `REGISTRY_ORIGIN` in each Worker's ignored `.dev.vars` file to the same local
HTTPS origin before browser sign-in. The checked-in configurations intentionally
omit it. Automated fixtures supply a reserved example origin, which is a test
value and cannot serve as a public endpoint. Packages use `ejm.package.json`;
rename earlier manifests and repack them to refresh their sizes and hashes.

The current protected CLI needs its coordinated explicit-origin update before
using this service contract. A browser example or package reference does not
configure the CLI. Keep credentials isolated by origin.

Secure cookies and the fixed HTTPS auth origin are intentional. A local browser
sign-in needs an explicit local HTTPS origin and a matching test OAuth
registration. Automated fixtures supply their own test identities and require
neither.

## HTTP contract

JSON API failures use `{ "error": { "code": "..." } }` and an appropriate HTTP
status. Internal storage, provider errors, credentials, and moderation reasons
are not returned. API responses and downloads use `Cache-Control: no-store` so
new requests honor quarantine and suspension. Already downloaded bytes cannot be
recalled.

| Request                                                                          | Success response                                                                     |
| -------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------ |
| `GET /v1/packages?q=&limit=&cursor=`                                             | `{packages:[{name,description,latest_version}],next_cursor}`                         |
| `GET /v1/packages/{publisher}/{name}?limit=&cursor=`                             | `{name,versions:[VersionRecord],next_cursor}`                                        |
| `GET /v1/packages/{publisher}/{name}/versions/{version}`                         | `VersionRecord`                                                                      |
| `GET /v1/packages/{publisher}/{name}/versions/{version}/download`                | Gzip archive, attachment                                                             |
| `GET /v1/me`                                                                     | `{user:{id,display_name},publisher:null\|{handle,display_name,suspended},suspended}` |
| `POST /v1/publishers` with `{handle,display_name}`                               | `{handle,display_name,suspended}`                                                    |
| `POST /v1/packages/{publisher}/{name}/versions` with `{manifest,sha256,size}`    | `VersionRecord` plus `{status,upload_url}`                                           |
| `PUT /v1/packages/{publisher}/{name}/versions/{version}/archive` with gzip bytes | `VersionRecord` plus `{status:"ready",upload_url}`                                   |

`VersionRecord` is `{manifest,sha256,size,download_url}`. Generated URLs use the
configured registry origin. Public reads include only ready versions owned by
active publishers and accounts, with quarantine disabled. `latest_version` means
most recently published, not highest semver. Clients select compatible semver
versions after following all version-page cursors. Catalog cursors are
`publisher/name`; version cursors are exact version strings. Both are opaque to
clients and use stable lexical ordering.

Catalog pages default to 20 entries and allow at most 50. Search is at most 128
bytes. Version pages default to and allow at most three complete manifests, which
keeps each response below 64 KiB. A publisher can reserve at most 1,000 versions.
Unknown packages return 404. A reserved version stays hidden until its archive
passes validation and D1 marks it ready.

## Publication and archive isolation

1. Sign in and claim one permanent publisher handle for the account. Reserved
   administration names are blocked. Handles cannot be renamed or transferred.
2. Reserve an exact version with a validated manifest, lowercase SHA-256, and
   archive length. D1 stores these fields permanently.
3. Upload at most 8 MiB. The Worker bounds the request stream, checks size and
   hash, invokes the shared package validator, and compares the complete archive
   manifest with the reserved manifest.
4. R2 writes use create-once conditional storage. An identical owner retry can
   repair a failure after R2 storage and before D1 finalization, without replacing
   bytes. Existing size/hash metadata must match. R2 permissions stay private to
   the registry Worker and approved operators.
5. D1 finalization rechecks active ownership and quarantine. SQL triggers also
   reject publication after account or publisher suspension. Concurrent uploads
   preserve one immutable object.

The shared validator rejects links, traversal, absolute or hidden paths,
duplicates, case conflicts, file/directory ancestor conflicts, absent declared
assets, more than 1,024 entries, and more than 32 MiB expanded content. Archives
are parsed in memory and never executed. R2 object names derive only from
validated namespace and semver components. No upload can select a bucket, host,
arbitrary object key, or local path.

Validation does not establish that package code is safe. There is no malware
scanner, dependency audit of uploaded code, code review, publisher identity
guarantee, or sandbox guarantee. Native hosts must retain explicit enablement,
effect admission, runtime limits, and their documented execution restrictions.
Do not describe registry publication as trusted execution.

## Authentication, sessions, and browser safety

Better Auth 1.7.10 owns OAuth, device codes, session tokens, cookies, and
revocation. GitHub is the sole login provider. Account linking is disabled.
Cookies are secure, HttpOnly, host-only, and SameSite=Lax. Session cookie caching
is disabled, so normalized identity requests check D1 revocation. The Rust Worker
returns only the subject, display name, and operator boolean from the private
auth service. It never accepts public operator headers.

The first-party client ID is `ejm-cli`. Its optional access label is exactly
`registry`. Device grants expire after ten minutes and require at least five
seconds between polls. The CLI starts `/api/auth/device/code` and polls
`/api/auth/device/token` with the maintained device grant type. Browser approval
loads the server-checked client, access label, owner-bound code, and pending
state before enabling a confirmation checkbox. Users must have started the
request on a device they control. Never approve a code sent by another person.

The consent plugin uses Better Auth's atomic adapter operation to commit only
the first pending, owner-matched decision. Concurrent approval and denial cannot
overwrite each other. Better Auth still owns input parsing, session checks,
expiry, polling, and single-use redemption. The local regression fixture covers
the unguarded consent writes in Better Auth 1.7.10; retain it when updating that
dependency.

The device token is a full, revocable session for this registry application.
The displayed access label does not restrict its authority. It can claim the
account's publisher name and publish under that account. An operator's device
session also carries that account's existing moderation authority. Sessions use
Better Auth's seven-day expiry and daily renewal policy. The account page can
revoke every other session, including CLI sessions.
Local CLI credentials must stay in private, origin-bound storage.

Cookie mutations require the exact configured Origin. CLI bearer requests allow
an absent Origin or that same origin. An explicit malformed bearer never falls
back to cookies. Only required auth endpoints are proxied, and `/internal/*` is
never exposed through the public Worker. The auth Worker has no public route,
workers.dev endpoint, or preview URL. Keep that configuration intact.

The browser uses external same-origin scripts and styles, text-only rendering of
publisher content, no third-party assets, a restrictive CSP, frame denial,
no-referrer policy, and no CORS grants. OAuth authorization is the sole external
navigation, checked against GitHub's exact HTTPS authorization endpoint. Do not
enable request/response body logging or URL logging of device and callback
queries.

## Quotas and spend bounds

| Bound                         | Initial limit                                    | Enforcement                                         |
| ----------------------------- | ------------------------------------------------ | --------------------------------------------------- |
| Anonymous API/auth requests   | 120 per IP per minute                            | Cloudflare location rate limiter, approximate       |
| Publisher claims              | 5 per account per hour                           | Atomic D1 counter                                   |
| Version reservations          | 20 per account per hour                          | Atomic D1 counter                                   |
| Archive upload attempts       | 40 per account per hour and day                  | Atomic D1 counters                                  |
| Uploaded bytes per account    | 64 MiB per 24-hour window                        | Atomic D1 counter charged before reading            |
| Publisher packages            | 100                                              | D1 insertion trigger                                |
| Publisher reserved versions   | 1,000                                            | D1 insertion trigger                                |
| Publisher reserved bytes      | 256 MiB                                          | D1 insertion trigger                                |
| Global registry accounts      | 20,000                                           | D1 insertion trigger                                |
| Global publishers             | 5,000                                            | D1 insertion trigger                                |
| Global reserved versions      | 20,000                                           | D1 insertion trigger                                |
| Global reserved archive bytes | 1 GiB                                            | D1 insertion trigger                                |
| Global upload attempts        | 2,000 per 24-hour window                         | Atomic D1 counter                                   |
| Global uploaded bytes         | 256 MiB per 24-hour window                       | Atomic D1 counter charged before reading            |
| Auth requests                 | 60 per IP per minute, tighter device/login rules | Better Auth database rate limiter plus edge limiter |

Counters use fixed windows starting with their first accepted attempt. All
pending reservations count toward permanent storage limits. Retries and
failed uploads consume rate and byte budgets. The global budget prevents creating
new accounts to bypass the archive spending cap. Expired counter rows are pruned
in indexed batches of at most 64. Counters never store raw tokens. D1 failure or
a missing global upload settings row denies publication. The required singleton
`registry_limits` row cannot be deleted. Reviewed migrations may change its
positive limits. There is no public limits API.

These caps bound archive growth and accepted upload work. They do not cap all
Cloudflare charges: request traffic, D1 reads, auth users/sessions, OAuth abuse,
and downloads can still incur charges. Set account billing notifications, WAF
rules, and a documented shutdown procedure before public launch. No background
job cleans permanent versions or promises account erasure.

## Operator access and abuse response

`OPERATOR_SUBJECT_IDS` is an exact comma-separated allowlist of immutable Better
Auth user IDs. It is empty by default. Configure it only after verifying the
operator's subject through an authorized sign-in. A display name, GitHub handle,
email, bearer text, request header, or database account flag cannot grant this
role. Treat changes to the private auth Worker and its allowlist as privileged
production changes.

Operator mutations require a live, active operator session and a nonempty reason:

| Request                                                   | Effect                                |
| --------------------------------------------------------- | ------------------------------------- |
| `POST /v1/operator/accounts/{subject}`                    | Suspend or restore a registry account |
| `POST /v1/operator/publishers/{handle}`                   | Suspend or restore a publisher        |
| `POST /v1/operator/versions/{publisher}/{name}/{version}` | Quarantine or restore a version       |

Each takes `{disabled:boolean,reason:string}`. The target update and private
audit log run in one D1 batch transaction. A suspended operator account cannot
write the audit log. Moderation is limited to 100 attempts per operator per hour.
Quarantine does not delete, replace, or reuse a version. A missing target can
still produce an audit entry; inspect the target record when handling abuse.

For a reported package, record its exact name, version, hash, evidence, and
reporter contact through the established private operations channel. Quarantine
the version or suspend the publisher/account as needed. Verify a new public
metadata/download request returns 404. Preserve the audit entry and immutable
archive for investigation. Restore only after review. Do not put private reasons,
credentials, or unsafe archive contents into public responses or logs.

Registry suspension blocks registry mutations and hides its packages. It does
not delete the Better Auth user or prevent GitHub login. Revoke affected auth
sessions separately when responding to account compromise. Rotation of an OAuth
secret does not revoke already issued registry sessions. Rotate
`BETTER_AUTH_SECRET` through the secret manager when needed, plan for cookie and
session disruption, and explicitly revoke affected sessions in auth D1 through
reviewed maintenance. Revoke a compromised GitHub OAuth credential at GitHub and
replace it in the private auth service. Never paste secrets into chat, CLI
arguments, source, fixture files, or logs.

## Deployment prerequisites and acceptance

Remote changes require explicit owner authorization. Before a deployment:

1. Choose an authorized HTTPS registry origin and set `REGISTRY_ORIGIN` to that
   exact origin in both Worker configurations. The source supplies no default.
   Create separate `ejm-registry` and `ejm-registry-auth` D1 databases and
   replace placeholder UUIDs in their Wrangler files. Apply the reviewed registry
   and generated auth migrations to the matching databases.
2. Create `ejm-registry-packages` as a private R2 bucket. Disable public bucket
   access and public custom domains. Restrict deployment and storage credentials
   to the required Workers, D1 databases, and bucket.
3. Register a GitHub OAuth application with a callback at
   `/api/auth/callback/github` on the chosen registry origin. Store
   `GITHUB_CLIENT_ID`, `GITHUB_CLIENT_SECRET`, and a fresh high-entropy
   `BETTER_AUTH_SECRET` through Wrangler's hidden secret prompt or the Cloudflare
   UI. The auth Worker alone receives these secrets.
4. Deploy the private auth Worker with no public route, workers.dev, or preview
   URL. Bind it only to the Rust registry Worker. Set the approved operator
   subject allowlist, or keep it empty until an operator is verified.
5. Build the Rust Worker with worker-build 0.8.7 and review the dry-run bundle.
   Add the authorized custom domain for the chosen origin to the public Worker
   only during the approved deployment. No domain route is preconfigured here.
6. Confirm exact origin, TLS, private binding isolation, database migration
   status, private bucket access, billing alerts, and response/log redaction.
7. With owner authorization, test real GitHub login, publisher claim, CLI device
   approval and denial, upload, interrupted upload retry, public download hash,
   session revocation, quarantine, and suspension. Record these separately from
   local fixture evidence. Confirm cookie flags and CSP in a real browser.

Run `npm audit --audit-level=moderate` for the standalone registry lock and
`cargo deny --manifest-path registry/Cargo.toml check` for Rust dependencies.
Review advisories and update maintained dependencies deliberately. In particular,
do not downgrade Better Auth below fixes for device-code consent and OAuth state
validation. No advisory exceptions are included here.

Primary references: [Workers Rust](https://developers.cloudflare.com/workers/languages/rust/),
[Better Auth device authorization](https://better-auth.com/docs/plugins/device-authorization),
[Better Auth bearer sessions](https://better-auth.com/docs/plugins/bearer),
[R2 conditional writes](https://developers.cloudflare.com/r2/api/workers/workers-api-reference/),
and [Cloudflare test integration](https://developers.cloudflare.com/workers/testing/vitest-integration/write-your-first-test/).
