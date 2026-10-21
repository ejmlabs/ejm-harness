# package-distribution Specification

## Purpose

Define a portable package format and registry selection that belong to the open source harness and work consistently across hosts.

## Requirements

### Requirement: Project-owned package manifest

Packages SHALL use `ejm.package.json` at the package root. Archive validation, native installation, SDK package fixtures, and public examples SHALL use the same name without a legacy alias.

#### Scenario: Install an authored package

- **WHEN** a host installs a valid package with `ejm.package.json`
- **THEN** its exports are available through the shared package contract after explicit grants

### Requirement: Explicit registry selection

Registry package operations SHALL require an explicitly configured origin. Local, npm, PyPI, and Git installs SHALL work without a registry. Registry transport SHALL preserve HTTPS or loopback HTTP, fixed origins, no redirects, bounded responses, and exact-origin credential storage.

#### Scenario: Registry origin is absent

- **WHEN** a host installs an exact registry package without a configured registry
- **THEN** installation fails with a configuration error before network access or persistence

#### Scenario: Install a local package without a registry

- **WHEN** a host installs a valid local package without a registry
- **THEN** installation succeeds and starts disabled with empty grants

#### Scenario: Select a new registry origin

- **WHEN** a host supplies a different registry origin
- **THEN** existing tokens for another origin are not reused or transferred
