# Verify a site

Read this before you claim a site looks right locally or is live. Local checks, the deploy and
live observations are separate results. Report each one, and call anything you did not check
unverified.

## Local visual check

Serve the built `dist` on a loopback port. Use the repository's preview port and command when
it has one (ejmlabs.com uses 4827):

```sh
python3 -m http.server 4827 --directory dist --bind 127.0.0.1
```

Run it in the background, then look at the pages:

- Each changed route at 375 px and desktop width, in light and dark. A repository skill may
  add widths (the IDE site also checks 320, 768 and 1440).
- No horizontal overflow: `document.documentElement.scrollWidth` equals `clientWidth` at
  375 px.
- Keyboard: the skip link first, visible focus, the theme toggle reachable.
- Static text and links work with scripts off where the site promises that.
- The console is empty.

Stop the server with a bracketed pattern:

```sh
pkill -f 'http[.]server 4827'
```

`pkill -f` matches full command lines, including the shell that runs pkill, because that
shell's command line contains the pattern. With `http.server` unbracketed, pkill kills its own
shell and the rest of the command. `[.]` still matches the server's `http.server`, but the
shell's own text `http[.]server` does not match it.

A plain static server does not apply `_headers`, so it shows no CSP errors. To see the headers
and CSP in the console before a deploy, run the Worker locally instead:

```sh
npx wrangler dev --config wrangler.jsonc --ip 127.0.0.1 --port 8787 --local
```

Sites with Worker routes (downloads, release metadata) use `wrangler dev --local` for every
local check, with local R2 and disposable fixtures only.

## Why live checks need a headed browser

Cloudflare bot protection on the ejmlabs zones answers curl and headless Chrome on HTML routes
with a 403 "Just a moment..." challenge page. Downloads, `/install.sh`, `/releases.json` and API
routes are not challenged and return 200 to curl. So:

- Check HTML routes in a headed browser: the user's Chrome through the browser extension, or
  a devtools driver started in headed mode.
- A 403 challenge from curl or a headless browser says nothing about the deploy. Do not report
  it as a failure, and do not count it as a pass.
- Machine routes may be checked with `curl -sSI`. That is also how to confirm machine clients
  still work after a rule change.
- Never add a skip rule, disable bot protection or widen an allowlist to make a check pass.
  Blanket browser challenges must never cover APIs, downloads, webhooks or service traffic.
- A new custom domain can look unreachable from a machine that cached the DNS miss. Wait for
  propagation, or try another network, before calling it a failure.

## Live check

After the deploy, over HTTPS on the canonical host:

1. Every changed route loads with status 200 and the new copy. Read the text, do not assume it.
2. Navigation and links on the changed pages go where they should.
3. A missing route returns the 404 page with status 404.
4. Each download returns 200 and the bytes the build produced (size, and SHA-256 where listed).
5. The response headers match the build (below).
6. The console is empty on each changed route.
7. Unwanted alternate hosts (`workers.dev`, preview URLs) do not serve the site.

## Compare live headers with `dist/_headers`

`dist/_headers` is the build's record of what each path should send. Compare each header it sets
for the path with the live response. In the headed page, after the challenge has passed, a
same-origin fetch reads the real response headers:

```js
const r = await fetch(location.pathname, { cache: "no-store" });
Object.fromEntries(r.headers);
```

Or read the document request in the browser's network panel. Then:

- Each header `_headers` sets for the path appears once, with the same value. A value joined by
  a comma means two rules set the same header (see [headers.md](headers.md)).
- Pages and unhashed files send `Cache-Control: public, max-age=0, must-revalidate`. Hashed
  files under `/assets/` send the immutable rule.
- Ignore headers Cloudflare adds itself, for example `cf-ray`, `cf-cache-status` and `server`.
- Responses a Worker generates (API and download routes) carry the headers their code sets, not
  `_headers`. Check those against the Worker's tests and docs.

If the zone injects the Cloudflare Web Analytics beacon and the CSP does not allow it, the
console shows CSP errors for the beacon. Either the site allows the two analytics origins in its
shared header source, or the owner turns off injection for that host. Never fix it with
`'unsafe-inline'`.

## Report

State, separately:

- Local: the commands that ran and their results, and which widths and themes you looked at.
- Deploy: the Worker name, commit, and previous and new version ids.
- Live: the routes, downloads and headers you checked, in which browser, and what you saw.
- Not checked: everything else, named.
