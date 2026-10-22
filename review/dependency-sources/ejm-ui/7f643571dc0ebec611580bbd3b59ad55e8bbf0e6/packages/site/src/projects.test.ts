import { existsSync, readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import type { ViteDevServer } from 'vite';
import { describe, expect, it, vi } from 'vitest';
import { siteDefaults } from './head';
import { PROJECT_LIST, PROJECTS, assetPaths, otherSites, sourceLinks } from './projects.mjs';
import { ejmSite, headTags, sharedFiles } from './vite.mjs';

const require = createRequire(import.meta.url);

describe('project registry', () => {
  it('keeps the four published discovery identities and their configured tokens unchanged', () => {
    expect(PROJECT_LIST.map(p => p.host)).toEqual(['ejmlabs.com', 'cli.ejmlabs.com', 'ide.ejmlabs.com', 'harness.ejmlabs.com']);
    for (const p of PROJECT_LIST) {
      expect(p.url).toBe(`https://${p.host}/`);
      expect(p.analyticsToken).toMatch(/^[0-9a-f]{32}$/);
      expect(p.repo).toMatch(/^https:\/\/github\.com\/ejmlabs/);
    }
    expect(new Set(PROJECT_LIST.map(p => p.analyticsToken)).size).toBe(4);
  });

  it.each(['ejmlabs', 'cli', 'ide', 'harness'] as const)('preserves the published external navigation for %s', id => {
    const published = [PROJECTS.ejmlabs, PROJECTS.cli, PROJECTS.ide, PROJECTS.harness];
    expect(otherSites(id)).toEqual(published.filter(p => p.id !== id).map(p => ({ label: p.name, href: p.url, external: true })));
    expect(sourceLinks(id)).toEqual([{ label: `${PROJECTS[id].name} on GitHub`, href: PROJECTS[id].repo, external: true }]);
  });

  it('keeps chess buildable without advertising its unverified destinations or inventing a source repository', () => {
    expect(PROJECTS.chess.discoverable).toBe(false);
    expect(PROJECT_LIST.some(p => p.id === 'chess')).toBe(false);
    expect(siteDefaults('chess').origin).toBe('https://chess.ejmlabs.com');
    expect(PROJECTS.chess.guides).toEqual([
      { label: 'Player guide', href: 'https://chess.ejmlabs.com/help/' },
      { label: 'Developer guide', href: 'https://chess.ejmlabs.com/docs/developer-guide/' },
    ]);
    expect(sourceLinks('chess')).toEqual([]);
    const files = sharedFiles(PROJECTS.chess);
    expect(files['favicon.svg']).toContain('/assets/projects/chess/');
    expect(JSON.parse(readFileSync(files['site.webmanifest']!, 'utf8')).name).toBe('Chess by ejmlabs');
  });

  it('falls back to the ejmlabs brand files for a project without a folder', () => {
    expect(assetPaths(PROJECTS.cli).favicon).toBe('favicon');
    expect(assetPaths(PROJECTS.cli, d => d === 'projects/cli')).toMatchObject({ favicon: 'projects/cli', logoInk: 'projects/cli/logo-ink.svg', socialCard: 'projects/cli/social-card.png' });
  });

  it('cli, ide and harness serve their own icons and social card, ejmlabs keeps the root set', () => {
    const own = (id: 'ejmlabs' | 'cli' | 'ide' | 'harness') => sharedFiles(PROJECTS[id]);
    for (const id of ['cli', 'ide', 'harness'] as const) {
      const files = own(id);
      expect(files['favicon.svg'], id).toContain(`/assets/projects/${id}/`);
      expect(files['apple-touch-icon.png'], id).toContain(`/assets/projects/${id}/`);
      expect(files['social-card.png'], id).toContain(`/assets/projects/${id}/`);
      // Different artwork per project, not the same bytes under three names.
      expect(readFileSync(files['favicon.svg']!), id).not.toEqual(readFileSync(own('ejmlabs')['favicon.svg']!));
    }
    expect(own('ejmlabs')['social-card.png']).toMatch(/assets\/social\/social-card\.png$/);
    expect(new Set(['cli', 'ide', 'harness'].map(id => readFileSync(own(id as 'cli')['social-card.png']!).toString('hex'))).size).toBe(3);
  });

  it('every shared file the plugin serves exists in @ejm/ui and @ejm/tokens', () => {
    for (const p of Object.values(PROJECTS)) for (const [name, path] of Object.entries(sharedFiles(p))) expect(existsSync(path), `${p.id}: ${name}`).toBe(true);
  });

  it('serves the original font notices as plain text for every project in development', () => {
    for (const project of PROJECT_LIST) {
      const use = vi.fn();
      const configure = ejmSite({ project }).configureServer;
      if (typeof configure !== 'function') throw new Error('Missing development server hook');
      configure.call({} as ThisParameterType<typeof configure>, { middlewares: { use } } as unknown as ViteDevServer);
      const serve = use.mock.calls[0]![0];
      for (const name of ['Manrope-OFL.txt', 'IBMPlexMono-OFL.txt']) {
        const setHeader = vi.fn();
        const end = vi.fn();
        const next = vi.fn();
        serve({ url: `/${name}?download=1` }, { setHeader, end }, next);
        expect(setHeader).toHaveBeenCalledWith('Content-Type', 'text/plain; charset=utf-8');
        expect(end).toHaveBeenCalledWith(readFileSync(require.resolve(`@ejm/ui/fonts/${name}`)));
        expect(next).not.toHaveBeenCalled();
      }
    }
  });
});

describe('shared head', () => {
  it('adds the theme boot as an external synchronous script before the stylesheet, with color-scheme and icons', () => {
    const html = headTags('/').join('\n');
    expect(html).toContain('<meta name="color-scheme" content="light dark">');
    expect(html).toContain('<script src="/theme-boot.js"></script>');
    expect(html).not.toMatch(/<script(?![^>]*\ssrc=)/);
    expect(html).not.toMatch(/<script[^>]*(defer|async|type="module")/);
    expect(html).toContain('rel="manifest" href="/site.webmanifest"');
    expect(headTags('/docs/').join('')).toContain('src="/docs/theme-boot.js"');
  });

  it('gives defineSite the origin, title template, social card and font preloads', () => {
    const d = siteDefaults('harness');
    expect(d.origin).toBe('https://harness.ejmlabs.com');
    expect(d.head.titleTemplate).toBe('%s | ejm-harness');
    expect(d.head.og).toMatchObject({ siteName: 'ejm-harness', image: '/social-card.png' });
    expect(d.head.meta).toContainEqual({ name: 'twitter:card', content: 'summary_large_image' });
    expect(d.fonts).toHaveLength(2);
    expect(siteDefaults('cli', { description: 'Mine.' }).head.description).toBe('Mine.');
  });
});
