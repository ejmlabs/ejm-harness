---
name: ejmlabs-cloudflare
description: Use when you build, preview, deploy, verify or roll back a Cloudflare Worker or static site, including wrangler.jsonc, _headers and CSP, bundle or load budgets, builds against pinned ejm-ui, wrangler deploy, live checks behind bot protection, release metadata Workers, R2 downloads.
---

# Cloudflare Workers and static sites

What is live must be a reviewed main commit, built from clean clones, checked, and seen working
in a real browser. A repository's own skill may add a stricter rule, never a weaker one.

## Hosting

- Cloudflare first: Workers with static assets for sites; Workers, R2, D1, KV and service
  bindings for backends. Use another cloud only for a concrete capability Cloudflare lacks,
  after a reviewed host decision recorded in the repository.
- Each site or service has its own Worker, named in its wrangler config. Deploy and change only
  that Worker. Never touch other Workers, zones, DNS or account settings.
- CI holds no deploy credential. Deploys use the owner's wrangler login. If auth fails, ask the
  user to run `wrangler login`. Never retry in a loop or create a token.

## Config

- `wrangler.jsonc` sits beside what it deploys. Set `account_id`, so a deploy cannot land in
  another account. Set `workers_dev: false` and `preview_urls: false` in every environment.
  Production routes use `custom_domain: true`.
- Pin wrangler as an exact devDependency of the package that owns the config, run it from that
  package (`npx wrangler`), and point `$schema` at `./node_modules/wrangler/config-schema.json`.
- `assets.directory` is build output (usually `./dist`), never a source folder. Set
  `not_found_handling` and ship a 404 page.
- Secrets go in with `wrangler secret put`, never in `vars`, the config, or assets.
- Keep `observability` off unless a reviewed need exists. Logs can hold request content.
- Change `compatibility_date` only in its own reviewed change, with the tests run.
- A preview environment stays disabled until its Access policy is enforced and verified: an
  anonymous request is denied and an authorized person gets in, at its real hostname.

## Public assets and headers

- Everything in the assets directory is public. Build it from named allowlists. Before every
  deploy, list it and remove anything you would not publish: notes, generators, `.env`, keys,
  logs, local paths, source archives.
- Take `_headers` from the shared source (`ejm-site-headers` in `@ejm/site`) and never edit the
  generated file. A site not yet on ejm-ui keeps a reviewed `public/_headers` that matches the
  baseline in [references/headers.md](references/headers.md).
- Strict CSP: `'self'` plus named origins, no `'unsafe-inline'` or `'unsafe-eval'` in
  `script-src`, `object-src 'none'`, `frame-ancestors 'none'`, `form-action 'none'`. Loosen one
  directive only for a concrete need, in the header source, and say why.
- `_headers` covers static assets only. Responses a Worker builds set their own headers in code.

## Budgets

- Every site has a budget that fails a check: byte budgets (`ejm-ssg budget`) or a load-time
  baseline (`npm run perf`). Measure the built site, not the dev server.
- Never raise a budget or re-record a baseline to make your change pass. Trim, or load later.
  Changing a budget is its own reviewed change with the measurement and the reason.

## Build and deploy

- Deploy only after the PR merges to main, from the merge commit. Never deploy a working tree,
  a feature branch, or a build you did not just make. Deploy is part of finish through ship
  unless the repository marks production deploys approval-gated; then wait for the user's
  explicit request each time.
- Build in a fresh scratch directory from clean clones, side by side: the site repository at
  the merge commit and ejm-ui at the revision pinned in the repository (`.github/ui-source.json`).
  The `file:` dependencies then resolve to the clean ejm-ui clone. Never build from the sibling
  checkout: it is often on a feature branch with uncommitted work, which breaks or skews builds.
- Run `npm ci --ignore-scripts` in both, the repository's check (typecheck, build, tests,
  budgets), a `--dry-run`, then `wrangler deploy --config wrangler.jsonc` from the site
  directory. Stop at the first failure.
- Record the live version id before the deploy and the new Worker version id wrangler prints
  after it. Both go in the report with the commit SHA.
- If wrangler asks to replace a DNS record, stop: the custom domain mapping changed. Give the
  user the exact command to run in their own terminal.

## Verify

- Locally, serve the built `dist` on a loopback port and look at it before you deploy. Stop the
  server with a bracketed `pkill -f` pattern, so it cannot match the shell running pkill.
- Cloudflare bot protection returns a 403 "Just a moment" challenge to curl and headless
  browsers on HTML routes. Downloads and machine routes return 200. Check HTML in a headed
  browser. A challenge page is neither a pass nor a failure.
- Live, check every changed route and its copy, the 404, downloads, and that the response
  headers match the built `dist/_headers`.
- Never weaken WAF, bot or Access rules to make a check pass.
- Report local checks, the deploy (version id) and live observations as separate results.

## Releases and rollback

- Download lists and release metadata change only through a reviewed change deployed from main.
  Never replace a listed file, overwrite a content-addressed object, or upload a fake release.
  See [references/release-metadata.md](references/release-metadata.md).
- Roll back a bad deploy with `wrangler rollback VERSION_ID` to the last verified version, under
  the same authorization as a deploy. Then land a revert on main and deploy it, so main matches
  live. A rollback restores code, assets and config, never R2, D1 or KV data.

## Read next

- [references/build-and-deploy.md](references/build-and-deploy.md): clean-clone build, deploy
  commands, version ids, rollback steps.
- [references/verify.md](references/verify.md): local preview, live checks in a headed browser,
  comparing live headers with `_headers`.
- [references/headers.md](references/headers.md): the header baseline, approved CSP additions,
  Cloudflare header behavior.
- [references/release-metadata.md](references/release-metadata.md): static release lists,
  download Workers on private R2, manifest rollback.
