---
name: harness-site
description: Change or deploy the harness.ejmlabs.com site in site/, built on ejm-ui with ejm-ssg, including its pin, honest release claims, and Cloudflare publishing.
---

# harness.ejmlabs.com site

The site is a static, assets-only Cloudflare Worker named `ejmlabs-harness` on the custom
domain `harness.ejmlabs.com`. Its config is `site/wrangler.jsonc`. It is separate from the
`registry/` Workers and has no backend, database, or secrets.

## Built on ejm-ui

`site/` is an ejm-ui site: `ejmlabs({ project: 'harness' })` is its whole Vite plugin list,
`tsconfig.json` extends `@ejm/ui/tsconfig.json`, and `src/styles.css` imports `@ejm/ui/styles.css`
and `@ejm/demos/styles.css`. Read ejm-ui's `ADOPTION.md` and `packages/ui/src/layout/README.md` at
the pinned revision before editing a page.

- The `@ejm/*` packages are `file:` links to a sibling `../ejm-ui` checkout at the revision in
  `.github/ui-source.json`. Run `npm ci --ignore-scripts` in that checkout first. Move the pin in
  its own PR, then regenerate `site/package-lock.json` against it.
- Pages are `Hero` and `Section` (or `DocsLayout` for a guide) from `@ejm/ui`. The design guard
  rejects `className`, `style`, local CSS and stylesheet links in `site/src`. A missing pattern is
  added to ejm-ui, never to this site.
- The product words and routes live here: `src/pages/Home.tsx`, `src/pages/HostAuthGuide.tsx`,
  and the route list in `src/site.tsx`. Keep guide prose inside `Static` so it stays out of the
  browser bundle.
- `ejmSite()` writes `_headers`, the favicon set and the theme script into `dist`. Do not keep a
  copy in `public/`. To extend the policy, pass `headers` to `ejmlabs()`.
- The build fails unless every page has one `h1`, ordered headings, the skip link first,
  `main#main`, and `rel="external"` on links to another host.

## Claims and copy

- State availability honestly. This is a source release, not published or packaged, until the
  user says otherwise. Do not show install commands for packages that do not exist.
- Cite benchmark numbers only with their caveats from [the benchmark notes](../../../docs/benchmarks.md):
  one local run, the host, and that the four-core target is not proven.
- Describe native execution as not a sandbox. No em dashes. Use plain, direct prose.
- Code samples use the real public API names. Do not invent a repository URL or email.

## Check

From `site/`: `npm ci --ignore-scripts`, then `npm run check` (types, the build with its page
checks, and `ejm-site-headers dist/_headers --check`). Run Prettier on changed files. Look at every
route at 375 px and desktop widths, in light and dark, and confirm `scrollWidth` equals
`clientWidth` at 375 px. Check the console for policy errors, for example by serving `site/dist`.
After a build, `node scripts/perf.mjs site/dist --baseline site/perf-baseline.json` from the
ejm-ui checkout fails when a route is a second or more slower than its recorded baseline.

## Deploy

A push to main deploys the site. `.github/workflows/deploy.yml` is the canonical caller of the
shared `deploy-site.yml` in `ejmlabs/.github`, pinned with `pr.yml` and `release.yml`. Never edit
it by hand: the Repository standards check requires it to match `scripts/standards.py` there
except for the pin. The merge, after the gate and its required checks, is the human decision.
Nobody approves a deploy afterward. The run then:

1. Builds with `contents: read` and no secrets: `npm ci --ignore-scripts` and `npm run build` in
   `site/` against ejm-ui at the pinned revision, then archives `site/wrangler.jsonc` and
   `site/dist`. The shared deploy must run this build step before the site can merge; until it
   does, a merge would publish an empty `site/dist`.
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
