import './ssg.css';
import {
  Component as ReactComponent,
  createContext,
  Suspense,
  useCallback,
  useContext,
  useEffect,
  useLayoutEffect,
  useRef,
  type AnchorHTMLAttributes,
  type MouseEvent,
  type ReactNode,
} from 'react';
import { createRoot, hydrateRoot } from 'react-dom/client';
import { flushSync } from 'react-dom';
import { navigate, usePathname } from 'wouter/use-browser-location';
import { applyHead, resolveHead } from './head';
import { matchRoute, normalizePath, stripBase, type Route, type Site } from './site';

const SiteContext = createContext<{ site: Site; ssrPath?: string } | null>(null);
const useSiteContext = () => {
  const context = useContext(SiteContext);
  if (!context) throw new Error('ssg: used outside of the site root');
  return context;
};
const useSite = () => useSiteContext().site;

/** Wouter owns history subscriptions; SSG owns the base-relative exact path. */
function useRoutePath() {
  const { site, ssrPath } = useSiteContext();
  return stripBase(site, usePathname({ ssrPath }));
}

/** The current path without the site base and without a trailing slash, e.g. `/about`. The prerender and the browser agree on it, so an `App` can render from it without a hydration mismatch. */
export const usePath = () => normalizePath(useRoutePath());

const noop = () => {};

function focusHeading() {
  const el = document.querySelector<HTMLElement>('#main h1') ?? document.querySelector<HTMLElement>('h1') ?? document.getElementById('main');
  if (!el) return;
  if (!el.hasAttribute('tabindex')) el.setAttribute('tabindex', '-1');
  el.focus({ preventScroll: true });
}

function scrollTo(hash: string) {
  let id = hash.slice(1);
  try { id = decodeURIComponent(id); } catch { /* A malformed fragment still navigates to the page. */ }
  const target = id ? document.getElementById(id) : null;
  if (target) target.scrollIntoView();
  else window.scrollTo(0, 0);
}

function startTransition(run: () => void) {
  const doc = document as Document & {
    startViewTransition?: (cb: () => void) => {
      ready: Promise<void>;
      finished: Promise<void>;
      updateCallbackDone: Promise<void>;
      skipTransition(): void;
    };
  };
  const root = document.documentElement;
  const reduced = window.matchMedia?.('(prefers-reduced-motion: reduce)').matches;
  // Switches on the CSS-only fallback in ssg.css. The first load never animates.
  root.setAttribute('data-ejm-nav', '');
  if (!doc.startViewTransition || reduced) return run();
  root.setAttribute('data-ejm-vt', '');
  let ran = false;
  const t = doc.startViewTransition(() => ((ran = true), run()));
  // The browser calls the update on its next rendered frame. If frames are not coming (a throttled tab), do not leave the page frozen.
  setTimeout(() => ran || t.skipTransition(), 300);
  // A skipped transition (another navigation, a hidden tab) rejects with AbortError; the page still updates.
  const quiet = (e: unknown) => {
    if ((e as { name?: string })?.name !== 'AbortError') console.error(e);
  };
  t.ready.catch(noop);
  t.updateCallbackDone.catch(quiet);
  t.finished.catch(noop).finally(() => root.removeAttribute('data-ejm-vt'));
}

export interface NavigateOptions {
  replace?: boolean;
}

function crossDocumentFragment(href: string) {
  if (!href.includes('#')) return false;
  const current = window.location;
  const destination = new URL(href, current.href);
  return destination.pathname !== current.pathname || destination.search !== current.search;
}

/** Cross-document fragments load the full HTML before the browser finds the target. Other site paths load the route chunk first, then navigate inside a view transition. */
export function useNavigate() {
  const site = useSite();
  return useCallback(
    async (to: string, opts: NavigateOptions = {}) => {
      if (!to.startsWith('/')) return void window.location.assign(to);
      if (crossDocumentFragment(site.base + to)) {
        if (opts.replace) window.location.replace(site.base + to);
        else window.location.assign(site.base + to);
        return;
      }
      const route = matchRoute(site, to);
      await route?.preload().catch(noop);
      const hash = to.includes('#') ? to.slice(to.indexOf('#')) : '';
      startTransition(() => {
        flushSync(() => navigate(site.base + to, { replace: opts.replace }));
        scrollTo(hash);
      });
    },
    [site],
  );
}

export type LinkProps = AnchorHTMLAttributes<HTMLAnchorElement> & {
  /** A site path such as `/about#team` (no base), or any absolute URL, which navigates normally. */
  href: string;
  replace?: boolean;
};

/** An `<a>` that navigates client side for internal paths. Cross-document fragments, file paths, absolute URLs, `rel="external"`, `target`, `download` and modified clicks behave as normal links. Declared file-shaped routes still use the router. */
export function Link({ href, replace, onClick, onPointerEnter, onFocus, rel, target, ...rest }: LinkProps) {
  const site = useSite();
  const navigate = useNavigate();
  const internal = href.startsWith('/') && !href.startsWith('//') && !/(^|\s)external(\s|$)/.test(rel ?? '');
  const path = href.split(/[?#]/, 1)[0]!;
  const routed = internal && (!!matchRoute(site, href) || !/\.[^/]+$/.test(path));
  const warm = () => void matchRoute(site, href)?.preload().catch(noop);
  return (
    <a
      {...rest}
      href={internal ? site.base + href : href}
      target={target}
      rel={rel ?? (/^[a-z][a-z0-9+.-]*:/i.test(href) && !href.startsWith('mailto:') ? 'external noopener' : undefined)}
      onPointerEnter={e => (onPointerEnter?.(e), routed && warm())}
      onFocus={e => (onFocus?.(e), routed && warm())}
      onClick={(e: MouseEvent<HTMLAnchorElement>) => {
        onClick?.(e);
        if (!routed || e.defaultPrevented || e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;
        if ((target && target !== '_self') || rest.download != null) return;
        if (!replace && crossDocumentFragment(site.base + href)) return;
        e.preventDefault();
        void navigate(href, { replace });
      }}
    />
  );
}

export interface StaticProps {
  /**
   * The content, for the server render only. Pass it from a build-time condition, so the browser bundle never contains it:
   * `render={import.meta.env.SSR || import.meta.env.DEV ? () => <Guide /> : undefined}`.
   * With no `render` (the browser build) the prerendered markup is kept as it is and not hydrated.
   */
  render?: () => ReactNode;
  className?: string;
}

/**
 * A part of a page that costs the first load nothing: prose, tables, a product view that nothing in the browser needs to change. The server
 * renders it in full, so it reads with scripts off. The browser keeps that markup and does not hydrate it (an element with
 * `dangerouslySetInnerHTML` makes React leave its children alone), and the code behind it is not in the browser bundle.
 * Internal links inside it (`<a href="/docs/">`, with the site base already in the href) use a delegated click handler. Cross-document
 * fragments keep native navigation so their target is present before scrolling. If the part is ever rendered in the browser with no markup to keep (client navigation into a route that has
 * one, or history navigation), there is nothing to show, so the page is reloaded and the server answers it.
 */
export function Static({ render, className }: StaticProps) {
  const site = useSite();
  const navigate = useNavigate();
  const el = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!render && el.current && !el.current.firstChild) window.location.reload();
  }, [render]);
  const onClick = (e: MouseEvent<HTMLDivElement>) => {
    const a = (e.target as Element).closest?.('a');
    if (!a || e.defaultPrevented || e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;
    if ((a.target && a.target !== '_self') || a.hasAttribute('download') || /(^|\s)external(\s|$)/.test(a.rel)) return;
    const url = new URL(a.href, window.location.href);
    if (url.origin !== window.location.origin) return;
    const path = stripBase(site, url.pathname);
    if (!matchRoute(site, path)) return;
    if (crossDocumentFragment(url.href)) return;
    e.preventDefault();
    void navigate(path + url.search + url.hash);
  };
  const props = { ref: el, className, 'data-ejm-static': '', onClick };
  return render ? (
    <div {...props}>{render()}</div>
  ) : (
    <div {...props} suppressHydrationWarning dangerouslySetInnerHTML={{ __html: '' }} />
  );
}

class RouteBoundary extends ReactComponent<{ children: ReactNode; href: string }, { failed: boolean }> {
  state = { failed: false };
  static getDerivedStateFromError() { return { failed: true }; }
  render() {
    if (!this.state.failed) return this.props.children;
    return <div role="alert" className="page py-rhythm">
      <h1 className="text-title">Page could not load</h1>
      <p className="py-4">Check your connection and reload this page to get the latest version.</p>
      <a href={this.props.href}>Reload this page</a>
    </div>;
  }
}

function Shell() {
  const site = useSite();
  const path = useRoutePath();
  const route: Route = matchRoute(site, path) ?? site.notFound;
  const last = useRef<Route>(route);
  useLayoutEffect(() => {
    const head = resolveHead(site, route);
    if (last.current === route) {
      // First paint: prerendered pages already carry their head. A dev page rendered on the client does not.
      if (!document.head.querySelector('[data-ejm]')) applyHead(head);
      return;
    }
    last.current = route;
    applyHead(head);
    const live = document.getElementById('ejm-announcer');
    if (live) live.textContent = head.title;
    focusHeading();
  }, [site, route]);
  const page = (
    <div data-ejm-route="" key={route.path}>
      <RouteBoundary href={site.base + route.href}>
        <Suspense fallback={null}>
          <route.Component />
        </Suspense>
      </RouteBoundary>
    </div>
  );
  return site.App ? <site.App>{page}</site.App> : page;
}

/** The whole tree: router, page and the live region that announces route changes. Server and client render the same thing. */
export function SiteRoot({ site, ssrPath }: { site: Site; ssrPath?: string }) {
  return (
    <SiteContext.Provider value={{ site, ssrPath: ssrPath?.split('?')[0] }}>
      <Shell />
      <div id="ejm-announcer" role="status" aria-live="polite" />
    </SiteContext.Provider>
  );
}

/** Client entry: loads the current route's chunk, then hydrates the prerendered HTML (or renders from scratch in dev). */
export async function hydrateSite(site: Site, container: HTMLElement | null = document.getElementById('root')) {
  if (!container) throw new Error('ssg: no #root element');
  // A missing chunk must reach the route recovery view instead of rejecting before the root mounts.
  await matchRoute(site, stripBase(site, location.pathname))?.preload().catch(noop);
  const tree = <SiteRoot site={site} />;
  if (container.firstElementChild) return hydrateRoot(container, tree);
  const root = createRoot(container);
  root.render(tree);
  return root;
}
