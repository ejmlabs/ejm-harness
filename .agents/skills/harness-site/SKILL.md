---
name: harness-site
description: Change the Harness public site, its source guides, shared ejm-ui composition, headers, demos and Cloudflare publication.
---

# Harness public site

The static site is the assets-only Cloudflare Worker `ejmlabs-harness` at
`harness.ejmlabs.com`. `site/wrangler.jsonc` serves the prerendered `site/dist`.
The separate registry Workers do not share its deployment, databases or secrets.

## Shared owner

Use public ejm-ui packages for tokens, layout, navigation, motion, docs,
prerendering, demos, assets and headers. Read its `AGENTS.md`, `QUALITY.md` and
site guidance. Check out the exact `.github/ui-source.json` revision beside this
repository. Do not import private sibling files, add local visual rules or
restore copied design-kit output. Extend the shared owner when a contract is
missing; keep product copy and composition here.

Legacy `site/public/design-kit` assets remain unchanged source history. Vite
sets `publicDir: false`, so they are not part of the new site or its design
authority. Do not sync or import that copy.

Use the fewest words that preserve meaning. Prefer a clear visual or no added
content when context already explains the task. Keep meaningful labels,
accessible names, honest state, security facts and recovery steps. Reveal
supporting detail when needed; operational guides remain complete.

## Content and playback

Edit guides in `site/content`. The shared compiler and docs components render
them. Keep published routes, heading anchors and README links working, including
`/docs/host-auth/`. Describe public hosted source rather than unfinished local
runtime. Keep package and native acceptance limits explicit.

Use one shared autoplay player in the hero. Pause, Resume and Restart sit
outside the inert native surface. Keep useful final output in server HTML.
The recorded offline worker output is historical playback, not a live model or
proof of an exact source build. Its CPU and memory figures are reservations.
Do not add terminal input, a harness dashboard or another playback engine.

Cite performance numbers with the host, workload and limits in
[the benchmark guide](../../../docs/benchmarks.md). Native execution is not a
sandbox. Code examples must use real public APIs. Never invent an installable
package, source URL or availability claim.

## Checks and publication

After installing pinned sibling UI and site dependencies, run
`npm --prefix site run check` and `npm --prefix site run budget`, then the normal
repository gate. Keep existing transfer limits. Check narrow and desktop widths,
light and dark themes, console errors and horizontal overflow. A changed shared
contract also needs its owner and affected consumer checks.

Publication needs user authorization and protected source acceptance. Run
`wrangler deploy -c site/wrangler.jsonc` from the accepted tree and verify public
routes, legacy anchors, assets and headers over the edge. A local build alone is
not publication. Do not touch the registry or another Worker as part of this
static upload.
