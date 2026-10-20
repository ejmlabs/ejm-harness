# Spec Delta

## Purpose

Point hosts at a package registry Everett controls on `ejmlabs.com`, never trust or fall back to the retired `meetgerry.ai` registry, and keep package integrity checks intact across the move.

## ADDED Requirements

### Requirement: Default registry origin is on ejmlabs.com

The harness MUST publish `https://registry.ejmlabs.com` as its default package registry origin. Everett confirmed this host on 2026-10-20. Every registry operation a host performs with the default MUST go to that origin. The hosted registry and auth Workers MUST be configured with the same origin, so the version records they return point downloads at it.

#### Scenario: Install from the default registry

- **WHEN** a host installs `@publisher/package@1.2.3` using the default registry
- **THEN** the version record and archive are fetched only from `https://registry.ejmlabs.com`

#### Scenario: Device login on the default registry

- **WHEN** a host starts registry sign-in with the default registry
- **THEN** the device code request, the verification URL and the token polling all use `https://registry.ejmlabs.com`

#### Scenario: Server origin mismatch

- **WHEN** the registry returns a version record whose `download_url` is on a different origin
- **THEN** the harness rejects the record and downloads nothing

### Requirement: Registry origins are plain HTTPS origins

A registry origin MUST be a plain origin with no path, credentials, query or fragment. It MUST use `https`, except plain `http` to a loopback host for local development, using the same loopback set as the provider-endpoint rule. Registry HTTP clients MUST NOT follow redirects.

#### Scenario: HTTPS origin accepted

- **WHEN** a host creates a registry client for `https://registry.ejmlabs.com`
- **THEN** the client is created

#### Scenario: Insecure or malformed origins rejected

- **WHEN** a host creates a registry client for `http://registry.example.com`, `https://user:pw@registry.ejmlabs.com`, `https://ejmlabs.com/registry` or `ftp://registry.ejmlabs.com`
- **THEN** creation fails and no request is sent

#### Scenario: Loopback development registry

- **WHEN** a host creates a registry client for `http://127.0.0.1:8787`
- **THEN** the client is created

#### Scenario: Redirect is not followed

- **WHEN** the registry answers any request with a redirect
- **THEN** the redirect is not followed and no credential is re-sent

### Requirement: The retired meetgerry registry is never trusted

The harness MUST NOT contain the retired `meetgerry.ai` registry as a default or fallback. It MUST reject any registry origin whose host is `meetgerry.ai` or a subdomain of it, with an error that names the current default. When the default registry is unavailable, the harness MUST NOT try any other origin.

#### Scenario: Old origin requested explicitly

- **WHEN** a host creates a registry client for `https://harness.meetgerry.ai`
- **THEN** creation fails with a message that the meetgerry registry is retired and that `https://registry.ejmlabs.com` is the default

#### Scenario: Default registry unreachable

- **WHEN** a request to the default registry fails to connect
- **THEN** the operation fails without contacting any other origin

### Requirement: Saved sessions for the retired origin are never sent

The harness MUST NOT send a registry session saved for the retired origin to any host, and MUST NOT re-key it to another origin. A legacy session file for the retired origin MUST produce an actionable message instead of permanently blocking sign-in to the current origin. The harness MUST let the host discard that session locally.

#### Scenario: Legacy session file for the retired origin

- **WHEN** the package store holds a legacy registry session for `https://harness.meetgerry.ai` and the user signs in to the default registry
- **THEN** sign-in is not silently blocked: the user is told that the old session belongs to a retired registry and how to discard it, and the old token is never transmitted

#### Scenario: Discarding the retired session

- **WHEN** the host asks the harness to discard the retired-origin session
- **THEN** the legacy file or vault record is removed locally without any network request, and sign-in to the default registry can proceed

#### Scenario: Vault record for the retired origin

- **WHEN** the encrypted vault holds a session for the retired origin
- **THEN** loading a session for the default origin does not return it

### Requirement: Package integrity is unchanged by the move

For registry installs, the harness MUST keep verifying the exact archive size, SHA-256 digest and manifest identity against the version record. It MUST keep downloading only from the registry's own origin without credentials, and installing or updating a package MUST leave it disabled, with no trust or grants, until the user enables it again. Already-installed snapshots MUST remain usable without contacting any registry.

#### Scenario: Digest mismatch

- **WHEN** an archive from the default registry does not match the version record's size or SHA-256
- **THEN** installation fails and nothing is registered

#### Scenario: Same version reinstalled from the new registry

- **WHEN** a package installed earlier is installed again at the same name and version from the new registry
- **THEN** the stored record reflects the new archive's digest, and the package is disabled with no trust or grants until the user enables it again

#### Scenario: Previously installed package

- **WHEN** a package installed before the move is enabled and loaded
- **THEN** it loads from its local snapshot without any registry request
