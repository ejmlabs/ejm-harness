# Spec Delta

## Purpose

Keep repository control files and harness state outside the coding tools' readable and writable workspace surfaces.

## ADDED Requirements

### Requirement: Protected paths remain protected through aliases

Coding file tools MUST reject protected repository and harness state paths for reads, listings, and edits, including symlinks that resolve into those paths. Normal workspace file operations MUST remain available.

#### Scenario: A protected path is requested through a symlink

- **WHEN** a file tool receives an alias into .git or harness state
- **THEN** the tool denies the operation before returning its contents
