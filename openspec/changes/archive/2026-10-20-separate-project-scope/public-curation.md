# Public source curation

Date: 2026-10-21

## Scope

This port brings the package and project-isolation contracts planned on
2026-10-20 into public source. It starts from accepted main
`a2e77031248d7f6cebb545798776e572a5a3f32d`, tree
`f73bf1a36095a8967b4f3a042ce444b1b5ef182a`. The existing TLS lockfile corrections
remain intact. The coordinated release tracks this prerequisite as task 1.6 of
`publish-public-site-updates` in the Studio repository.

The earlier local implementation also contained capacity, provider, auth,
platform and presentation work. This port excludes it. Earlier local gate,
archive and publication reports do not verify this curated tree. This record
covers the public port; it does not reproduce the earlier local working tree.

## Public contracts

- `ejm_harness_package_spec::MANIFEST_FILE` is `ejm.package.json`, with no legacy
  alias. Authors must rename earlier manifests and repack archives, then use the
  resulting sizes and hashes.
- The packages crate exports no default registry. `install_source` takes
  `Option<&RegistryClient>`. Exact registry references require `Some(&client)`;
  local, npm, PyPI and Git sources accept `None`.
- Hosts select registry configuration and own its error UI. The CLI needs a
  coordinated source update before consuming these breaking APIs. A new origin
  neither transfers credentials nor authenticates that service.
- Worker and SDK wire types are unchanged. Their package fixtures consume the
  shared manifest contract.
- Registry metadata, private service bindings and resource placeholders belong
  to this project. Checked-in deployment configs supply no origin. Tests and
  schema generation inject a reserved example; it is not a live endpoint.

The harness owns the package and registry contracts. CLI and IDE consume its
public APIs; the dependency direction remains host to harness. No dependency,
service, process, adapter or background task is added. No performance change is
claimed.

## Security and compatibility

Fixed origins, HTTPS or loopback HTTP, redirect refusal, bounded responses,
archive validation, immutable versions, explicit grants, origin-scoped encrypted
credentials and atomic device decisions retain their existing owners and
behavior. Native extension code remains unsandboxed.

The Linux FIFO credential regression and existing deadlines remain unchanged.
This port does not bring in platform-test changes or alter dependency versions.
The registry keeps its existing public asset build until the separate
presentation prerequisite is accepted.

## Encoded archive fixtures

All three gzip tar payloads now contain `ejm.package.json`. Their manifest JSON
still matches the fixture record. The symlink fixture retains `escape` pointing
to `../outside`. The ancestor-conflict fixture retains both the `parent` file
and `parent/child.txt`.

| Fixture           | Bytes | SHA256                                                             |
| ----------------- | ----: | ------------------------------------------------------------------ |
| Valid             |   246 | `a1b5f0385ebeb0aae802a7b8053b0b16bbec50d6fe63d470497cc2922298f5e2` |
| Symlink           |   273 | `1e2f9a3f7bfad18bafa1dccc5892d300efe5541924b0f7064776eab8e2d53dee` |
| Ancestor conflict |   297 | `77a435fc545181d54ce9163be7822d34b9a647f73df09a02d9be76fbcac89ee2` |

These are revised fixtures, not measured product archives. Accepted main has no
tracked native measurement source bundle to revise. Unrelated local measurement
archives and their receipts are not imported.

## Verification

Local checks used macOS on Apple Silicon, Node 24.22.0, Rust 1.99.1, two Cargo
jobs and two test threads:

- All 42 focused package and archive tests passed, including missing registry
  configuration, local installation without a registry, origin isolation and
  hostile archives.
- The unchanged `./scripts/check-registry` passed native and Wasm Clippy, Rust
  policy tests, schema freshness and constraints, dependency audits, both
  deployment dry-run bundles and all 24 API/auth fixtures.
- Both added specifications passed strict OpenSpec validation.
- Decoded archive member, manifest, size and digest checks passed. The root
  Cargo lock is unchanged; the registry Cargo lock changes only its root package
  name. Both npm dependency graphs remain unchanged.
- The Linux FIFO credential regression is preserved. It was not executed on
  macOS. Existing deadlines and unrelated platform coverage remain unchanged.

The first focused run could not bind local fixture ports in the filesystem
sandbox. The retry passed with local socket access. No test or deadline changed.

The unchanged normal pre-commit hook runs `./scripts/check` and must pass before
this source commit. Its result accompanies the source PR. Coordinated CLI and IDE
public-contract checks remain the host owners' responsibility. These local
results do not claim hosted review or Linux and Windows acceptance.

## Availability

This is source preparation. No native installer, SDK package, registry service,
cloud resource, DNS record, credential migration or site is published or changed.
Local checks and dry-run bundles do not establish hosted CI, native platform
acceptance, real GitHub sign-in or deployment. Protected review, consumer
integration and each publication gate remain separate.
