---
name: harness-site
description: Change or deploy the static harness.ejmlabs.com site in site/, including the ejmlabs design kit copy, shared headers, honest release claims, and Cloudflare publishing.
---

# harness.ejmlabs.com site

The site is a static, assets-only Cloudflare Worker named `ejmlabs-harness` on the custom
domain `harness.ejmlabs.com`. Its config is `site/wrangler.jsonc`. It is separate from the
`registry/` Workers and has no backend, database, or secrets.

## Follow the kit

The ejmlabs design kit at `../ejmlabs/design-kit` is canonical and its rules change. Read
its current docs before editing the page instead of relying on this summary: `SITES.md`,
`CATALOG.md`, `web/PATTERNS.md`, and `UI.md`.

- Run `scripts/sync-design-kit.sh` to copy the kit files the site needs into
  `site/public/design-kit`. Never hand-edit that copy. It is excluded from Prettier.
- Build the page from the kit's patterns and components. Do not write page CSS or inline
  styles. A missing pattern is added to the kit, not to this site.
- Validate the page with the kit's checker, `node ../ejmlabs/design-kit/scripts/check-site.mjs`
  on the built HTML, and follow the kit's rules for shared header, theme toggle, motion,
  analytics, and project demos.
- The kit's color balance applies: Sage leads, Paper and Mist carry the page, Forest is a rare
  accent. The ejmlabs wordmark is vector artwork and is never retyped. Sage is not text color.
- Headers come from the kit (`web/_headers`), not from harness-specific choices. Keep a
  strict policy and loosen a directive only for a concrete need the kit approves.
- Another session adopts the kit across the ejmlabs project sites and may be editing `site/`.
  Check `git status`, coordinate with it, and commit only your own paths.

## Claims and copy

- State availability honestly. This is a source release, not published or packaged, until the
  user says otherwise. Do not show install commands for packages that do not exist.
- Cite benchmark numbers only with their caveats from [the benchmark notes](../../../docs/benchmarks.md):
  one local run, the host, and that the four-core target is not proven.
- Describe native execution as not a sandbox. No em dashes. Use plain, direct prose.
- Code samples use the real public API names. Do not invent a repository URL or email.

## Check

Run Prettier on changed files. Check the page at 375 px and desktop widths, in light and dark,
and confirm `scrollWidth` equals `clientWidth` at 375 px. Check the console for policy errors,
for example through `wrangler dev -c site/wrangler.jsonc`.

## Deploy

A push to main deploys the site. `.github/workflows/deploy.yml` is the canonical caller of the
shared `deploy-site.yml` in `ejmlabs/.github`, pinned with `pr.yml` and `release.yml`. Never edit
it by hand: the Repository standards check requires it to match `scripts/standards.py` there
except for the pin. The merge, after the gate and its required checks, is the human decision.
Nobody approves a deploy afterward. The run then:

1. Builds with `contents: read` and no secrets. The site has no build step, so the job archives
   `site/wrangler.jsonc` and `site/public` from the main commit.
2. Deploys staging with `wrangler deploy --env staging`: the Worker `ejmlabs-harness-staging` at
   `staging.harness.ejmlabs.com`.
3. Deploys production once staging succeeds, with the same files and the top-level config: the
   Worker `ejmlabs-harness` at `harness.ejmlabs.com`.

The deploy takes `site/wrangler.jsonc` from the reviewed commit and refuses a build whose copy
differs, so change the config in source. Keep `env.staging` with its own Worker name and routes,
or staging would inherit the production domain. Deploy jobs run Wrangler 4.153.0, pinned in
`deploy/package.json` of `ejmlabs/.github`, not the `registry/` dependency. They use the
`CLOUDFLARE_API_TOKEN` secret of the `staging` and `production` GitHub environments.

Never run `wrangler deploy` from a workstation, create a token or change the environments. A
local `wrangler deploy -c site/wrangler.jsonc --dry-run` is fine.

### Owner setup before the first deploy

The shared deploy fails closed until the owner has done these:

- Run `python3 scripts/set_deploy_token.py` in `ejmlabs/.github`. It creates the `staging` and
  `production` environments, each with a custom deployment branch policy of exactly `main`, and
  stores the `CLOUDFLARE_API_TOKEN` secret in each, for every site repository at once. The token
  needs Worker and custom-domain rights on the account that owns `ejmlabs.com`.

If a DNS record for `staging.harness.ejmlabs.com` exists outside Wrangler, Wrangler prompts in CI
and nobody can answer. Stop and give the owner the exact command to run.

### After each merge

Staging is public and serves the latest main. Finishing through ship covers the live check:
confirm the deploy run succeeded, check staging over the edge, then production the same way.
Record the source SHA and the Worker version ID. A freshly created custom domain can be
unreachable from a machine that cached a miss, so test with `curl --resolve` against another
ejmlabs.com edge address before assuming a failure. Do not touch the other ejmlabs Workers.

To roll back, redeploy an earlier Worker version in Cloudflare, or revert on main. Never serve a
source branch with failed gates.
