# Spec Delta

## Purpose

Build the worker binary and SDK packages reproducibly in CI without write access, so a release can attach checked files.

## ADDED Requirements

### Requirement: Read-only reproducible builds

Worker and SDK build jobs SHALL hold read-only repository access and no secrets, build with locked dependencies, and record the SHA-256 of every produced file.

#### Scenario: A build job runs

- **WHEN** the build workflow runs on a main commit
- **THEN** it uploads archives and checksums as workflow artifacts and holds no write permission or secret

### Requirement: Smoke before offer

A worker archive SHALL be offered only after its binary completes the protocol handshake and both SDKs find it through `EJM_HARNESS_WORKER_PATH`.

#### Scenario: Handshake fails

- **WHEN** the built worker does not answer `hello` with the protocol and version
- **THEN** the smoke job fails and no archive is offered

### Requirement: Publication stays gated

SDK registry publication SHALL use a separate job that runs no project code and SHALL NOT be enabled until the owner confirms the package names and registry trust settings.

#### Scenario: Names unconfirmed

- **WHEN** the owner has not confirmed the package names
- **THEN** no workflow in this repository publishes a package

### Requirement: Honest release notes

Release notes SHALL describe builds as not signed with a developer certificate and not notarized, name the Linux runtime baseline, and list open native evidence as known limits.

#### Scenario: Notes are read

- **WHEN** a reader opens the release notes
- **THEN** they claim no signing, notarization or certified native support
