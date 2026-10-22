import { lazy, type ComponentType, type ReactNode } from 'react';

/** What the head of a page can say. Everything is optional; a route inherits the site head and overrides it. */
export interface RouteHead {
  title?: string;
  description?: string;
  /** A full URL, or a path that is joined to `origin` + `base`. Defaults to the route's own URL when the site has an `origin`. */
  canonical?: string;
  robots?: string;
  og?: { title?: string; description?: string; image?: string; type?: string; url?: string; siteName?: string };
  /** Extra tags: `{ name: 'theme-color', content: '#fff' }` or `{ property: 'og:locale', content: 'en' }`. */
  meta?: Array<{ name?: string; property?: string; content: string }>;
  /**
   * Extra `<link>` tags, one object of attributes each, for example a hero image preload:
   * `{ rel: 'preload', as: 'image', imagesrcset: '...', imagesizes: '...', fetchpriority: 'high' }`. The site's links and the route's are
   * both emitted; an identical link is emitted once. Attribute names are written as HTML has them (lowercase). Undefined values are skipped.
   */
  links?: Array<Record<string, string | undefined>>;
}

export interface SiteHead extends RouteHead {
  /** `'%s | ejmlabs'`: applied to every route title that is set. */
  titleTemplate?: string;
}

export type PageModule = { default: ComponentType };

type Page = { component: ComponentType; load?: never } | { load: () => Promise<PageModule>; component?: never };
export type RouteDef = { path: string; head?: RouteHead } & Page;

/** Sizes in KB (1 KB = 1024 bytes). Images and other files are not counted. */
export interface Budget {
  js: number;
  css: number;
  /** Fonts the HTML preloads, raw size (WOFF2 is already compressed). */
  fonts: number;
  html: number;
}

export interface SiteConfig {
  /** Every route is prerendered to `dist/<path>/index.html`. Paths are exact; the site has no params. */
  routes: RouteDef[];
  /** Rendered for any other URL and written to `dist/404.html`. Has a plain default. */
  notFound?: { head?: RouteHead } & Page;
  /** Header, footer, theme: wraps the current page. Rendered inside the router, so it can use `Link`. */
  App?: ComponentType<{ children: ReactNode }>;
  head?: SiteHead;
  /** Where the site is served from, e.g. `/docs`. Defaults to Vite's `base`. */
  base?: string;
  /** `https://ejmlabs.com`, for canonical and `og:url`. */
  origin?: string;
  /** URLs of fonts to preload on every route (`rel=preload as=font crossorigin`). Use `import url from './x.woff2?url'`. */
  fonts?: string[];
  /** Overrides of the default budgets, which `ejm-ssg budget` and the build enforce. */
  budget?: Partial<Budget>;
  /** Globs of built pages the budget gate skips: static pages that are not routes (copied brand assets). Matched against the file path under dist or the route, for example `brand/kit/**`. */
  ignore?: string[];
}

export interface Route {
  /** Normalized: leading slash, no trailing slash except for `/`. */
  path: string;
  /** The path as declared (`/about/` keeps its slash): used for canonical URLs. */
  href: string;
  head: RouteHead;
  /** The page, or a `React.lazy` stand-in until `preload()` has resolved. */
  readonly Component: ComponentType;
  /** Loads the chunk of a lazy route. Resolves at once for eager routes. */
  preload(): Promise<void>;
  /** The original `load`, kept so the build can find the route's chunk. */
  load?: () => Promise<PageModule>;
}

export interface Site {
  routes: Route[];
  notFound: Route;
  App?: ComponentType<{ children: ReactNode }>;
  head: SiteHead;
  /** `''` or `/docs`: no trailing slash. */
  base: string;
  origin?: string;
  fonts: string[];
  budget: Partial<Budget>;
  ignore: string[];
}

export const normalizePath = (p: string) => {
  const clean = (p.split(/[?#]/)[0] || '/').replace(/\/+$/, '');
  return clean || '/';
};

function makeRoute(path: string, href: string, def: { head?: RouteHead } & Page): Route {
  let resolved: ComponentType | undefined = def.component;
  let pending: Promise<void> | undefined;
  const preload = () => {
    if (resolved || !def.load) return Promise.resolve();
    return (pending ??= def.load().then(m => void (resolved = m.default)));
  };
  // One load() call feeds both preload() and the Suspense fallback path used for back/forward to an unloaded route.
  const Lazy = def.load ? lazy(() => preload().then(() => ({ default: resolved! }))) : undefined;
  return {
    path,
    href,
    head: def.head ?? {},
    get Component() {
      return resolved ?? Lazy!;
    },
    preload,
    load: def.load,
  };
}

const DefaultNotFound = () => null;

const viteBase = () => (import.meta as { env?: { BASE_URL?: string } }).env?.BASE_URL ?? '/';

export function defineSite(config: SiteConfig): Site {
  const seen = new Set<string>();
  const routes = config.routes.map(def => {
    if (!def.path.startsWith('/')) throw new Error(`ssg: route path "${def.path}" must start with "/"`);
    const path = normalizePath(def.path);
    if (seen.has(path)) throw new Error(`ssg: duplicate route "${path}"`);
    seen.add(path);
    return makeRoute(path, def.path, def);
  });
  const nf: NonNullable<SiteConfig['notFound']> = config.notFound ?? { component: DefaultNotFound };
  const notFound = makeRoute('/404', '/404', { ...nf, head: { title: 'Page not found', robots: 'noindex', ...nf.head } } as RouteDef);
  const base = (config.base ?? viteBase()).replace(/\/+$/, '');
  return {
    routes,
    notFound,
    App: config.App,
    head: config.head ?? {},
    base,
    origin: config.origin?.replace(/\/+$/, ''),
    fonts: config.fonts ?? [],
    budget: config.budget ?? {},
    ignore: config.ignore ?? [],
  };
}

export const matchRoute = (site: Site, pathname: string): Route | undefined => {
  const p = normalizePath(pathname);
  return site.routes.find(r => r.path === p);
};

function decodePath(path: string) {
  try { return decodeURI(path); } catch { return path; }
}

/** A decoded path relative to the site base. Outside paths keep Wouter's `~` prefix and cannot match a site route. */
export function stripBase(site: Site, pathname: string) {
  const path = decodePath(pathname);
  const base = decodePath(site.base);
  return !base || (path.toLowerCase() + '/').startsWith(base.toLowerCase() + '/')
    ? path.slice(base.length) || '/'
    : '~' + path;
}
