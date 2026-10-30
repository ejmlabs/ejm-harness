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
- A consumer that links ejm-ui with `file:` adds these `paths`. Without them `tsc` finds two
  copies of the React types and fails inside `@ejm/ui`. Point them at the folder that holds
  `node_modules` (`../node_modules/...` for a site in a subfolder).

```jsonc
"paths": {
  "react": ["./node_modules/@types/react"],
  "react/*": ["./node_modules/@types/react/*"],
  "react-dom": ["./node_modules/@types/react-dom"],
  "react-dom/*": ["./node_modules/@types/react-dom/*"]
}
```

## eslint.config.mjs

`eslint.strict.mjs` is synced beside it. It holds `strictTypeChecked`, `stylisticTypeChecked`,
the ban on `@ts-` directives, required reasons on disable comments, `consistent-type-imports` and
`switch-exhaustiveness-check`.

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
4. Add the React `paths` above and `resolve.dedupe` below. Two copies of React break hooks.

## vite.config.ts for a site

```ts
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";
import { ejmSite } from "@ejm/site/vite";
import { ejmSsg } from "@ejm/ssg/vite";

export default defineConfig({
  resolve: { dedupe: ["react", "react-dom"] },
  plugins: [tailwindcss(), react(), ejmSite({ project: "cli" }), ejmSsg({ site: "src/site.tsx" })],
  build: { target: "es2022", cssMinify: "lightningcss" },
});
```

- Keep the `vite` and plugin imports before the `@ejm/*` imports. TypeScript 6.0.3 has crashed
  on the reverse order.
- An app drops `ejmSite` and `ejmSsg`. Dev and preview servers bind to `127.0.0.1`.
- The entry stylesheet is one line, `@import "@ejm/ui/styles-site.css" source(none);` for a site
  (`@ejm/ui/styles.css` for an app), plus the docs, app, static or terminal partial for every
  folder whose components you render.
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
