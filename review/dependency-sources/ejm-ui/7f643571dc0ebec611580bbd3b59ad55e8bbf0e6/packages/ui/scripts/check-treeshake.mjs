#!/usr/bin/env node
/**
 * A named import from the @ejm/ui barrel must cost what the deep import costs. Builds two tiny Vite apps, React left out:
 *   import { Band } from '@ejm/ui'          (the barrel)
 *   import { Band } from '@ejm/ui/layout/Band'   (deep)
 * and fails when the barrel build is more than 1 KB gzip larger. It also fails when package.json loses `sideEffects: ["*.css"]`,
 * which is what lets any bundler (not only Rollup's own analysis) drop the modules a page does not use while keeping the CSS import.
 * Usage: node packages/ui/scripts/check-treeshake.mjs   (prints both sizes; exit 1 on a regression)
 */
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { gzipSync } from 'node:zlib';

const repo = resolve(dirname(fileURLToPath(import.meta.url)), '../../..');
const require = createRequire(join(repo, 'package.json'));
const { build } = await import(pathToFileURL(require.resolve('vite')).href);
const react = (await import(pathToFileURL(require.resolve('@vitejs/plugin-react')).href)).default;
const TOLERANCE = 1024;

async function bundle(source, tmp) {
  const page = join(tmp, `p${Math.random().toString(36).slice(2)}.tsx`);
  writeFileSync(page, source);
  const result = await build({
    root: tmp, configFile: false, logLevel: 'silent', plugins: [react()],
    build: { write: false, minify: true, target: 'es2022', rollupOptions: { input: page, external: [/^react($|\/)/, /^react-dom($|\/)/], output: { format: 'es' } } },
  });
  const out = (Array.isArray(result) ? result : [result]).flatMap(r => r.output);
  return out.filter(o => o.type === 'chunk');
}

export async function size(source, tmp) {
  return (await bundle(source, tmp)).reduce((n, c) => n + gzipSync(c.code, { level: 9 }).length, 0);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const flag = JSON.parse(readFileSync(join(repo, 'packages/ui/package.json'), 'utf8')).sideEffects;
  if (!Array.isArray(flag) || !flag.includes('*.css')) {
    console.error('FAIL: packages/ui/package.json needs "sideEffects": ["*.css"] (JS modules are pure; CSS imports must stay).');
    process.exit(1);
  }
  const cache = join(repo, 'node_modules/.cache');
  mkdirSync(cache, { recursive: true });
  const tmp = mkdtempSync(join(cache, 'ejm-treeshake-'));
  try {
    const use = from => `import { Band } from '${from}';\nconsole.log(Band({ children: 'x' }));\n`;
    const barrel = await size(use('@ejm/ui'), tmp);
    const deep = await size(use('@ejm/ui/layout/Band'), tmp);
    console.log(`tree-shaking: Band by name from the barrel ${(barrel / 1024).toFixed(1)} KB gzip, deep import ${(deep / 1024).toFixed(1)} KB`);
    if (barrel > deep + TOLERANCE) {
      console.error(`FAIL: the barrel import is ${((barrel - deep) / 1024).toFixed(1)} KB larger. Check "sideEffects" in packages/ui/package.json and top-level statements in the modules.`);
      process.exitCode = 1;
    }
    const controls = await bundle(`import { buttonBaseClass, buttonGhostClass, buttonMediumClass } from '@ejm/ui/control-recipes';
console.log([buttonBaseClass, buttonGhostClass, buttonMediumClass].join(' '));
`, tmp);
    const adapters = controls.flatMap(chunk => Object.keys(chunk.modules)).filter(id => /class-variance-authority|@radix-ui\/react-slot|lucide-react/.test(id));
    if (adapters.length) {
      console.error('FAIL: fixed native control styles must not load variant, slot or icon adapters.', adapters);
      process.exitCode = 1;
    } else {
      console.log('fixed native controls: canonical styles load without variant, slot or icon adapters');
    }
    const docs = await bundle(`import { DocCodeBlock } from '@ejm/ui/docs/DocCodeBlock';
console.log(<DocCodeBlock block={{ type: 'code', code: 'npm test', lang: 'sh' }} />);
`, tmp);
    const byName = new Map(docs.map(chunk => [chunk.fileName, chunk]));
    const entry = docs.find(chunk => chunk.isEntry);
    const tabs = docs.find(chunk => Object.keys(chunk.modules).some(id => id.replaceAll('\\', '/').endsWith('/docs/CodeTabsFrame.tsx')));
    const reachable = new Set();
    const pending = entry ? [entry.fileName] : [];
    while (pending.length) {
      const file = pending.pop();
      if (reachable.has(file)) continue;
      reachable.add(file);
      pending.push(...(byName.get(file)?.imports ?? []).filter(name => byName.has(name)));
    }
    if (!entry || !tabs || reachable.has(tabs.fileName)) {
      console.error('FAIL: plain compiled docs must keep the available tab frame outside their static JavaScript imports.');
      console.error(JSON.stringify(docs.map(({ fileName, imports, dynamicImports }) => ({ fileName, imports, dynamicImports })), null, 2));
      process.exitCode = 1;
    } else {
      console.log('compiled docs: tab controls are deferred outside the static import graph');
    }
  } finally {
    rmSync(tmp, { recursive: true, force: true });
  }
}
