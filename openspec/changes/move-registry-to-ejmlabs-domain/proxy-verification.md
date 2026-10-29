# Registry proxy verification

## Scope

This 2026-10-20 follow-up starts at
`715f9462665876440bb7fa864d42bdf1153e2fae`. That source already disables
proxies in `RegistryClient::new` for accepted HTTP loopback origins. This patch
preserves the transport and strengthens its existing Wiremock regression test.

The child process calls `me` with a synthetic account bearer. The direct
registry requires that bearer and receives exactly one request. The proxy must
receive none. The parent checks that child output contains no bearer, and
`kill_on_drop` bounds cleanup if the child times out. No real credentials or
external services are used.

## Results on this Mac

- `rustfmt --edition 2024 crates/ejm-harness-packages/tests/registry_http.rs`:
  passed.
- `git diff --check`: passed.
- `openspec validate move-registry-to-ejmlabs-domain --strict`: passed.
- The focused Cargo command below compiled successfully in 7.08 seconds. Its
  first test run failed because the sandbox denied Wiremock's loopback bind.
  This was an environment failure before any request.
- Rerunning the compiled exact test with approved local-network access passed:
  one passed, zero failed, four filtered out, 0.03 seconds. The shared Cargo
  coordination lock was held during that run.

```sh
CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 \
CARGO_TARGET_DIR=/Users/everettmorgan/Desktop/ejm-harness/target \
cargo test --offline --locked -p ejm-harness-packages \
  --test registry_http loopback_http_registry_ignores_http_proxy \
  -- --exact --test-threads=1
```

The successful rerun executed:

```sh
/Users/everettmorgan/Desktop/ejm-harness/target/debug/deps/registry_http-2af80ad88b83df5e \
  loopback_http_registry_ignores_http_proxy --exact --test-threads=1
```

## Limits and integration order

`./scripts/check`, `./scripts/check-registry`, the complete package test suite,
and Linux/Windows checks were not rerun for this follow-up. Disk was constrained
and builds were coordinated with other owners' performance measurements. The
fresh review clone has no installed Git hooks; no hook configuration or bypass
flag was used. These focused results do not replace the normal review or CI
gates.

The harness owns registry transport. The CLI consumes the public
`ejm-harness-packages` contract and keeps its existing immutable harness pin.
That older pin needs the CLI's temporary proxy configuration guard. Verify and
merge the harness through protected main first. Adopt only a compatible,
reviewed full commit in the CLI, reconcile its credential-storage integration,
and pass the pinned route and credential checks before removing the adapter.

The public package guide and changelog describe this contract. The static
product page has no registry proxy controls or claims, so it needs no content
change for this fixture follow-up. Publication was not verified. No tag,
release, CI approval rule, DNS, Worker, or deployment was changed. Keep the
OpenSpec change active until its remaining review and full-gate work is done.
