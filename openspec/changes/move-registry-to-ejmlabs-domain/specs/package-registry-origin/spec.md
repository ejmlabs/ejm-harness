# Spec Delta

## Purpose

Point hosts at a package registry Everett controls on `ejmlabs.com`, never trust or fall back to the retired `meetgerry.ai` registry, and keep package integrity checks intact across the move.

## ADDED Requirements

### Requirement: The public registry origin is selected explicitly

Registry origins MUST be configured explicitly, with no implicit default. `PUBLIC_REGISTRY_ORIGIN` identifies `https://registry.ejmlabs.com` for hosts that choose the public registry. Both hosted Workers MUST use the same exact origin, so their version records point downloads at it.

#### Scenario: Install from the explicitly selected public registry

- **WHEN** a host installs `@publisher/package@1.2.3` using the explicitly selected public registry
- **THEN** the version record and archive are fetched only from `https://registry.ejmlabs.com`

#### Scenario: Device login on the explicitly selected public registry

- **WHEN** a host starts registry sign-in with the explicitly selected public registry
- **THEN** the device code request, the verification URL and the token polling all use `https://registry.ejmlabs.com`

#### Scenario: Server origin mismatch

- **WHEN** the registry returns a version record whose `download_url` is on a different origin
- **THEN** the harness rejects the record and downloads nothing

### Requirement: Other package sources need no registry configuration

`install_source` MUST require `Some(&client)` for registry sources. Local, npm, PyPI, and Git sources MUST accept `None` without constructing a registry client.

#### Scenario: Registry source without a client

- **WHEN** a host installs an exact registry reference with `None`
- **THEN** installation fails without contacting any registry

#### Scenario: Local source without a client

- **WHEN** a host installs a valid local package with `None`
- **THEN** installation uses the local source without selecting a registry

### Requirement: Registry origins are plain HTTPS origins

A registry origin MUST be a plain origin with no path, credentials, query or fragment. It MUST use `https`, except plain `http` to only `localhost`, `127.0.0.1`, or `[::1]`. Registry HTTP clients MUST NOT follow redirects. A plain HTTP loopback client MUST NOT send requests through `HTTP_PROXY`, `http_proxy`, `ALL_PROXY`, or `all_proxy`.

#### Scenario: HTTPS origin accepted

- **WHEN** a host creates a registry client for `https://registry.ejmlabs.com`
- **THEN** the client is created

#### Scenario: Insecure or malformed origins rejected

- **WHEN** a host creates a registry client for `http://registry.example.com`, `https://user:pw@registry.ejmlabs.com`, `https://ejmlabs.com/registry` or `ftp://registry.ejmlabs.com`
- **THEN** creation fails and no request is sent

#### Scenario: Loopback development registry

- **WHEN** a host creates a registry client for `http://127.0.0.1:8787`, `http://localhost:8787`, or `http://[::1]:8787`
- **THEN** the client is created

#### Scenario: Other loopback addresses are rejected

- **WHEN** a host creates a registry client for `http://127.0.0.2`
- **THEN** creation fails and no request is sent

#### Scenario: Loopback HTTP ignores an HTTP proxy

- **WHEN** a host creates a registry client for `http://127.0.0.1` while `HTTP_PROXY` points at another host
- **THEN** the request is sent to that loopback origin and not to the proxy

#### Scenario: Redirect is not followed

- **WHEN** the registry answers any request with a redirect
- **THEN** the redirect is not followed and no credential is re-sent

### Requirement: The retired meetgerry registry is never trusted

The harness MUST NOT contain the retired `meetgerry.ai` registry as a default or fallback. It MUST reject any registry origin whose host is `meetgerry.ai` or a subdomain of it, including a path, userinfo, or `http` scheme, with an error that names the public registry and requires an explicit origin. When the explicitly selected public registry is unavailable, the harness MUST NOT try any other origin.

#### Scenario: Old origin requested explicitly

- **WHEN** a host creates a registry client for `https://harness.meetgerry.ai`, `http://harness.meetgerry.ai`, `https://user:pw@harness.meetgerry.ai`, or `https://harness.meetgerry.ai/registry`
- **THEN** creation fails with a message that the meetgerry registry is retired and that `https://registry.ejmlabs.com` is the public registry

#### Scenario: Selected registry unreachable

- **WHEN** a request to the explicitly selected public registry fails to connect
- **THEN** the operation fails without contacting any other origin

### Requirement: Saved sessions for the retired origin are never sent

The harness MUST NOT save, load, import or send a registry session for the retired origin, and MUST NOT re-key it to another origin.

#### Scenario: Retired origin in a credential call

- **WHEN** a host saves, loads or migrates a registry session for `https://harness.meetgerry.ai`
- **THEN** the call fails with the retired-registry message naming the public registry, and no token is stored, returned or sent

#### Scenario: Legacy session file for the retired origin

- **WHEN** the package store holds a legacy registry session for `https://harness.meetgerry.ai` and the host migrates for another origin
- **THEN** migration refuses the file as belonging to another origin, and the old token is never imported or transmitted

#### Scenario: Vault record for the retired origin

- **WHEN** the encrypted vault holds a session for the retired origin
- **THEN** loading a session for the explicitly selected origin does not return it

### Requirement: Package integrity is unchanged by the move

For registry installs, the harness MUST keep verifying the exact archive size, SHA-256 digest and manifest identity against the version record. It MUST keep downloading only from the registry's own origin without credentials, and installing or updating a package MUST leave it disabled, with no trust or grants, until the user enables it again. Already-installed snapshots MUST remain usable without contacting any registry.

#### Scenario: Digest mismatch

- **WHEN** an archive from the explicitly selected public registry does not match the version record's size or SHA-256
- **THEN** installation fails and nothing is registered

#### Scenario: Same version reinstalled from the new registry

- **WHEN** a package installed earlier is installed again at the same name and version from the new registry
- **THEN** the stored record reflects the new archive's digest, and the package is disabled with no trust or grants until the user enables it again

#### Scenario: Previously installed package

- **WHEN** a package installed before the move is enabled and loaded
- **THEN** it loads from its local snapshot without any registry request

### Requirement: Loopback account requests remain on the direct route

For every accepted HTTP loopback registry origin, `RegistryClient` MUST disable proxies for credential-bearing requests. HTTPS proxy support MUST remain available.

#### Scenario: Account bearer with proxy environment

- **WHEN** a host calls `me` with a synthetic account bearer on an HTTP loopback registry while HTTP and all-protocol proxy variables point at another local fixture
- **THEN** only the direct registry receives the authenticated request, the proxy receives no request, and the bearer does not appear in process output
