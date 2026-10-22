#!/usr/bin/env node
/**
 * Generates src/lib/merge-config.ts: tailwind-merge's own default config reduced to the class groups listed in KEEP.
 *
 * tailwind-merge is 8.4 KB gzip, of which 6.4 KB is the default config for all 379 Tailwind class groups. `cn` needs the groups a
 * component or a page overrides, so this keeps those with their conflict rules, copied from the installed tailwind-merge (no hand
 * transcription), and drops the rest. A class in a dropped group is kept as written and never removed, so the worst outcome is a
 * missed override, not a lost class. Add a group name to KEEP and run the script to cover more.
 *
 * Usage: node packages/ui/scripts/build-merge-config.mjs [--check]   (--check exits 1 when the file is stale)
 */
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const out = join(here, '..', 'src', 'lib', 'merge-config.ts');
const require = createRequire(import.meta.url);

export const KEEP = new Set([
  // layout and position
  'container', 'container-type', 'container-named', 'isolation', 'appearance', 'overscroll', 'overscroll-x', 'overscroll-y', 'col-start', 'col-end', 'row-start', 'row-end', 'display', 'position', 'visibility', 'z', 'sr', 'overflow', 'overflow-x', 'overflow-y', 'inset', 'inset-x', 'inset-y', 'start', 'end', 'top', 'right', 'bottom', 'left', 'aspect', 'object-fit',
  // flex and grid
  'basis', 'flex-direction', 'flex-wrap', 'flex', 'grow', 'shrink', 'order', 'grid-cols', 'col-start-end', 'grid-rows', 'row-start-end', 'gap', 'gap-x', 'gap-y',
  'justify-content', 'justify-items', 'justify-self', 'align-items', 'align-self', 'place-items',
  // spacing and size
  'p', 'px', 'py', 'ps', 'pe', 'pt', 'pr', 'pb', 'pl', 'm', 'mx', 'my', 'ms', 'me', 'mt', 'mr', 'mb', 'ml', 'space-x', 'space-y',
  'size', 'w', 'min-w', 'max-w', 'h', 'min-h', 'max-h',
  // type
  'font-size', 'font-style', 'font-weight', 'font-family', 'fvn-normal', 'fvn-ordinal', 'fvn-slashed-zero', 'fvn-figure', 'fvn-spacing', 'fvn-fraction', 'tracking', 'line-clamp', 'leading', 'list-style-type', 'text-alignment', 'text-color', 'text-decoration',
  'text-decoration-thickness', 'underline-offset', 'text-transform', 'text-overflow', 'text-wrap', 'vertical-align', 'whitespace', 'break',
  // color, border, effects
  'bg-color', 'rounded', 'rounded-s', 'rounded-e', 'rounded-t', 'rounded-r', 'rounded-b', 'rounded-l', 'rounded-tl', 'rounded-tr', 'rounded-br', 'rounded-bl',
  'border-w', 'border-w-x', 'border-w-y', 'border-w-t', 'border-w-r', 'border-w-b', 'border-w-l', 'border-style', 'border-color', 'border-color-t', 'border-color-r', 'border-color-b', 'border-color-l',
  'divide-x', 'divide-y', 'divide-color', 'outline-style', 'outline-offset', 'outline-w', 'outline-color', 'shadow', 'shadow-color', 'ring-w', 'ring-color', 'opacity',
  // motion, transform, interaction
  'touch', 'accent', 'transition', 'duration', 'ease', 'delay', 'animate', 'rotate', 'scale', 'translate', 'translate-x', 'translate-y', 'cursor', 'pointer-events', 'select', 'resize', 'fill', 'stroke-w', 'stroke',
]);

const ts = (await import(pathToFileURL(require.resolve('typescript')).href)).default;
const twDir = dirname(require.resolve('tailwind-merge')); // .../tailwind-merge/dist
const dist = join(twDir, 'bundle-mjs.mjs');
const sourceText = readFileSync(dist, 'utf8');
const sf = ts.createSourceFile(dist, sourceText, ts.ScriptTarget.ES2022, true, ts.ScriptKind.JS);
const printer = ts.createPrinter({ removeComments: true });
const text = node => printer.printNode(ts.EmitHint.Unspecified, node, sf);
const keyName = p => (ts.isIdentifier(p.name) || ts.isStringLiteral(p.name) ? p.name.text : null);

const decl = sf.statements.flatMap(s => (ts.isVariableStatement(s) ? s.declarationList.declarations : [])).find(d => d.name.getText(sf) === 'getDefaultConfig');
const body = decl.initializer.body;
const helpers = body.statements.filter(ts.isVariableStatement);
const config = body.statements.find(ts.isReturnStatement).expression;
const prop = name => config.properties.find(p => keyName(p) === name).initializer;

const allGroups = prop('classGroups').properties;
const missing = [...KEEP].filter(k => !allGroups.some(p => keyName(p) === k));
if (missing.length) throw new Error(`build-merge-config: not a tailwind-merge class group: ${missing.join(', ')}`);

const idents = src => new Set(src.match(/[A-Za-z_$][\w$]*/g));
const helperText = new Map(helpers.flatMap(h => h.declarationList.declarations.map(d => [d.name.getText(sf), `const ${text(d)};`])));
/** Identifiers a source reaches through helper consts. */
const reach = src => {
  const seen = new Set();
  const queue = [...idents(src)];
  while (queue.length) {
    const id = queue.pop();
    if (seen.has(id)) continue;
    seen.add(id);
    if (helperText.has(id)) queue.push(...idents(helperText.get(id)));
  }
  return seen;
};
// A group that accepts any word (colors: \`text-<anything>\`) swallows the names of its dropped siblings (\`text-balance\`, \`border-collapse\`,
// \`bg-fixed\`) as if they were colors, and would then drop real classes. So when a group is kept, every group that shares its first
// word (text, bg, border, outline, ring, shadow, font, ...) is kept too.
const OPEN = new Set(['isAny', 'isAnyNonArbitrary', 'themeColor', 'themeFont']);
const heads = p => {
  const first = k => k.split('-')[0];
  return (p.initializer.elements ?? []).flatMap(e => (ts.isStringLiteral(e) ? [first(e.text)] : ts.isObjectLiteralExpression(e) ? e.properties.map(x => first(keyName(x))) : []));
};
const info = new Map(allGroups.map(p => [keyName(p), { p, heads: heads(p), open: [...reach(text(p.initializer))].some(i => OPEN.has(i)) }]));
const kept = new Set(KEEP);
for (let grew = true; grew; ) {
  grew = false;
  const openHeads = new Set([...kept].filter(k => info.get(k).open).flatMap(k => info.get(k).heads));
  for (const [k, g] of info) if (!kept.has(k) && g.heads.some(h => openHeads.has(h))) (kept.add(k), (grew = true));
}
const classGroups = allGroups.filter(p => kept.has(keyName(p)));
// These IDs are private lookup labels. Equal width avoids ambiguous modifier/group cache keys.
// Keep font-size named because utils.ts extends that public configuration group.
const idWidth = Math.max(1, Math.ceil(Math.log(Math.max(classGroups.length / 26, 1)) / Math.log(36)));
const suffixCount = 36 ** idWidth;
const ids = new Map(classGroups.map((p, i) => [keyName(p), keyName(p) === 'font-size' ? 'font-size' : String.fromCharCode(97 + Math.floor(i / suffixCount)) + (i % suffixCount).toString(36).padStart(idWidth, '0')]));
if (new Set(ids.values()).size !== ids.size) throw new Error('build-merge-config: duplicate compact group IDs');
const compact = id => ids.get(id);
const filterList = arr => `[${arr.elements.filter(e => kept.has(e.text)).map(e => JSON.stringify(compact(e.text))).join(', ')}]`;
const conflicts = prop('conflictingClassGroups').properties.filter(p => kept.has(keyName(p)))
  .map(p => `${JSON.stringify(compact(keyName(p)))}: ${filterList(p.initializer)}`).filter(l => !l.endsWith(': []'));
const postfix = filterList(prop('postfixLookupClassGroups'));

const groupsSrc = classGroups.map(p => `${JSON.stringify(compact(keyName(p)))}: ${text(p.initializer)}`).join(',\n    ');
const rest = [`cacheSize: 200`, `theme: ${text(prop('theme'))}`, `classGroups: {\n    ${groupsSrc},\n  }`, `conflictingClassGroups: {\n    ${conflicts.join(',\n    ')},\n  }`,
  `conflictingClassGroupModifiers: { ${prop('conflictingClassGroupModifiers').properties.filter(p => kept.has(keyName(p))).map(p => `${JSON.stringify(compact(keyName(p)))}: ${filterList(p.initializer)}`).join(', ')} }`, `postfixLookupClassGroups: ${postfix}`, `orderSensitiveModifiers: ${text(prop('orderSensitiveModifiers'))}`];

// Keep only the helper consts the kept groups use, directly or through another helper.
const used = new Set([...reach(rest.join('\n'))].filter(id => helperText.has(id)));
const helperSrc = [...helperText].filter(([id]) => used.has(id)).map(([, t]) => t).join('\n  ');
const validatorNames = [...idents([helperSrc, ...rest].join('\n'))].filter(id => /^is[A-Z]/.test(id) && !helperText.has(id)).sort();

const version = JSON.parse(readFileSync(join(twDir, '..', 'package.json'), 'utf8')).version;
const file = `// GENERATED by packages/ui/scripts/build-merge-config.mjs from tailwind-merge ${version}. Do not edit: add a group to KEEP there and run it.
// tailwind-merge's default config reduced to ${classGroups.length} of its class groups, so \`cn\` costs about 4 KB gzip less. Classes in other groups are kept as written.
import { fromTheme, validators } from 'tailwind-merge';
import type { Config } from 'tailwind-merge';

const { ${validatorNames.join(', ')} } = validators;

export const slimConfig = (): Config<string, string> => {
  ${helperSrc}
  return {
  ${rest.join(',\n  ')},
  } as unknown as Config<string, string>;
};
`;
if (process.argv.includes('--check')) {
  const current = existsSync(out) ? readFileSync(out, 'utf8') : '';
  if (current !== file) {
    console.error('merge-config.ts is stale: run node packages/ui/scripts/build-merge-config.mjs');
    process.exit(1);
  }
  console.log(`merge-config ok: ${classGroups.length} class groups from tailwind-merge ${version}`);
} else {
  writeFileSync(out, file);
  console.log(`wrote ${out} (${classGroups.length} class groups, ${file.length} bytes)`);
}
