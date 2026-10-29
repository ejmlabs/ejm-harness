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

A push to main runs `.github/workflows/deploy.yml`, the canonical caller of the shared
`ejmlabs/.github` workflow `deploy-site.yml`. Its build job (no secrets, `contents: read`)
builds the site from that main commit. Its staging job then runs `wrangler deploy --env staging`,
which publishes the Worker `ejmlabs-harness-staging` at `harness-staging.ejmlabs.com`. Its
production job deploys the same built files with the top-level config only after the required
reviewer (@everettmorgan) approves the run in the GitHub `production` environment.

Approving the production job is the explicit authorization for each publication. Do not run
`wrangler deploy` from a workstation; a local `wrangler deploy -c site/wrangler.jsonc --dry-run`
is fine. A newer merge cancels an older production deploy that is still waiting. Deploy jobs run
Wrangler 4.151.3 pinned in the shared repository, not the `registry/` dependency, and use the
`CLOUDFLARE_API_TOKEN` secret of the `staging` and `production` GitHub environments.

Staging is public and serves the latest main. After each merge, confirm the staging response over
the edge, and confirm production the same way after approval. A freshly created custom domain can
be unreachable from a machine that cached a miss, so test with `curl --resolve` against another
ejmlabs.com edge address before assuming a failure. Do not touch the other ejmlabs Workers.

To roll back, redeploy an earlier Worker version in Cloudflare, or approve an earlier run's
production job while it is still available.
