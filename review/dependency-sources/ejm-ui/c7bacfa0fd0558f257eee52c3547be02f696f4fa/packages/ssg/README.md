# @ejm/ssg

A tiny prerender package for Vite and React 19 sites. Every route is rendered to static HTML at build time, so the first paint needs no JavaScript, then `hydrateRoot` takes over and navigation is client side. History and location subscriptions use [Wouter's public browser hooks](https://github.com/molefrog/wouter#usebrowserlocation). SSG owns exact route matching and the site base. It does not bundle Wouter's route components or matching engine. React and react-dom account for most of the browser JavaScript.

## Set up a site

`index.html` has two markers, one in the head and one inside `#root`:

```html
<head>
  <meta charset="utf-8" />
  <!--ejm-head-->
</head>
<body>
  <div id="root"><!--ejm-root--></div>
  <script type="module" src="/src/main.tsx"></script>
</body>
```

`vite.config.ts`:

```ts
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';
import { ejmSsg } from '@ejm/ssg/vite';

export default defineConfig({ plugins: [react(), ejmSsg({ site: 'src/site.tsx' })] });
```

`src/main.tsx`:

```ts
import { hydrateSite } from '@ejm/ssg';
import site from './site';

void hydrateSite(site);
```

`src/site.tsx` (the whole site config, read by the browser, the server build and the budget gate):

```tsx
import { defineSite, Link } from '@ejm/ssg';
import manrope from '@ejm/tokens/assets/fonts/Manrope.woff2?url';
import './styles.css';

export default defineSite({
  origin: 'https://ejmlabs.com',
  head: { titleTemplate: '%s | ejmlabs', description: 'Default description.', og: { siteName: 'ejmlabs' } },
  fonts: [manrope],                                  // rel=preload as=font crossorigin on every route
  budget: { js: 95, css: 14, fonts: 70, html: 30 },  // KB; these are the defaults
  ignore: ['brand/assets/**'],                       // built pages the budget gate skips (static files that are not routes)
  App: ({ children }) => (<><Header /><main id="main">{children}</main><Footer /></>),
  routes: [
    { path: '/', component: Home, head: { title: 'Home', description: 'Home page.' } },
    { path: '/work/', load: () => import('./pages/Work'), head: { title: 'Work' } },
  ],
  notFound: { component: NotFound, head: { title: 'Not found' } },
});
```

Scripts (they take no flags: use `vite` or `vite build` directly for those, the plugin does the work either way): `ejm-ssg build` (a Vite build; the plugin prerenders and then checks the budget, so a breach fails the build), `ejm-ssg dev` (Vite dev server, client render only), `ejm-ssg budget [dist]`.

## API

```ts
defineSite(config: SiteConfig): Site
// SiteConfig: { routes: RouteDef[]; notFound?; App?; head?: SiteHead; base?; origin?; fonts?: string[]; budget?: Partial<Budget>; ignore?: string[] }
// RouteDef:   { path: string; head?: RouteHead } & ({ component } | { load: () => Promise<{ default: Component }> })
// RouteHead:  { title?, description?, canonical?, robots?, og?: { title, description, image, type, url, siteName }, meta?: [{ name | property, content }], links?: [{ rel, href, ... }] }
// SiteHead:   RouteHead & { titleTemplate?: '%s | Site' }

hydrateSite(site: Site, container?: HTMLElement): Promise<Root>   // client entry
Link(props: AnchorProps & { href: string; replace?: boolean })       // page paths navigate client side; files use native navigation
Static(props: { render?: () => ReactNode; className? })              // server-rendered, not hydrated, not in the browser bundle (see Static parts)
useNavigate(): (to: string, opts?: { replace?: boolean }) => Promise<void>
usePath(): string                                                    // current path without the base or a trailing slash (`/work`), same on the server and in the browser
ejmSsg({ site }): VitePlugin                                         // from '@ejm/ssg/vite'
checkBudget(dist, budget?, { base?, ignore? }) / formatReport / main         // from '@ejm/ssg/budget'
```

Rules:

- Route paths are exact (no params) and are matched ignoring a trailing slash. Write them the way the host serves them: Cloudflare serves `dist/work/index.html` at `/work/`, so write `/work/` to avoid a redirect. The declared form is used for the canonical URL.
- `head` is merged: site head, then route head. The title template applies to route titles. Canonical and `og:url` default to `origin + base + path` (not on the 404 page, which gets `robots: noindex`). `og:title`, `og:description` and `og:type` have defaults.
- `fonts` are plain URLs. Use `?url` imports so they carry Vite's `base` and hash (fonts under Vite's 4 KB `assetsInlineLimit` would be inlined as `data:` URLs, which a preload cannot use; real fonts are larger).
- `base` defaults to Vite's `base`. `Link` hrefs and `useNavigate` paths never include the base; the package adds it.
- The 404 page is written to `dist/404.html` and is what a client renders at any unknown URL.
- `App` wraps the page and renders inside the router, so it can use `Link` and `usePath`. The page is wrapped in a `<div data-ejm-route>`.

## Head links

`head.links` is a list of `<link>` tags as attribute objects, for things like a hero image preload. The site's links and the route's links are both written, an identical link once, and an undefined value is skipped. Attribute names are HTML's (lowercase: `imagesrcset`, `fetchpriority`).

```ts
import { shotPreloadLinks } from '@ejm/ui/layout/ProductShot';
{ path: '/', component: Home, head: { title: 'Home', links: shotPreloadLinks(hero) } }
```

For a route-specific stylesheet, use a Vite CSS URL and the same public head contract:

```ts
import docsStyles from './styles-docs.css?url';
{ path: '/docs/', component: Docs, head: { links: [{ rel: 'stylesheet', href: docsStyles }] } }
```

The stylesheet is in the prerendered HTML and counts toward the route's CSS
budget. Navigation keeps the painted sheet until all replacements load, reuses
matching links in the destination's declared order, and ignores an older navigation's completion. A failed replacement
leaves the prior sheet in place. Each complete Tailwind sheet must preserve its
responsive utility order; appending a partial sheet can override earlier rules.

## Static parts

`<Static>` marks a part of a page that nothing in the browser needs to change: prose, tables, a docs article. The server renders it in full, so it reads with scripts off. The browser keeps that markup and does not hydrate it, and the code behind it is not in the browser bundle. The ejm-harness site reported docs parts of about 9 KB gzip plus up to 8 KB of guide text per page, which does not fit the 95 KB budget as JavaScript; as Static HTML it costs no script.

```tsx
import { Static } from '@ejm/ssg';

const Guide = import.meta.env.SSR || import.meta.env.DEV ? (await import('./Guide')).default : undefined;

<Static render={Guide ? () => <Guide /> : undefined} />
```

Images and fonts imported only by a Static part are emitted into the final site too. Shared files keep Vite's asset names and are reused when their content matches; a server asset with the same filename and different content fails the build instead of overwriting the browser file.

The condition guards the import on purpose: the bundler then drops `Guide` and everything only it uses from the browser build (the test checks that a marker string in the prose is in the HTML and in no script). In dev the content renders in the browser too. Internal links inside the part (`<a href="/docs/x/">`, with the site base already in the href) use one delegated click handler. A link with a fragment that changes the pathname or query uses native document navigation, so the server HTML contains the target before the browser scrolls. Same-document, same-query fragments keep the current markup. Other sites, `rel="external"`, `target`, `download` and modified clicks are left to the browser. Do not put anything interactive in it (a copy button, a filter): it is never hydrated. Keep the docs frame and shared code-copy or tab controls outside the boundary; `@ejm/ui/docs/DocsPage` supports this split. If a fragment-free client navigation lands on a page whose Static part has no markup to keep, the page is reloaded and the server answers it, so a docs site should prefer plain navigation between its pages. It must be used inside the site root (any page).

## Lazy routes

`load: () => import('./pages/Work')` puts the page in its own chunk. At build time the page is loaded before rendering, so its HTML is complete, and the route's HTML gets `<link rel="modulepreload">` for the chunk (and its CSS), so the hydrating browser fetches it in parallel with the entry. On the client the chunk is loaded before `hydrateRoot` (no fallback flash, no mismatch), is warmed on link hover or focus, and is loaded before a `Link` navigation commits. `React.lazy` with a `Suspense` boundary is the safety net for back and forward to a route that is not loaded yet.

The shared Vite plugin uses native module preloading and leaves the configured JavaScript target unchanged. Browsers without `rel=modulepreload` still fetch and run imported modules, but lose that preload acceleration. The plugin also resolves the exact `use-sync-external-store/shim/index.js` compatibility import to React's native export: the package requires React 19, where the shim already chooses that same hook. Selector shims and other imports are unchanged.

The build finds a route's chunk from its `import('...')` call in the server bundle, so keep the `import()` literal inside `load`. If it cannot be found, the build warns and that route is simply not preloaded.

## Navigation

`base` is applied to links and programmatic navigation. A path at the base or its trailing slash is `/`; matching the base ignores case and requires a path boundary. URI decoding preserves encoded separators and leaves malformed escapes intact. A path outside the base cannot match a site route. Server paths, including a query string, resolve to the same route when hydrated. Wouter owns push, replace, popstate and hash subscriptions, so Back and Forward retain the browser's history.

- Internal page `Link` clicks without a cross-document fragment use `history.pushState`. A fragment link to a different pathname or query loads the full document before the browser finds its target, including targets inside `Static` prose. This also applies to delegated links inside `Static` and programmatic `useNavigate` calls. The site base, query and fragment are preserved; `replace: true` replaces the history entry. Same-document, same-query hashes stay in the current document. Undeclared file paths such as `/license.txt?raw=1` keep native navigation; a declared file-shaped route still uses the router. Extensionless unknown paths retain the site's client-side 404. Absolute URLs, `rel="external"`, `target`, `download` and clicks with modifier keys or a non-primary button are left to the browser. Absolute URLs get `rel="external noopener"` unless you pass `rel`.
- After a route change focus moves to the page `h1` (first `#main h1`, else the first `h1`, else `#main`) with `tabindex="-1"`, `document.title` and the head tags are swapped, and the title is announced in `#ejm-announcer` (`role="status"`, `aria-live="polite"`). This also happens on back and forward. Give each page an `h1`.
- Scroll goes to the top, or to the `#hash` target.
- The route change runs in `document.startViewTransition` when it exists and the visitor has not asked for reduced motion; only the page area crossfades for 320 ms with the shared easing, and the header stays still. Skipped transitions are swallowed, and a transition that cannot start within 300 ms is skipped so the page never freezes. Without the API, a CSS fade-in runs after the first navigation. Under `prefers-reduced-motion: reduce` nothing is animated and the API is not called. The first load never animates.
- `ssg.css` (under 1 KB) is imported by the package itself.

## Budget gate

```
ejm-ssg budget [dist] [--js 95] [--css 14] [--fonts 70] [--html 30] [--ignore glob]... [--json]
node scripts/ssg-budget.mjs [dist] ...        # same, from the repo root
```

For every `.html` file the gate follows only the files that page references: `<script src>`, `modulepreload`, the static imports inside those scripts, `<link rel=stylesheet>` and `preload as=font`. Lazy chunks that are not preloaded are not counted. Sizes are gzip (level 9) per file and summed, in KB of 1024 bytes; fonts are raw size because WOFF2 is already compressed. Budgets come from `defineSite({ budget })`, saved to `.ejm-ssg/build.json` by the build, and `--flags` override them. Every `.html` file under `dist` is a route, so static pages you copy in (brand assets, a generated report) would be judged as routes. Skip them with `ignore` in `defineSite` (saved to `build.json`, so `ejm-ssg budget` and `scripts/perf.mjs` both read it) or with `--ignore <glob>`, repeatable. A glob (`*` inside a folder, `**` across folders, `?`) is tried against the file path under `dist` (`brand/assets/kit/index.html`) and against the route without its leading slash (`brand/assets/kit/`), so `brand/assets/**` skips the whole folder. `node scripts/perf.mjs <dist> --ignore <glob>` takes the same flag. It prints a table per route and exits 1 on a breach, naming the route and the overage:

```
/lazy/: js is 96.2 KB gzip, over the 95 KB budget by 1.2 KB
```

## Notes for consumers

- `@ejm/ssg/vite` (the plugin), `@ejm/ssg/budget` and the `ejm-ssg` bin are plain `.mjs` with `.d.mts` types, so Node loads them from `node_modules` (it refuses to strip types there). The client runtime (`SiteRoot`, `Link`, `hydrateSite`) and the server render code are TypeScript source that your Vite compiles: the plugin builds them with `ssr.noExternal: [/^@ejm\//]`. Works from an `npm pack` tarball, a `file:` link or the workspace (`scripts/pack-proof.mjs` checks the tarball route). Node 22 or newer.
- The server build is written to `<root>/.ejm-ssg/` and removed after prerendering; only `build.json` stays. Ignore `.ejm-ssg` in git.
- Set `NODE_ENV=production` for the build (`ejm-ssg build` does). A leaked `NODE_ENV=test` bundles React's development build.
- Security headers: the package adds no inline scripts or styles.

## Route loading failures

The reload link preserves the requested path, query and fragment.
If a route chunk fails to load, the site keeps its frame and shows a readable
error with a normal **Reload this page** link. Reload requests the current HTML,
which can recover an old tab after a release removes its previous chunk. No
automatic reload loop runs. An offline visitor must reconnect before retrying.
Malformed percent escapes in URL fragments fall back to the literal fragment;
they do not stop navigation.
