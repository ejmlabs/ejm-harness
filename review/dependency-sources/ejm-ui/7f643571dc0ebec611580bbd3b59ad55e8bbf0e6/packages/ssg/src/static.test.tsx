import { act } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { defineSite, hydrateSite, Static } from './index';
import { renderRoute } from './server';

// The server renders the prose; the browser build passes no `render`, which the flag below stands in for.
let onServer = true;
const Prose = () => (
  <p id="prose">
    Words only in the HTML. <a id="to-about" href="/about/?view=source">About</a> <a id="out" href="https://example.com/">Elsewhere</a> <a id="same-tab" href="/about/" rel="external">Plain</a>
  </p>
);
const Home = () => (
  <>
    <h1>Home</h1>
    <Static render={onServer ? () => <Prose /> : undefined} />
  </>
);
const site = defineSite({
  base: '',
  routes: [
    { path: '/', component: Home, head: { title: 'Home' } },
    { path: '/about/', component: () => <h1>About</h1>, head: { title: 'About' } },
  ],
});

type Root = Awaited<ReturnType<typeof hydrateSite>>;
let root: Root | undefined;
let errors: ReturnType<typeof vi.spyOn>;
const click = (el: Element) => act(async () => void el.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true, button: 0 })));

async function boot() {
  history.replaceState(null, '', '/');
  onServer = true;
  const { html } = await renderRoute(site, site.routes[0]!);
  document.body.innerHTML = `<div id="root">${html}</div>`;
  onServer = false;
  await act(async () => {
    root = await hydrateSite(site);
  });
}

beforeEach(() => {
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  window.scrollTo = vi.fn();
  errors = vi.spyOn(console, 'error').mockImplementation(() => {});
});
afterEach(() => {
  act(() => root?.unmount());
  root = undefined;
  vi.restoreAllMocks();
});

describe('Static', () => {
  it('is in the server HTML in full, so the page reads with scripts off', async () => {
    onServer = true;
    const { html } = await renderRoute(site, site.routes[0]!);
    expect(html).toContain('data-ejm-static');
    expect(html).toContain('Words only in the HTML.');
    onServer = false; // what the browser build renders: the placeholder, no content
    expect((await renderRoute(site, site.routes[0]!)).html).not.toContain('Words');
    onServer = true;
  });

  it('is hydrated without a mismatch and without losing or rebuilding its markup', async () => {
    await boot();
    const prose = document.getElementById('prose');
    expect(prose?.textContent).toContain('Words only in the HTML.');
    expect(errors).not.toHaveBeenCalled();
    await act(async () => void (await new Promise(r => setTimeout(r, 10))));
    expect(document.getElementById('prose')).toBe(prose);
  });

  it('follows an internal link without a reload, and leaves other sites, rel external and modified clicks alone', async () => {
    await boot();
    const reload = vi.fn();
    window.addEventListener('beforeunload', reload);
    await click(document.getElementById('to-about')!);
    await act(async () => void (await new Promise(r => setTimeout(r, 10))));
    expect(location.pathname).toBe('/about/');
    expect(location.search).toBe('?view=source');
    expect(location.hash).toBe('');
    expect(document.querySelector('h1')?.textContent).toBe('About');
    expect(reload).not.toHaveBeenCalled();

    await boot();
    for (const id of ['out', 'same-tab']) {
      const event = new MouseEvent('click', { bubbles: true, cancelable: true, button: 0 });
      await act(async () => void document.getElementById(id)!.dispatchEvent(event));
      expect(event.defaultPrevented, id).toBe(false);
    }
    const modified = new MouseEvent('click', { bubbles: true, cancelable: true, button: 0, ctrlKey: true });
    await act(async () => void document.getElementById('to-about')!.dispatchEvent(modified));
    expect(modified.defaultPrevented).toBe(false);
  });
});
