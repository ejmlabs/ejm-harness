#!/usr/bin/env node
/**
 * Size and laziness check for the demo islands (the 40 KB island budget in QUALITY.md).
 *
 * 1. Builds each island entry (islands/cli.tsx, ide.tsx, harness.tsx) with Vite in a temp dir, React left out, and prints the gzip size of
 *    the island and of the code it shares with the others. The budget is on the island with everything it imports, React excluded.
 * 2. Builds a tiny page that lazy-loads a demo through LazyDemo, with React included, and proves the island is a separate chunk that the
 *    page's entry does not import: none of the island-only strings may appear in the entry or in anything it imports statically.
 *
 * 3. First-load cost. Builds a page per entry (React included) and prints the initial JavaScript each adds on top of an empty React page:
 *    the per-product lazy entries (cli-lazy, ide-lazy, harness-lazy), the all-products LazyDemo, and ProductHeroStage with one window. A page
 *    that shows one product must not carry the frames or view code of the other two: the check looks for strings only those frames hold.
 *
 * Usage: node packages/demos/scripts/size-islands.mjs [--budget 40] [--first-load 14]
 *   Exit 1 when over budget, when island code is in the entry, when a one-product page carries another product's view, or when a
 *   one-product page adds more than --first-load KB gzip over the empty React page (default 14; it includes `cn` and ProductWindow, about 7 KB, which a site's shell ships anyway).
 */
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { gzipSync } from 'node:zlib';

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, '../../..');
const require = createRequire(join(repo, 'package.json'));
const { build } = await import(pathToFileURL(require.resolve('vite')).href);
const react = (await import(pathToFileURL(require.resolve('@vitejs/plugin-react')).href)).default;
const flag = (name, fallback) => Number(process.argv[process.argv.indexOf(name) + 1]) || fallback;
const budget = flag('--budget', 40);
const firstLoadBudget = flag('--first-load', 14);
const gz = (code) => gzipSync(code, { level: 9 }).length;
const kb = (n) => (n / 1024).toFixed(1).padStart(6) + ' KB';
const src = (f) => join(repo, 'packages/demos/src', f);

async function bundle(root, input, external) {
  const out = join(root, 'out');
  const result = await build({
    root, configFile: false, logLevel: 'silent', plugins: [react()],
    resolve: { conditions: ['development'] },
    build: { outDir: out, emptyOutDir: true, minify: true, target: 'es2022', write: false, rollupOptions: { input, external, preserveEntrySignatures: 'strict', output: { format: 'es' } } },
  });
  const chunks = (Array.isArray(result) ? result : [result]).flatMap((r) => r.output).filter((o) => o.type === 'chunk');
  return chunks;
}

/** The chunk and every chunk it imports statically (a dynamic import is not part of it). */
const closure = (chunks, entry) => {
  const byName = new Map(chunks.map((c) => [c.fileName, c]));
  const seen = new Set();
  const walk = (c) => { if (seen.has(c.fileName)) return; seen.add(c.fileName); c.imports.forEach((i) => byName.has(i) && walk(byName.get(i))); };
  walk(entry);
  return [...seen].map((n) => byName.get(n));
};

// Inside node_modules, so the temp page resolves react-dom like the rest of the repo. Git ignores it and it is removed at the end.
const cache = join(repo, 'node_modules/.cache');
mkdirSync(cache, { recursive: true });
const tmp = mkdtempSync(join(cache, 'ejm-islands-'));
let failed = false;
try {
  // 1. Each island on its own, React external.
  const names = ['cli', 'ide', 'harness'];
  const chunks = await bundle(tmp, Object.fromEntries(names.map((n) => [n, src(`islands/${n}.tsx`)])), [/^react($|\/)/, /^react-dom($|\/)/]);
  const users = new Map();
  for (const n of names) for (const c of closure(chunks, chunks.find((x) => x.isEntry && x.name === n))) users.set(c.fileName, (users.get(c.fileName) ?? 0) + 1);
  console.log('Islands, gzip, React excluded (budget ' + budget + ' KB, island plus everything it imports)\n');
  for (const n of names) {
    const entry = chunks.find((x) => x.isEntry && x.name === n);
    const all = closure(chunks, entry);
    const total = all.reduce((s, c) => s + gz(c.code), 0);
    const shared = all.filter((c) => c !== entry && users.get(c.fileName) > 1).reduce((s, c) => s + gz(c.code), 0);
    const over = total > budget * 1024;
    failed ||= over;
    console.log(`  ${n.padEnd(8)} entry ${kb(gz(entry.code))}   shared with another island ${kb(shared)}   total ${kb(total)}   ${over ? 'OVER BUDGET' : 'ok'}`);
  }

  // 2. A page that lazy-loads a demo: the island must be its own chunk, outside the entry's static closure.
  const page = join(tmp, 'page.tsx');
  writeFileSync(page, `import { createRoot } from 'react-dom/client';\nimport { LazyDemo } from ${JSON.stringify(src('islands/LazyDemo.tsx'))};\ncreateRoot(document.body).render(<LazyDemo product="cli" />);\n`);
  const pageChunks = await bundle(tmp, { page }, undefined);
  const entry = pageChunks.find((c) => c.isEntry);
  const initial = closure(pageChunks, entry);
  const islandOnly = ['Run started.', 'Skip past this demo', 'Streaming', 'Grant write'];
  const leaked = islandOnly.filter((s) => initial.some((c) => c.code.includes(s)));
  const lazy = pageChunks.filter((c) => !initial.includes(c));
  const initialGz = initial.reduce((s, c) => s + gz(c.code), 0);
  console.log('\nA page with <LazyDemo product="cli" />, React included\n');
  console.log(`  initial JavaScript (entry and static imports)  ${kb(initialGz)}   in ${initial.length} chunk(s)`);
  for (const c of lazy) console.log(`  lazy chunk ${c.fileName.padEnd(28)} ${kb(gz(c.code))}   island strings: ${islandOnly.filter((s) => c.code.includes(s)).join(', ') || 'none'}`);
  const found = lazy.length > 0 && islandOnly.every((s) => lazy.some((c) => c.code.includes(s)));
  console.log(`  island strings in the initial JavaScript: ${leaked.length ? leaked.join(', ') + '  LEAK' : 'none'}`);
  console.log(`  island is a separate chunk: ${found ? 'yes' : 'NO'}`);
  failed ||= leaked.length > 0 || !found;

  // 3. First-load cost of the entries.
  const emptyPage = `import { createRoot } from 'react-dom/client';\ncreateRoot(document.body).render(<p>x</p>);\n`;
  const initialOf = async (name, source) => {
    const file = join(tmp, `${name}.tsx`);
    writeFileSync(file, source);
    const out = await bundle(tmp, { [name]: file }, undefined);
    const closed = closure(out, out.find((c) => c.isEntry));
    return { size: closed.reduce((s, c) => s + gz(c.code), 0), code: closed.map((c) => c.code).join('\n'), lazyCode: out.filter((c) => !closed.includes(c)).map((c) => c.code).join('\n'), lazySize: out.filter((c) => !closed.includes(c)).reduce((s, c) => s + gz(c.code), 0) };
  };
  const mount = (imports, jsx) => `import { createRoot } from 'react-dom/client';\n${imports}\ncreateRoot(document.body).render(${jsx});\n`;
  // Strings only one product's frame holds, to tell whose view is in a bundle.
  const strings = (p) => { const out = new Set(); JSON.stringify(JSON.parse(readFileSync(src(`views/frames/${p}.json`), 'utf8')), (k, v) => (typeof v === 'string' && v.length >= 14 && out.add(v), v)); return out; };
  const frameStrings = Object.fromEntries(names.map((n) => [n, strings(n)]));
  const only = (n) => [...frameStrings[n]].filter((s) => names.filter((m) => m !== n).every((m) => !frameStrings[m].has(s))).slice(0, 4).map((s) => JSON.stringify(s).slice(1, -1));
  const mark = Object.fromEntries(names.map((n) => [n, only(n)]));
  const base = (await initialOf('empty', emptyPage)).size;
  const Cap = (n) => n[0].toUpperCase() + n.slice(1);
  // Build-only counter data, never a claimed product capture. The late marker must stay in its lazy data chunk.
  const recordedMarker = 'RECORDED_SIZE_TIMELINE_ONLY';
  const recordedId = 'a'.repeat(64);
  const recordedFile = join(tmp, 'recorded-size-data.js');
  writeFileSync(recordedFile, `export default ${JSON.stringify({ id: recordedId, beats: [{ at: 0, update: { blocks: [{ id: 1, kind: 'notice', title: 'Size fixture', lines: [{ t: recordedMarker, r: null }] }] } }] })};\n`);
  const recordedFrame = { provider: 'size fixture', cwd: 'fixture', title: 'Recorded size fixture', sessionId: 'test', write: false, agents: 0, models: 0, tools: 0, blocks: [], hints: [] };
  const recordedPage = mount(`import { LazyCliShowcase } from ${JSON.stringify(src('islands/cli-lazy.tsx'))};\nconst frame = ${JSON.stringify(recordedFrame)};\nconst source = { id: ${JSON.stringify(recordedId)}, initialFrame: frame, finalFrame: frame, load: () => import(${JSON.stringify(recordedFile)}).then(m => m.default) };`, '<LazyCliShowcase source={source} />');
  const harnessRecordedMarker = 'RECORDED_HARNESS_SIZE_TIMELINE_ONLY';
  const harnessRecordedFile = join(tmp, 'recorded-harness-size-data.js');
  writeFileSync(harnessRecordedFile, `export default ${JSON.stringify({ id: recordedId, beats: [{ at: 0, update: { journal: [{ seq: 1, tick: 0, time: '+0.00s', event: harnessRecordedMarker, detail: 'Size fixture', level: 'info', session: null }] } }] })};\n`);
  const harnessRecordedFrame = { limit: 3, commands: 2, stats: { running: 0, queued: 0, done: 0, commandsHeld: 0 }, sessions: [], journal: [], sample: { id: 'rust', label: 'Size fixture', caption: 'Harness recorded size fixture', code: '// size fixture' } };
  const harnessRecordedPage = mount(`import { LazyHarnessShowcase } from ${JSON.stringify(src('islands/harness-lazy.tsx'))};\nconst frame = ${JSON.stringify(harnessRecordedFrame)};\nconst source = { id: ${JSON.stringify(recordedId)}, initialFrame: frame, finalFrame: frame, load: () => import(${JSON.stringify(harnessRecordedFile)}).then(m => m.default) };`, '<LazyHarnessShowcase source={source} />');
  const cases = [
    { label: 'LazyHarnessShowcase (caller data)', only: 'harness', required: 'Harness recorded size fixture', recordingMarker: harnessRecordedMarker, source: harnessRecordedPage },
    { label: 'LazyCliShowcase (caller data)', only: 'cli', required: 'Recorded size fixture', recordingMarker: recordedMarker, source: recordedPage },
    ...names.map((n) => ({ label: `Lazy${Cap(n)}Demo (${n}-lazy)`, only: n, ...{ source: mount(`import { Lazy${Cap(n)}Demo } from ${JSON.stringify(src(`islands/${n}-lazy.tsx`))};`, `<Lazy${Cap(n)}Demo />`) } })),
    { label: 'ProductHeroStage with CliHeroWindow', only: 'cli', source: mount(`import { ProductHeroStage, CliHeroWindow } from ${JSON.stringify(src('hero/ProductHero.tsx'))};`, '<ProductHeroStage title="t" windows={<CliHeroWindow />} />') },
    { label: 'LazyDemo product="cli" (all three views)', only: null, source: mount(`import { LazyDemo } from ${JSON.stringify(src('islands/LazyDemo.tsx'))};`, '<LazyDemo product="cli" />') },
    { label: 'ProductHero (all three windows)', only: null, source: mount(`import { ProductHero } from ${JSON.stringify(src('hero/ProductHero.tsx'))};`, '<ProductHero title="t" />') },
  ];
  console.log(`\nFirst load added by each entry, gzip, over an empty React page (${kb(base).trim()}); budget ${firstLoadBudget} KB for a one-product page\n`);
  for (const c of cases) {
    const r = await initialOf(c.label.replace(/\W+/g, '-'), c.source);
    const added = r.size - base;
    const foreign = c.only ? names.filter((n) => n !== c.only).filter((n) => mark[n].some((s) => r.code.includes(s))) : [];
    const own = c.required ? r.code.includes(c.required) : !c.only || mark[c.only].some((s) => r.code.includes(s));
    const over = c.only && added > firstLoadBudget * 1024;
    const recordingLeak = c.recordingMarker && (r.code.includes(c.recordingMarker) || !r.lazyCode.includes(c.recordingMarker) || r.lazySize > budget * 1024 || names.filter((n) => n !== c.only).some((n) => mark[n].some((s) => r.lazyCode.includes(s))) || ['Grant write', 'Crash worker'].some((s) => r.lazyCode.includes(s)));
    failed ||= foreign.length > 0 || !own || over || recordingLeak;
    if (c.recordingMarker) console.log(`  recorded lazy closure ${kb(r.lazySize)} ${recordingLeak ? 'LEAK, MISSING OR OVER BUDGET' : 'data stays lazy, ok'}`);
    console.log(`  ${c.label.padEnd(44)} +${kb(added)}   ${c.only ? (foreign.length ? 'LEAK: ' + foreign.join(', ') + ' view' : over ? 'OVER BUDGET' : own ? 'only the ' + c.only + ' view' : 'MISSING the ' + c.only + ' view') : 'all three views'}`);
  }
  // Additive native previews retain server excerpts without shipping them in the
  // initial browser closure. Their complete per-product player/data stays lazy.
  const previewMarkers = { cli: 'done · shell cargo test --quiet', ide: 'workbench_write_file', harness: 'reopened saved session: Harbor checks' };
  console.log('\nNative previews, initial addition and complete lazy closure, gzip\n');
  for (const name of names) {
    const r = await initialOf(`${name}-preview`, mount(`import { Lazy${Cap(name)}Preview } from ${JSON.stringify(src(`islands/${name}-preview.tsx`))};`, `<Lazy${Cap(name)}Preview />`));
    const added = r.size - base;
    const foreign = names.filter(other => other !== name && (r.code + r.lazyCode).includes(previewMarkers[other]));
    const invalid = added > firstLoadBudget * 1024 || r.lazySize > budget * 1024 || r.code.includes(previewMarkers[name]) || !r.lazyCode.includes(previewMarkers[name]) || foreign.length > 0;
    failed ||= invalid;
    console.log(`  Lazy${Cap(name)}Preview initial +${kb(added)}, lazy ${kb(r.lazySize)}   ${invalid ? 'MISSING, LEAK OR OVER BUDGET' : 'one product, stays lazy, ok'}`);
  }
} finally {
  rmSync(tmp, { recursive: true, force: true });
}
process.exit(failed ? 1 : 0);
