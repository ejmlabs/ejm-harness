# Host integration

ejm-harness owns agent execution and provider contracts. A host owns account
controls, callback listeners, browser launch, protected credential storage,
diagnostic retention, and its UI. ejm-cli demonstrates the same public APIs.

## Authentication and provider selection

Use `ejm_harness_auth::AuthEngine` with an injected `CredentialStore`. The supplied
`FileStore` is supported on Unix. An OS keychain adapter must retain the same
exclusive cross-process transaction through refresh and atomic replacement.
See [the auth contract](auth.md) for the callback flow, typed `RetryRequired`,
rotation, logout, and safe status types.

For subscriptions, pass `auth.source(registration_id)` to
`ResponsesProvider::new`. For an API key, pass
`Arc::new(StaticCredential::new(SecretString::new(api_key)))`. Both use the
public `https://api.openai.com/v1/responses` endpoint, with `store: false` and
`stream: true`. Subscription credentials never go to a private backend endpoint.

Use `ResponsesConfig { model, reasoning_effort }` to retain the requested model
and effort. `stream_with_config(&config, request, cancel, deltas)` overrides
these options for one call while retaining the pooled HTTP client. Validation
checks protocol values, not whether a particular model supports every value.
Surface a provider rejection rather than silently changing either choice.

`ResponsesProvider` is `Clone`. `provider.clone().with_observer(observer)` can
attach per-call capture while sharing the client and credential source.

Use native Responses for OpenAI profiles. Other vendors using a compatible
endpoint can use `OpenAiProvider`, which implements Chat Completions.
`AnthropicProvider` uses native API keys. Anthropic subscription sign-in belongs
to unmodified external Claude Code and is not a credential source for this
provider. Do not present external execution as governed by this harness.

## Models and usage

`ResponsesProvider::available_models(credentials, cancel)` returns
`Vec<AvailableModel>` without requiring a model first. `provider.models(cancel)`
uses an existing instance. Each model has `slug` and `display_name`.

The catalog accepts ChatGPT `models[]` and public API `data[]`. It preserves
server order, filters subscription entries to `visibility: "list"`, and uses
public API model IDs as display names. Model order does not establish task
suitability. Fetching is bounded to 1 MiB and 1,024 entries.

`Usage` preserves input/output totals plus optional `cached_input_tokens` and
`reasoning_output_tokens`. These details are subsets of the totals, not extra
tokens. Missing detail is `None`, not zero. Run aggregation sums the detail
reported by its requests, which can be partial if some responses omit detail.
Native Responses rejects a cached-input or reasoning-output detail greater
than its corresponding total.

## Remaining provider parity work

The native Responses contract covers per-call model and effort, encrypted
reasoning continuation, detailed usage, and optional HTTP capture. Hosts that
need those features across other provider transports should retain their
adapters until matching shared contracts and fixtures exist. In particular,
DeepSeek `reasoning_content` replay, Anthropic signed-thinking replay, and
equivalent per-call options and capture remain integration work.

## Stateless reasoning and persistence

`ModelResponse` and `Message` both implement Serde and carry optional
`ProviderContinuation`. `Message::assistant(&response)` copies the continuation
alongside text and tool calls. Retain it through tool turns and replay.
The engine journal already does this.

Use `Message::tool(call_id, &output)` for tool results. It preserves `is_error`
and `artifact` as well as text. Both fields have Serde defaults for older
journals. A Rust host using struct literals must supply them. Built-in providers
render the metadata, and Anthropic receives native `tool_result.is_error`.
An artifact path identifies a result; it does not grant access to it.

The default context policy masks older successful observations under byte
pressure before dropping complete older exchanges. Its latest intent and
exchange remain mandatory. The journal retains the original observations.
See [harness engineering decisions](harness-engineering.md) for the bounds and
local regression evidence. `RunStatus::Completed` means the run ended cleanly;
host acceptance checks determine whether the user's task is satisfied.

Hosts that own conversation persistence can serialize the public message type:

```rust
let assistant = ejm_harness::Message::assistant(&response);
let stored = serde_json::to_vec(&assistant)?;
let restored: ejm_harness::Message = serde_json::from_slice(&stored)?;
```

`ProviderContinuation::OpenAiResponses { reasoning }` carries bounded opaque
encrypted reasoning items. Pass them back to the same provider/account/model
conversation. Clear them when switching that binding. Do not show encrypted
content as readable reasoning. Without the encrypted item, a stateless
reasoning tool turn can be rejected by the provider.

## Opt-in HTTP diagnostics

Implement `HttpObserver::observe(&HttpTraceRecord)`. The callback runs inline,
must return promptly, and must bound retained bytes and records. Use a bounded
`try_send` or fixed-size ring; do not block on storage or the UI. Capture is off
by default and makes no trace payload copies when off.

`HttpTraceRecord` implements Serde. It includes `request_id`, `phase`, `payload`,
`status`, `redactions`, `truncated`, `payload_bytes`, optional `method` and `url`,
and `headers: Vec<HttpTraceHeader { name, value }>`. A host can serialize a
record after applying its own additional secret filter:

```rust
let json = serde_json::to_vec(&host_filtered_record)?;
```

The phases are `request_body`, `response_head`, `sse_payload`, `http_error_body`,
and `finished`. The request phase describes the built client request and its
JSON body. Transport-added headers may not appear. Response-head records
contain status and the same allowlist of diagnostic headers. Credential,
cookie, and arbitrary custom headers are excluded. Headers are capped at 32
and 16 KiB combined.

SSE capture contains **data payloads after framing and UTF-8 decoding**, before model-event
interpretation. It is not raw TCP, HTTP framing, or a complete response-body
capture. `payload_bytes` counts the decoded payload before redaction. For SSE
it excludes framing; it must not be labeled network or transfer bytes. The
finished marker is only `completed`, `failed`, or `cancelled`.

The current bearer token is redacted in literal and JSON-escaped content
before the callback. Undecodable or truncated JSON bodies are omitted because
escaped credentials cannot be safely inspected. `truncated` marks that loss.
Non-2xx body capture is limited to 64 KiB and ten seconds and follows
cancellation. The host must also filter other accounts' credentials, tool
secrets, and sensitive prompt contents before display or export. Public account
status is safe to serialize; authentication attempts and stored secrets are not.

## Current bounds and evidence

Initial provider bounds are 64 KiB of text, 64 KiB of continuation, 16 tools,
64 canonical Responses output items, and 512 KiB per buffered SSE field. The
dedicated SSE library applies its cap independently to data, event name, and
last-event ID. Native streams also stop at 32 MiB of cumulative HTTP body bytes
or 100,000 emitted decoder events, including events the model adapter ignores.
The byte limit counts chunks delivered by Reqwest before SSE decoding, including
SSE comments and framing. It excludes HTTP framing. An overflowing chunk is
rejected whole. The event limit counts successful decoder messages and retry
events, rather than raw comment or blank-only frames. These are fixed bounds in this release. A host migrating from
larger limits must show the lower ceilings and surface budget errors.

HTTP, JWT, refresh, persistence, and provider tests use local fixtures. Browser
sign-in, paid inference, other platforms, and target-laptop acceptance remain
separate checks. See [benchmarks](benchmarks.md) and
[execution guarantees](executor.md) for the scope of local evidence.
