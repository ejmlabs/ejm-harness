# Headers and CSP

Read this before you change `_headers`, a CSP directive, caching, or the headers a Worker sets.

## Where headers come from

- Sites on ejm-ui: `@ejm/site` writes `dist/_headers` with `ejm-site-headers` during the build.
  Never edit the generated file or keep a copy in `public/`. Change the options, or the shared
  owner in ejm-ui, and rebuild. Check it with `ejm-site-headers dist/_headers --check` where the
  site's check script does not already run it.
- Sites not yet on ejm-ui keep a reviewed `public/_headers`, copied into `dist` by the build.
  It must meet the baseline below, and moves to the shared source when the site adopts ejm-ui.
- Responses a Worker builds (API, download and redirect routes under `run_worker_first`) do not
  get `_headers`. Set `X-Content-Type-Options`, `Cache-Control` and any CSP in the Worker code,
  and test them.

## Baseline

Every static site sends at least this. It matches the shared generator's output, minus the
additions listed below.

```text
/*
  Content-Security-Policy: default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self'; font-src 'self'; connect-src 'self'; object-src 'none'; base-uri 'self'; form-action 'none'; frame-ancestors 'none'; upgrade-insecure-requests
  Permissions-Policy: camera=(), microphone=(), geolocation=()
  Referrer-Policy: strict-origin-when-cross-origin
  Strict-Transport-Security: max-age=31536000; includeSubDomains
  X-Content-Type-Options: nosniff
  X-Frame-Options: DENY

/assets/*
  Cache-Control: public, max-age=31536000, immutable
```

Stricter is always allowed: `default-src 'none'`, `base-uri 'none'`, `font-src 'none'`,
`Referrer-Policy: no-referrer`, `Cross-Origin-Opener-Policy: same-origin`, more denied
permissions.

## Approved additions

Add a source only for a feature the site really uses, and name the reason in the change.

| Need | Addition |
| --- | --- |
| Cloudflare Web Analytics | `script-src https://static.cloudflareinsights.com` and `connect-src https://cloudflareinsights.com` |
| Web workers (for example a WebGL scene) | `worker-src 'self'` |
| Shared components that set inline style attributes or data URI images | `style-src-attr 'unsafe-inline'` and `img-src 'self' data:`, owned by the shared generator; a hand-written file does not copy them without the same need |

Never add `'unsafe-inline'` or `'unsafe-eval'` to `script-src`, a wildcard, a bare scheme
(`https:`, `data:` in `script-src`), or a third-party origin for convenience. No inline scripts
or `<style>` elements; the theme boot script is a file.

## Cloudflare behavior

- When several `_headers` rules match a path and set the same header, Cloudflare joins the
  values with a comma. Two CSP values joined that way make a broken policy. Set each header in
  exactly one rule: security headers in `/*`, only `Cache-Control` in `/assets/*`.
- Cloudflare already sends `Cache-Control: public, max-age=0, must-revalidate` for pages and
  unhashed files. Add the immutable rule only for content-hashed paths. Never give
  `/install.sh`, `/releases.json`, the manifest or `theme-boot.js` a long cache.
- Zone features can change responses after the Worker: Web Analytics injection adds a beacon
  script, and bot protection serves its own challenge page. Neither belongs in `_headers`. If an
  injected script conflicts with the CSP, decide in the shared header source or the zone, never
  by loosening `script-src` to inline.
- Release metadata and download routes have their own cache rules. See
  [release-metadata.md](release-metadata.md).

## Checks

- Build, then read `dist/_headers`. It is the record you compare live headers with.
- `wrangler dev --local` applies `_headers`, so CSP violations show in the console before a
  deploy. A plain static server does not apply them.
- After a deploy, compare live with `dist/_headers` as [verify.md](verify.md) describes.
