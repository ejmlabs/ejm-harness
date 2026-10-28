## ADDED Requirements

### Requirement: TypeScript projects use the shared strictest base

Every TypeScript project in the repository (`registry`, `sdk/typescript`) SHALL extend `@tsconfig/strictest` at the pinned version and SHALL set only environment options (target, lib, module, module resolution, JSX, types, paths) itself, and every TypeScript file SHALL belong to a checked project.

#### Scenario: A project turns a strict flag off

- **WHEN** a tsconfig sets any flag from the strictest base to a weaker value
- **THEN** review refuses the change and the typecheck no longer matches the shared base

#### Scenario: A TypeScript file outside every project

- **WHEN** a `.ts` or `.tsx` file is in no checked tsconfig
- **THEN** the typed lint step fails with a project-service parse error

### Requirement: Code passes the shared typed ESLint layer

TypeScript code SHALL pass the shared `eslint.strict.mjs` layer (typescript-eslint `strictTypeChecked` and `stylisticTypeChecked` with type information), and JavaScript files SHALL pass its untyped rules, with zero warnings.

#### Scenario: A floating promise

- **WHEN** a change adds an unawaited promise in TypeScript
- **THEN** the lint step fails

### Requirement: Python is strictly typed and linted

Python under `scripts/`, `sdk/python/`, `crates/ejm-harness-packages/runtime/`, `registry/tests/` and `examples/` SHALL pass `mypy --strict` (2.4.0) and ruff (0.17.0) with the shared `ruff.strict.toml`, run from a locked development group.

#### Scenario: An unannotated function

- **WHEN** a change adds a Python function without annotations
- **THEN** the check fails

### Requirement: Rust uses the shared lint tables

The workspace `Cargo.toml` and `registry/Cargo.toml` SHALL carry the shared Rust lint tables (clippy `all` and `pedantic` denied, plus the shared restriction lints) at the same or a stricter level, and clippy SHALL run with `-D warnings` on all targets.

#### Scenario: An unwrap in library code

- **WHEN** a change adds `.unwrap()` outside a reasoned `#[expect]`
- **THEN** clippy fails

### Requirement: Shell scripts pass strict ShellCheck

`scripts/check`, `scripts/check-registry`, `scripts/install-hooks` and `scripts/*.sh` SHALL pass ShellCheck 0.11.0 with the shared `.shellcheckrc` (all optional checks except SC2250).

#### Scenario: A masked return value

- **WHEN** a script uses a command substitution whose failure would be ignored
- **THEN** the shell check fails

### Requirement: CI enforces the strict settings

`./scripts/check`, which the shared gate already runs, SHALL run every tool above.

#### Scenario: A violation reaches a pull request

- **WHEN** a pull request introduces any finding from these tools
- **THEN** the repository's required check fails

### Requirement: Escape hatches are line-scoped and reasoned

The repository SHALL contain no explicit `any`, no `@ts-ignore`, `@ts-expect-error` or `@ts-nocheck`, no unlimited or file-wide lint disable, and no strict compiler flag or lint rule turned off. Every remaining suppression SHALL name one rule and carry a reason.

#### Scenario: Undescribed suppression

- **WHEN** a change adds `// eslint-disable-next-line` without a rule name and a ` -- reason`, or a Rust `#[allow]` without a reason
- **THEN** the repository's check fails
