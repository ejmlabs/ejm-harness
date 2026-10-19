---
name: harness-auth
description: Change ejm-harness provider sign-in, identity validation, credential storage, token refresh, logout, or Responses authentication without putting account UI in the shared engine.
---

# Provider authentication

Read [the auth contract](../../../docs/auth.md) and the owning Rust code before
changing credential behavior. Fetch current official provider documentation when
the supported authentication flow or inference endpoint could have changed.

- Keep `ejm-harness-auth` independent of agent execution and terminal or browser UI.
  Hosts start the callback listener before opening the browser and pass its exact
  URI into the engine. Browser launch URLs must not contain identity tokens.
- Bind every attempt to fresh PKCE, state, nonce, the exact callback URI, and
  its selected registration. Validate identity before replacing credentials.
- An issued client ID and verified issuer/subject identify a registration.
  Email is a label. Never exchange or save `dynamic_agent_client` as an issued ID.
- Resolve tokens through `CredentialSource`. Keep secrets out of arguments,
  events, journal data, diagnostics, source control, and public status objects.
- Serialize renewal across every process sharing storage. Retain exclusive
  ownership through rotation and atomic persistence. A crashed uncertain refresh
  requires recovery rather than retrying an old rotating token.
- Logout clears local credentials even when remote revocation cannot be
  confirmed. Report that distinction and retain the account/client mapping.
- Responses uses the public endpoint with `store: false` and `stream: true`.
  Preserve bounded encrypted reasoning across tool turns. Success requires
  `response.completed`; streamed text alone does not establish success.
- Anthropic subscription credentials remain inside unmodified Claude Code.
  Native API keys and external Claude Code have separate execution contracts.

Use Wiremock for local HTTP fixtures and the established JWT library for signed
identity fixtures. Keep HTTP parsing, TLS, OAuth primitives, and JWT verification
in maintained libraries. Cover invalid state, nonce,
signature, audience, identity changes, rotation races, interrupted persistence,
and redacted errors for the contract being changed. Never run a paid inference
or real browser sign-in as an automated check. Run `./scripts/check` when ready.

## Shared vault and official client

Use the [credential vault](../../../docs/credential-vault.md) for provider state and scoped host secrets. The OS key service holds only its key. Keep the stable legacy lock, rejecting provider barrier and permanent host migration markers. Never reset missing keys or fall back to plaintext. Memory-only storage must be explicitly selected.

Use `ClaudeCodeClient` for official Claude account controls and require the subscription source before every inference. Use `resolve_claude_code_program` for explicit override, native install and maintained PATH discovery. Keep API-key routes separate. See the [official-client contract](../../../docs/claude-code.md).
