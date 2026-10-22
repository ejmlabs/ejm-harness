import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { FAVICONS, versionedFavicons } from './favicon.mjs';
import { PROJECT_LIST, PROJECTS } from './projects.mjs';
import { ejmSite, headTags, sharedFiles } from './vite.mjs';

const family = project => {
  const files = sharedFiles(project);
  return Object.fromEntries(FAVICONS.map(name => [name, readFileSync(files[name])]));
};
const iconHrefs = tags => tags.filter(tag => /rel="(?:icon|apple-touch-icon|manifest)"/.test(tag)).map(tag => /href="([^"]+)"/.exec(tag)[1]);
const manifestIcons = result => JSON.parse(result.sources['site.webmanifest'].toString()).icons;

describe('favicon publication', () => {
  it('retains each project’s artwork and manifest identity with a distinct content revision', () => {
    const versions = [];
    for (const project of PROJECT_LIST) {
      const original = family(project);
      const result = versionedFavicons(original);
      versions.push(result.version);
      for (const name of FAVICONS.filter(name => name !== 'site.webmanifest')) expect(result.sources[name]).toEqual(original[name]);
      const before = JSON.parse(original['site.webmanifest'].toString());
      const after = JSON.parse(result.sources['site.webmanifest'].toString());
      expect({ ...after, icons: before.icons }).toEqual(before);
      expect(after.icons).toHaveLength(before.icons.length);
      for (const icon of after.icons) expect(new URL(icon.src, project.url).searchParams.get('v')).toBe(result.version);
      expect(versionedFavicons(original)).toEqual(result);
    }
    expect(new Set(versions).size).toBe(PROJECT_LIST.length);
  });

  it('refreshes every head URL and manifest icon reference when any family file changes', () => {
    const original = family(PROJECTS.ejmlabs);
    const before = versionedFavicons(original);
    for (const name of FAVICONS) {
      const changed = name === 'site.webmanifest'
        ? Buffer.from(JSON.stringify({ ...JSON.parse(original[name].toString()), name: 'Updated name' }))
        : Buffer.concat([original[name], Buffer.from([0])]);
      const after = versionedFavicons({ ...original, [name]: changed });
      expect(after.version, name).not.toBe(before.version);
      const hrefs = iconHrefs(headTags('/docs/', after.version));
      expect(hrefs).toHaveLength(4);
      for (const href of hrefs) {
        const url = new URL(href, PROJECTS.ejmlabs.url);
        expect(url.pathname.startsWith('/docs/')).toBe(true);
        expect(url.searchParams.get('v'), name).toBe(after.version);
      }
      for (const icon of manifestIcons(after)) expect(new URL(icon.src, PROJECTS.ejmlabs.url).searchParams.get('v'), name).toBe(after.version);
    }
    expect(iconHrefs(headTags('/docs/')).every(href => href.startsWith('/docs/') && !href.includes('?'))).toBe(true);
  });

  it.each(['ide', 'chess'])('serves the same %s manifest in dev and build while preserving the existing root asset URLs', id => {
    const plugin = ejmSite({ project: id, headers: false });
    plugin.configResolved({ base: '/', build: {} });
    const emitted = new Map();
    plugin.generateBundle.call({ emitFile: asset => emitted.set(asset.fileName, asset.source) });
    expect([...emitted.keys()].sort()).toEqual(Object.keys(sharedFiles(PROJECTS[id])).sort());
    let middleware;
    plugin.configureServer({ middlewares: { use: fn => { middleware = fn; } } });
    let body;
    middleware({ url: '/site.webmanifest?v=current' }, { setHeader() {}, end: value => { body = value; } }, () => { throw new Error('Manifest was not served'); });
    expect(body).toEqual(emitted.get('site.webmanifest'));
    const html = plugin.transformIndexHtml.handler('<head><!--ejm-head--></head>');
    const hrefs = iconHrefs(html.split('\n'));
    expect(hrefs).toHaveLength(4);
    for (const href of hrefs) expect(new URL(href, PROJECTS[id].url).searchParams.get('v')).toBe(versionedFavicons(family(PROJECTS[id])).version);
  });

  it('publishes the chess identity with its own artwork while discovery remains closed', () => {
    const files = sharedFiles(PROJECTS.chess);
    const manifest = JSON.parse(readFileSync(files['site.webmanifest'], 'utf8'));
    expect(manifest.name).toBe('Chess by ejmlabs');
    expect(readFileSync(files['favicon.svg']).equals(readFileSync(sharedFiles(PROJECTS.ejmlabs)['favicon.svg']))).toBe(false);
    expect(PROJECT_LIST.some(project => project.id === 'chess')).toBe(false);
  });
});
