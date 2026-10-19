# Authentication

`ejm-harness-auth` supplies reusable ChatGPT sign-in and token renewal. It does not
depend on the harness, worker, CLI, browser, or terminal. The host owns its
account picker, active-account setting, loopback listener, browser launch, and
credential-store location.

## Host flow

1. Open a private `FileStore`, or inject a `CredentialStore` backed by the host's
   protected storage. Construct `AuthEngine` with the app's consistent name.
2. Bind an IPv4 listener on `127.0.0.1` with an unused nonzero port. Keep the
   listener open. Call `start` with that listener's exact
   `http://127.0.0.1:<port>/auth/callback` URI.
3. Open `LoginAttempt.authorization_url`, or display it on an explicit manual
   sign-in path. Pass the full received callback URI to `complete`. Each attempt
   is one use, expires after ten minutes, and can be cancelled by ID. At most
   eight attempts may be live.
4. Select the returned registration only after `complete` validates and saves
   it. Obtain `engine.source(registration_id)` and pass that `CredentialSource`
   to the provider. It resolves or renews a bearer token for each request.

If code exchange returns `invalid_grant`, `complete` returns the typed
`RetryRequired` error through `anyhow`. Downcast it and call
`retry(&error.attempt_id, new_redirect_uri, cancel)` while keeping the old attempt
alive. Bind or retain the callback listener before retrying. This consumes a
bounded in-memory hint and returns fresh state, nonce, and PKCE while retaining
the issued client ID. Hints expire with the original attempt and are removed by
`cancel`. A hint never persists an unverified subject. The host should bound
automatic retries and cancel any unused attempt.

For a saved account, pass its registration ID to `start`. New registrations use
`dynamic_agent_client`; code exchange uses the issued client ID from the
callback. Returning accounts retain their issued client ID and validated
subject. A host ID is generated once and retained across sign-out. Email is
display metadata, and is never used to match or merge registrations.

Authorization uses fresh state, OIDC nonce, and S256 PKCE. Completion checks the
callback URI, state, issued client, RS256 signature, issuer, audience, expiry,
nonce, and any returning subject. Discovery and signing keys come from the
OpenAI issuer. Production HTTP uses TLS, rejects redirects and foreign-origin
metadata, bounds bodies to 1 MiB, and bounds each request to twenty seconds.

The maintained `oauth2` crate generates the PKCE pair and independent 256-bit
state and nonce values. Its default HTTP features are disabled, so auth keeps
one bounded `reqwest` client. Generated secrets move directly into zeroed
`SecretString` storage. Provider-specific registration, exchange, and renewal
rules stay in this crate.

Authorization URLs omit `id_token_hint` and `login_hint`. The URL still contains
one-use state and nonce, so `LoginAttempt` redacts it in `Debug`. Hosts must not
log callbacks, codes, credential values, or form bodies.

## Public state and secrets

`registrations` and `status` read stored metadata without network calls. These
are the exact JSON-safe status fields:

```json
{
    "id": "registration-id",
    "issuer": "https://auth.openai.com",
    "subject": "validated-subject",
    "client_id": "issued-client-id",
    "email": null,
    "signed_in": true,
    "plan_enabled": true,
    "expires_at": 1900000000
}
```

`expires_at` is an optional Unix timestamp in seconds. Status is cached; it is
not proof that the remote grant remains valid. Inference requires both
`resource.invoke` and `chatgpt.tokens.use.direct`. A valid identity without that
grant can remain signed in while `plan_enabled` is false. The source then
rejects inference.

`LogoutResult` serializes as `{"remote_revoked":true}`. `AuthConfig` and
`LoginAttempt` do not implement serialization. `SecretString` has explicit
`expose_secret` access, redacted `Debug`, zeroed owned storage on drop, and no
serialization implementation. `StoredAuth`, `StoredRegistration`, and
`StoredCredentials` are available to injected stores but do not implement
serialization. Hosts must not add these sensitive structs to events, session
journals, analytics, SDK responses, or diagnostics.

## Storage and renewal

`CredentialStore::lock` returns an exclusive transaction held until drop. It
must serialize transactions across processes, read current state after locking,
and atomically save the full credential set. A renewal holds this transaction
through the exchange and durable replacement. The latest access token, rotating
refresh token, scopes, and expiry are saved together.

Before submitting a refresh, the engine durably sets `refresh_pending`. If a
process exits, the request outcome is uncertain, or replacement cannot be saved,
later callers reject that credential set and require sign-in. They do not retry
a possibly consumed refresh token. A successful save clears the marker. The
engine refreshes near expiry. It accepts an optional `earliest_refresh_at`
integer in Unix seconds and does not refresh before it. The provider reference
names this field without specifying its encoding. Other encodings are rejected,
and live provider acceptance remains unverified.

`FileStore` now adapts the shared `SecretVault`. It writes `vault.bin`, an
authenticated encrypted snapshot, beside the retained stable `.lock`. The
versioned format and transaction code are shared by macOS, Windows, and Linux.
The 32-byte encryption key lives in macOS Keychain, Windows Credential Manager,
or an unlocked Linux Secret Service. Key setup runs under the same lock as
credential renewal. No key or plaintext payload is written beside the vault.

On Unix, the directory must have mode 0700 and files mode 0600, owned by the
effective user. macOS entries with an extended allow ACL are refused even when
those mode bits are private. Symlink, hardlink, directory-replacement, and lock-replacement
checks remain. Writes synchronize a private encrypted temporary file, rename,
and synchronize the directory. The Windows adapter uses maintained protected
DACL creation, pinned directory/file handles, reparse rejection, file flush,
and write-through publication. Native Windows acceptance is still pending.
Use trusted local filesystems with reliable locking and atomic publication.

Async lock waiting, including native key access, is cancellable and bounded to
thirty seconds from the initial request. Native key-service access runs on a
Tokio blocking worker and may require a user unlock prompt. An already-started
OS call cannot be stopped: the caller returns, but its worker keeps the stable
lock until that call returns. It checks cancellation and the deadline before
later key or durable changes and before returning a transaction. A cancelled
first key write can still finish; its bootstrap marker permits recovery with
the same key. Do not remove that marker or initialize a competing key.
`SecretVault::lock_blocking` is synchronous and cannot interrupt OS key calls;
hosts must keep it off async and UI event threads. Runtime shutdown may also
wait for an in-flight blocking call.
An existing vault with a missing, replaced, malformed, or unavailable key fails
without reset or plaintext fallback. Unknown formats, authentication failures,
and oversized payloads fail before state becomes available. Preserve the vault
and restore its original OS key record; do not delete files to suppress errors.

First setup writes a nonsecret `.bootstrap` marker before key creation. It
removes that marker after reopening the initial encrypted vault and before
admitting credential operations. An interrupted first setup can retry with the
same key while that marker remains. A previously admitted vault with missing
ciphertext requires recovery. Keep bootstrap files intact when reporting an
error; do not create a marker to bypass a missing-data error.

Provider state retains the 1 MiB and 64-registration bounds. The common vault
also supports 128 scoped host secrets of at most 16 KiB, 128 permanent migration
markers, and 4 MiB total plaintext. Encrypted input is capped at 8 MiB before
reading. Product, purpose, and origin/account identity form an exact `SecretId`.
Provider credentials, relay identities, IDE keys, and registry sessions share
the implementation; different products can use different physical vaults.

Hosts can explicitly choose `FileStore::memory()` for provider credentials or
`SecretVault::memory()` for scoped secrets. These never import or persist state.
Persistent Linux use requires an unlocked Secret Service in the user's D-Bus
session, including on headless systems. Missing-service errors require setup
or explicit host-supported memory mode. The store never changes modes itself.

### Legacy migration and recovery

Opening a provider `FileStore` migrates a valid private version-one
`state.json` under the existing `.lock`. It preserves host/account mappings,
credentials, `refresh_pending`, and authorization revisions. Migration first
commits and reopens encrypted prepared state, then publishes a nonsecret
version-two `state.json` barrier that supported old readers reject, then
records completion. Keep the barrier and stable lock. Deleting either can
break the supported-client upgrade boundary.

Before the barrier is published, interrupted migration rereads the final
legacy source under the old lock. After publication, it completes from the
verified encrypted state. Completed migration rejects a restored plaintext
snapshot or missing ciphertext. It never restores old credentials after
logout or rotation. Downgrading to an old plaintext writer is unsupported.

Legacy IDE, relay, and registry writers cannot all honor this lock. Their
owning hosts require a controlled upgrade with older clients stopped before
import or reference publication. Permanent destination markers survive logout
and replacement, so failed legacy cleanup cannot cause stale reimport. Follow
the host's migration command and recovery guidance. Deleting legacy files does
not erase snapshots or guarantee erasure from storage hardware.

OS-user protection does not isolate arbitrary code running as that user and
does not prevent replay of an entire valid older vault by that user. This
version has no passphrase recovery, cloud sync, or portable backup format.
Moving `vault.bin` to another path changes its authenticated identity. See the
[vault decision and measurements](credential-vault.md) for exact key identity,
format, limits, and native verification gaps. Never copy credential state into
a repository or support report.

## Sign-out

`logout` clears local tokens and preserves account/client mapping first, then
tries remote revocation using the refresh token and discovered endpoint. An
empty HTTP 200 confirms success. Network failures or server errors receive
at most three attempts with backoff, within a total twenty-five-second remote
revocation deadline. The storage transaction is released before the remote
request. If revocation is unconfirmed, the return value has
`remote_revoked=false`; the host should report this and link to ChatGPT Settings
so the user can disconnect the app. Cancellation does not restore cleared
credentials. A saved authorization revision invalidates earlier returning-account
sign-ins and retry hints, including attempts held by another host process.

## References

- [OpenAI sign-in flow](https://developers.openai.com/siwc/token-sharing-open-source/sign-in)
- [Accounts and sessions](https://developers.openai.com/siwc/token-sharing-open-source/profiles-and-sessions)
- [Token reference](https://developers.openai.com/siwc/token-sharing-open-source/token-reference)
- [ChatGPT usage and app access](https://chatgpt.com/settings/usage)

Tests use local HTTP fixtures and signed test identities. No live OAuth or model
call is required to test this crate. Real browser acceptance remains a host
integration check.

## Claude subscription access through the official client

`ClaudeCodeClient` delegates `auth login --claudeai`, `auth status --json` and
`auth logout` to an installed, unmodified Claude Code executable. It never reads
Claude credential files or copies subscription tokens into `FileStore`.
`ClaudeCodeStatus` retains only signed-in state, the authentication method,
email and subscription type. Status is bounded to 16 KiB per output stream;
unknown fields and raw stderr are discarded. Client failures never include
captured output in errors.

`ClaudeCodeStatus::from_client_output(exit_code, stdout)` exposes the same bounded
decoder for host-supervised status commands. The caller must first reject
cancelled, timed-out and truncated results. The Claude provider uses this path
so its account check shares executor admission with inference and tools.

The host supplies an executable path and suspends its terminal reader before
calling `login`. The official client inherits stdio and owns its browser flow.
The auth supervisor enforces a ten-minute sign-in deadline and a fifteen-second
status/sign-out deadline. Cancellation and dropped callers trigger child
termination and reaping. Hosts must keep the Tokio runtime alive for cleanup.
Captured commands use a separate Unix process group; interactive sign-in keeps
the foreground group for terminal input. This is native execution, not a sandbox.
Successful captured Unix cleanup retires its numeric group identity after
termination and child reaping, before later output-drain waits. Cleanup errors
and abandoned waits retain ownership; dropping a completed owner cannot signal
a reused group identifier.

Subscription mode removes ambient API-key, bearer-token and cloud-routing
environment overrides listed by `CLAUDE_CODE_ENV_OVERRIDES`, and
`require_subscription` accepts only the official client's `claude.ai` method.
An inherited `CLAUDE_CONFIG_DIR` must be absolute so account controls and
inference cannot resolve it against different working directories.
An API-key source does not silently become subscription access. A successful
official sign-out remains observable when cancellation races completion;
uncertain cancellation performs a fresh bounded status read to confirm it.
This does not claim remote revocation beyond the official client's result.

The separate [Claude provider](claude-code.md) uses the shared executor and
keeps harness tools in the ordinary agent loop. API-key Anthropic and
OpenAI-compatible providers retain their HTTP contracts.

Current provider references:

- [Official-client authentication and credential use](https://code.claude.com/docs/en/legal-and-compliance#authentication-and-credential-use)
- [Subscription use, updated October 7, 2026](https://support.claude.com/en/articles/15036540-use-the-claude-agent-sdk-with-your-claude-plan)
- [Official CLI authentication commands](https://code.claude.com/docs/en/cli-reference)

Offline subprocess fixtures check safe metadata, signed-out and API-key states,
malformed/oversized output, cancellation, dropped callers, sign-out races and
the inherited-stdio deadline. They do not verify a real subscription sign-in.
