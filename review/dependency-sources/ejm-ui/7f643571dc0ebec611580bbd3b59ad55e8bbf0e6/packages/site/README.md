# @ejm/site

The shared build and design boundary for every ejmlabs site: the project registry, Cloudflare Web Analytics, the shared head with the theme boot script, and the security headers. It sits next to `@ejm/ui` (components) and `@ejm/ssg` (prerender), and `examples/starter` shows all three together.

| Entry | What it is |
| --- | --- |
| `@ejm/site` | Re-exports `Analytics`, the analytics helpers, `siteDefaults`, `siteFonts` and the registry. |
| `@ejm/site/projects` | The four sites: `PROJECTS`, `PROJECT_LIST`, `otherSites(id)`, `sourceLinks(id)`, `assetPaths`. |
| `@ejm/site/analytics` | The `Analytics` component and `analyticsToken`, `loadAnalytics`, `optedOut`. |
| `@ejm/site/head` | `siteDefaults(id)` for `defineSite`, and `siteFonts()`. |
| `@ejm/site/headers` | `headersFile`, `csp`, `directives`: the policy as a typed generator. |
| `@ejm/site/docs` | Node-only Markdown compiler and static/live documentation split. |
| `@ejm/site/vite` | The `ejmSite({ project })` Vite plugin, with mandatory design ownership checks. |
| `ejm-site-headers` | CLI that writes or checks a `_headers` file. |

`projects`, `headers` and `vite` are plain `.mjs` files with `.d.mts` types, so Node can load them from `node_modules` (Node refuses to strip types from TypeScript there). `analytics` and `head` are TypeScript source that Vite compiles, like the other `@ejm` packages.

## Use it

```ts
// vite.config.ts
import tailwindcss from '@tailwindcss/vite';
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';
import { ejmSite } from '@ejm/site/vite';
import { ejmSsg } from '@ejm/ssg/vite';

export default defineConfig({
  plugins: [tailwindcss(), react(), ejmSite({ project: 'cli' }), ejmSsg({ site: 'src/site.tsx' })],
});
```

```tsx
// src/site.tsx
export default defineSite({ ...siteDefaults('cli'), App, routes: [/* ... */], notFound: { component: NotFound } });

// in App
<SiteLayout header={{ ..., nav }} footer={{ columns: [{ heading: 'Sites', links: otherSites('cli') }] }} linkComponent={Link}>{children}</SiteLayout>
<Analytics />
```

## Design ownership

`ejmSite({ project })` checks every consumer's `src` tree before bundling,
including unused stylesheets. The check is mandatory. Remove the obsolete
`enforceDesignKit` option from existing configurations; passing its former false
value does not disable the check. Dev checks new files and edits, then checks
once more when the initial watcher scan ends. Files found during startup receive the same checks. A local
override shows an error; correcting the source clears it. The check protects
source ownership; it is not a security sandbox or a browser layout proof.

Sites own content, routes, product data, semantic markup and accessible state.
`ejm-ui` owns visual recipes: typography, spacing, themes, motion, branding,
controls and reusable composition. Import public shared components rather than
styling a native wrapper. Add missing patterns to the shared owner, then rebuild
all consumers. A shared recipe change takes effect on those builds; it does not
publish any site by itself.

The source check accepts:

- Public CSS exports from `@ejm/ui`, `@ejm/tokens` and `@ejm/demos`, such as
  `@import "@ejm/ui/styles-site.css" source(none);`. The optional `source(none)`
  keeps unrelated installed package files out of the scan while retaining each
  public profile's explicit component sources. Consumer CSS files contain only
  these imports and comments. The exact `@source not "../content";` declaration may
  exclude authored guide text from the content scan.
- Named JSX content, route and state props through shared contracts. Keep
  heading semantics with `as` and choose a shared `SectionHead` purpose.
- `Static` imported from `@ejm/ssg` with literal `className="contents"`. The
  structural wrapper retains prerendered content. Imported aliases and
  namespaces work too.
- The public `docNavigation()` helper spread from `@ejm/ui/docs`. It returns
  route data rather than visual props. Other JSX spreads are opaque and fail.

It rejects local CSS rules and imports, theme or utility declarations, inline
styles, `<style>` elements, stylesheet links, raw markup through
`dangerouslySetInnerHTML`, consumer `className` or `class`, opaque JSX spreads,
and removed visual overrides such as `Hero.motion` or `SectionHead.size`.
Stylesheet imports in scripts must use public shared exports or a CSS entry
inside the consumer `src` tree. Put the site's own aggregate CSS import in the
entry module; it may import only shared CSS.

Errors include `src/file:line:column` and direct the author to the shared owner.
They also inspect imported component aliases, namespace imports, and header or
footer objects passed to `SiteLayout`. Use `identity` data for shared shell
branding. The reference starter shows these contracts together.

The library's own gallery, identified by its exact resolved path inside the
`ejm-ui` checkout, is a development surface for visual recipes. It is the only
non-consumer source root and does not create a flag or package-name exception
for downstream sites. The starter follows the same checks as all other
consumers. This is an engineering ownership check, not a security sandbox for
untrusted source. Normal review and type checks still apply.

Checks use Vite's public TSX parser and PostCSS in Node. Neither parser enters
browser code, and the check adds no browser dependency. Files are cached between
dev checks; changed and deleted files invalidate that cache.

## Native HTML applications

Applications that own native forms and DOM state use the public
`@ejm/site/native` Node build check with `@ejm/ui/static.css`. It checks authored
templates, presentation setters and finished route markup against the shared
native profile without adding a browser runtime. Native controls require their
shared recipes and buttons need an explicit type. Use `group` for related
content with React `AppGroup` spacing. See
[Native HTML applications](native-html.md) for composition, fixed recipes,
asset serving, privacy-sensitive themes, no-script forms and checks.

## projects

One registry feeds the analytics host map, the headers, the nav and the footers. Each entry has `id`, `name` (`ejm-cli`), `host`, `url`, `tagline`, `repo`, `analyticsToken` and `assetDir`. `otherSites(id)` returns the other three as `{ label, href, external: true }`, ready for the `nav` and footer `links` of `@ejm/ui`, which render them with `ExternalLink` (`rel="external noopener"`). `sourceLinks(id)` is the repo link.

Brand files: ejmlabs uses the root sets of `@ejm/tokens/assets` (`favicon/`, `logos/`, `social/`). cli, ide and harness each have their own family in `@ejm/tokens/assets/projects/<id>/` (marks, logos, icons, the favicon set, `apple-touch-icon.png`, `android-chrome-*`, `site.webmanifest`, `social-card`), imported from `design-kit/projects/` with sha256 in `assets/PROVENANCE.md`. The studio favicon family is generated in `@ejm/tokens`; its symbol geometry matches the studio logo. `assetPaths(project, exists)` points at that folder when it exists and falls back to the ejmlabs files otherwise; `ejmSite` serves the project's favicon set, manifest and social card at the site root.

## analytics

`Analytics` renders nothing. After hydration it adds `<script defer src="https://static.cloudflareinsights.com/beacon.min.js" data-cf-beacon='{"token":"..."}'>` when all of these hold: the host is `ejmlabs.com` or one of the three subdomains, `navigator.doNotTrack` and `window.doNotTrack` are not `"1"`, and `navigator.globalPrivacyControl` is not `true`. It never runs on `localhost`, previews or `workers.dev`, sets no cookies, and runs once. Nothing is sent from the server render, so the prerendered HTML never contains the beacon. The beacon follows client route changes by itself.

`analyticsToken(env)` and `loadAnalytics(env, doc)` take a fake `{ location, navigator, window }`, which is how the tests cover every host and every opt-out signal. To add a site, add it to the registry with its Cloudflare site token (tokens are public).

## headers

`ejmSite()` writes `_headers` into the build. `ejm-site-headers [file] [--check] [--no-analytics] [--list]` does the same from the command line, and `--check` exits 1 when the file differs from what this version writes (use it in CI for a site that commits its own `_headers`). The policy, ported from the old `design-kit/web/_headers`:

- `Content-Security-Policy`: `default-src 'self'`; `script-src 'self' https://static.cloudflareinsights.com` (no inline script: `theme-boot.js` is an external file); `connect-src 'self' https://cloudflareinsights.com`; fonts and images from `'self'` (images also `data:`); `object-src 'none'`, `frame-ancestors 'none'`, `form-action 'none'`, `base-uri 'self'`, `upgrade-insecure-requests`.
- Styles: `style-src 'self'` plus `style-src-attr 'unsafe-inline'`. The old kit allowed `'unsafe-inline'` for all styles. Components set CSS variables through `style` attributes (the entrance stagger index), so attributes stay allowed, and `<style>` elements do not.
- `Strict-Transport-Security: max-age=31536000; includeSubDomains`, `X-Content-Type-Options: nosniff`, `X-Frame-Options: DENY`, `Referrer-Policy: strict-origin-when-cross-origin`, `Permissions-Policy: camera=(), microphone=(), geolocation=()`.
- `Cache-Control: public, max-age=31536000, immutable` on `/assets/*` (Vite's hashed files). Pages and unhashed files (`theme-boot.js`, the manifest) keep Cloudflare's default `public, max-age=0, must-revalidate`. Cache-Control is not set on `/*` because Cloudflare joins a header set by two rules with a comma.

`ejmSite({ project, headers })` takes `headers: false` to skip the file, or `{ analytics: false, extra: { 'img-src': ['https://img.example'] } }` to change the policy.

## head

`siteDefaults(id, { description?, titleTemplate?, image? })` returns `{ origin, head, fonts }` for `defineSite`: the origin from the registry, `%s | ejm-cli` as the title template, the tagline as the description, Open Graph (`siteName`, `image: /social-card.png`, `type`), `twitter:card` and the image size, and the two font preloads (`siteFonts()` is `fontPreloads` from `@ejm/ui/font-preloads` as URLs; nothing in this package imports the `@ejm/ui` barrel, so `siteDefaults` adds no component to a page). `ssg` adds the canonical URL and `og:url` per route.

`ejmSite()` adds the static tags `ssg` cannot carry to `index.html`, right before `<!--ejm-head-->`: `color-scheme` (`light dark`), `theme-color` for both schemes, the synchronous external `<script src="/theme-boot.js">`, the icon links and the manifest. It also writes `theme-boot.js` (from `@ejm/ui`), the favicon set, `site.webmanifest` and `social-card.png` (from `@ejm/tokens`) into the build, and serves them in `ejm-ssg dev`. `index.html` keeps the charset and the viewport (`viewport-fit=cover`, needed for safe-area insets) and the two ssg markers. Do not keep your own copies of these files in `public/`.


The build adds one SHA256 content revision to the icon, touch-icon and manifest
URLs. The revision covers all nine files in that project's favicon family.
Changing an icon also changes the manifest URL and its local Android icon
references. The existing unversioned root paths keep working. Dev serves the
same manifest bytes as the build. No browser JavaScript, CSS or package
is added for this refresh.

`headTags(base)` keeps its unversioned behavior for existing callers.
`headTags(base, version)` adds the supplied revision to icon and manifest links;
`ejmSite()` computes that revision from the selected project's assets. Keep each
family's files together and rebuild after changing them. See `@ejm/tokens` for
the studio favicon generation command.

## Documentation compiler

`@ejm/site/docs` turns repository Markdown into the JSON contract rendered by
`@ejm/ui/docs`. Import it only from build scripts or a Vite plugin. The browser
entry does not import `marked`, YAML, the entity decoder, or the heading slugger.
Product sites own guide text, route lists, required metadata and repository-link
rewrites. Shared packages own parsing, reading layout and controls.

```js
import { parseDoc, splitDoc } from '@ejm/site/docs';

const page = parseDoc(source, {
  metadata: { slug: 'user-guide' },
  requireFrontMatter: true,
  requiredMetadata: ['title', 'lead', 'description', 'group'],
  requireExplicitIds: true,
  link: (href) => ({ href: rewriteProductLink(href), external: isExternal(href) }),
});
const { full, client, requiresRich } = splitDoc(page);
```

`parseDoc` also works on repository references without front matter. Supply their
page title, lead, description, group and slug in `metadata`. Explicit metadata
wins over front matter. The first `#` heading supplies a missing title; it does
not add a second page heading. Front matter is a YAML mapping of scalar text
fields. Quote values that contain a colon followed by a space. `order` becomes a
number when present.

### Author guides

- `## Setup {#setup}` starts a section. `### Detail {#detail}` adds a subsection.
  Explicit IDs remain stable. Without `requireExplicitIds`, headings get
  GitHub-compatible slugs with per-page duplicate suffixes. Duplicate explicit
  IDs stop the build.
- CommonMark paragraphs, inline code, emphasis, strong text, links, reference
  links, line breaks, nested lists, ordered-list starts and GFM tables are
  supported. All table columns remain visible in a locally scrolling table.
  Code spans preserve literal entities such as `&lt;tag&gt;`; ordinary text
  decodes entities before React escapes its output.
- `<!-- table: Caption -->` or a `Table: Caption` paragraph names the next table.
  Otherwise the nearest section title names it.
- A blockquote can start with `[!NOTE]`, `[!TIP]`, `[!WARNING]`, `[!IMPORTANT]`,
  `[!CAUTION]`, or `**Note:**`, `**Tip:**`, `**Warning:**`. A plain quote is a note.
  Callouts can contain paragraphs, lists and code.
- A code fence's language is its first info word. Follow it with a plain label,
  `label="Run tests"`, or `title="Run tests"`.
- Wrap consecutive fences in `<!-- tabs: Package manager -->` and
  `<!-- /tabs -->`. Give each fence a distinct `tab="npm"` attribute.
- HTML is limited to comments and inline `<br>`. Images and unsupported blocks
  fail the build rather than vanish. Comments have no visible output.

The link callback receives a safe authored href and returns the final href plus
its external-link flag. HTTP, HTTPS, mail and telephone links are allowed;
relative paths, anchors and protocol-relative web links are also valid. Unsafe
schemes and control characters are rejected before and after rewriting. The
consumer still decides which repository paths become site routes.

### Keep prose out of browser JavaScript

`splitDoc` returns `full` prose runs and a `client` page with metadata, contents,
section IDs, prose markers, code and tabs. Code inside a list or callout keeps
that block live so its copy button works. The build-only `requiresRich` flag is
true for such nested interactive blocks. Use it to generate a `DocDataBlock`
import and a `DocsPage.renderBlock` callback only for routes that need them.
Simple code and tabs use the smaller `DocCodeBlock` path by default. The flag
does not enter the client page data. Keep the full JSON import and the
`DocProse` renderer behind a constant server/dev condition in the generated
route. See the `@ejm/ui/docs` guide for the frame API. The static boundary must
wrap prose runs and highlighted source only; sidebar filtering, contents, tabs, copy and pager links
remain live. Measure the emitted route against its existing budget.

## Unreleased build identities

The registry can hold an unreleased build identity with `discoverable: false`.
Use `PROJECTS[id]` for its own build; `PROJECT_LIST` and `otherSites` expose
only discoverable projects. Empty repository and analytics fields remain
unconfigured. The source helper returns no link for an empty repository.
Chess uses this contract while its public service and guide destinations
remain unverified. Original brand artwork supplies its local head metadata
and favicons without adding Chess to other sites' navigation.

The plugin also distributes the original font notices from the public `@ejm/ui/fonts/Manrope-OFL.txt` and `@ejm/ui/fonts/IBMPlexMono-OFL.txt` exports. Every build includes their exact bytes at `/Manrope-OFL.txt` and `/IBMPlexMono-OFL.txt`, alongside the bundled font assets. Development serves the same paths as `text/plain; charset=utf-8`. Their stable names stay outside `/assets/*` and keep the default revalidation policy described above. Keep both notices with any deployed site that distributes these fonts.
