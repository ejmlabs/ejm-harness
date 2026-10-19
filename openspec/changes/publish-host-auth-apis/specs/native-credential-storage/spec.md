# Spec Delta

## Purpose

Give every supported native host one credential-vault contract, with protected local persistence and consistent transaction, migration, and recovery behavior.

## ADDED Requirements

### Requirement: Shared encrypted local vault

The system SHALL use one local vault implementation and versioned encrypted format on macOS, Windows, and Linux. It SHALL protect the vault key with the current user's OS credential service and preserve product, provider, account, and credential-purpose separation.

#### Scenario: Store and reopen credentials on each OS

- **WHEN** an authorized host saves provider credentials or a namespaced host secret and restarts on a supported OS
- **THEN** it can recover the same record through the shared store
- **AND** neither the vault file nor diagnostic output contains plaintext credentials or the unprotected vault key

#### Scenario: Separate credential purposes

- **WHEN** provider, relay, IDE profile, and registry records exist
- **THEN** a lookup for one identity and purpose cannot return a record from another namespace
- **AND** using the same implementation does not require different products to use the same physical vault

### Requirement: Exclusive credential transactions

The system SHALL serialize credential mutation across processes sharing a vault. Exclusive ownership SHALL cover credential reads, rotating-token exchange, and durable replacement. Lock waits SHALL be cancellable and bounded to 30 seconds.

#### Scenario: Two hosts renew the same account

- **WHEN** two processes attempt renewal against one vault
- **THEN** only one holds its credential transaction at a time
- **AND** the second reloads the committed state before deciding whether renewal is needed

#### Scenario: Cancel a blocked transaction

- **WHEN** a caller cancels while another process holds the vault lock
- **THEN** that caller stops waiting without altering state or leaving lock ownership behind

#### Scenario: Native key access does not return

- **WHEN** native key access blocks during asynchronous transaction acquisition
- **THEN** the asynchronous caller returns when cancelled or when its original 30-second deadline expires
- **AND** the blocking worker retains exclusive ownership until its already-started native call returns and checks cancellation and the deadline before later key or durable changes or transaction delivery
- **AND** an already-started key write may finish without exposing a credential transaction, while its bootstrap marker remains available for recovery without key replacement

### Requirement: Atomic bounded authenticated persistence

The system SHALL authenticate encrypted state before decoding or using it, reject unknown versions and oversized data, and atomically commit complete updates. It SHALL retain the provider-state limits of 1 MiB and 64 registrations and impose explicit limits on other namespaces and encrypted files.

#### Scenario: Corrupt or substituted vault

- **WHEN** the vault is truncated, changed, belongs to another identity, or exceeds its limits
- **THEN** opening it fails with a redacted actionable error
- **AND** the system does not accept partial state, create an empty replacement, or fall back to plaintext

#### Scenario: Save is interrupted

- **WHEN** a process stops during persistence
- **THEN** reopening yields the prior complete state or the committed complete state
- **AND** temporary files contain only encrypted payloads and cannot bypass path protections

### Requirement: Uncertain refresh remains uncertain

The system SHALL preserve account mappings, authorization revisions, and interrupted-refresh evidence across vault updates and migration. After an uncertain token exchange or save it SHALL require the existing recovery path instead of retrying the old rotating credential.

#### Scenario: Crash after a token exchange

- **WHEN** renewal may have reached the provider but final persistence is unconfirmed
- **THEN** restart exposes recovery-required state without replaying the prior refresh token

### Requirement: Missing key services fail closed

The system SHALL return setup or unlock guidance when the configured OS credential service or vault key is unavailable. It SHALL never silently change persistence mode, generate a replacement key for an existing vault, or store an unprotected key beside encrypted data.

#### Scenario: Linux has no unlocked secret service

- **WHEN** a host requests persistent storage without a usable user secret service
- **THEN** it receives an actionable error without writing a plaintext vault or key
- **AND** any supported memory-only credential mode requires explicit selection and reports that it will not persist

#### Scenario: Vault key is lost

- **WHEN** an encrypted vault exists but its protecting OS record has been deleted
- **THEN** the system preserves the vault and requires explicit recovery or sign-in
- **AND** it does not overwrite or reset the encrypted state automatically

### Requirement: Migration preserves usable credentials

The system SHALL migrate supported legacy credentials under exclusive ownership, verify the new encrypted record before changing active references, and resume safely after interruption. It SHALL avoid silently restoring stale legacy credentials after a migrated record is revoked or changed.

#### Scenario: Migrate a private provider file

- **WHEN** a valid legacy provider store opens for migration
- **THEN** the same accounts, client mappings, credentials, and recovery markers become available through the encrypted vault
- **AND** the legacy source is not removed before the new state is verified

#### Scenario: Host credential migration cannot finish

- **WHEN** copying an existing host secret succeeds but its active reference update fails
- **THEN** the original remains usable and a retry reconciles the migration without exposing secret values
- **AND** a completed migration cannot later fall back to an older revoked source

#### Scenario: Legacy provider process overlaps migration

- **WHEN** an older provider host already holds the legacy store transaction
- **THEN** migration waits on that same stable lock and rereads the final legacy state after acquiring it
- **AND** successful handoff leaves a nonsecret legacy version barrier that older readers reject instead of creating fresh plaintext state

#### Scenario: Legacy host has no compatible migration lock

- **WHEN** old keyring-based host processes cannot participate in the vault lock
- **THEN** migration requires those clients to stop before cutover and documents concurrent mixed-version use as unsupported
- **AND** an unconfirmed handoff defers migration without deleting or replacing the source identity

### Requirement: Native path protection

The system SHALL reject unsafe vault, lock, key-reference, and temporary-file paths on each OS. It SHALL protect them against other local users and reject directory or lock replacement, symlink or reparse redirection, and unsupported filesystem semantics.

#### Scenario: Credential directory is redirected

- **WHEN** a symlink, Windows junction, reparse point, or replaced lock redirects an open vault path
- **THEN** reads and updates fail without disclosing or modifying credentials through the redirected path

#### Scenario: Caller runs as the same OS user

- **WHEN** support guidance describes the vault's security boundary
- **THEN** it states that OS-user protection does not isolate arbitrary code running as that user and does not provide cloud sync or portable backup recovery
