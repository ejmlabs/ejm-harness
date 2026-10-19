# Spec Delta

## Purpose

Keep registry account sessions bound to their issuing origin and prevent stale plaintext credentials from returning after migration or logout.

## ADDED Requirements

### Requirement: Registry sessions use the protected vault

The system SHALL persist registry account tokens only in the encrypted shared vault, scoped to the normalized registry origin. It SHALL return an actionable error when the OS key service is unavailable and SHALL keep credentials out of package downloads and diagnostics.

#### Scenario: Select another registry

- **WHEN** a caller loads a saved session for a different origin
- **THEN** the original token is not returned or sent to the new registry

#### Scenario: Key service is unavailable

- **WHEN** the host requests persistent registry credentials without a usable OS key service
- **THEN** storage fails without writing a plaintext token or unprotected vault key

### Requirement: Legacy migration requires a controlled handoff

The system SHALL import a legacy registry session only after the host confirms older clients have stopped. It SHALL validate source privacy, size and origin, commit and reopen the encrypted destination before deleting the source, and preserve a permanent migration marker.

#### Scenario: Handoff is unconfirmed

- **WHEN** a caller requests migration without confirming older clients have stopped
- **THEN** migration fails before changing either credential source

#### Scenario: Legacy source is a FIFO

- **WHEN** the legacy registry source is a FIFO without a writer
- **THEN** migration rejects it without waiting for a writer or reading credentials
- **AND** the source and existing encrypted destination remain unchanged

#### Scenario: Cleanup is interrupted

- **WHEN** encrypted migration commits but legacy source deletion fails
- **THEN** retry can retire the source without overwriting a newer destination record

### Requirement: Revoked sessions cannot return from legacy files

The system SHALL retain the migration marker after logout or replacement and SHALL never automatically fall back to a legacy plaintext record.

#### Scenario: Logout with a stale source present

- **WHEN** a migrated session is deleted and an old plaintext source remains
- **THEN** future loads remain signed out and another migration cannot restore the old token
