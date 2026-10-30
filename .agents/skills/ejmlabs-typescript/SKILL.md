---
name: ejmlabs-typescript
description: Use when writing, reviewing or configuring TypeScript or React, including .ts and .tsx files, tsconfig, ESLint, package.json, lockfiles, npm scripts, Vite builds and budgets, Vitest or Playwright tests, or any code that consumes ejm-ui through @ejm/* packages.
---

# TypeScript, React and frontend

Strict types with no escape hatch, exact pins, one React, ejm-ui through its public exports
only, and a measured performance gate that never moves to fit a change.

## Types and lint

- Every tsconfig extends `@tsconfig/strictest` and adds only environment options (target, lib,
  module, jsx, types, paths). Never turn a strict option off.
- Lint with an ESLint flat config that spreads the synced `eslint.strict.mjs` first. Never edit
  the synced file. A repo adds stricter rules after it, never weaker ones.
- Zero warnings. Typecheck, lint and format check all run in the repository's gate.
- Never write `any`, `@ts-ignore`, `@ts-expect-error` or `@ts-nocheck`. Never use `as` or a `!`
  assertion to quiet the checker. Fix the type, or narrow `unknown` with a check.
- Parse every value from outside the program (network, storage, URL, `postMessage`, files) at
  the boundary with a schema or type guard. It stays `unknown` until then.
- An `eslint-disable` names one rule and gives its reason after `--`. Blanket disables fail.
- Use `import type` for types. Every `switch` on a union is exhaustive.
- Format with the repository's one formatter: Prettier by default, Biome where a repo already
  uses it. Two spaces, 100 columns. Biome lint never replaces the typed ESLint layer.

## Node and npm

- Node 24 LTS: `"engines": { "node": ">=24 <25" }`, with `engine-strict=true` in `.npmrc`.
- npm only, with workspaces for a monorepo. Commit `package-lock.json`. Use `npm ci` except when
  you change dependencies.
- Pin exact versions (`save-exact=true`): no `^` or `~`. A version change ships with its lockfile.
- Adopt a new release only after it has been public for two weeks.
- Install with lifecycle scripts off (`npm ci --ignore-scripts`). A native rebuild is a named
  exception (`npm rebuild <pkg> --ignore-scripts=false`) and runs without credentials.
- Code that Node loads from a package ships as `.mjs` with `.d.mts` types. Node will not strip
  types under `node_modules`.

## React

- Real React 19 and `react-dom`, one copy. Never add Preact or a compat alias to an ejm-ui
  consumer. A repo skill may name another runtime for a repo outside ejm-ui (gerry uses Preact).
- Derive values during render. Never copy props or server data into state with an effect.
- An effect only syncs with something outside React (subscription, observer, timer, DOM API)
  and returns its cleanup.
- Remote data has one owner per app, a query cache such as TanStack Query. No second cache or
  global store for the same data.
- Add `memo`, `useMemo` or `useCallback` only for a measured repeated cost.
- A view that loads data shows loading, empty and error states, and keeps unsent input when a
  request fails.
- Use the shared primitives (Radix through `@ejm/ui`) for focus, menus, dialogs and popovers.
  Never hand-write that machinery.

## Modules and files

| Kind                         | Name                                                       |
| ---------------------------- | ---------------------------------------------------------- |
| Components, pages, providers | `PascalCase.tsx`, exporting the component of the same name |
| Hooks                        | `useThing.ts`, exporting `useThing`                        |
| Other modules                | kebab-case `.ts`, such as `theme-store.ts`                 |
| Tests                        | Beside the source, same stem plus `.test`: `Hero.test.tsx` |
| Folders                      | Lowercase, one per feature or capability                   |

- Imports point one way. Feature and shared code never import the app shell or routes. No cycles.
- Route and page files stay thin and compose feature hooks and components.
- When you split a file, the original stays as the composition root and keeps its exports.
- A repo with another settled convention (ejm-ide names hooks `use-thing.ts`) records it in its
  own skill and applies it everywhere.

## Consuming ejm-ui

- Import only paths listed in each `@ejm/*` package's `exports`. Everything else is private.
  Never copy shared source into a product.
- A missing component, token or behavior goes into ejm-ui, its owner. Then rebuild the consumers.
- Sites add no local CSS, inline styles or visual `className`. The `ejmSite` plugin fails on them.
- Linked packages need one React at build and type check time. See references/setup.md.

## Builds and performance

- Vite builds every web target. Sites prerender every route to HTML that reads with scripts off.
- Lazy load what the first view does not show: demos, charts, code highlighting, heavy routes.
- The repository's gate measures cost. ejm-ui sites fail when first contentful paint is 1000 ms
  or more over the committed baseline. Byte budgets, where a repo sets them, fail the build.
- Never raise a budget to fit a change: trim it or load it later. Record a new load-time baseline
  only for an intended change, in the same commit.
- Prefer the maintained library that does the job. Size alone never justifies custom code.
- Measure before you claim a speedup or change architecture for a reported slowdown.

## Tests

- Vitest for units and components. `node:test` for tooling and checks of built output.
- Site tests read the built HTML without running scripts: one `h1`, skip link first,
  `main#main`, external links `rel="external"`, no inline script or style.
- Playwright drives browsers: `@playwright/test` for user flows, `playwright-core` with the
  installed Chrome for scripts and captures. A test skipped for lack of Chrome verified nothing.
- A changed user flow gets a browser test or a recorded browser check.

## Read next

- [references/setup.md](references/setup.md): when you write or fix tsconfig, ESLint,
  package.json, `.npmrc`, a Vite config, a budget gate, or link ejm-ui into a consumer.
- [references/react.md](references/react.md): when you place state, write a hook or effect, or
  fetch, cache or paginate data.
