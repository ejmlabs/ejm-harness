## Why

The owner asked for strict type checking and strict linting in every ejmlabs repository where they apply, from one shared layer, enforced by CI. This repository is already the closest: typed ESLint on the registry and SDK, clippy `pedantic` denied with restriction lints, and `mypy --strict` on the scripts and SDK. A few gaps remain.

## What Changes

- Copy `eslint.strict.mjs` and compose `eslint.config.mjs` from it, keeping the repository's extra rules (`explicit-function-return-type`, `no-floating-promises` with `ignoreVoid: false`); typed rules cover every TypeScript file, untyped rules every JavaScript file.
- `registry/tsconfig.json` and `sdk/typescript/tsconfig.json` extend `@tsconfig/strictest`.
- Add `clippy::exit`, `mem_forget` and `unreachable` to the workspace and registry lint tables.
- Root `ruff.toml` and the SDK's `[tool.ruff]` extend `ruff.strict.toml`; `mypy --strict` also covers `crates/ejm-harness-packages/runtime`, `registry/tests`, `examples` and `sdk/python/tests`; the SDK development group moves to `ruff==0.17.0`, `mypy==2.4.0` and adds `shellcheck-py==0.11.0.1`.
- Copy `.shellcheckrc`; `scripts/check` runs ShellCheck on `scripts/check`, `scripts/check-registry`, `scripts/install-hooks` and `scripts/*.sh`.
- Pin `typescript@6.0.3`, `@tsconfig/strictest@2.0.8`, `eslint@10.13.0`, `@eslint/js@10.0.1`, `typescript-eslint@8.71.2`, `globals@17.13.0`, `@eslint-community/eslint-plugin-eslint-comments@4.8.1` at the root.

## Capabilities

### New Capabilities

- `strict-lint`: the strict type-checking and lint settings this repository holds, how CI enforces them, and how suppressions are allowed.

## Impact

`eslint.config.mjs`, new `eslint.strict.mjs`, `ruff.strict.toml`, `.shellcheckrc`, the two tsconfigs, `Cargo.toml`, `registry/Cargo.toml`, `ruff.toml`, `sdk/python/pyproject.toml` and `uv.lock`, `package.json` and lock, `scripts/check`, and the code the findings touch.
