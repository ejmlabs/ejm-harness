# Project setup

The house form of each config file. Match it when you create one; move an existing file toward
it when you touch it.

## tsconfig.json

```jsonc
{
  "extends": "@tsconfig/strictest/tsconfig.json",
  "compilerOptions": {
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "lib": ["ES2023", "DOM", "DOM.Iterable"],
    "jsx": "react-jsx",
    "noEmit": true,
    "types": ["node", "vite/client"],
  },
  "include": ["src", "vite.config.ts"],
}
```

- Add only environment options: target, lib, module, jsx, types, paths.
- A repo alias such as `"@/*": ["./src/*"]` is for imports across folders. Inside a folder, use
  relative paths.
- A consumer of ejm-ui also extends `@ejm/ui/tsconfig.json`, after strictest. The base sets the
  environment options and the `paths` that give linked and installed packages one copy of the
  React and Vite types. Write no `paths` for `react`, `react-dom` or `vite` of your own.

```jsonc
{
  "extends": ["@tsconfig/strictest/tsconfig.json", "@ejm/ui/tsconfig.json"],
  "include": ["src", "vite.config.ts"],
}
```

## eslint.config.mjs

`eslint.strict.mjs` is synced beside it. It holds `strictTypeChecked`, `stylisticTypeChecked`,
the ban on `@ts-` directives, required reasons on disable comments, `consistent-type-imports`,
`switch-exhaustiveness-check`, the ban on `innerHTML`, `outerHTML`, `insertAdjacentHTML` and
`eval`, and relaxed `unbound-method` and non-null rules in test files.

```js
import jsxA11y from "eslint-plugin-jsx-a11y";
import reactHooks from "eslint-plugin-react-hooks";
import strict from "./eslint.strict.mjs";

export default [
  ...strict,
  {
    files: ["src/**/*.tsx"],
    plugins: { "react-hooks": reactHooks, "jsx-a11y": jsxA11y },
    rules: {
      ...reactHooks.configs.recommended.rules,
      ...jsxA11y.configs.strict.rules,
    },
  },
];
```

- React code always adds the hooks and accessibility rules.
- Repo rules go after `...strict`. A repo may add complexity, size or import-boundary limits.
- Pinned tool versions on 2026-10-27: eslint 10.13.0, @eslint/js 10.0.1, typescript-eslint
  8.71.2, typescript 6.0.3 (typed lint needs a version under 6.1), @tsconfig/strictest 2.0.8,
  globals 17.13.0, @eslint-community/eslint-plugin-eslint-comments 4.8.1,
  eslint-plugin-react-hooks 7.1.1. Move them together in one reviewed change.

## package.json and .npmrc

```json
{
  "type": "module",
  "private": true,
  "engines": { "node": ">=24 <25" },
  "dependencies": { "react": "19.3.1", "react-dom": "19.3.1" }
}
```

```ini
engine-strict=true
save-exact=true
```

- `react`, `react-dom` and `@types/react` match the versions ejm-ui uses.
- A consumer of ejm-ui also lists `vite`, a peer of `@ejm/ui`. It lists no Tailwind or Vite React
  plugin: `@ejm/ui` depends on them.
- Keep `dist`, `.ejm-ssg` and `node_modules` in `.gitignore`. Tailwind skips ignored paths;
  without the entry it scans the last build and the stylesheet grows each time.

## Linking ejm-ui

1. Keep `ejm-ui` beside the product checkout. Run `npm ci --ignore-scripts` inside it first;
   linked packages resolve their own dependencies from there.
2. Depend on the folders: `"@ejm/ui": "file:../ejm-ui/packages/ui"`, and the same for `core`,
   `tokens`, and (for sites) `ssg` and `site`. Add `demos` only for a product demo. List
   `@ejm/core` and `@ejm/tokens` yourself: `@ejm/ui` needs them at an exact version.
3. Pin the ejm-ui source by full commit SHA in `.github/ui-source.json`. Regenerate the lockfile
   when the pin moves.
4. Extend `@ejm/ui/tsconfig.json` and use the preset below. Together they give the linked
   packages one copy of React and its types. Add no `resolve.dedupe` or React `paths`.

## vite.config.ts

A site is one preset. `ejmlabs({ project })` adds Tailwind, React, one copy of React, the design
guard, the shared head and headers, the built-page check, prerendering and the build options.

```ts
import { defineConfig } from "vite";
import { ejmlabs } from "@ejm/site/vite";

export default defineConfig({ plugins: [ejmlabs({ project: "cli" })] });
```

An app uses `ejmUi()` (Tailwind, React, one copy of React and the design guard) and sets the
build options itself.

```ts
import { defineConfig } from "vite";
import { ejmUi } from "@ejm/ui/vite";

export default defineConfig({
  plugins: [ejmUi()],
  build: { target: "es2022", cssMinify: "lightningcss" },
});
```

- Keep the `vite` import before the `@ejm/*` imports. TypeScript 6.0.3 has crashed on the
  reverse order.
- Add no Tailwind or React plugin and no `resolve.dedupe`; the preset carries them. Your own
  plugins go in the same list. Dev and preview servers bind to `127.0.0.1`.
- The entry stylesheet is one line for a site or an app, `@import "@ejm/ui/styles.css";`, with
  no `source(none)` and no `@source`. A product demo adds its `@ejm/demos` stylesheet after it.
- Import deep paths in modules that run on first load (`@ejm/ui/font-preloads`,
  `@ejm/ui/layout/Hero`). Route entries load heavy pages with `load: () => import('./pages/X')`.
- `theme-boot.js` is an external file and `_headers` comes from `@ejm/site`. Never inline the
  script or hand-edit the headers.

## Performance gates

- Load time, for ejm-ui sites: after `npm run build`, run ejm-ui's `scripts/perf.mjs` from the
  site folder with `dist --baseline perf-baseline.json`. It needs Chrome. Add `--update` only
  after an intended change, and commit the new baseline with it.
- Byte budgets, where a repo sets them: a build step gzips the emitted JavaScript (and CSS, HTML
  or fonts where budgeted) and fails over the cap. Check the total and each chunk; splitting a
  chunk can raise the total. Hold back a dependency update that breaks a budget until it fits.
