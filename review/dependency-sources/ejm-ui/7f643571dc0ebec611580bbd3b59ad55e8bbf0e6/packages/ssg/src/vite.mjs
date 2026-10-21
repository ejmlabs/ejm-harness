import { mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { dirname, join, posix, relative, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { build } from 'vite';
import { checkBudget, formatReport } from './budget.mjs';

// Plain JavaScript with a .d.mts next to it: Vite loads this file through vite.config.ts, and Node refuses to strip types from
// TypeScript under node_modules. server.ts is not loaded here, it is bundled by the server build below (Vite compiles it).
// The shapes (Site, Route, Rendered, the manifest) are in server.ts, site.ts and Vite's own types.

const FONT_TYPES = { woff2: 'font/woff2', woff: 'font/woff', ttf: 'font/ttf', otf: 'font/otf' };
/** @param {Record<string, { imports?: string[] }>} m */
const closure = (m, key, seen = new Set()) => {
  if (seen.has(key) || !m[key]) return seen;
  seen.add(key);
  for (const k of m[key].imports ?? []) closure(m, k, seen);
  return seen;
};
const attr = (s) => s.replace(/&/g, '&amp;').replace(/"/g, '&quot;');
const outFile = (route, notFound) =>
  route === notFound ? '404.html' : route.path === '/' ? 'index.html' : `${route.path.slice(1)}/index.html`;

/** Prerenders every route after the client build. Use with an `index.html` that holds `<!--ejm-head-->` in the head and `<div id="root"><!--ejm-root--></div>`. */
/** @param {import('./vite.d.mts').EjmSsgOptions} options @returns {import('vite').Plugin} */
export function ejmSsg(options) {
  /** @type {import('vite').ResolvedConfig} */
  let config;
  let written = false;
  return {
    name: 'ejm-ssg',
    apply: 'build',
    config(_, env) {
      return {
        // The server build bundles @ejm/* (they ship TypeScript source); the client build needs the manifest to find route chunks.
        ssr: { noExternal: [/^@ejm\//] },
        // React 19 supplies this exact public shim export; the compatibility implementation is unused.
        resolve: env.isSsrBuild ? {} : { alias: [{ find: /^use-sync-external-store\/shim\/index\.js$/, replacement: 'react' }] },
        // Native imports still work without modulepreload support; only preload acceleration is lost.
        build: env.isSsrBuild ? {} : { manifest: true, modulePreload: { polyfill: false } },
      };
    },
    configResolved(c) {
      config = c;
    },
    buildStart() {
      written = false;
    },
    writeBundle() {
      written = true;
    },
    async closeBundle() {
      // closeBundle also runs after failed builds. Preserve the original diagnostic.
      if (config.build.ssr || !written) return;
      const root = config.root;
      const dist = resolve(root, config.build.outDir);
      const work = join(root, '.ejm-ssg');
      const manifestPath = join(dist, '.vite', 'manifest.json');
      const manifest = JSON.parse(await readFile(manifestPath, 'utf8'));
      const template = await readFile(join(dist, 'index.html'), 'utf8');
      if (!template.includes('<!--ejm-head-->') || !template.includes('<!--ejm-root-->'))
        throw new Error('ejm-ssg: index.html needs <!--ejm-head--> in the head and <!--ejm-root--> inside #root');

      // Server build of the site module, next to the project so bare imports resolve like they do for the client.
      const server = join(work, 'server');
      await rm(work, { recursive: true, force: true });
      await mkdir(work, { recursive: true });
      const siteModule = resolve(root, options.site);
      const serverModule = fileURLToPath(new URL('./server.ts', import.meta.url));
      const entry = join(work, 'entry.js');
      await writeFile(
        entry,
        `import site from ${JSON.stringify(siteModule)};\nexport { renderRoute, allRoutes } from ${JSON.stringify(serverModule)};\nexport { site };\n`,
      );
      const result = await build({
        root,
        configFile: config.configFile,
        mode: config.mode,
        base: config.base, // command line overrides such as --base are not in the config file
        logLevel: 'warn',
        build: { ssr: entry, outDir: server, emptyOutDir: true, minify: false, copyPublicDir: false, manifest: false, emitAssets: true },
      });
      const output = Array.isArray(result) ? result.flatMap(r => r.output) : 'output' in result ? result.output : [];
      const chunks = output.filter(c => c.type === 'chunk');
      // Static parts can import images and fonts absent from the client graph. Keep referenced assets, not server code or CSS.
      const importedAssets = new Set(chunks.flatMap(c => [...(c.viteMetadata?.importedAssets ?? [])]));
      for (const asset of output.filter(c => c.type === 'asset' && importedAssets.has(c.fileName))) {
        const file = join(dist, asset.fileName);
        await mkdir(dirname(file), { recursive: true });
        try {
          await writeFile(file, asset.source, { flag: 'wx' });
        } catch (error) {
          if (error.code !== 'EEXIST') throw error;
          if (!(await readFile(file)).equals(Buffer.from(asset.source)))
            throw new Error(`ejm-ssg: server asset conflicts with client output: ${asset.fileName}`);
        }
      }
      const entryChunk = chunks.find(c => c.isEntry);
      /** @type {{ site: any, renderRoute(site: any, route: any): Promise<{ html: string, head: string }>, allRoutes(site: any): any[] }} */
      const bundle = await import(pathToFileURL(join(server, entryChunk.fileName)).href);
      const { site } = bundle;

      // A lazy route's chunk: its `import('./x-hash.js')` in the server bundle names a chunk, whose source file is the manifest key.
      const facadeOf = new Map(chunks.filter(c => c.isDynamicEntry && c.facadeModuleId).map(c => [c.fileName, relative(root, c.facadeModuleId).split('\\').join('/')]));
      const entryKey = Object.keys(manifest).find(k => manifest[k].isEntry && k.endsWith('.html'));
      const inEntry = closure(manifest, entryKey);
      const base = config.base;
      const chunkTags = route => {
        const spec = route.load && /import\(\s*["']([^"']+)["']\s*\)/.exec(route.load.toString())?.[1];
        const key = spec && facadeOf.get(posix.normalize(posix.join(posix.dirname(entryChunk.fileName), spec)));
        if (!key || !manifest[key]) {
          if (route.load) config.logger.warn(`ejm-ssg: no chunk found for lazy route ${route.path}; it will not be preloaded`);
          return [];
        }
        const keys = [...closure(manifest, key)].filter(k => !inEntry.has(k));
        const css = new Set(keys.flatMap(k => manifest[k].css ?? []));
        for (const f of inEntry) for (const c of manifest[f].css ?? []) css.delete(c);
        return [
          ...[...css].map(f => `<link rel="stylesheet" crossorigin href="${attr(base + f)}">`),
          ...keys.map(k => `<link rel="modulepreload" crossorigin href="${attr(base + manifest[k].file)}">`),
        ];
      };
      const fontTags = site.fonts.map(f => {
        const type = FONT_TYPES[f.split(/[?#]/)[0].split('.').pop().toLowerCase()];
        return `<link rel="preload" as="font"${type ? ` type="${type}"` : ''} crossorigin href="${attr(f)}">`;
      });

      for (const route of bundle.allRoutes(site)) {
        const { html, head } = await bundle.renderRoute(site, route);
        const page = template
          .replace(/<title>[\s\S]*?<\/title>\s*/i, '')
          .replace('<!--ejm-head-->', () => [head, ...fontTags, ...chunkTags(route)].join('\n'))
          .replace('<!--ejm-root-->', () => html);
        const file = join(dist, outFile(route, site.notFound));
        await mkdir(dirname(file), { recursive: true });
        await writeFile(file, page);
      }

      await rm(join(dist, '.vite'), { recursive: true, force: true });
      await rm(server, { recursive: true, force: true });
      await rm(entry, { force: true });
      await writeFile(join(work, 'build.json'), JSON.stringify({ dist, base, budget: site.budget, ignore: site.ignore }, null, 2) + '\n');

      const report = checkBudget(dist, site.budget, { base, ignore: site.ignore });
      console.log(formatReport(report));
      if (report.breaches.length) throw new Error(`ejm-ssg: budget breached\n  ${report.breaches.join('\n  ')}`);
    },
  };
}
