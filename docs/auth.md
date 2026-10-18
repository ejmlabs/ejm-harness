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

The supplied `FileStore` supports Unix. It creates a directory with mode 0700,
uses 0600 files owned by the effective user, rejects a symlink at the store and
symlink or hardlink state/lock entries, and checks directory and lock inode
identity. Each transaction opens its own lock description. Lock acquisition is
cancellable and bounded to thirty seconds. Writes use a temporary private file,
file synchronization, atomic rename, and directory synchronization. There is a
1 MiB state limit and a 64-registration limit. Corrupt or unknown state is
rejected. A missing state file yields empty state.

The private file is `state.json`, beside a stable `.lock` file. Its version-one
shape is `{"version":1,"host_id":"...","registrations":[...]}`. Each
registration stores its identity fields, nullable credentials,
`refresh_pending`, and `authorization_revision`. Older version-one records default
the revision to zero. Credentials contain nullable access and refresh tokens, a
retained ID token, scopes, and optional expiry/earliest-refresh timestamps. Only
private store adapters serialize these values. Never copy this file into a
repository or support report.

`FileStore` is exported on other operating systems, but its constructor and
lock operation fail with an unsupported-platform error. A Windows host must
inject a protected store with equivalent cross-process locking and atomic
replacement. There is no bundled OS keychain backend. File permissions protect
against other users; they do not isolate untrusted code running as the same OS
user. Use trusted local storage with reliable lock and rename semantics.

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
