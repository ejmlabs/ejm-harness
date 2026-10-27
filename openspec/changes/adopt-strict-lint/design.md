## Context

The gate runs `npm ci`, `npm --prefix registry ci` and `./scripts/check`. The registry compiles with TypeScript 7.0.2 while the root lint resolves TypeScript 6.0.3, the split typed lint needs. Measured: TypeScript 3 (TS4111 in one registry file), ESLint 8 with the shared layer, ruff 58, mypy 2, clippy 1 (`process::exit`), ShellCheck about 3.

## Decisions

- **Keep the TypeScript split.** `registry` stays on TypeScript 7.0.2 for `tsc`; the root lint keeps TypeScript 6.0.3.
- **unittest stays.** The shared layer already allows unittest assertions; tests keep `assert`-free unittest style.
- **The one `exit`** in the worker becomes a returned error or a reasoned `#[expect]` if it is the process entry point.
- **No loosening.** Escape hatches are line-scoped and reasoned. There is no explicit `any`, no `@ts-ignore`, `@ts-expect-error` or `@ts-nocheck`, no file-wide or unlimited disable, and no strict flag or rule turned off. A remaining `eslint-disable-next-line <rule> -- <reason>`, `# noqa: <code>` with a reason, `# type: ignore[<code>]` with a reason, `#[expect(<lint>, reason = "...")]` or `# shellcheck disable=<code>` with a reason names one rule and says why.

## Risks

Small. The SDK lock is regenerated with uv 0.9.26 so the gate can read it.
