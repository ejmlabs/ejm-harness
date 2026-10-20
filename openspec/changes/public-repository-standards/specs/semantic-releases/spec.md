# Spec Delta

## Purpose

Provide independent semantic versions and a consistent, verifiable release process across all ejmlabs projects.

## ADDED Requirements

### Requirement: Version metadata agrees

Each repository MUST use a canonical SemVer version and keep its releasable package metadata consistent with that version. Breaking, feature, and fix releases SHALL follow SemVer, including explicit prereleases.

#### Scenario: Versions diverge

- **WHEN** a package version disagrees with the canonical version
- **THEN** the release validation fails before a tag or release is created

### Requirement: Releases use checked immutable commits

A release MUST select an exact main commit with successful required checks, validate its version and notes, and produce a draft release with source archives and SHA-256 checksums. Archive validation MUST preserve original source and notice bytes, including line endings. Existing version tags or released assets MUST NOT be overwritten.

#### Scenario: A release is prepared

- **WHEN** a maintainer requests a release of a checked main commit
- **THEN** the pipeline validates the version and creates a draft with traceable sources and checksums

#### Scenario: A notice uses CRLF line endings

- **WHEN** the checked source contains an original notice with CRLF line endings
- **THEN** archive validation compares the original bytes without normalizing them

#### Scenario: A version already exists

- **WHEN** a release request reuses an existing version
- **THEN** the pipeline refuses to move the tag or replace its release

### Requirement: Publication remains explicit

Pipeline setup MUST NOT publish a product release, package, installer, or deployment. Native installer signing and package registry publication MUST have separate, explicit gates.

#### Scenario: The pipeline is installed

- **WHEN** repository standards are adopted
- **THEN** no product version is tagged or published by that setup
