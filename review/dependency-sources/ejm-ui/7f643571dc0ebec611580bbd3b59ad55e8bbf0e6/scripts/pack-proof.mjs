#!/usr/bin/env node
// The tarball proof: can a project outside this repo use ejm-ui from `npm pack` tarballs alone?
//
//   node scripts/pack-proof.mjs [--dir <folder outside the repo>]
//
// NEEDS NETWORK: the throwaway consumer installs react, vite, tailwind and typescript from the npm registry. That is why this is not
// part of `npm run check`. Run it before relying on a change to a package.json, `files`, `exports` or the Vite plugins.
//
// What it does, in order:
//   1. builds examples/starter in the workspace (the reference output),
//   2. `npm pack` for @ejm/tokens, core, ui, ssg, site and demos into <dir>/tarballs,
//   3. copies examples/starter to <dir>/consumer and installs ONLY the six tarballs from disk (no workspace, no links),
//   4. in the consumer runs `ejm-ssg build`, `ejm-ssg budget`, `ejm-site-headers dist/_headers --check` and `tsc`
//      (the typecheck also compiles a probe file that imports every documented deep import of @ejm/ui),
//   5. diffs the consumer's HTML against the workspace build (hashed asset names are folded; CSS and JS are compared by size).
// Exit code 0 only if every step passed and the HTML is identical. The folder is never deleted, so you can look at it (without --dir it is a new folder under the system temp directory).
import { execFileSync } from 'node:child_process';
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

const repo = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const args = process.argv.slice(2);
const argValue = name => (args.includes(name) ? args[args.indexOf(name) + 1] : undefined);
const dir = resolve(argValue('--dir') ?? mkdtempSync(join(tmpdir(), 'ejm-pack-proof-')));
if (dir === repo || dir.startsWith(repo + sep)) throw new Error(`pack-proof: --dir must be outside the repo, got ${dir}`);

const PACKAGES = ['tokens', 'core', 'ui', 'ssg', 'site', 'demos'];
const step = title => console.log(`\n== ${title}`);
const run = (cmd, cmdArgs, cwd, extra = {}) => execFileSync(cmd, cmdArgs, { cwd, stdio: 'inherit', ...extra });
const read = (cmd, cmdArgs, cwd) => execFileSync(cmd, cmdArgs, { cwd, encoding: 'utf8', stdio: ['ignore', 'pipe', 'inherit'] });
const problems = [];

// 1. The reference build, inside the workspace.
step('workspace build of examples/starter');
run('npm', ['run', 'build', '-w', '@ejm/starter'], repo);
const reference = join(dir, 'workspace-dist');
rmSync(reference, { recursive: true, force: true });
mkdirSync(dir, { recursive: true });
cpSync(join(repo, 'examples', 'starter', 'dist'), reference, { recursive: true });

// 2. Tarballs, with their sizes.
step('npm pack');
const tarballs = join(dir, 'tarballs');
rmSync(tarballs, { recursive: true, force: true });
mkdirSync(tarballs, { recursive: true });
const packed = PACKAGES.map(p => {
  const [info] = JSON.parse(read('npm', ['pack', '-w', `packages/${p}`, '--pack-destination', tarballs, '--json'], repo));
  console.log(`${info.name.padEnd(14)} ${String(info.size).padStart(9)} bytes gzip, ${String(info.unpackedSize).padStart(9)} unpacked, ${String(info.entryCount).padStart(4)} files`);
  return info;
});

// 3. The consumer: the starter's source, a package.json that names tarballs, and nothing else from this repo.
step('consumer install from the tarballs only');
const consumer = join(dir, 'consumer');
rmSync(consumer, { recursive: true, force: true });
const starter = join(repo, 'examples', 'starter');
cpSync(starter, consumer, { recursive: true, filter: src => !/(^|[\\/])(dist|\.ejm-ssg|node_modules)$/.test(src) });
const rootPkg = JSON.parse(readFileSync(join(repo, 'package.json'), 'utf8'));
const range = name => rootPkg.dependencies?.[name] ?? rootPkg.devDependencies?.[name] ?? (() => { throw new Error(`pack-proof: ${name} is not in the root package.json`); })();
const ejm = Object.fromEntries(packed.map(i => [i.name, `file:../tarballs/${i.filename}`]));
writeFileSync(join(consumer, 'package.json'), JSON.stringify({
  name: 'ejm-pack-proof-consumer',
  version: '0.0.0',
  private: true,
  type: 'module',
  scripts: { build: 'ejm-ssg build', budget: 'ejm-ssg budget', typecheck: 'tsc -p tsconfig.json --pretty false' },
  dependencies: { ...ejm, react: range('react'), 'react-dom': range('react-dom') },
  devDependencies: Object.fromEntries(['@tailwindcss/vite', '@types/node', '@types/react', '@types/react-dom', '@vitejs/plugin-react', 'tailwindcss', 'typescript', 'vite'].map(n => [n, range(n)])),
}, null, 2) + '\n');
// No `extends` into the repo and no `paths`: types must resolve through the packages' `exports`.
const base = JSON.parse(readFileSync(join(repo, 'tsconfig.base.json'), 'utf8')).compilerOptions;
delete base.paths;
writeFileSync(join(consumer, 'tsconfig.json'), JSON.stringify({ compilerOptions: { ...base, types: ['node', 'vite/client'] }, include: ['src', 'vite.config.ts'] }, null, 2) + '\n');
// The deep imports that ADOPTION.md and the sites use must typecheck for an outside consumer too.
writeFileSync(join(consumer, 'src', 'deep-imports.ts'), `// Written by scripts/pack-proof.mjs.
import { fontPreloads } from '@ejm/ui/font-preloads';
import { Hero } from '@ejm/ui/layout/Hero';
import { ProductWindow } from '@ejm/ui/layout/ProductWindow';
import { NativeCheckbox } from '@ejm/ui/components/NativeCheckbox';
import { Disclosure } from '@ejm/ui/shell/Disclosure';
import { Header } from '@ejm/ui/shell/Header';
import { AppShell } from '@ejm/ui/shell/AppShell';
import { ActionRows, ActionRow } from '@ejm/ui/layout/ActionRows';
import { DocumentContent } from '@ejm/ui/layout/DocumentContent';
import {
  TerminalWorkspace, TerminalHeader, TerminalLink, TerminalOutput, TerminalPairing, TerminalLog,
  TerminalEntry, TerminalStatus, TerminalComposer, TerminalTextarea, TerminalSelect,
} from '@ejm/ui/components/TerminalWorkspace';
import { useInView } from '@ejm/ui/hooks/useInView';
import { useReducedMotion } from '@ejm/ui/hooks/useReducedMotion';
import { useTheme } from '@ejm/ui/hooks/useTheme';
import { tokenize } from '@ejm/ui/docs/highlight';
import ripple from '@ejm/ui/art/inline/divider-ripple.svg?raw';
import { ejmSsg } from '@ejm/ssg/vite';
import { checkBudget } from '@ejm/ssg/budget';
import { PROJECTS } from '@ejm/site/projects';
import { CliHeroWindow, ProductHeroStage } from '@ejm/demos/hero';
import { LazyCliDemo } from '@ejm/demos/islands/cli-lazy';
import { ShowcaseFrame } from '@ejm/demos/ShowcaseFrame';
import { LazyCliPreview } from '@ejm/demos/islands/cli-preview';
import { LazyIdePreview } from '@ejm/demos/islands/ide-preview';
import { LazyHarnessPreview } from '@ejm/demos/islands/harness-preview';
export const probe = [fontPreloads, Hero, ProductWindow, NativeCheckbox, Disclosure, Header, AppShell, ActionRows, ActionRow, DocumentContent,
  TerminalWorkspace, TerminalHeader, TerminalLink, TerminalOutput, TerminalPairing, TerminalLog, TerminalEntry, TerminalStatus, TerminalComposer, TerminalTextarea, TerminalSelect,
  useInView, useReducedMotion, useTheme, tokenize, ripple, ejmSsg, checkBudget, PROJECTS, CliHeroWindow, ProductHeroStage, LazyCliDemo,
  ShowcaseFrame, LazyCliPreview, LazyIdePreview, LazyHarnessPreview];
`);
run('npm', ['install', '--no-audit', '--no-fund'], consumer);

// 4. The checks, all through the installed bins.
const bin = name => join(consumer, 'node_modules', '.bin', name);
step('ejm-ssg build');
run(bin('ejm-ssg'), ['build'], consumer);
step('ejm-ssg budget');
run(bin('ejm-ssg'), ['budget'], consumer);
step('ejm-site-headers --check');
run(bin('ejm-site-headers'), ['dist/_headers', '--check'], consumer);
step('typecheck');
run(bin('tsc'), ['-p', 'tsconfig.json', '--pretty', 'false'], consumer);

// 5. Same output as the workspace build.
step('diff against the workspace build');
const files = root => {
  const out = [];
  const walk = d => readdirSync(d).sort().forEach(n => {
    const p = join(d, n);
    if (statSync(p).isDirectory()) walk(p);
    else out.push(relative(root, p).split(sep).join('/'));
  });
  walk(root);
  return out;
};
const fold = text => text.replace(/(\/assets\/[A-Za-z0-9_.-]*?)-[A-Za-z0-9_-]{8}(\.[a-z0-9]+)/g, '$1$2');
const ours = join(consumer, 'dist');
const names = [...new Set([...files(reference), ...files(ours)])].sort();
let same = 0;
for (const n of names) {
  const a = join(reference, n);
  const b = join(ours, n);
  if (!existsSync(a) || !existsSync(b)) {
    if (!/^assets\//.test(n)) problems.push(`only in ${existsSync(a) ? 'workspace' : 'consumer'}: ${n}`);
    continue;
  }
  if (n.endsWith('.html')) {
    if (fold(readFileSync(a, 'utf8')) === fold(readFileSync(b, 'utf8'))) same++;
    else problems.push(`HTML differs: ${n}`);
  } else if (!n.startsWith('assets/')) {
    if (readFileSync(a).equals(readFileSync(b))) same++;
    else problems.push(`file differs: ${n}`);
  }
}
console.log(`${same} files identical (HTML with hashed asset names folded, plus the root files)`);
for (const kind of ['js', 'css']) {
  const total = root => files(root).filter(f => f.startsWith('assets/') && f.endsWith(`.${kind}`)).reduce((s, f) => s + statSync(join(root, f)).size, 0);
  console.log(`${kind}: workspace ${total(reference)} bytes, consumer ${total(ours)} bytes`);
}

if (problems.length) {
  console.error(`\npack-proof FAILED\n  ${problems.join('\n  ')}\nkept: ${dir}`);
  process.exit(1);
}
// Compile optional CSS independently, preserving the starter's real route budget and output comparison.
step('installed opt-in CSS exports');
const cssProbe = join(consumer, 'css-entry-proof.mjs');
writeFileSync(cssProbe, `
import { compile } from '@tailwindcss/node';
import { Scanner } from '@tailwindcss/oxide';
import { writeFileSync } from 'node:fs';
const css = ['styles.css', 'styles-docs.css', 'styles-app.css', 'styles-static.css', 'styles-terminal.css']
  .map(name => '@import "@ejm/ui/' + name + '";')
  .concat(['cli.css', 'ide-preview.css', 'harness.css'].map(name => '@import "@ejm/demos/' + name + '";')).join('\\n');
const compiler = await compile(css, { base: process.cwd(), onDependency() {} });
const scanner = new Scanner({ sources: compiler.sources });
const output = compiler.build(scanner.scan());
const neutralRefs = new Set([...output.matchAll(/var\\((--ejm-ui-n\\d+)\\)/g)].map(match => match[1]));
if (!neutralRefs.size) throw new Error('Installed native preview has no neutral colors');
for (const name of neutralRefs) {
  if (!output.includes(name + ':')) throw new Error('Installed preview color is missing ' + name);
}
for (const name of ['native-check-label', 'disclosure-menu', 'document-content', 'terminal-workspace', 'ejm-ide-viewport']) {
  if (!output.includes('.' + name)) throw new Error('Installed CSS is missing ' + name);
}
writeFileSync('css-entry-proof.css', output);
console.log('Every installed CSS entry resolves and generates its components.');
`);
run('node', [cssProbe], consumer);
console.log(`\npack-proof ok. Kept: ${dir}`);
