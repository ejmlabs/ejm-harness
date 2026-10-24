import { execFileSync, spawnSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { beforeAll, describe, expect, it } from 'vitest';
import { csp, headersFile } from './headers.mjs';

// A real build of examples/starter through the ejm-ssg CLI, then the files it wrote. DOMParser only parses: no script runs.
const here = dirname(fileURLToPath(import.meta.url));
const starter = join(here, '..', '..', '..', 'examples', 'starter');
const dist = join(starter, 'dist');
const ssgBin = join(here, '..', '..', 'ssg', 'bin', 'ejm-ssg.mjs');
const headersBin = join(here, '..', 'bin', 'ejm-site-headers.mjs');
const ROUTES = ['index.html', 'about/index.html', '404.html'];
const page = (file: string) => new DOMParser().parseFromString(readFileSync(join(dist, file), 'utf8'), 'text/html');

beforeAll(() => {
  execFileSync('node', [ssgBin, 'build'], { cwd: starter, stdio: 'pipe' });
}, 180_000);

describe.each(ROUTES)('%s', file => {
  let doc: Document;
  beforeAll(() => { doc = page(file); });

  it('has a skip link first, one h1, and one main', () => {
    const first = doc.body.querySelector('a')!;
    expect(first.classList.contains('skip-link')).toBe(true);
    expect(first.getAttribute('href')).toBe('#main');
    expect(doc.querySelectorAll('h1')).toHaveLength(1);
    expect(doc.querySelectorAll('main#main')).toHaveLength(1);
  });

  it('puts the theme toggle last among the header tools, after the nav and the menu', () => {
    const toggle = doc.querySelector<HTMLButtonElement>('header button[aria-label^="Switch to"]')!;
    const tools = toggle.parentElement!;
    expect(tools.querySelector('nav')).not.toBeNull();
    const buttons = [...tools.querySelectorAll('button')];
    expect(buttons.at(-1)).toBe(toggle);
    // Nothing a visitor can use follows the toggle: only its own status live region.
    for (const later of tools.querySelectorAll('a, nav, details, button')) {
      if (later !== toggle) expect(toggle.compareDocumentPosition(later) & Node.DOCUMENT_POSITION_FOLLOWING, later.outerHTML.slice(0, 60)).toBe(0);
    }
    expect(toggle.nextElementSibling?.getAttribute('role')).toBe('status');
    expect(tools.nextElementSibling).toBeNull();
  });

  it('boots the theme from an external script before any stylesheet, and runs no inline or analytics script', () => {
    const head = [...doc.head.children];
    const boot = doc.querySelector('script[src="/theme-boot.js"]')!;
    expect(boot.hasAttribute('defer') || boot.hasAttribute('async') || boot.getAttribute('type') === 'module').toBe(false);
    expect(head.indexOf(boot)).toBeLessThan(head.findIndex(n => n.matches('link[rel=stylesheet]')));
    expect(doc.querySelector('meta[name=color-scheme]')?.getAttribute('content')).toBe('light dark');
    for (const s of doc.querySelectorAll('script')) expect(s.getAttribute('src'), 'inline script').toBeTruthy();
    expect(doc.querySelector('script[data-cf-beacon]')).toBeNull();
    expect(doc.documentElement.outerHTML).not.toContain('cloudflareinsights');
  });

  it('marks every link to another site as external', () => {
    for (const a of doc.querySelectorAll<HTMLAnchorElement>('a[href^="http"]')) {
      expect(a.getAttribute('rel'), a.href).toMatch(/\bexternal\b/);
      expect(new URL(a.href).hostname).not.toBe('localhost');
    }
  });
});

describe('the starter build', () => {
  it('the home page links to the other three ejmlabs sites as external', () => {
    const hrefs = [...page('index.html').querySelectorAll('main a[rel~=external]')].map(a => a.getAttribute('href'));
    expect(hrefs).toEqual(['https://cli.ejmlabs.com/', 'https://ide.ejmlabs.com/', 'https://harness.ejmlabs.com/']);
  });

  it('writes the shared files at the site root, and no Vite manifest', () => {
    for (const f of ['_headers', 'theme-boot.js', 'favicon.svg', 'favicon.ico', 'apple-touch-icon.png', 'site.webmanifest', 'social-card.png']) expect(existsSync(join(dist, f)), f).toBe(true);
    expect(existsSync(join(dist, '.vite'))).toBe(false);
    expect(page('index.html').querySelector('meta[property="og:image"]')?.getAttribute('content')).toBe('https://ejmlabs.com/social-card.png');
  });

  it('writes the shared _headers: valid, current, and no inline script allowed', () => {
    const headers = readFileSync(join(dist, '_headers'), 'utf8');
    expect(headers).toBe(headersFile());
    expect(headers).toContain(`Content-Security-Policy: ${csp()}`);
    expect(headers).not.toMatch(/script-src[^;\n]*unsafe-inline/);
    expect(spawnSync('node', [headersBin, join(dist, '_headers'), '--check']).status).toBe(0);
  });

  it('keeps every route inside the budgets', () => {
    const run = spawnSync('node', [ssgBin, 'budget'], { cwd: starter, encoding: 'utf8' });
    expect(run.stdout).toContain('all routes within budget');
    expect(run.status).toBe(0);
  });
});
