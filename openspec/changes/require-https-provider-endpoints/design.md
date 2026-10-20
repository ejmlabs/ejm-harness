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

`proposal.md` and delta specs are intentionally absent. The product manager
owns those files and may add them on this branch. Implementation waits for
those artifacts and for Everett's approval.

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

1. **Validate in private `endpoint()`, not in the worker, SDKs, or CLI.**
   `OpenAiProvider::new` and `AnthropicProvider::new` are the public
   constructors both the CLI and the worker call. A check there covers direct
   Rust hosts and the worker with one implementation. The CLI keeps passing
   the string through. The worker keeps mapping `anyhow` to
   `configuration`. SDK facades keep forwarding that error.

   Alternative considered: duplicate the check in `ejm-cli` before reading the
   API key. Rejected as the enforcement point. The key is read into the config
   value before `new` returns, and a failed constructor must still send no
   request. A second parser in the CLI can drift. The CLI draft may update
   help text and its own changelog. It should keep treating a harness error as
   authoritative.

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
   `docs/ide-integration.md`, and `## [Unreleased]` in `CHANGELOG.md`. Add a
   dated decision-record row in `docs/contributing.md`. Do not edit the dated
   `0.1.0` section. Do not change `VERSION`, workspace package versions, or
   the worker handshake version string (it reads `CARGO_PKG_VERSION`).

## Edge cases

Decide from the parsed URL, then run the existing suffix join unchanged.
`HTTP` and `HTTPS` are already lowercase by the time the scheme is checked, so
`HTTPS://api.example.test/v1` and `HTTP://127.0.0.1/v1` follow the same rules
as their lowercase forms. `Http://LocalHost/v1` is loopback.

| Input | Result |
| --- | --- |
| `https://api.deepseek.com` and `https://api.deepseek.com/chat/completions` | Accepted. Suffix join stays as it is today. |
| `https://api.example.test:8443/v1` | Accepted. A non-443 port does not matter. |
| `https://127.0.0.1/v1` and `https://localhost/v1` | Accepted. |
| `http://127.0.0.1:11434/v1`, `http://127.0.0.2/v1`, `http://localhost/v1`, `http://LOCALHOST:9/v1` | Accepted. |
| `http://[::1]:1234/v1` | Accepted. The parser requires brackets. `http://::1/v1` is an invalid URL. |
| `http://api.example.test/v1`, `http://api.example.test:443/v1` | Rejected. Port 443 does not make `http` safe. |
| `http://192.168.1.1/v1`, `http://10.0.0.1/v1`, `http://0.0.0.0/v1`, `http://[::ffff:127.0.0.1]/v1` | Rejected. IPv4-mapped IPv6 is not `Ipv6Addr::is_loopback()`. |
| `http://foo.localhost/v1`, `http://localhost.example/v1` | Rejected. The domain must be exactly `localhost`. |
| `http://user@127.0.0.1/v1`, `https://key:secret@host/v1`, query, fragment | Rejected by the existing credential rule. The secret must not appear in the error. |
| `file:///tmp/a`, a relative string, an empty host | Rejected by the new scheme message, or by `invalid provider base URL` when parse fails. |
| Exotic IPv4 text the parser rewrites into `127.0.0.0/8` | Accepted, because the parsed host is loopback. Text the parser leaves as a domain is rejected. |

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

**ejm-cli.** The CLI repository depends on this crate by path and versions
itself separately. Nothing in either repository is published yet. After this
behavior is on the harness revision the CLI builds, `--base-url` and
`EJM_CLI_BASE_URL` for OpenAI-compatible and Anthropic API-key access fail
inside `new` with the stable message above. `main` already prints the `anyhow`
chain. The CLI does not need a new error type. Its draft should update the
`--base-url` help text and any fixture that expects cleartext to a non-loopback
host to succeed. It should not reimplement `Url::host()`. Doctor prints the
configured base URL and does not attach the API key. That print stays out of
this change.

**Versioning.** `RELEASING.md` says that before 1.0 a minor release may break
behavior and a patch may not. Record the break under `## [Unreleased]`. Leave
the dated `0.1.0` notes and `VERSION` untouched in the implementation. The
release that first ships this behavior should be a minor (`0.2.0`), not a
`0.1.x` patch. SDK package versions track the harness version and change in
that same release, not in the implementation patch. The CLI's own `VERSION`
moves only in the CLI repository, after it adopts the harness revision. No
worker protocol bump is required, because `hello.protocol` stays `1` and
`base_url` stays a string.

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
- Implementation on current main changes source behavior while `VERSION` still
  reads `0.1.0`. The Unreleased changelog is the notice. Shipping it inside the
  already dated `0.1.0` section would hide a break in notes that were prepared
  as the first source release.

## Test strategy

Tests stay offline. No paid model, no live provider, and no new HTTP server.

In `crates/ejm-harness/src/provider.rs`, extend the existing `endpoint()` unit
test. Cover the accepted and rejected rows above, the uppercase scheme, an
explicit `https` port, userinfo on loopback, and a planted secret that must be
absent from every error string. Preserve the DeepSeek suffix assertion and the
embedded-credential assertion.

In `crates/ejm-harness-worker/src/server.rs`, extend provider configuration
tests. `configure_provider` with `kind: openai`, a cleartext non-loopback
`base_url`, and a planted API key returns an error whose `Display` contains the
stable message and does not contain the key. The same helper accepts
`http://127.0.0.1:9/v1` and `http://[::1]:9/v1` without connecting. Worker
Anthropic and Responses cases stay on their fixed HTTPS endpoints.

Run `cargo test -p ejm-harness --test provider_http` to show existing loopback
wiremock fixtures still construct OpenAI, Anthropic, and Responses providers.
Run `python3 scripts/generate-protocol.py --check` to show the schema did not
change. Then run `./scripts/check` with `CARGO_BUILD_JOBS=2`. SDK suites need
no new assertion unless a fixture was sending cleartext to a non-loopback host.
Registry tests stay untouched.

## Open questions

1. Confirm `127.0.0.0/8`, chosen so the predicate matches ejm-ide
   `Ipv4Addr::is_loopback()`, rather than only `127.0.0.1`.
2. Confirm exact `localhost` only. `foo.localhost` stays rejected.
3. Confirm the terse harness message rather than ejm-ide's longer sentence.
4. Confirm the break stays under `## [Unreleased]` and ships later as minor
   `0.2.0`, leaving the prepared `0.1.0` notes unchanged.
