# Tasks

Implementation waits for `proposal.md`, the delta specs, and Everett's approval.
Until then, leave every source file untouched. The decisions in `design.md` are
the behavior to implement.

## 1. Reject cleartext non-loopback provider endpoints

- [ ] 1.1 In `endpoint()` (`crates/ejm-harness/src/provider.rs`), keep parse failure as `invalid provider base URL` without interpolating the raw input. Keep rejecting userinfo, query, and fragment, including on loopback, with `provider URL must not contain credentials, query, or fragment`. Then accept `https`, or `http` only when `Url::host()` is ASCII-case-insensitive `localhost`, an IPv4 address in `127.0.0.0/8`, or IPv6 `::1`. Use the locked `url` 2.5.8 `Host` type, adding a direct `url = "2.5.8"` dependency only when naming `Host` requires it, with no second `url` version. Replace `provider URL must use HTTP or HTTPS` with `provider URL must use HTTPS, or HTTP on a loopback host`. Leave suffix joining unchanged. Do not rewrite `http` to `https`, add an override, or include the URL or API key in the error.
- [ ] 1.2 Leave `ResponsesProvider::for_endpoint` on its HTTP IP-loopback fixture rule, and leave `ResponsesProvider::new` pinned to `https://api.openai.com/v1`. Do not change registry `parse_origin`, package downloads, OAuth callbacks, or the Claude Code adapter.

## 2. Tests

- [ ] 2.1 Extend the `endpoint()` unit test for the accepted and rejected cases in `design.md`: uppercase `https`, a non-443 `https` port, `http://127.0.0.1`, `http://127.0.0.2`, `http://localhost`, `http://LOCALHOST`, `http://[::1]:1234`, remote `http` including port 443, LAN and `0.0.0.0`, IPv4-mapped `::ffff:127.0.0.1`, `foo.localhost`, userinfo on loopback, query, fragment, and a non-HTTP scheme. Assert the stable message and that a planted secret is absent from the error. Keep the existing DeepSeek suffix assertion.
- [ ] 2.2 Add a worker `configure_provider` test for `kind: openai` that rejects `http://api.example.test/v1` with the stable message, accepts `http://127.0.0.1:9/v1` and `http://[::1]:9/v1` without connecting, and shows a planted API key is absent from the error. Leave the Anthropic and Responses worker cases on their fixed HTTPS endpoints.
- [ ] 2.3 Run `cargo test -p ejm-harness --test provider_http` and confirm the existing wiremock loopback fixtures still construct OpenAI, Anthropic, and Responses providers.

## 3. Contract, hosts, and version note

- [ ] 3.1 Update constructor rustdoc, `docs/implementation-contract.md`, `docs/sdk.md`, `docs/ide-integration.md`, `## [Unreleased]` in `CHANGELOG.md`, and a dated decision-record row in `docs/contributing.md`. State that worker protocol 1 and the generated schema are unchanged. Leave the dated `0.1.0` section and `VERSION` unchanged.
- [ ] 3.2 Do not edit `sdk/typescript`, `sdk/python`, `registry/`, `ejm-cli`, or `ejm-ide`. In the implementation report, tell the CLI session that `--base-url` and `EJM_CLI_BASE_URL` fail inside `OpenAiProvider::new` and `AnthropicProvider::new` with `provider URL must use HTTPS, or HTTP on a loopback host`, that `main` already prints that `anyhow` chain, and that the worker surfaces it as protocol code `configuration`. Note that a later release which includes this behavior is minor `0.2.0`, not a patch, and that the CLI's own version moves only in the CLI repository.

## 4. Verification

- [ ] 4.1 Run the focused provider and worker tests, `cargo clippy -p ejm-harness -p ejm-harness-worker --all-targets --locked -- -D warnings`, and `python3 scripts/generate-protocol.py --check`.
- [ ] 4.2 Run `./scripts/check` with `CARGO_BUILD_JOBS=2`. Record the platform and any step that did not run. A skipped gate step stays unverified.

## Workflow follow-up

- Apply this change only after Everett approves the design and the product manager has added `proposal.md` and the delta specs.
- Sync the specs and archive only after the tasks above and the required gate pass.
- Do not publish, tag, or deploy as part of this work.
