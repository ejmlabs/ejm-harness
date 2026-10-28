## 1. Layer

- [x] 1.1 Sync `eslint.strict.mjs`, `ruff.strict.toml`, `.shellcheckrc`; rebuild `eslint.config.mjs` on the shared layer.
- [x] 1.2 tsconfigs extend `@tsconfig/strictest`; Rust tables gain `exit`, `mem_forget`, `unreachable`.
- [x] 1.3 ruff and mypy coverage and versions; `uv.lock` with uv 0.9.26.

## 2. Fixes

- [x] 2.1 TypeScript 3, ESLint 8, ruff 65, mypy 4, clippy 1, ShellCheck 2.

## 3. Enforcement and verification

- [x] 3.1 `scripts/check` runs ShellCheck and passes; the registry typecheck passes (`check-registry` needs Node 24 and runs in CI).
