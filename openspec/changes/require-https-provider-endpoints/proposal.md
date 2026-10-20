# Proposal

## Why

Security audit finding **L2** (`security-baseline-2026-10-19.md`, ejm-harness at `8d3dc39`) blocks the coordinated 0.1.0 release. `endpoint()` in `crates/ejm-harness/src/provider.rs:436-441` accepts `http` or `https` for any host. The OpenAI-compatible provider then sends `bearer_auth(api_key)` (`provider.rs:144`), and the Anthropic provider sends `x-api-key` (`provider.rs:301`). A typo or a hostile base URL can therefore send a long-lived provider key over plaintext HTTP to a remote host. ejm-cli passes user-supplied URLs straight into these constructors. ejm-ide already enforces "TLS except loopback" in its own validator (`ejm-ide/src-tauri/src/providers/driver.rs:433-455`, used from `profiles.rs:56`, tested at `profiles.rs:520`).

## What Changes

- **BREAKING:** provider base URLs must use `https`. Plain `http` is accepted only when the host is loopback: `localhost` (exact, case-insensitive), any IPv4 address in `127.0.0.0/8`, or `::1`. Any other `http` URL, and any other scheme, is rejected when the provider is constructed. That happens before any connection is opened or any credential is sent.
- The error clearly says HTTPS is required except for loopback. It never includes the API key.
- Hostnames that merely look local are not treated as local. Examples: `localhost.evil.com`, `127.0.0.1.nip.io`, `localhost.` and LAN addresses.
- The rule covers every place a base URL reaches these providers. That includes the Rust `OpenAiConfig`/`AnthropicConfig` constructors and the worker's `openai` provider configuration, which the TypeScript and Python SDKs send over stdio.
- Existing rules stay: no embedded credentials, query or fragment (`provider.rs:442-448`), and redirects are refused (`http_client()`, `provider.rs:430`).
- There is no opt-out flag for insecure HTTP.

### Out of scope

- `ResponsesProvider::for_endpoint` (`provider/responses.rs:80-99`). It is already stricter: HTTP loopback IPs only, for test fixtures. It is left unchanged.
- The registry and auth origin policies (`ejm-harness-packages/src/registry.rs:335`, `ejm-harness-auth/src/engine.rs:139`).
- Certificate pinning, custom CA handling, and proxy configuration.
- ejm-ide, which already enforces this rule.
- CLI-side flag and env handling, which is covered by the same-named change in ejm-cli.

## Capabilities

### New Capabilities

- `provider-endpoint-security`: transport rules a model provider base URL must meet before the harness will send credentials to it.

### Modified Capabilities

None. `openspec/specs/` in ejm-harness has no main specs (only `.gitkeep`). `provider-selection` exists only in the unarchived `publish-host-auth-apis` change and covers provider readiness and binding, not URL transport, so it is not modified.

## Impact

- `crates/ejm-harness/src/provider.rs` (`endpoint()` and its unit tests).
- Worker provider configuration errors surface to SDK callers. The SDK protocol types do not change.
- Existing wiremock tests use `http://127.0.0.1:<port>` and stay valid.
- Downstream: ejm-cli builds against the sibling harness path (`ejm-cli/Cargo.toml:18-23`), so it inherits the enforcement. ejm-cli adds early validation in its own change.
- Docs: note the rule wherever a custom base URL is documented.
- Users running a remote OpenAI-compatible server over plain HTTP (for example Ollama on a LAN host) must switch to HTTPS or to a loopback tunnel.

## Open Questions

1. **Loopback set.** Recommendation: accept `localhost`, all of `127.0.0.0/8`, and `::1`. This matches ejm-ide's `validate_endpoint`, which uses `Ipv4Addr::is_loopback()`. The audit text lists only `127.0.0.1`, `::1` and `localhost`, and the harness registry check (`registry.rs:335`) uses exactly those three. The alternative is the narrower three-host set, which would make ejm-ide and the harness disagree.
2. **IPv4-mapped IPv6** (`::ffff:127.0.0.1`) and `0.0.0.0`. The draft rejects both. Rust's `Ipv6Addr::is_loopback` is false for mapped addresses, which keeps parity with ejm-ide.
3. **Insecure opt-out** (for example `--allow-insecure-http` or an env var). Recommendation: **no**. An opt-out that can be set from the environment brings back the exact hostile-env risk L2 describes. A loopback tunnel covers the legitimate LAN case.
4. **`localhost` resolution.** The draft trusts the name `localhost` without resolving it, matching ejm-ide. A hostile `/etc/hosts` is outside this threat model.
