#!/usr/bin/env node
/**
 * Honesty gate for the demos. Every command, flag, key combo, environment variable and tool name
 * a demo shows must appear in that product's README. Names that only exist elsewhere (the CLI
 * reference, a crate) go in honesty-allow.json with a source file and a reason, and the gate
 * checks that the source really contains the name. Stale allow entries fail too.
 *
 * Usage: node scripts/check-demos-honesty.mjs [--root DIR]
 * DIR defaults to ~/Desktop, where ejm-cli, ejm-ide and ejm-harness are checked out as siblings.
 * Exit 0 when everything is documented, 1 otherwise.
 */
import { existsSync, readFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const pkg = resolve(here, '..');
const argRoot = process.argv.indexOf('--root');
const root = argRoot > 0 ? resolve(process.argv[argRoot + 1]) : join(homedir(), 'Desktop');
const readJson = (p) => JSON.parse(readFileSync(p, 'utf8'));
const sc = (f) => readJson(join(pkg, 'src/scenarios', f));
const labels = sc('labels.json');

/** Which scenario data and engine file belong to which product. */
const PRODUCTS = {
  'ejm-cli': { data: [sc('cli.json'), labels.cli], engine: ['cli.ts'], skip: ['edits'], preview: 'cli' },
  'ejm-ide': { data: [sc('ide.json'), labels.ide], engine: ['ide.ts'], skip: ['edits'], preview: 'ide' },
  'ejm-harness': { data: [sc('harness.json')], engine: ['harness.ts'], skip: ['langs'] },
};

/** Words the READMEs spell out in full. Kept to one entry; anything else belongs in honesty-allow.json. */
const ALIAS = { Esc: 'Escape' };

/* ---- extraction ---- */

/** Every string in a JSON value, with its key path. Keys named in `skip` (code samples) are left out. */
function* strings(v, skip, path = '') {
  if (typeof v === 'string') yield [path, v];
  else if (Array.isArray(v)) for (const [i, x] of v.entries()) yield* strings(x, skip, `${path}[${i}]`);
  else if (v && typeof v === 'object') for (const [k, x] of Object.entries(v)) if (!skip.includes(k)) yield* strings(x, skip, path ? `${path}.${k}` : k);
}

/** String literals of a TypeScript file. Rough, and enough: the engines keep claims in plain quotes. */
const literals = (src) => [...src.matchAll(/'((?:[^'\\\n]|\\.)*)'|"((?:[^"\\\n]|\\.)*)"|`((?:[^`\\]|\\.)*)`/g)].map((m) => m[1] ?? m[2] ?? m[3]);

const KEY = /\b(?:Ctrl|Cmd|Alt|Shift|Option)\+[A-Za-z0-9]+/g;
const FLAG = /(?<![\w-])--[a-z][a-z-]*/g;
const ENV = /\b[A-Z][A-Z0-9]*(?:_[A-Z0-9]+)+\b/g;
const TOOL = /\b[a-z]+(?:_[a-z]+)+\b/g;
const SLASH = /(?<=^|[\s(`'"])\/[a-z]+(?=$|[\s.,;:)`'"])/g;

/**
 * Tool names come from the data fields that name them. Engine string literals are also scanned
 * for snake_case words, in case a tool name slips into code.
 * @returns {Map<string, {kind: string, where: Set<string>}>}
 */
function extract(corpus) {
  const found = new Map();
  const add = (kind, name, where) => {
    const k = `${kind}\0${name}`;
    if (!found.has(k)) found.set(k, { kind, name, where: new Set() });
    found.get(k).where.add(where);
  };
  for (const [where, text] of corpus.text) {
    for (const m of text.matchAll(KEY)) add('key', m[0], where);
    for (const m of text.matchAll(FLAG)) add('flag', m[0], where);
    for (const m of text.matchAll(ENV)) add('env', m[0], where);
    if (where.startsWith('src/')) for (const m of text.matchAll(TOOL)) add('tool', m[0], where);
    if (!corpus.noSlash) for (const m of text.matchAll(SLASH)) add('command', m[0], where);
  }
  for (const [where, name] of corpus.tools) add('tool', name, where);
  for (const [where, name] of corpus.commands) add('command', name, where);
  for (const [where, name] of corpus.keys) add('key', name, where);
  return found;
}

/** Splits a key column such as "PageUp/PageDown" or "Up / Down" into single keys. */
const splitKeys = (s) => s.split(/\s*\/\s*|\s*,\s*/).map((x) => x.trim()).filter(Boolean);

function corpusFor(name, product) {
  const text = [];
  const tools = [];
  const commands = [];
  const keys = [];
  for (const data of product.data) {
    for (const [where, s] of strings(data, product.skip)) text.push([`${where}`, s]);
    walk(data, '');
  }
  function walk(v, path) {
    if (Array.isArray(v)) {
      v.forEach((x, i) => walk(x, `${path}[${i}]`));
    } else if (v && typeof v === 'object') {
      if (typeof v.tool === 'string') tools.push([path, v.tool]);
      for (const [k, x] of Object.entries(v)) {
        if (product.skip.includes(k)) continue;
        if (k === 'patchTool' || k === 'writeTool') tools.push([`${path}.${k}`, x]);
        if ((k === 'commands' || k === 'unsupportedCommands') && Array.isArray(x)) for (const c of x) commands.push([`${path}.${k}`, c]);
        if (k === 'hints' && Array.isArray(x)) for (const [a] of x) (a.startsWith('/') ? commands : keys).push([`${path}.hints`, a]);
        if (k === 'help' && x && typeof x === 'object') {
          for (const [sec, rows] of Object.entries(x)) {
            if (!Array.isArray(rows)) continue;
            for (const [a] of rows) {
              if (a.startsWith('/')) commands.push([`help.${sec}`, a.split(/\s/)[0]]);
              else for (const key of splitKeys(a)) keys.push([`help.${sec}`, key]);
            }
          }
        }
        walk(x, path ? `${path}.${k}` : k);
      }
    }
  }
  // Strings in the engine source that name flags, keys, env vars or tools.
  for (const f of product.engine) {
    const src = readFileSync(join(pkg, 'src', f), 'utf8');
    for (const s of literals(src)) text.push([`src/${f}`, s]);
  }
  // Preview code/output contains user source. Read tool identity fields only,
  // so Rust function names and shell flags cannot become product API claims.
  if (product.preview) {
    const file = `src/previews/recordings/${product.preview}.json`;
    const visit = value => {
      if (!value || typeof value !== 'object') return;
      if (value.kind === 'tool') {
        const name = value.name ?? value.title?.match(/^(?:running|done|error) · ([a-z_]+)/)?.[1];
        if (name) tools.push([file, name]);
      }
      for (const child of Object.values(value)) visit(child);
    };
    visit(readJson(join(pkg, file)));
  }
  return { text, tools, commands, keys, noSlash: false };
}

/* ---- matching ---- */

const esc = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
const mentions = (doc, name) => new RegExp(`(?<![\\w/+-])${esc(name)}(?![\\w-])`).test(doc);
const bare = (name) => name.replace(/\s.*$/, '');
/** A tool name counts only when it is code formatted or quoted, so a plain word like "search" in prose is not taken for the tool. */
const documented = (doc, kind, name) =>
  kind === 'tool' ? doc.includes(`\`${name}\``) || doc.includes(`"${name}"`) : mentions(doc, ALIAS[bare(name)] ?? bare(name));
const sourceHas = (text, kind, name) => (kind === 'tool' ? text.includes(`"${name}"`) || text.includes(`\`${name}\``) : mentions(text, bare(name)));

const allow = readJson(join(pkg, 'honesty-allow.json'));
let failures = 0;
const out = (s = '') => console.log(s);

for (const [prod, product] of Object.entries(PRODUCTS)) {
  const dir = join(root, prod);
  const readme = join(dir, 'README.md');
  out(`${prod}`);
  if (!existsSync(readme)) {
    out(`  FAIL ${readme} does not exist; pass --root DIR with the product checkouts`);
    failures++;
    continue;
  }
  const doc = readFileSync(readme, 'utf8');
  const found = extract(corpusFor(prod, product));
  const entries = allow[prod] ?? [];
  const used = new Set();
  const missing = [];
  let ok = 0;
  let allowed = 0;
  for (const { kind, name, where } of found.values()) {
    if (documented(doc, kind, name)) {
      ok++;
      continue;
    }
    const a = entries.find((e) => e.name === name && e.kind === kind);
    if (a) {
      const srcPath = resolve(root, a.source);
      if (!existsSync(srcPath) || !sourceHas(readFileSync(srcPath, 'utf8'), kind, name)) {
        missing.push({ kind, name, where, note: `allow entry cites ${a.source}, which does not contain it` });
      } else {
        used.add(a);
        allowed++;
      }
    } else missing.push({ kind, name, where });
  }
  for (const e of entries) if (!used.has(e) && !missing.some((m) => m.name === e.name && m.kind === e.kind)) {
    missing.push({ kind: e.kind, name: e.name, where: new Set(['honesty-allow.json']), note: 'stale allow entry: the name is no longer shown, or the README now documents it' });
  }

  // Harness code samples: each line must be in a README of the product or its SDKs.
  if (prod === 'ejm-harness') {
    const docs = ['README.md', 'sdk/typescript/README.md', 'sdk/python/README.md'].map((p) => join(dir, p)).filter(existsSync).map((p) => readFileSync(p, 'utf8').replace(/\s+/g, ' '));
    const sample = (sc('harness.json').langs ?? []);
    for (const lang of sample) {
      const lines = lang.code.split('\n').map((l) => l.trim()).filter(Boolean);
      const bad = lines.filter((l) => !docs.some((d) => d.includes(l.replace(/\s+/g, ' '))));
      for (const l of bad) missing.push({ kind: 'sample line', name: `${lang.label}: ${l}`, where: new Set(['langs.' + lang.id]) });
      ok += lines.length - bad.length;
    }
  }

  out(`  ${ok} documented in README.md, ${allowed} documented elsewhere (honesty-allow.json), ${missing.length} not documented`);
  for (const m of missing) {
    out(`  MISSING ${m.kind} ${JSON.stringify(m.name)}  (${[...m.where].slice(0, 2).join(', ')})${m.note ? '  ' + m.note : ''}`);
    failures++;
  }
}

out();
out(failures ? `honesty: FAILED, ${failures} name(s) not documented in the product README` : 'honesty: ok');
process.exit(failures ? 1 : 0);
