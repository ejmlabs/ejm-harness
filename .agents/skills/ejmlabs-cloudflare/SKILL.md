---
name: ejmlabs-cloudflare
description: Use when you build, preview, deploy, verify or roll back a Cloudflare Worker or static site, including wrangler.jsonc, _headers and CSP, budgets, builds against pinned ejm-ui, live checks behind bot protection, release metadata Workers, R2 downloads.
---

# Cloudflare Workers and static sites

What is live must be a reviewed main commit, built from clean clones, checked, and seen working
in a real browser. A repository skill may add stricter rules, never weaker ones.

## Hosting

- Cloudflare first: Workers with static assets for sites; Workers, R2, D1, KV and service
  bindings for backends. Another cloud needs a reviewed host decision recorded in the repository.
- Each site or service has its own Worker, named in its wrangler config. Change only that Worker;
  never touch other Workers, zones, DNS or account settings.
- With a `.github/workflows/deploy.yml` caller, CI deploys: each push to main builds without
  secrets, deploys staging, then production once staging succeeds; never deploy by hand then.
  Without the caller, deploy by hand with the owner's wrangler login; never create a token.

## Config

- `wrangler.jsonc` sits beside what it deploys. Set `account_id`, and `workers_dev: false` and
  `preview_urls: false` in every environment. Production routes use `custom_domain: true`.
- `assets.directory` is build output (usually `./dist`), never a source folder.
- Secrets go in with `wrangler secret put`, never in `vars`, the config, or assets.
- A preview environment stays disabled until its Access policy is enforced and verified at its
  real hostname.
- The wrangler pin, 404 handling, `observability` and `compatibility_date` rules are in
  [references/build-and-deploy.md](references/build-and-deploy.md).

## Public assets and headers

- Everything in the assets directory is public. Build it from named allowlists and list it before
  every deploy; remove anything you would not publish (notes, `.env`, keys, logs, archives).
- Take `_headers` from the shared source (`ejm-site-headers` in `@ejm/site`); never edit the
  generated file. A site not yet on ejm-ui keeps a reviewed `public/_headers` that matches
  [references/headers.md](references/headers.md).
- Strict CSP: `'self'` plus named origins, never `'unsafe-inline'` or `'unsafe-eval'` in
  `script-src`. Loosen one directive only for a concrete need, in the header source, and say why.
  `_headers` covers static assets only; Worker responses set their own headers in code.

## Budgets

- Every site has a budget that fails a check (`ejm-ssg budget` or `npm run perf`), measured on the
  built site. Never raise a budget or re-record a baseline to make your change pass: trim, or load
  later. Changing a budget is its own reviewed change with the measurement and the reason.

## Build and deploy

- Deploy only the merge commit on main, never a working tree, branch or old build. With CI
  deploys, check staging live, then production.
- Build in a fresh scratch directory from clean clones: the site at the merge commit and ejm-ui
  at the revision the repository pins (`.github/ui-source.json`). Never build from a sibling
  checkout; it may hold uncommitted work.
- Run the repository's check, a `--dry-run`, then `wrangler deploy --config wrangler.jsonc`,
  stopping at the first failure. Record the live version id before and the new one after, with
  the commit SHA.
- If wrangler asks to replace a DNS record, stop and give the user the exact command to run.

## Verify

- Serve the built `dist` on loopback and look at it before you deploy. After the deploy, follow
  the live check in [references/verify.md](references/verify.md).
- Bot protection challenges curl and headless browsers on HTML routes. Check HTML in a headed
  browser; a challenge is neither a pass nor a failure. Never weaken WAF, bot or Access rules to
  make a check pass.
- Report local checks, the deploy (version id) and live observations separately.

## Releases and rollback

- Download lists and release metadata change only through a reviewed change deployed from main.
  Never replace a listed file or overwrite a content-addressed object. See
  [references/release-metadata.md](references/release-metadata.md).
- Roll back with `wrangler rollback VERSION_ID` to the last verified version, under the same
  authorization as a deploy, then land a revert on main and deploy it. A rollback never restores
  R2, D1 or KV data.

## Read next

- [references/build-and-deploy.md](references/build-and-deploy.md): config details, clean-clone
  build, deploy commands, version ids, rollback.
- [references/verify.md](references/verify.md): local preview, the live check, live headers.
- [references/headers.md](references/headers.md): the header baseline, approved CSP additions.
- [references/release-metadata.md](references/release-metadata.md): release lists, download
  Workers, manifest rollback.
