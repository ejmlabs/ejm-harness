## 1. Layer

- [x] 1.1 Sync `eslint.strict.mjs`, `ruff.strict.toml`, `.shellcheckrc`; rebuild `eslint.config.mjs` on the shared layer.
- [x] 1.2 tsconfigs extend `@tsconfig/strictest`; Rust tables gain `exit`, `mem_forget`, `unreachable`.
- [x] 1.3 ruff and mypy coverage and versions; `uv.lock` with uv 0.9.26.

## 2. Fixes

- [x] 2.1 TypeScript 3, ESLint 8, ruff 58 (SLF001 19 outside tests, TRY301 18, BLE001 7), mypy 2, clippy 1, ShellCheck about 3.

## 3. Enforcement and verification

- [x] 3.1 `scripts/check` runs ShellCheck; `./scripts/check` and `./scripts/check-registry` pass.
