# Gallery deployment

The canonical host is `https://ui.ejmlabs.com`. The static Worker is
`ejmlabs-ui`; workers.dev and version preview URLs are disabled. The gallery
has no API, session service or analytics beacon. It uses the shared security
headers emitted by `@ejm/site`.

From the reviewed source commit, install the locked dependencies, then run:

```sh
npm ci --ignore-scripts
npm run check
npm run build -w @ejm/gallery
npm run budget -w @ejm/gallery
```

The existing Cloudflare account must own the domain and the deploying identity
must have Worker/custom-domain rights. Review a Wrangler dry run against
`apps/gallery/wrangler.jsonc`, then deploy that exact built directory with the
approved account. The repository's protected source review gates apply before
publication. A local build or source PR does not publish the site.

After publication, verify the canonical host, shared headers, fonts and original
OFL notices, 404 handling, complete no-script docs, keyboard navigation and
light/dark phone/desktop layouts. Retain the source SHA and Worker version ID.
For rollback, restore a previously accepted immutable Worker version and verify
the same host and routes; do not serve a source branch with failed gates.
