import { execFileSync, spawnSync } from 'node:child_process';
import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import { beforeAll, describe, expect, it } from 'vitest';
import { headersFile } from '@ejm/site/headers';
import { docRoutes, pages } from './pages';

// A real build of the gallery through the ejm-ssg CLI, then the files it wrote. DOMParser only parses: no script runs, which is what a
// visitor with scripts turned off gets. Every route is checked, so adding a page to src/pages.ts adds it to this suite.
const here = dirname(fileURLToPath(import.meta.url));
const app = join(here, '..');
const dist = join(app, 'dist');
const ssgBin = join(here, '..', '..', '..', 'packages', 'ssg', 'bin', 'ejm-ssg.mjs');
const ORIGIN = 'https://ui.ejmlabs.com';
const SITE_TITLE = 'ejm-ui, the ejmlabs design system';

const expected = [
  { path: '/', title: SITE_TITLE, description: /ejm-ui is the ejmlabs design system/ },
  ...[...pages, ...docRoutes].map(p => ({ path: p.path, title: `${p.title} | ejm-ui`, description: p.description })),
];
const fileOf = (path: string) => (path === '/' ? 'index.html' : `${path.slice(1)}index.html`);
const read = (file: string) => new DOMParser().parseFromString(readFileSync(join(dist, file), 'utf8'), 'text/html');
const htmlFiles = (dir: string): string[] =>
  readdirSync(dir).flatMap(n => {
    const p = join(dir, n);
    return statSync(p).isDirectory() ? (n === 'assets' ? [] : htmlFiles(p)) : n.endsWith('.html') ? [relative(dist, p)] : [];
  });

beforeAll(() => {
  execFileSync('node', [ssgBin, 'build'], { cwd: app, stdio: 'pipe', env: { ...process.env, NODE_ENV: 'production' } });
}, 240_000);

it('writes one static page for every route in src/pages.ts, plus the 404 page, and nothing else', () => {
  expect(htmlFiles(dist).sort()).toEqual([...expected.map(r => fileOf(r.path)), '404.html'].sort());
});

describe.each(expected)('$path', route => {
  const doc = () => read(fileOf(route.path));

  it('has a skip link first, one h1 with text, one main, and the page text without any script', () => {
    const d = doc();
    const first = d.body.querySelector('a')!;
    expect(first.classList.contains('skip-link')).toBe(true);
    expect(first.getAttribute('href')).toBe('#main');
    const h1s = d.querySelectorAll('h1');
    expect(h1s).toHaveLength(1);
    expect(h1s[0]!.textContent!.trim().length).toBeGreaterThan(3);
    expect(d.querySelectorAll('main#main')).toHaveLength(1);
    expect(d.querySelector('main#main')!.textContent!.trim().length, 'the page body is in the HTML').toBeGreaterThan(300);
    expect(d.querySelector('main#main h1'), 'the h1 is inside main').not.toBeNull();
  });

  it('puts the theme toggle last in the header, with nothing usable after it', () => {
    const d = doc();
    const header = d.querySelector('header')!;
    const toggle = header.querySelector<HTMLButtonElement>('button[aria-label^="Switch to"]')!;
    expect(toggle).not.toBeNull();
    for (const later of header.querySelectorAll('a, button, details, summary, input')) {
      if (later !== toggle) {
        const after = toggle.compareDocumentPosition(later) & Node.DOCUMENT_POSITION_FOLLOWING;
        expect(after, later.outerHTML.slice(0, 80)).toBe(0);
      }
    }
    expect(toggle.nextElementSibling?.getAttribute('role')).toBe('status');
  });

  it('has its own title, description and canonical URL', () => {
    const d = doc();
    expect(d.title).toBe(route.title);
    const description = d.querySelector('meta[name=description]')!.getAttribute('content')!;
    if (typeof route.description === 'string') expect(description).toBe(route.description);
    else expect(description).toMatch(route.description);
    expect(d.querySelector('link[rel=canonical]')!.getAttribute('href')).toBe(ORIGIN + route.path);
    expect(d.querySelector('meta[property="og:url"]')!.getAttribute('content')).toBe(ORIGIN + route.path);
    expect(d.querySelector('meta[property="og:title"]')!.getAttribute('content')).toBe(route.title);
    expect(d.querySelector('meta[name=robots]')).toBeNull();
  });
});

describe('every page', () => {
  const all = [...expected.map(r => fileOf(r.path)), '404.html'];

  it('has a different title and description on each route', () => {
    const titles = expected.map(r => read(fileOf(r.path)).title);
    const descriptions = expected.map(r => read(fileOf(r.path)).querySelector('meta[name=description]')!.getAttribute('content'));
    expect(new Set(titles).size).toBe(titles.length);
    expect(new Set(descriptions).size).toBe(descriptions.length);
  });

  it('boots the theme from an external script before any stylesheet, and runs no inline or analytics script', () => {
    for (const file of all) {
      const d = read(file);
      const head = [...d.head.children];
      const boot = d.querySelector('script[src="/theme-boot.js"]')!;
      expect(boot, file).not.toBeNull();
      expect(boot.hasAttribute('defer') || boot.hasAttribute('async') || boot.getAttribute('type') === 'module', file).toBe(false);
      expect(head.indexOf(boot), file).toBeLessThan(head.findIndex(n => n.matches('link[rel=stylesheet]')));
      expect(d.documentElement.getAttribute('lang')).toBe('en');
      expect(d.querySelector('meta[name=color-scheme]')?.getAttribute('content')).toBe('light dark');
      for (const s of d.querySelectorAll('script')) expect(s.getAttribute('src'), `${file}: inline script`).toBeTruthy();
      expect(d.documentElement.outerHTML, file).not.toContain('cloudflareinsights');
    }
  });

  it('keeps the theme attribute out of the React tree: <html> is the static template, so hydration cannot warn about it', () => {
    for (const file of all) {
      const html = readFileSync(join(dist, file), 'utf8');
      expect(html, file).toMatch(/^<!doctype html>\s*<html lang="en">/i);
      expect(html.match(/<html[ >]/g), file).toHaveLength(1);
    }
  });

  it('links only to pages that exist, and marks other sites as external', () => {
    const real = new Set(expected.map(r => r.path.replace(/\/$/, '') || '/'));
    for (const file of all) {
      for (const a of read(file).querySelectorAll<HTMLAnchorElement>('a[href]')) {
        const href = a.getAttribute('href')!;
        if (/^https?:/.test(href)) expect(a.getAttribute('rel'), `${file}: ${href}`).toMatch(/\bexternal\b/);
        else if (href.startsWith('/')) {
          const path = href.split(/[?#]/)[0]!.replace(/\/$/, '') || '/';
          if (/\.[a-z0-9]+$/.test(path)) expect(existsSync(join(dist, path)), `${file}: ${href}`).toBe(true);
          else expect(real.has(path), `${file}: ${href}`).toBe(true);
        }
      }
    }
  });

  it('uses no old studio or workbench wording', () => {
    for (const file of all) {
      const text = readFileSync(join(dist, file), 'utf8');
      expect(text, file).not.toMatch(/independent studio|>\s*Workbench\s*<|>\s*We /);
    }
  });
});

describe('the 404 page', () => {
  it('is noindex, has no canonical, and still has the frame and one h1', () => {
    const d = read('404.html');
    expect(d.querySelector('meta[name=robots]')!.getAttribute('content')).toBe('noindex');
    expect(d.querySelector('link[rel=canonical]')).toBeNull();
    expect(d.querySelectorAll('h1')).toHaveLength(1);
    expect(d.querySelectorAll('main#main')).toHaveLength(1);
    expect(d.title).toBe('Page not found | ejm-ui');
  });
});

describe('the build output', () => {
  it('writes the shared files and a _headers that allows no inline script and no analytics beacon', () => {
    for (const f of ['_headers', 'theme-boot.js', 'favicon.svg', 'favicon.ico', 'apple-touch-icon.png', 'site.webmanifest', 'social-card.png']) expect(existsSync(join(dist, f)), f).toBe(true);
    expect(existsSync(join(dist, '.vite'))).toBe(false);
    const headers = readFileSync(join(dist, '_headers'), 'utf8');
    expect(headers).toBe(headersFile({ analytics: false }));
    expect(headers).not.toMatch(/script-src[^;\n]*unsafe-inline/);
    expect(headers).not.toContain('cloudflareinsights');
  });

  it('keeps every route inside the budgets (JS 95 KB, CSS 14 KB, fonts 70 KB, HTML 30 KB gzip)', () => {
    const run = spawnSync('node', [ssgBin, 'budget'], { cwd: app, encoding: 'utf8' });
    expect(run.stdout).toContain('all routes within budget');
    expect(run.status).toBe(0);
    for (const r of expected) expect(run.stdout, r.path).toContain(r.path === '/' ? '\n/ ' : `${r.path.replace(/\/$/, '')}`);
  });
});
