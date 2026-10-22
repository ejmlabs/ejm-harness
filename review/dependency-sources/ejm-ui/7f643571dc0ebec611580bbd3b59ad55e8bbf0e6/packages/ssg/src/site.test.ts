import { describe, expect, it } from 'vitest';
import { resolveHead } from './head';
import { defineSite, matchRoute, stripBase } from './site';

const Page = () => null;

describe('defineSite', () => {
  const site = defineSite({
    base: '/docs/',
    origin: 'https://example.com/',
    head: { titleTemplate: '%s | Site', title: 'Site', description: 'Default' },
    routes: [
      { path: '/', component: Page },
      { path: '/about/', component: Page, head: { title: 'About', description: 'About us', og: { image: '/a.png' } } },
    ],
  });

  it('matches paths ignoring a trailing slash, query and hash', () => {
    expect(matchRoute(site, '/about')?.path).toBe('/about');
    expect(matchRoute(site, '/about/?x=1#y')?.path).toBe('/about');
    expect(matchRoute(site, '/nope')).toBeUndefined();
  });

  it('strips the base from a pathname', () => {
    expect(stripBase(site, '/docs/about/')).toBe('/about/');
    expect(stripBase(site, '/docs')).toBe('/');
    expect(stripBase(site, '/documents')).toBe('~/documents');
  });

  it('resolves the head: template, canonical with base and declared slash, og defaults', () => {
    const head = resolveHead(site, matchRoute(site, '/about')!);
    expect(head.title).toBe('About | Site');
    const tags = head.tags.map(t => `${t.attrs.name ?? t.attrs.property ?? t.attrs.rel}=${t.attrs.content ?? t.attrs.href}`);
    expect(tags).toContain('canonical=https://example.com/docs/about/');
    expect(tags).toContain('og:image=https://example.com/a.png');
    expect(tags).toContain('description=About us');
    expect(tags).toContain('og:title=About | Site');
  });

  it('merges site and route links, drops undefined values and repeats', () => {
    const own = defineSite({
      head: { links: [{ rel: 'preload', as: 'image', href: '/a.png' }] },
      routes: [{ path: '/', component: Page, head: { links: [{ as: 'image', rel: 'preload', href: '/a.png' }, { rel: 'preload', as: 'image', href: '/b.png', media: undefined }, {}] } }],
    });
    const links = resolveHead(own, own.routes[0]!).tags.filter(t => t.tag === 'link');
    expect(links.map(t => t.attrs)).toEqual([{ rel: 'preload', as: 'image', href: '/a.png' }, { rel: 'preload', as: 'image', href: '/b.png' }]);
  });

  it('uses the site title untemplated when a route has none, and no canonical on the 404', () => {
    expect(resolveHead(site, matchRoute(site, '/')!).title).toBe('Site');
    expect(resolveHead(site, site.notFound).tags.some(t => t.attrs.rel === 'canonical')).toBe(false);
  });

  it('rejects duplicate and relative paths', () => {
    expect(() => defineSite({ routes: [{ path: '/a', component: Page }, { path: '/a/', component: Page }] })).toThrow(/duplicate/);
    expect(() => defineSite({ routes: [{ path: 'a', component: Page }] })).toThrow(/start with/);
  });
});
