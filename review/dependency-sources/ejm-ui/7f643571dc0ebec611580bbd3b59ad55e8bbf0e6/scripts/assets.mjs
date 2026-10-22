#!/usr/bin/env node
// Import selected brand assets from the old design-kit and verify their provenance.
//
//   node scripts/assets.mjs            verify: every row of PROVENANCE.md matches its file here and, when the
//                                      design-kit is present, its source there (exit 1 on any mismatch)
//   node scripts/assets.mjs --local    verify the files here against PROVENANCE.md only (used by `npm run check`,
//                                      so a change in the old kit never breaks this repo)
//   node scripts/assets.mjs --sync     copy from the design-kit and rewrite PROVENANCE.md (the only way it is written)
//   node scripts/assets.mjs --sync --project relay
//                                      import only Relay production assets and preserve other provenance rows
//
// The design-kit is read only. Its location is $EJM_DESIGN_KIT, else ../ejmlabs/design-kit next to this repo.
import { createHash } from 'node:crypto';
import { copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync, statSync, writeFileSync } from 'node:fs';
import { dirname, join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';

const repo = join(dirname(fileURLToPath(import.meta.url)), '..');
const tokens = join(repo, 'packages', 'tokens');
const kit = process.env.EJM_DESIGN_KIT || join(repo, '..', 'ejmlabs', 'design-kit');
const provenance = join(tokens, 'assets', 'PROVENANCE.md');

// [source path in the design-kit, destination under packages/tokens].
// Retired social profile kits and ZIP archives are excluded from imports.
const MOVES = [
  ['logos', 'assets/logos'], ['favicon', 'assets/favicon'], ['icons', 'assets/icons'], ['motifs', 'assets/motifs'],
  ['social', 'assets/social'], ['lockups', 'assets/lockups'], ['templates', 'assets/templates'], ['fonts', 'assets/fonts'],
  ['tokens/palette.json', 'assets/brand/palette.json'], ['tokens/design-tokens.json', 'assets/brand/design-tokens.json'],
  ['ui/rust/ejm_tui.rs', 'rust/ejm_tui.rs'], ['ui/TUI.md', 'rust/TUI.md'],
  // Public project families include approved editable CLI/IDE/Harness logos.
  ['projects/cli', 'assets/projects/cli'], ['projects/ide', 'assets/projects/ide'], ['projects/harness', 'assets/projects/harness'],
  ['projects/relay', 'assets/projects/relay'],
  ...['dark', 'light'].flatMap(theme => ['svg', 'png'].map(ext => [`projects/family-${theme}.${ext}`, `assets/projects/family-${theme}.${ext}`])),
];
const GENERATED_DIRS = ['assets/projects/chess'];
const skipped = rel => rel.split(/[\\/]/).includes('editable') && !/^editable\/logo-(?:ink|paper|primary)\.svg$/.test(posix(rel));
// This is the public production bundle. Review boards and editable sources stay in the kit.
const relayFiles = new Set([
  ...['mark-sage', 'mark-ink', 'mark-paper', 'logo-primary', 'logo-ink', 'logo-paper', 'icon-sage', 'icon-forest', 'social-card'].flatMap(name => [`${name}.svg`, `${name}.png`]),
  'favicon.svg', 'favicon.ico', 'favicon-16.png', 'favicon-24.png', 'favicon-32.png', 'favicon-48.png',
  'apple-touch-icon.png', 'android-chrome-192.png', 'android-chrome-512.png', 'site.webmanifest',
]);

const sha = path => createHash('sha256').update(readFileSync(path)).digest('hex');
const walk = dir => readdirSync(dir).sort().flatMap(n => {
  const p = join(dir, n);
  if (n === '.DS_Store') return [];
  return statSync(p).isDirectory() ? walk(p) : [p];
});
const posix = p => p.split('\\').join('/');

function plan(project) {
  const rows = [];
  for (const [from, to] of MOVES) {
    if (project && from !== `projects/${project}`) continue;
    const src = join(kit, from);
    const files = from === 'projects/relay' ? [...relayFiles].sort().map(name => join(src, name)) : statSync(src).isDirectory() ? walk(src) : [src];
    for (const file of files) {
      const rel = file === src ? '' : relative(src, file);
      const source = posix(join(from, rel));
      if (source.split('/').includes('x-kit') || source.endsWith('.zip')) continue;
      if (from.startsWith('projects/') && skipped(rel)) continue;
      rows.push({ local: posix(join(to, rel)), source, file });
    }
  }
  if (!project) for (const directory of GENERATED_DIRS) {
    const target = join(tokens, directory);
    if (!existsSync(target)) continue;
    for (const file of walk(target)) rows.push({ local: posix(relative(tokens, file)), source: `generated:chess-assets/${relative(target, file)}`, file });
  }
  return rows;
}

const { values } = parseArgs({ options: { sync: { type: 'boolean' }, local: { type: 'boolean' }, project: { type: 'string' } } });
if (values.project !== undefined && (!values.sync || values.project !== 'relay')) throw new Error('Use --sync --project relay for a targeted import.');
if (values.sync && values.local) throw new Error('--sync and --local are separate operations.');
const rowText = r => `| \`${r.local}\` | \`${r.source}\` | \`${r.sha}\` |`;
if (values.sync) {
  if (!existsSync(kit)) { console.error(`design-kit not found at ${kit} (set EJM_DESIGN_KIT)`); process.exit(2); }
  const rows = plan(values.project);
  const previous = values.project ? readFileSync(provenance, 'utf8') : '';
  const boundary = previous.indexOf('\n## Moved and then edited\n');
  if (values.project && (boundary < 0 || !previous.slice(0, boundary).includes('| File | Source | sha256 |'))) throw new Error('Existing provenance table and edited-source section are required.');
  // Read and hash the entire selection before writing any imported file.
  for (const r of rows) r.sha = sha(r.file);
  for (const r of rows) {
    const dest = join(tokens, r.local);
    mkdirSync(dirname(dest), { recursive: true });
    if (r.file !== dest) copyFileSync(r.file, dest);
    if (sha(dest) !== r.sha) throw new Error(`Source changed during import: ${r.source}`);
  }
  if (values.project) {
    const retained = previous.slice(0, boundary).split('\n').filter(line => !line.startsWith('| `assets/projects/relay/')).join('\n').trimEnd();
    writeFileSync(provenance, `${retained}\n${rows.map(rowText).join('\n')}\n${previous.slice(boundary)}`);
    console.log(`copied ${rows.length} Relay files, preserved other provenance rows and edited-source section`);
    process.exit(0);
  }
  const edited = [
    ['ui/theme.json', 'packages/tokens/theme.json', 'extended with the `web` block (bands, elevation)'],
    ['scripts/build-ui.mjs', 'packages/tokens/scripts/build-ui.mjs', 'paths adapted, Tailwind output added, contrast gates moved to contrast.mjs'],
    ['scripts/build-art.mjs', 'packages/ui/scripts/build-art.mjs', 'paths adapted, inline classes renamed'],
    ['scripts/check-art.mjs', 'packages/ui/scripts/check-art.mjs', 'paths adapted, gradient surface section dropped'],
    ['scripts/shoot.mjs', 'scripts/shoot.mjs', 'docs and skip-link selectors updated'],
    ['QUALITY.md', 'QUALITY.md', 'commands updated, Performance section added'],
  ].map(([from, to, why]) => `| \`${from}\` | \`${to}\` | \`${sha(join(kit, from)).slice(0, 12)}\` | ${why} |`);
  writeFileSync(provenance, [
    '# Provenance',
    '',
    'Everything below was copied unchanged from the old `design-kit/` (read only) by `node scripts/assets.mjs --sync`.',
    'Do not edit this table by hand. `node scripts/assets.mjs` re-hashes every file here and, when the design-kit is present, every source.',
    'Paths are relative to `packages/tokens/` here and to `design-kit/` at the source.',
    'Retired social profile kits and ZIP archives are excluded from imports.',
    '',
    '| File | Source | sha256 |',
    '| --- | --- | --- |',
    ...rows.map(rowText),
    '',
    '## Moved and then edited',
    '',
    'These came from the design-kit too but were changed on purpose, so their hashes are not compared. The short hash is the original.',
    '',
    '| Source | Now | Original sha256 (12) | Change |',
    '| --- | --- | --- | --- |',
    ...edited,
    '',
  ].join('\n'));
  console.log(`copied ${rows.length} files, wrote ${relative(repo, provenance)}`);
  process.exit(0);
}

const local = values.local;
const problems = [];
const text = readFileSync(provenance, 'utf8');
const rows = [...text.matchAll(/^\| `([^`]+)` \| `([^`]+)` \| `([0-9a-f]{64})` \|$/gm)].map(m => ({ local: m[1], source: m[2], sha: m[3] }));
if (!rows.length) problems.push('PROVENANCE.md has no rows');
const listed = new Set(rows.map(r => r.local));
let compared = 0;
for (const r of rows) {
  const file = join(tokens, r.local);
  if (!existsSync(file)) { problems.push(`missing: ${r.local}`); continue; }
  if (sha(file) !== r.sha) problems.push(`changed here: ${r.local}`);
  if (!local && existsSync(kit) && !r.source.startsWith('generated:')) {
    const src = join(kit, r.source);
    if (!existsSync(src)) problems.push(`source gone: ${r.source}`);
    else if (sha(src) !== r.sha) problems.push(`source differs: ${r.source}`);
    else compared++;
  }
}
// A file in assets/ that PROVENANCE.md does not list is also a failure.
for (const to of [...MOVES.map(([, to]) => to), ...GENERATED_DIRS]) {
  const dir = join(tokens, to);
  if (!existsSync(dir)) continue;
  for (const f of statSync(dir).isDirectory() ? walk(dir) : [dir]) {
    const rel = posix(relative(tokens, f));
    if (!listed.has(rel)) problems.push(`not in PROVENANCE.md: ${rel}`);
  }
}
if (problems.length) { console.error(problems.join('\n')); process.exit(1); }
const where = local ? 'local only' : existsSync(kit) ? `${compared} sources matched` : 'design-kit not found, local only';
console.log(`assets ok: ${rows.length} files match PROVENANCE.md (${where})`);
