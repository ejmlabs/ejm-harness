// Writers for the Tailwind v4 theme and opt-in product window CSS profiles.
// See packages/ui/THEME.md for how they are used.
import { rgb } from './theme.mjs';

const hex = v => `#${v.toLowerCase()}`;
const kebab = k => k.replace(/([A-Z])/g, '-$1').replace(/([a-z])(\d)/g, '$1-$2').toLowerCase();
const css = k => `--ejm-${kebab(k)}`;

// Selectors that make a block apply to a theme. Light is the default; dark applies to [data-theme="dark"] anywhere
// in the tree and, when the root has no data-theme, to the system preference.
const LIGHT = ':root, [data-theme="light"]';
const DARK = '[data-theme="dark"]';
const SYSTEM = ':root:not([data-theme])';
const themeBlocks = (decls) => [
  `  ${LIGHT} {\n${decls('light')}\n  }`,
  `  ${DARK} {\n${decls('dark')}\n  }`,
  `  @media (prefers-color-scheme: dark) {\n    ${SYSTEM} {\n${decls('dark').replace(/^/gm, '  ')}\n    }\n  }`,
].join('\n');

const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);

// {ink/10} becomes an rgb() color at 10 percent.
function shadow(value, brand) {
  const color = name => (name === 'black' ? [0, 0, 0] : rgb(brand[name]));
  return value.replace(/\{(\w+)\/(\d+)\}/g, (_, name, pct) => `rgb(${color(name).join(' ')} / ${Number(pct) / 100})`);
}

export function tailwindCss({ theme, brand, resolved, bands, banner }) {
  const keys = theme.web.keys;
  const names = Object.keys(bands);
  const themed = names.filter(b => !same(bands[b].light, bands[b].dark));
  const out = [];

  out.push(`/* ${banner}
   Tailwind v4 theme for ejmlabs sites and apps. packages/ui/src/styles.css imports it. Names and behavior: packages/ui/THEME.md.
   Brand colors are fixed. The semantic colors (ground, fg, muted, line, tone, ...) point at --ejm-* variables, and a
   [data-band="paper|mist|sage|ink"] element sets all of them for its subtree. Light is the default, [data-theme="dark"] or the
   system preference (when the root has no data-theme) switches Paper and Mist; Sage and Ink look the same in both themes. */`);

  // Brand colors, always emitted, and the reset that keeps every other palette color out of the build.
  out.push(`@theme static {\n  --color-*: initial;\n  --shadow-*: initial;\n  --drop-shadow-*: initial;\n  --inset-shadow-*: initial;\n  --text-shadow-*: initial;\n${Object.entries(brand).map(([k, v]) => `  --color-${k}: ${hex(v)};`).join('\n')}\n}`);

  // Semantic colors. inline: the utility reads the variable itself, so it follows the nearest band and theme.
  const sem = keys.filter(k => k !== 'scheme' && k !== 'hues').map(kebab);
  const colors = [
    ...sem.map(k => `  --color-${k}: var(--ejm-${k});`),
    ...[1, 2, 3, 4, 5, 6].map(i => `  --color-hue-${i}: var(--ejm-hue-${i});`),
    '  /* shadcn/ui names, so copied-in component source keeps working. There is no card or popover surface: popover is the ground. */',
    ...Object.entries({
      background: 'ground', foreground: 'fg', 'muted-foreground': 'muted', popover: 'ground', 'popover-foreground': 'fg',
      'primary-foreground': 'on-primary', secondary: 'tone', 'secondary-foreground': 'fg', accent: 'tone', 'accent-foreground': 'fg',
      destructive: 'danger', 'destructive-foreground': 'on-danger', border: 'line', input: 'control', ring: 'focus',
    }).map(([a, b]) => `  --color-${a}: var(--ejm-${b});`),
  ];
  out.push(`@theme inline {\n${colors.join('\n')}\n  --shadow-rest: var(--ejm-shadow-rest);\n  --shadow-raised: var(--ejm-shadow-raised);\n  --shadow-float: var(--ejm-shadow-float);\n}`);

  // Every variable a band sets, with hues expanded to hue-1 to hue-6. A variable that is the same in every themed band
  // (Paper and Mist share their text, link, control and status colors) is written once per theme as --ejm-theme-NAME;
  // only the differing ones get a per-band copy. Static bands (Sage, Ink) write plain values.
  const flat = keys.flatMap(k => (k === 'hues' ? [1, 2, 3, 4, 5, 6].map(i => `hue${i}`) : [k]));
  const val = (b, mode, k) => (k === 'scheme' ? bands[b][mode].scheme : hex(bands[b][mode][k]));
  const shared = flat.filter(k => ['light', 'dark'].every(m => themed.every(b => val(b, m, k) === val(themed[0], m, k))));
  const varName = (b, k) => `--ejm-${kebab(k)}`;
  const source = (b, k) => (shared.includes(k) ? `var(--ejm-theme-${kebab(k)})` : `var(--ejm-${b}-${kebab(k)})`);

  const themeDecls = mode => [
    `    --ejm-page-scheme: ${mode};`,
    ...['rest', 'raised', 'float'].map(l => `    --ejm-shadow-${l}: ${shadow(theme.web.elevation[mode][l], brand)};`),
    ...shared.map(k => `    --ejm-theme-${kebab(k)}: ${val(themed[0], mode, k)};`),
    ...themed.flatMap(b => flat.filter(k => !shared.includes(k)).map(k => `    --ejm-${b}-${kebab(k)}: ${val(b, mode, k)};`)),
  ].join('\n');

  const mapDecl = band => flat.map(k => `    ${varName(band, k)}: ${source(band, k)};`).join('\n');
  const staticDecl = band => flat.map(k => `    ${varName(band, k)}: ${val(band, 'light', k)};`).join('\n');

  const rules = [themeBlocks(themeDecls)];
  rules.push(`  /* The page is the Paper band. Any [data-theme] element starts a new page scheme for its subtree. */\n  :root, [data-theme], [data-band="paper"] {\n${mapDecl('paper')}\n  }`);
  for (const b of names.filter(n => n !== 'paper')) {
    rules.push(themed.includes(b)
      ? `  [data-band="${b}"] {\n${mapDecl(b)}\n  }`
      : `  /* ${b[0].toUpperCase()}${b.slice(1)} is the same in both themes. */\n  [data-band="${b}"] {\n${staticDecl(b)}\n  }`);
  }
  rules.push('  :root, [data-theme], [data-band] {\n    color-scheme: var(--ejm-scheme);\n  }');
  out.push(`@layer base {\n${rules.join('\n')}\n}`);
  return `${out.join('\n\n')}\n`;
}

// The ui roles (editor, raised, text, line, syntax, hues) for product windows and editors on the web. They follow the page
// theme, never the band, so a window looks the same wherever it sits. Names: ui-ground, ui-editor, ui-text, syn-keyword, ui-hue-1.
export function windowCss({ resolved, banner }) {
  const vars = mode => {
    const t = resolved[mode];
    return [
      ...Object.entries(t.roles).map(([k, v]) => `--ejm-ui-${k}: ${hex(v)};`),
      ...t.hues.map((v, i) => `--ejm-ui-hue-${i + 1}: ${hex(v)};`),
      ...Object.entries(t.syntax).map(([k, v]) => `--ejm-ui-syn-${k}: ${hex(v)};`),
    ].map(l => `    ${l}`).join('\n');
  };
  const t = resolved.light;
  const colors = [
    ...Object.keys(t.roles).map(k => `  --color-ui-${k}: var(--ejm-ui-${k});`),
    ...t.hues.map((_, i) => `  --color-ui-hue-${i + 1}: var(--ejm-ui-hue-${i + 1});`),
    ...Object.keys(t.syntax).map(k => `  --color-syn-${k}: var(--ejm-ui-syn-${k});`),
  ];
  return `/* ${banner}
   The ui theme as web variables, light by default and dark with [data-theme="dark"] or the system preference. They follow the page
   theme, not the band. Tailwind colors: bg-ui-editor, text-ui-text, border-ui-line, text-syn-keyword, text-ui-hue-3, and so on. */
@theme inline {
${colors.join('\n')}
}

@layer base {
${themeBlocks(vars)}
}
`;
}

// Native product styles use the complete neutral scale alongside the semantic ui roles.
// Keep this opt-in so pages without those styles do not carry the extra variables.
export function neutralCss({ resolved, banner }) {
  const vars = mode => resolved[mode].neutral
    .map((value, i) => `    --ejm-ui-n${i}: ${hex(value)};`).join('\n');
  return `/* ${banner}
   Neutral colors for native product styles on the web. They follow the page theme, not the band. */
@layer base {
${themeBlocks(vars)}
}
`;
}
