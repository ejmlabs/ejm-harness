# Spec Delta

## Purpose

Make sure model provider API keys are only sent over TLS, except to loopback endpoints on the same machine, so a mistyped or hostile base URL cannot leak a credential in cleartext.

## ADDED Requirements

### Requirement: Provider endpoints require HTTPS except for loopback

The harness MUST accept a provider base URL only if its scheme is `https`, or its scheme is `http` and its host is loopback. Loopback hosts are `localhost` (exact, ASCII case-insensitive), an IPv4 address in `127.0.0.0/8`, or the IPv6 address `::1`. Every other scheme or host MUST be rejected. The existing rejection of embedded credentials, query and fragment MUST remain.

#### Scenario: HTTPS remote endpoint

- **WHEN** a provider is configured with `https://api.deepseek.com/v1`
- **THEN** the provider is created and requests use that HTTPS endpoint

#### Scenario: HTTP remote endpoint

- **WHEN** a provider is configured with `http://api.example.com/v1`
- **THEN** provider construction fails with an error stating that HTTPS is required except for loopback hosts

#### Scenario: HTTP loopback endpoints

- **WHEN** a provider is configured with `http://localhost:11434/v1`, `http://LOCALHOST:8080`, `http://127.0.0.1:1234/v1`, `http://127.0.0.2:1234` or `http://[::1]:1234/v1`
- **THEN** the provider is created

#### Scenario: Lookalike hosts are not loopback

- **WHEN** a provider is configured with `http://localhost.evil.com/v1`, `http://127.0.0.1.nip.io/v1`, `http://localhost./v1`, `http://evil-localhost/v1`, `http://0.0.0.0:8080`, `http://[::ffff:127.0.0.1]:8080`, `http://10.1.2.3/v1`, `http://172.16.5.5/v1` or `http://192.168.1.10:11434`
- **THEN** provider construction fails

#### Scenario: Other schemes

- **WHEN** a provider is configured with a `ws`, `wss`, `ftp`, `file` or schemeless URL
- **THEN** provider construction fails

#### Scenario: Embedded credentials still rejected

- **WHEN** a provider is configured with `https://key:secret@api.example.com/v1` or a URL with a query or fragment
- **THEN** provider construction fails as it does today

### Requirement: Endpoint rejection happens before any credential leaves the process

The harness MUST validate the base URL when the provider is constructed, before opening any network connection or attaching any API key to a request. The rejection error MUST NOT include the API key.

#### Scenario: Rejected endpoint sends nothing

- **WHEN** an OpenAI-compatible or Anthropic provider is constructed with `http://api.example.com/v1` and a non-empty API key
- **THEN** construction fails, no connection is attempted, and the error text does not contain the key

### Requirement: Endpoint rule applies to every configuration source

The same rule MUST apply however the base URL reaches a key-bearing provider. That covers the Rust provider configuration APIs and the worker's provider configuration message used by the TypeScript and Python SDKs. No configuration value, environment variable or flag MAY disable it.

#### Scenario: Worker configuration from an SDK

- **WHEN** an SDK host sends an `openai` provider configuration with `base_url` `http://api.example.com/v1` to the worker
- **THEN** the worker rejects the configuration with the HTTPS-required error and keeps no provider configured from it

#### Scenario: Worker configuration with loopback

- **WHEN** an SDK host sends an `openai` provider configuration with `base_url` `http://127.0.0.1:8080/v1`
- **THEN** the worker accepts the configuration

### Requirement: HTTPS endpoints cannot be downgraded by redirect

Key-bearing provider HTTP clients MUST NOT follow redirects, so an accepted HTTPS endpoint cannot redirect a request carrying a credential to a plaintext URL.

#### Scenario: Redirect to plaintext

- **WHEN** an accepted HTTPS provider endpoint responds with a redirect to an `http://` URL
- **THEN** the redirect is not followed and the request fails without re-sending the credential

### Requirement: Plain HTTP loopback does not use a proxy

An `http` provider endpoint is accepted only for loopback. The HTTP client built for that endpoint MUST NOT send the request through `HTTP_PROXY`, `http_proxy`, `ALL_PROXY`, or `all_proxy`. The API key MUST remain on the loopback connection. An `https` provider endpoint MUST continue to honor the process environment's proxies.

#### Scenario: Loopback HTTP ignores an HTTP proxy

- **WHEN** a provider is configured with `http://127.0.0.1:1234/v1` and `HTTP_PROXY` names another host
- **THEN** no request carrying the API key is sent to the proxy
