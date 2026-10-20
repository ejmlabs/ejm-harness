# Design

## Context

Security finding L2 (baseline audit, 2026-10-19) is in provider endpoint
construction. `endpoint()` in `crates/ejm-harness/src/provider.rs` parses a base
URL and accepts any `http` or `https` URL that has a host and no userinfo,
query, or fragment. `OpenAiProvider::new` calls it with suffix
`/chat/completions`. `AnthropicProvider::new` calls it with suffix `/messages`.
Construction does not open a connection. The first request then attaches the
API key: `bearer_auth` for OpenAI-compatible Chat Completions, and `x-api-key`
for Anthropic Messages. The HTTP client already refuses redirects.

`ejm-ide` already enforces the same class of rule in `validate_endpoint`
(`src-tauri/src/providers/profiles.rs` calls it; the function lives in
`src-tauri/src/providers/driver.rs`). That function accepts `https`, or `http`
when the parsed host is loopback: a domain equal to `localhost` ignoring ASCII
case, an IPv4 address in `127.0.0.0/8`, or an IPv6 address for which
`is_loopback()` is true (`::1`). Its rejection text is
`Use an HTTPS provider endpoint or an HTTP endpoint on localhost.`

`ejm-cli` does not parse the endpoint. `--base-url` and `EJM_CLI_BASE_URL` are
optional. `Args::api_base_url()` returns that value, or
`https://api.openai.com/v1` / `https://api.anthropic.com/v1` when it is absent.
For `--provider openai` and `--provider anthropic`, `main.rs` places the string
and the API key into `OpenAiConfig` or `AnthropicConfig` and calls `new`. A
`?` sends the `anyhow` error to `main`, which prints
`ejm-cli: {error:#}` through `render::safe_text`. ChatGPT and native Responses
call `responses_credentials`, which rejects `--base-url` /
`EJM_CLI_BASE_URL`, and then use `ResponsesProvider::new` on the fixed official
HTTPS endpoint.

The worker `openai` variant passes `base_url` through to `OpenAiProvider::new`.
The worker `anthropic` variant hardcodes `https://api.anthropic.com/v1`. The
worker `responses` variant uses `ResponsesProvider::new`, which is pinned to
`https://api.openai.com/v1`. `configure` maps a constructor error to protocol
code `configuration` via `error.to_string()`. TypeScript and Python turn that
into `HarnessError`. Both SDKs require worker protocol `1` and do not branch on
this message. `base_url` stays a string in the generated schema.

`ResponsesProvider::for_endpoint` is a fixture constructor. It allows `http`
only when the host parses as an IP address and `is_loopback()` is true. It
rejects hostname `localhost`, `https`, userinfo, query, and fragment. Registry
origins already require `https`, with an `http` exception only for the host
strings `localhost`, `127.0.0.1`, and `[::1]`.

`proposal.md` and `specs/provider-endpoint-security/spec.md` are the product
requirements for this change. Implementation still waits for Everett's
approval. This change does not edit Rust, the worker, the SDKs, or the
registry.

## Goals / Non-Goals

**Goals:**

- Refuse a provider base URL that would send an API key over cleartext to a
  non-loopback host.
- Apply that rule once, inside `endpoint()`, before either provider builds a
  request.
- Match ejm-ide's parsed-host loopback rule, including `127.0.0.0/8`, `::1`,
  and exact `localhost`.
- Keep loopback `http` working for local fixtures and local model servers.
- Return a stable, secret-free error that the CLI, worker, and SDKs already
  know how to surface.
- Leave the worker schema, protocol number, and public Rust signatures
  unchanged.

**Non-Goals:**

- An insecure override, environment flag, or config field that re-enables
  cleartext to a non-loopback host. The finding's hostile environment variable
  is the same channel such a flag would use.
- Rewriting `http` to `https`, following redirects, or probing the host.
- DNS resolution. `localhost` is trusted as a name. A hostile resolver is
  outside this check.
- Changing `ResponsesProvider::for_endpoint`, `ResponsesProvider::new`, the
  Claude Code adapter, OAuth callback URLs, package download origins, or
  registry `parse_origin`.
- A new length cap. ejm-ide rejects endpoints longer than 2048 bytes. This
  change does not add that cap. The worker frame limit remains the bound.
- Editing `ejm-cli` or `ejm-ide`, bumping `VERSION`, or regenerating SDK types.
- Publishing a release.

## Decisions

1. **Validate in private `endpoint()` for every sender, and let the CLI pre-check.**
   `OpenAiProvider::new` and `AnthropicProvider::new` are the public
   constructors both the CLI and the worker call. A check there covers direct
   Rust hosts and the worker with one implementation. The worker keeps
   mapping `anyhow` to `configuration`. SDK facades keep forwarding that
   error. The CLI may reject earlier, as pull request 11 does, and still
   reaches this check when it calls `new`.

    `ejm-cli` pull request
    <https://github.com/ejmlabs/ejm-cli/pull/11> adds an earlier check in
    `api_base_url()`, before the API key is read and before the constructors.
    That early check is part of the coordinated plan. It is not a substitute
    for `endpoint()`. Its sentence is `provider base URL must use https, or
http on a loopback host (localhost, 127.0.0.0/8, or ::1). Change --base-url
or EJM_CLI_BASE_URL.` The spec requires the error to state that HTTPS is
    required except for loopback. Both sentences do that. A CLI user sees the
    CLI sentence because `api_base_url()` returns first. Direct Rust callers
    and the worker still get the harness sentence below, mapped to protocol
    code `configuration`. Pull request 11 does not move
    `.github/harness-revision`. That pin stays
    `4a751c6a3f24d7d143140f79ff53517fb93e3531` until a later adoption after
    this behavior is on protected `main`.

    Alternative considered: validate only in the CLI and leave `endpoint()`
    unchanged. Rejected. The worker and any other Rust host would still send
    a key over cleartext. A second parser can also drift, so the harness check
    stays mandatory.

    Alternative considered: validate only in the worker. Rejected because the
    CLI links the Rust crate and never sends `base_url` through the worker.

2. **Keep `anyhow::Result` and one stable message.** Do not add a public error
   enum. Parse failure stays `invalid provider base URL`, using
   `context` so the raw input is not interpolated. `url::ParseError` does not
   contain the input. Userinfo, query, and fragment keep the existing message
   `provider URL must not contain credentials, query, or fragment`, including
   on loopback. Every other scheme or host failure uses:

    `provider URL must use HTTPS, or HTTP on a loopback host`

    Check credentials before the scheme so `http://key:secret@host` still hits
    the credential message. Neither message includes the URL or the API key.
    Replace `provider URL must use HTTP or HTTPS`. That sentence would describe
    cleartext to a remote host as valid.

    Alternative considered: copy ejm-ide's sentence verbatim. Rejected so this
    crate keeps its existing short error style. The IDE keeps its own string.
    Hosts can match on the harness sentence above.

    Alternative considered: a typed error variant for the worker. Rejected
    because protocol code `configuration` already identifies constructor
    failure, and a new code would be a schema change for no extra safety.

3. **Loopback matches ejm-ide's parsed host, which is wider than the three
   names in the audit summary.** After `reqwest::Url::parse` (the `url` crate
   already locked at 2.5.8):

    - `https` is allowed for any host that passes the existing checks.
    - `http` is allowed only when `Url::host()` is a domain equal to
      `localhost` with `eq_ignore_ascii_case`, an `Ipv4Addr` for which
      `is_loopback()` is true (`127.0.0.0/8`), or an `Ipv6Addr` for which
      `is_loopback()` is true (`::1`).
    - Any other scheme, or `http` with any other host, fails.

    Name `url::Host` for that match. Add a direct `url = "2.5.8"` dependency
    only if naming `Host` requires it. That crate is already in the lock
    through `reqwest` 0.13.5. Do not take a second `url` version. Do not
    hand-roll bracket stripping or IPv4 parsing.

    `127.0.0.0/8` is intentional. `127.0.0.1` alone would reject a local
    listener on `127.0.0.2` and would disagree with ejm-ide. Exact `localhost`
    is intentional. `foo.localhost` and `localhost.example` are domains, so
    they fail.

    Alternative considered: string-match only `127.0.0.1`, `::1`, and
    `localhost`, as the audit's parenthetical list and as registry
    `parse_origin` does. Rejected for provider URLs. The parsed `Host` is the
    address reqwest will use, and it closes numeric forms the URL parser
    normalizes into `127.0.0.0/8`. Leave the registry predicate unchanged.

4. **Do not loosen `ResponsesProvider::for_endpoint`.** It remains an HTTP
   IP-loopback fixture gate. Routing it through `endpoint()` would accept
   remote `https` and hostname `localhost` on a constructor that exists to
   keep credentials off arbitrary hosts. Production Responses stays on
   `https://api.openai.com/v1`.

5. **Fail closed with no recovery path.** A previously accepted
   `http://` non-loopback base URL fails in `new`, before `bearer_auth` or
   `x-api-key` is attached to a request. Callers switch the endpoint to
   `https` or bind the server to a loopback host. The client continues to use
   `redirect::Policy::none()`, so an accepted `https` URL cannot redirect the
   key onto `http`.

6. **Documentation records a public behavior change.** Update the rustdoc on
   both constructors, the provider bullets in `docs/implementation-contract.md`
   and `docs/sdk.md`, the compatible-endpoint sentence in
   `docs/ide-integration.md`, and the existing `## [0.1.0] - 2026-10-19`
   section in `CHANGELOG.md`. Leave `## [Unreleased]` empty for this behavior.
   Add a dated decision-record row in `docs/contributing.md`. Do not open a
   `0.2.0` section and do not change `VERSION`, workspace package versions, or
   the worker handshake version string (it reads `CARGO_PKG_VERSION`). Keep
   the re-point sentence already in the `0.1.0` section.

## Edge cases

Decide from the parsed URL, then run the existing suffix join unchanged.
`HTTP` and `HTTPS` are already lowercase by the time the scheme is checked, so
`HTTPS://api.example.test/v1` and `HTTP://127.0.0.1/v1` follow the same rules
as their lowercase forms. `Http://LocalHost/v1` is loopback.

| Input                                                                                                                                                                      | Result                                                                                                                                                  |
| -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `https://api.deepseek.com` and `https://api.deepseek.com/chat/completions`                                                                                                 | Accepted. Suffix join stays as it is today.                                                                                                             |
| `https://api.example.test:8443/v1`                                                                                                                                         | Accepted. A non-443 port does not matter.                                                                                                               |
| `https://127.0.0.1/v1` and `https://localhost/v1`                                                                                                                          | Accepted.                                                                                                                                               |
| `http://127.0.0.1:11434/v1`, `http://127.0.0.2/v1`, `http://localhost/v1`, `http://LOCALHOST:9/v1`                                                                         | Accepted.                                                                                                                                               |
| `http://[::1]:1234/v1`                                                                                                                                                     | Accepted. The parser requires brackets. `http://::1/v1` is an invalid URL.                                                                              |
| `http://api.example.test/v1`, `http://api.example.test:443/v1`                                                                                                             | Rejected. Port 443 does not make `http` safe.                                                                                                           |
| `http://192.168.1.1/v1`, `http://10.1.2.3/v1`, `http://172.16.5.5/v1`, `http://0.0.0.0/v1`, `http://[::ffff:127.0.0.1]/v1`                                                 | Rejected. Private, LAN, unspecified, and IPv4-mapped addresses are not loopback. `Ipv6Addr::is_loopback()` is false for the mapped address.             |
| `http://foo.localhost/v1`, `http://localhost.example/v1`, `http://localhost.evil.com/v1`, `http://localhost./v1`, `http://evil-localhost/v1`, `http://127.0.0.1.nip.io/v1` | Rejected. The domain must be exactly `localhost`.                                                                                                       |
| `http://user@127.0.0.1/v1`, `https://key:secret@api.example.com/v1`, query, fragment                                                                                       | Rejected by the existing credential rule, including when the scheme is `https`. The secret must not appear in the error. This is not the HTTPS message. |
| `ws://api.example.test/v1`, `wss://api.example.test/v1`, `ftp://api.example.test/v1`, `file:///tmp/a`, a schemeless string, a relative string, an empty host               | Rejected by the new scheme message, or by `invalid provider base URL` when parse fails.                                                                 |
| Exotic IPv4 text the parser rewrites into `127.0.0.0/8`                                                                                                                    | Accepted, because the parsed host is loopback. Text the parser leaves as a domain is rejected.                                                          |

A trailing dot that survives parsing as `localhost.` is a different domain and
is rejected. Do not special-case it.

## Compatibility and migration

This rejects input that `OpenAiProvider::new` and `AnthropicProvider::new`
accept today. Signatures, field names, and the worker schema stay the same.
Protocol stays at 1. Echo, replay, worker Anthropic, worker Responses, and
`ResponsesProvider::new` are unchanged. Existing `provider_http` fixtures build
their base URL from wiremock's `MockServer::uri()`, an HTTP address on
`127.0.0.1`, so they remain valid.

Anyone currently sending a provider key to a non-loopback host over `http`
must change that base URL to `https` or move the server to loopback
(`127.0.0.0/8`, `::1`, or `localhost`) before the next run. There is no
compatibility flag. Local Ollama-style defaults on `127.0.0.1` or `localhost`
keep working. `https://api.openai.com/v1` and `https://api.anthropic.com/v1`
keep working. Cleartext to a LAN address, a Docker host name, or a custom DNS
name stops at construction.

**ejm-cli.** The CLI repository depends on this crate by path
(`ejm-cli/Cargo.toml` lines 18-23) and versions itself separately. Nothing in
either repository is published yet. Pull request 11 validates `--base-url` and
`EJM_CLI_BASE_URL` in `api_base_url()` before the key is read, and doctor
fails before it prints the URL. The CLI sentence differs from the harness
sentence, and both meet the spec. After the CLI adopts a harness revision
that contains `endpoint()`, a direct constructor call still fails closed if
the CLI check is skipped. `main` already prints the `anyhow` chain. The CLI
does not need a new error type. Its change owns help text and fixtures. This
repository does not edit `ejm-cli`. The CLI pin `.github/harness-revision`
stays `4a751c6a3f24d7d143140f79ff53517fb93e3531` until a later adoption.
`ejm-ide` uses the same pin path and currently records
`09b779ee0452dda07b7f52503be22e65fe238373`. Moving either pin waits until the
published `0.1.0` commit exists.

**Versioning.** `VERSION` stays `0.1.0`. Everett deletes and recreates the
unpublished `v0.1.0` tag at the protected `main` commit after this pull
request and the registry pull request merge. Record the HTTPS break only in
the dated `## [0.1.0] - 2026-10-19` section. Do not leave a copy under
`## [Unreleased]`, and do not open `0.2.0`. Keep the sentence that `v0.1.0`
was re-pointed from `8d3dc39` to the protected main commit recorded in
provenance.json before publication. The published commit SHA cannot appear
inside the commit that is being tagged. SDK package versions stay on the
harness `VERSION`. The CLI's own `VERSION` moves only in the CLI repository,
after it adopts the harness revision. No worker protocol bump is required,
because `hello.protocol` stays `1` and `base_url` stays a string. Recreating
the tag follows `RELEASING.md` and is not part of this change.

**ejm-ide.** It already rejects the same non-loopback `http` URLs before use.
No IDE change is required for this finding. Public Rust signatures do not
change, so a later `cargo check --locked --tests` in the sibling trees is a
compatibility check, not a migration. Do not edit those trees from this
repository.

**SDKs and registry.** TypeScript and Python need no code change. An `openai`
config with a cleartext non-loopback `base_url` fails at `configure` with code
`configuration` and the stable message, and the API key stays out of that
string. Registry login already has its own origin rule and a different
credential. Unifying that rule with `127.0.0.0/8` is a separate registry
change.

## Risks / Trade-offs

- Local servers on a non-loopback name, including `foo.localhost` and
  `host.docker.internal`, lose cleartext access. Operators use `https` or a
  loopback bind. That is the accepted break.
- `localhost` is not resolved. The check matches ejm-ide and stops typos and
  hostile environment values such as `http://api.example`. It does not stop a
  resolver that points `localhost` at a remote address.
- `127.0.0.0/8` is larger than the single address named in the audit summary.
  It matches `Ipv4Addr::is_loopback()` and ejm-ide. A process bound only to
  `127.0.0.2` remains a local process.
- The CLI still holds the API key in memory when `new` returns the error. The
  key is not placed on a request and must not be copied into the error.
- A direct `url` dependency, if required to name `Host`, must stay on the
  locked 2.5.8 line. A second `url` crate would fail the duplicate-version
  policy.
- `VERSION` stays `0.1.0`. The notice is only in the dated `0.1.0` section.
  Everett recreates that unpublished tag after this change and the registry
  move are on protected `main`.

## Test strategy

Tests stay offline. No paid model, no live provider, and no new HTTP server.

In `crates/ejm-harness/src/provider.rs`, extend the existing `endpoint()` unit
test. Cover every row above, including the spec's exact loopback URLs
(`http://localhost:11434/v1`, `http://LOCALHOST:8080`,
`http://127.0.0.1:1234/v1`, `http://127.0.0.2:1234`, `http://[::1]:1234/v1`),
the spec's lookalikes, `ws`, `wss`, `ftp`, `file`, and a schemeless string.
`https://api.deepseek.com/v1` stays accepted, including the existing suffix
assertion. `https://key:secret@api.example.com/v1`, plus a query and a
fragment, still fail with `provider URL must not contain credentials, query,
or fragment`. A planted secret must be absent from every error string.

Call `OpenAiProvider::new` and `AnthropicProvider::new` with
`http://api.example.com/v1` and a planted non-empty key, with no server
listening. Both constructors fail, and neither error contains the key. That
is the spec's "no connection" evidence: construction returns before
`bearer_auth` or `x-api-key` is attached to a request.

In `crates/ejm-harness-worker/src/server.rs`, extend provider configuration
tests. `configure_provider` with `kind: openai`, `base_url`
`http://api.example.com/v1`, and a planted API key returns an error whose
`Display` contains the stable message and does not contain the key, and the
worker keeps no provider from that configuration. The same helper accepts
`http://127.0.0.1:8080/v1` without connecting. Worker Anthropic and Responses
cases stay on their fixed HTTPS endpoints.

Redirects use the shared `http_client()`, which sets `redirect::Policy::none()`
for every scheme. Wiremock's `MockServer::uri()` is `http://127.0.0.1:<port>`,
so an offline fixture cannot terminate TLS. Add a loopback fixture that
returns `302` to an `http://` URL, assert exactly one request reached the
mock, and assert the call fails without a second request. That shows the
credential is not re-sent. The policy is the same client an accepted HTTPS
endpoint uses.

Run `cargo test -p ejm-harness --test provider_http` to show existing loopback
wiremock fixtures still construct OpenAI, Anthropic, and Responses providers.
Run `python3 scripts/generate-protocol.py --check` to show the schema did not
change. Then run `./scripts/check` with `CARGO_BUILD_JOBS=2`. SDK suites need
no new assertion unless a fixture was sending cleartext to a non-loopback host.
Registry tests stay untouched.

## Settled decisions

1. Loopback is exact `localhost` (ASCII case-insensitive), every IPv4 address
   in `127.0.0.0/8`, and `::1`. The spec and proposal settle this. It matches
   ejm-ide `Ipv4Addr::is_loopback()`. The registry predicate stays narrower.
2. `::ffff:127.0.0.1` and `0.0.0.0` are rejected. The spec lists both as
   lookalikes. `Ipv6Addr::is_loopback()` is false for the mapped address.
3. There is no insecure opt-out. The spec says no configuration value,
   environment variable, or flag may disable the rule.
4. `localhost` is trusted as a name and is not resolved. The proposal settles
   this. A hostile resolver is outside the check.
5. The spec requires the error to state that HTTPS is required except for
   loopback. It does not require one shared sentence. The harness keeps
   `provider URL must use HTTPS, or HTTP on a loopback host`. ejm-ide keeps
   its own sentence. ejm-cli pull request 11 keeps its longer sentence.
6. `VERSION` stays `0.1.0`. The HTTPS break is recorded only in the dated
   `0.1.0` section. Everett deletes and recreates the unpublished `v0.1.0`
   tag at the protected `main` commit after this change and the registry move
   land. There is no `0.2.0` section and no Unreleased copy.
