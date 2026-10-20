# Tasks

`proposal.md` and `specs/provider-endpoint-security/spec.md` are in this
change. Implementation still waits for Everett's approval. Until then, leave
every source file untouched. The decisions in `design.md` are the behavior to
implement. Every spec scenario below has a test task.

## 1. Reject cleartext non-loopback provider endpoints

- [ ] 1.1 In `endpoint()` (`crates/ejm-harness/src/provider.rs`), keep parse failure as `invalid provider base URL` without interpolating the raw input. Keep rejecting userinfo, query, and fragment, including on loopback and on `https`, with `provider URL must not contain credentials, query, or fragment`. Check that rule before the scheme. Then accept `https`, or `http` only when `Url::host()` is ASCII-case-insensitive `localhost`, an IPv4 address in `127.0.0.0/8`, or IPv6 `::1`. Use the locked `url` 2.5.8 `Host` type, adding a direct `url = "2.5.8"` dependency only when naming `Host` requires it, with no second `url` version. Replace `provider URL must use HTTP or HTTPS` with `provider URL must use HTTPS, or HTTP on a loopback host`. Leave suffix joining unchanged. Do not rewrite `http` to `https`, add an override, or include the URL or API key in the error.
- [ ] 1.2 Leave `ResponsesProvider::for_endpoint` on its HTTP IP-loopback fixture rule, and leave `ResponsesProvider::new` pinned to `https://api.openai.com/v1`. Do not change registry `parse_origin`, package downloads, OAuth callbacks, or the Claude Code adapter. No configuration value, environment variable, or flag may disable the rule.

## 2. Tests

- [ ] 2.1 Extend the `endpoint()` unit test for every scenario in `specs/provider-endpoint-security/spec.md` and the rows in `design.md`. HTTPS remote: `https://api.deepseek.com/v1` is created and the existing DeepSeek suffix assertion stays. HTTP remote: `http://api.example.com/v1` fails with `provider URL must use HTTPS, or HTTP on a loopback host`. HTTP loopback is created: `http://localhost:11434/v1`, `http://LOCALHOST:8080`, `http://127.0.0.1:1234/v1`, `http://127.0.0.2:1234`, and `http://[::1]:1234/v1`. Lookalikes fail: `http://localhost.evil.com/v1`, `http://127.0.0.1.nip.io/v1`, `http://localhost./v1`, `http://evil-localhost/v1`, `http://0.0.0.0:8080`, `http://[::ffff:127.0.0.1]:8080`, and `http://192.168.1.10:11434`. Other schemes fail: `ws://api.example.com/v1`, `wss://api.example.com/v1`, `ftp://api.example.com/v1`, `file:///tmp/a`, and a schemeless string. Embedded credentials still use today's message: `https://key:secret@api.example.com/v1`, plus a query and a fragment, fail with `provider URL must not contain credentials, query, or fragment` and not the HTTPS message. Also cover uppercase `https` and a non-443 `https` port. A planted secret is absent from every error.
- [ ] 2.2 Call `OpenAiProvider::new` and `AnthropicProvider::new` with `http://api.example.com/v1` and a planted non-empty API key, with no server listening. Both constructors fail, neither error contains the key, and construction returns before a request exists.
- [ ] 2.3 Add a worker `configure_provider` test for `kind: openai`. `base_url` `http://api.example.com/v1` is rejected with the stable HTTPS message, the planted API key is absent from the error, and no provider is retained from that configuration. `base_url` `http://127.0.0.1:8080/v1` is accepted without connecting. Leave the Anthropic and Responses worker cases on their fixed HTTPS endpoints.
- [ ] 2.4 Add a redirect fixture on the shared `http_client()`. Wiremock serves `http://127.0.0.1:<port>`, so the fixture uses an accepted loopback HTTP endpoint that returns `302` to an `http://` URL. Assert exactly one request reached the mock and the call fails without a second request. The redirect policy is `redirect::Policy::none()` on the same client an accepted HTTPS endpoint uses. It is not scheme-specific.
- [ ] 2.5 Run `cargo test -p ejm-harness --test provider_http` and confirm the existing wiremock loopback fixtures still construct OpenAI, Anthropic, and Responses providers.

## 3. Contract, hosts, and version note

- [ ] 3.1 Update constructor rustdoc, `docs/implementation-contract.md`, `docs/sdk.md`, `docs/ide-integration.md`, the existing `## [0.1.0] - 2026-10-19` section in `CHANGELOG.md`, and a dated decision-record row in `docs/contributing.md`. State that worker protocol 1 and the generated schema are unchanged. Add the behavior bullet to that `0.1.0` section. Do not add an `## [Unreleased]` bullet for it and do not open a `0.2.0` section. Leave `VERSION` unchanged. Keep the sentence that `v0.1.0` was re-pointed from `8d3dc39` to the protected main commit recorded in provenance.json before publication.
- [ ] 3.2 Do not edit `sdk/typescript`, `sdk/python`, `registry/`, `ejm-cli`, or `ejm-ide`. In the implementation report, point at ejm-cli pull request 11. Say that its `api_base_url()` check returns the CLI sentence before the key is read, and that `OpenAiProvider::new` and `AnthropicProvider::new` still return `provider URL must use HTTPS, or HTTP on a loopback host` for every other caller. `main` already prints that `anyhow` chain. The worker surfaces the harness sentence as protocol code `configuration`. The first published release stays `0.1.0`. The CLI pin `.github/harness-revision` stays `4a751c6a3f24d7d143140f79ff53517fb93e3531`, and the ejm-ide pin at the same path stays `09b779ee0452dda07b7f52503be22e65fe238373`, until a later adoption of the published commit. The CLI's own version moves only in the CLI repository.

## 4. Verification

- [ ] 4.1 Run the focused provider and worker tests, `cargo clippy -p ejm-harness -p ejm-harness-worker --all-targets --locked -- -D warnings`, and `python3 scripts/generate-protocol.py --check`.
- [ ] 4.2 Run `./scripts/check` with `CARGO_BUILD_JOBS=2`. Record the platform and any step that did not run. A skipped gate step stays unverified.

## Workflow follow-up

- Apply this change only after Everett approves the design. The proposal and delta spec are already present.
- Sync the specs and archive only after the tasks above and the required gate pass.
- Do not publish, tag, or deploy as part of this work. Recreating `v0.1.0` is a separate maintainer step after the published tree is on protected `main`.
