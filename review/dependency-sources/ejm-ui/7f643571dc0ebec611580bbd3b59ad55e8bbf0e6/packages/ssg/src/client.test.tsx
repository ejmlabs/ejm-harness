import { act } from 'react';
import { renderToString } from 'react-dom/server';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { defineSite, hydrateSite, Link, matchRoute, SiteRoot, Static, useNavigate, usePath } from './index';
import { renderRoute } from './server';
import site from '../fixtures/basic/src/site';

type Root = Awaited<ReturnType<typeof hydrateSite>>;
let root: Root | undefined;
let errors: ReturnType<typeof vi.spyOn>;

async function boot(path: string) {
  history.replaceState(null, '', path);
  document.head.innerHTML = '';
  const route = matchRoute(site, path) ?? site.notFound;
  const { html, head } = await renderRoute(site, route);
  document.head.innerHTML = head;
  document.body.innerHTML = `<div id="root">${html}</div>`;
  await act(async () => {
    root = await hydrateSite(site);
  });
}
const click = (el: Element, init: MouseEventInit = {}) =>
  act(async () => {
    el.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true, button: 0, ...init }));
  });
const link = (name: string) => [...document.querySelectorAll('a')].find(a => a.textContent === name)!;
const settled = () => act(async () => void (await new Promise(r => setTimeout(r, 10))));

beforeEach(() => {
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  window.scrollTo = vi.fn();
  Element.prototype.scrollIntoView = vi.fn();
  errors = vi.spyOn(console, 'error').mockImplementation(() => {});
});
afterEach(() => {
  vi.unstubAllGlobals();
  act(() => root?.unmount());
  root = undefined;
  vi.restoreAllMocks();
  delete (document as { startViewTransition?: unknown }).startViewTransition;
  document.documentElement.removeAttribute('data-ejm-nav');
});

describe('hydration', () => {
  it('adopts the prerendered markup without a mismatch or a second render', async () => {
    await boot('/');
    const h1 = document.querySelector('h1');
    expect(errors).not.toHaveBeenCalled();
    await settled();
    expect(document.querySelector('h1')).toBe(h1); // same node: nothing was thrown away and rebuilt
    expect(document.activeElement).toBe(document.body); // first paint does not steal focus
  });

  it('hydrates a lazy route in place, with no fallback', async () => {
    await boot('/lazy/');
    expect(errors).not.toHaveBeenCalled();
    expect(document.querySelector('h1')?.textContent).toBe('Lazy heading');
  });

  it('hydrates the 404 page at an unknown URL', async () => {
    await boot('/nope/');
    expect(errors).not.toHaveBeenCalled();
    expect(document.querySelector('h1')?.textContent).toBe('Nothing here');
  });
});

describe('document fragment navigation', () => {
  const destinations = [
    { from: '/kit/', route: '/', name: 'another page', native: true },
    { from: '/kit/guide/?view=summary', route: '/guide/', name: 'another query', native: true },
    { from: '/kit/guide/?view=source', route: '/guide/', name: 'the same document and query', native: false },
  ];

  for (const via of ['Link', 'Static'] as const) {
    it.each(destinations)(`preserves document boundaries in ${via} for $name`, async ({ from, route, native }) => {
      let onServer = true;
      const href = '/guide/?view=source#contributing';
      const own = defineSite({
        base: '/kit',
        App: ({ children }) => <>
          {via === 'Link'
            ? <Link href={href}>Contribute</Link>
            : <Static render={onServer ? () => <a href={'/kit' + href}>Contribute</a> : undefined} />}
          <main>{children}</main>
          <footer>End of page</footer>
        </>,
        routes: [
          { path: '/', component: () => <h1>Home</h1> },
          { path: '/guide/', component: () => <>
            <h1>Guide</h1>
            <Static render={onServer ? () => <h2 id="contributing">Contribute a change</h2> : undefined} />
          </> },
        ],
      });
      const destination = await renderRoute(own, own.routes[1]!);
      expect(destination.html).toContain('<h2 id="contributing">Contribute a change</h2>');
      history.replaceState(null, '', from);
      const { html } = await renderRoute(own, matchRoute(own, route)!);
      document.body.innerHTML = `<div id="root">${html}</div>`;
      onServer = false;
      await act(async () => { root = await hydrateSite(own); });
      const heading = document.querySelector('h1');
      const push = vi.spyOn(history, 'pushState');
      const replace = vi.spyOn(history, 'replaceState');
      let intercepted: boolean | undefined;
      // Observe React's decision, then stop jsdom's unsupported document load.
      document.addEventListener('click', event => {
        intercepted = event.defaultPrevented;
        event.preventDefault();
      }, { once: true });
      expect(link('Contribute').getAttribute('href')).toBe('/kit' + href);
      await click(link('Contribute'));
      expect(intercepted).toBe(!native);
      expect(location.pathname + location.search + location.hash).toBe(from + (native ? '' : '#contributing'));
      expect(document.querySelector('h1')).toBe(heading);
      if (native) {
        expect(push).not.toHaveBeenCalled();
        expect(Element.prototype.scrollIntoView).not.toHaveBeenCalled();
      } else {
        expect(push).toHaveBeenCalledOnce();
        expect(Element.prototype.scrollIntoView).toHaveBeenCalledOnce();
      }
      expect(replace).not.toHaveBeenCalled();
      expect(errors).not.toHaveBeenCalled();
    });
  }

  it.each([false, true])('uses document navigation before rendering a fragment route (replace=%s)', async replace => {
    const Go = () => {
      const navigate = useNavigate();
      return <button onClick={() => void navigate('/guide/?view=source#contributing', { replace })}>Contribute</button>;
    };
    const own = defineSite({
      base: '/kit',
      routes: [
        { path: '/', component: () => <><h1>Home</h1><Go /></> },
        { path: '/guide/', component: () => <Static /> },
      ],
    });
    history.replaceState(null, '', '/kit/');
    document.body.innerHTML = '<div id="root"></div>';
    await act(async () => { root = await hydrateSite(own); });
    const browser = window;
    const native = {
      href: browser.location.href, pathname: browser.location.pathname, search: browser.location.search,
      assign: vi.fn(), replace: vi.fn(),
    };
    vi.stubGlobal('window', new Proxy(browser, {
      get: (target, key) => key === 'location' ? native : Reflect.get(target, key, target),
    }));
    const push = vi.spyOn(history, 'pushState');
    const replaceState = vi.spyOn(history, 'replaceState');
    await click(document.querySelector('button')!);
    expect(native[replace ? 'replace' : 'assign']).toHaveBeenCalledExactlyOnceWith('/kit/guide/?view=source#contributing');
    expect(native[replace ? 'assign' : 'replace']).not.toHaveBeenCalled();
    expect(document.querySelector('h1')?.textContent).toBe('Home');
    expect(push).not.toHaveBeenCalled();
    expect(replaceState).not.toHaveBeenCalled();
    expect(errors).not.toHaveBeenCalled();
  });
});

describe('usePath', () => {
  it('is the path without a trailing slash on the server and in the browser, so an App that renders from it hydrates cleanly', async () => {
    const App = ({ children }: { children: React.ReactNode }) => (
      <>
        <p id="at">{usePath()}</p>
        {children}
      </>
    );
    const own = defineSite({ App, base: '', routes: [{ path: '/lazy/', component: () => <h1>Page</h1> }] });
    history.replaceState(null, '', '/lazy/');
    const { html } = await renderRoute(own, own.routes[0]!);
    expect(html).toContain('<p id="at">/lazy</p>');
    document.body.innerHTML = `<div id="root">${html}</div>`;
    await act(async () => {
      root = await hydrateSite(own);
    });
    expect(errors).not.toHaveBeenCalled();
    expect(document.getElementById('at')?.textContent).toBe('/lazy');
  });
});

describe('client navigation', () => {
  it('follows an internal link without a reload, then focuses the new h1 and announces the title', async () => {
    await boot('/');
    const reload = vi.fn();
    window.addEventListener('beforeunload', reload);
    await click(link('Lazy'));
    await settled();
    expect(location.pathname).toBe('/lazy/');
    expect(document.querySelector('h1')?.textContent).toBe('Lazy heading');
    expect(document.activeElement).toBe(document.querySelector('h1'));
    expect(document.activeElement?.getAttribute('tabindex')).toBe('-1');
    expect(document.getElementById('ejm-announcer')?.textContent).toBe('Lazy page | Fixture');
    expect(document.title).toBe('Lazy page | Fixture');
    expect(document.head.querySelector('link[rel=canonical]')?.getAttribute('href')).toBe('https://fixture.test/lazy/');
    expect(window.scrollTo).toHaveBeenCalledWith(0, 0);
    expect(reload).not.toHaveBeenCalled();
    expect(errors).not.toHaveBeenCalled();
  });

  it('renders the not found page for an unknown route', async () => {
    await boot('/');
    await click(link('Missing'));
    await settled();
    expect(document.querySelector('h1')?.textContent).toBe('Nothing here');
    expect(document.getElementById('ejm-announcer')?.textContent).toBe('Not found | Fixture');
  });

  it('leaves external links, modified clicks and rel=external to the browser', async () => {
    await boot('/');
    let prevented = true;
    // Runs after React's handler (the root listener is nested deeper): records the verdict, then stops jsdom from "navigating".
    const record = (e: Event) => ((prevented = e.defaultPrevented), e.preventDefault());
    document.addEventListener('click', record);
    const cases: Array<[string, MouseEventInit]> = [['Example', {}], ['Lazy', { ctrlKey: true }], ['Lazy', { metaKey: true }], ['Lazy', { shiftKey: true }], ['Lazy', { altKey: true }], ['Lazy', { button: 1 }]];
    for (const [name, init] of cases) {
      prevented = true;
      await click(link(name), init);
      expect(prevented, `${name} ${JSON.stringify(init)}`).toBe(false);
    }
    document.removeEventListener('click', record);
    expect(location.pathname).toBe('/');
  });

  it('scrolls to a same-document hash target instead of the top', async () => {
    await boot('/lazy/');
    const scrollIntoView = vi.fn();
    Element.prototype.scrollIntoView = scrollIntoView;
    await click(link('Details'));
    await settled();
    expect(location.pathname + location.hash).toBe('/lazy/#details');
    expect(scrollIntoView).toHaveBeenCalledOnce();
    expect(window.scrollTo).not.toHaveBeenCalled();
  });
});

it('keeps same-origin files as native links and routes known pages', async () => {
  const files = defineSite({
    origin: 'https://example.test', head: { title: 'Files' },
    routes: [{ path: '/', component: () => <><h1>Files</h1><Link href="/license.txt?raw=1">License</Link><Link href="/next/">Next</Link></> }, { path: '/next/', component: () => <h1>Next page</h1> }],
    notFound: { component: () => <h1>Not found</h1> },
  });
  history.replaceState(null, '', '/');
  const { html } = await renderRoute(files, files.routes[0]!);
  document.body.innerHTML = `<div id="root">${html}</div>`;
  await act(async () => { root = await hydrateSite(files); });
  const event = new MouseEvent('click', { bubbles: true, cancelable: true, button: 0 });
  let prevented = true;
  const preventNative = (e: Event) => { prevented = e.defaultPrevented; e.preventDefault(); };
  document.addEventListener('click', preventNative);
  await act(async () => { document.querySelector('a')!.dispatchEvent(event); });
  document.removeEventListener('click', preventNative);
  expect(prevented).toBe(false);
  expect(document.querySelector('h1')?.textContent).toBe('Files');
  expect(document.querySelector('a')?.getAttribute('href')).toBe('/license.txt?raw=1');
  await click(document.querySelectorAll('a')[1]!);
  await settled();
  expect(document.querySelector('h1')?.textContent).toBe('Next page');
});

describe('view transitions', () => {
  const rejecting = () => {
    const abort = () => Promise.reject(new DOMException('Transition was skipped', 'AbortError'));
    return vi.fn((cb: () => void) => {
      cb();
      return { ready: abort(), finished: abort(), updateCallbackDone: abort() };
    });
  };

  it('wraps the route change in startViewTransition and swallows a skipped transition', async () => {
    const start = rejecting();
    (document as { startViewTransition?: unknown }).startViewTransition = start;
    window.matchMedia = vi.fn(() => ({ matches: false })) as unknown as typeof window.matchMedia;
    await boot('/');
    await click(link('Lazy'));
    await settled();
    expect(start).toHaveBeenCalledOnce();
    expect(location.pathname).toBe('/lazy/'); // the page still updated
    expect(errors).not.toHaveBeenCalled(); // and nothing leaked to the console (an unhandled rejection would fail the run)
  });

  it('skips the API entirely under prefers-reduced-motion', async () => {
    const start = rejecting();
    (document as { startViewTransition?: unknown }).startViewTransition = start;
    window.matchMedia = vi.fn(() => ({ matches: true })) as unknown as typeof window.matchMedia;
    await boot('/');
    await click(link('Lazy'));
    await settled();
    expect(start).not.toHaveBeenCalled();
    expect(location.pathname).toBe('/lazy/');
    expect(document.activeElement).toBe(document.querySelector('h1'));
  });
});


describe('route recovery', () => {
  it('keeps the site frame and offers a normal reload after a route chunk fails', async () => {
    const own = defineSite({
      base: '',
      App: ({ children }) => <><header>Site navigation</header><main>{children}</main></>,
      routes: [
        { path: '/', component: () => <><h1>Home</h1><Link href="/broken/">Open example</Link></> },
        { path: '/broken/', load: () => Promise.reject(new TypeError('Failed to fetch dynamically imported module')) },
      ],
    });
    history.replaceState(null, '', '/');
    document.body.innerHTML = '<div id="root"></div>';
    await act(async () => { root = await hydrateSite(own); });
    await click(link('Open example'));
    await settled();
    expect(document.querySelector('header')?.textContent).toBe('Site navigation');
    expect(document.querySelector('[role="alert"]')?.textContent).toContain('Page could not load');
    expect(link('Reload this page').getAttribute('href')).toBe('/broken/');
  });

  it('handles a malformed same-document fragment without throwing', async () => {
    const own = defineSite({ base: '', routes: [
      { path: '/', component: () => <><h1>Home</h1><Link href="/#%">Malformed fragment</Link></> },
    ] });
    history.replaceState(null, '', '/');
    document.body.innerHTML = '<div id="root"></div>';
    await act(async () => { root = await hydrateSite(own); });
    await click(link('Malformed fragment'));
    await settled();
    expect(location.pathname + location.hash).toBe('/#%');
    expect(document.querySelector('h1')?.textContent).toBe('Home');
    expect(window.scrollTo).toHaveBeenCalledWith(0, 0);
    expect(errors).not.toHaveBeenCalled();
  });
});


describe('base-relative routing', () => {
  it.each([
    ['/docs', '/docs?query=1', '/'],
    ['/docs', '/docs/', '/'],
    ['/docs', '/DOCS/a%20b/', '/a b'],
    ['/d%6Fcs', '/docs/a%20b/', '/a b'],
    ['/docs', '/docs/part%2Fname/', '/part%2Fname'],
    ['/docs', '/docs/bad%zz/', '/bad%zz'],
    ['/docs', '/docs-other/page/', '~/docs-other/page'],
  ])('agrees on SSR and hydration for base %s at %s', async (base, path, expected) => {
    const own = defineSite({ base,
      App: ({ children }) => <><p id="current-path">{usePath()}</p>{children}</>,
      routes: ['/', '/a b/', '/part%2Fname/', '/bad%zz/'].map(path => ({ path, component: () => <h1>Known page</h1> })),
      notFound: { component: () => <h1>Unknown page</h1> },
    });
    history.replaceState(null, '', path);
    const html = renderToString(<SiteRoot site={own} ssrPath={path} />);
    document.head.innerHTML = '';
    document.body.innerHTML = `<div id="root">${html}</div>`;
    const heading = document.querySelector('h1');
    expect(document.getElementById('current-path')?.textContent).toBe(expected);
    expect(heading?.textContent).toBe(expected.startsWith('~') ? 'Unknown page' : 'Known page');
    await act(async () => { root = await hydrateSite(own); });
    expect(document.getElementById('current-path')?.textContent).toBe(expected);
    expect(document.querySelector('h1')).toBe(heading);
    expect(errors).not.toHaveBeenCalled();
  });

  it('keeps base-prefixed links, query/hash, replace and Back/Forward on the same history', async () => {
    const own = defineSite({ base: '/docs', origin: 'https://example.test',
      App: ({ children }) => <><nav><Link href="/next/?query=1">Next</Link><Link href="/other/" replace>Replace</Link><Link href="/other/#target">Details</Link></nav><main id="main">{children}</main></>,
      head: { titleTemplate: '%s | Test' },
      routes: ['/', '/next/', '/other/'].map(path => ({ path, head: { title: path }, component: () => <><h1>{path}</h1><h2 id="target">Target</h2></> })),
    });
    history.replaceState(null, '', '/docs/');
    const { html, head } = await renderRoute(own, own.routes[0]!);
    document.head.innerHTML = head;
    document.body.innerHTML = `<div id="root">${html}</div>`;
    await act(async () => { root = await hydrateSite(own); });
    const length = history.length;
    Element.prototype.scrollIntoView = vi.fn();
    expect(link('Next').getAttribute('href')).toBe('/docs/next/?query=1');
    await click(link('Next'));
    expect(location.pathname + location.search + location.hash).toBe('/docs/next/?query=1');
    expect(history.length).toBe(length + 1);
    expect(document.title).toBe('/next/ | Test');
    expect(document.activeElement).toBe(document.querySelector('h1'));
    expect(document.getElementById('target')?.scrollIntoView).not.toHaveBeenCalled();
    await click(link('Replace'));
    expect(location.pathname).toBe('/docs/other/');
    expect(history.length).toBe(length + 1);
    for (const [move, expected] of [[() => history.back(), '/'], [() => history.forward(), '/other/']] as const) {
      await act(async () => { move(); await new Promise(resolve => setTimeout(resolve, 30)); });
      expect(document.querySelector('h1')?.textContent).toBe(expected);
      expect(document.title).toBe(`${expected} | Test`);
      expect(history.length).toBe(length + 1);
    }
    await click(link('Details'));
    expect(location.pathname + location.hash).toBe('/docs/other/#target');
    expect(document.getElementById('target')?.scrollIntoView).toHaveBeenCalledOnce();
    expect(errors).not.toHaveBeenCalled();
  });

  it('delegates base-prefixed links in retained Static HTML and leaves native link choices alone', async () => {
    const own = defineSite({ base: '/docs', routes: [
      { path: '/', component: () => <><h1>Start</h1><Static render={() => <p><a href="/docs/next/?query=1">Static next</a></p>} />
        <Link href="/next/" target="_blank">New tab</Link><Link href="/next/" download>Download</Link><Link href="/next/" onClick={event => event.preventDefault()}>Cancelled</Link></> },
      { path: '/next/', component: () => <><h1>Next</h1><h2 id="target">Target</h2></> },
    ] });
    history.replaceState(null, '', '/docs/');
    const { html } = await renderRoute(own, own.routes[0]!);
    document.body.innerHTML = `<div id="root">${html}</div>`;
    await act(async () => { root = await hydrateSite(own); });
    let prevented = true;
    // Record after React's root handler, then stop jsdom's native navigation.
    const record = (event: Event) => { prevented = event.defaultPrevented; event.preventDefault(); };
    document.addEventListener('click', record);
    try {
      for (const name of ['New tab', 'Download']) {
        await click(link(name));
        expect(prevented, name).toBe(false);
      }
      await click(link('Cancelled'));
      expect(prevented).toBe(true);
      expect(location.pathname).toBe('/docs/');
    } finally { document.removeEventListener('click', record); }
    await click(link('Static next'));
    expect(location.pathname + location.search + location.hash).toBe('/docs/next/?query=1');
    expect(document.querySelector('h1')?.textContent).toBe('Next');
    expect(errors).not.toHaveBeenCalled();
  });
});
