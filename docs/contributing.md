# Contributing to ejm-harness

ejm-harness is a reusable Rust engine. The CLI is a separate consumer. A host
chooses providers, tools, policies, and limits through public APIs. TypeScript
and Python clients own one persistent Rust worker each and adapt that protocol
to their language.

This open source project has its own source identity, contracts, service
configuration, credentials, and distribution. Keep unrelated proprietary product
names, domains, assets, and metadata out of source, docs, examples, fixtures,
generated files, and release archives. A public example must not imply an
unverified deployed endpoint.

## Set up

Use Rust 1.99.1 with rustfmt and Clippy, Node 24 or later, Python 3.11 or later,
and uv. `rust-toolchain.toml` and workspace `rust-version` agree on Rust 1.99.1.
An older compiler is not a supported configuration. Linux builds require `pkg-config`
and D-Bus development headers for the native Secret Service key adapter. On
Debian/Ubuntu install `pkg-config` and `libdbus-1-dev` before the Cargo gate.
Offline fixtures inject key storage and do not require a desktop secret service;
real persistent use still requires an unlocked user Secret Service. Cargo, npm, and Python
development locks belong in version control.

Install the quality tools once:

```sh
cargo install --locked --jobs 2 --version 0.20.2 cargo-deny
cargo install --locked --jobs 2 --version 0.9.2 cargo-machete
uv tool install pre-commit==4.3.0
npm ci --ignore-scripts
npm --prefix registry ci --ignore-scripts
uv sync --project sdk/python --group dev --locked
./scripts/install-hooks
```

Run the same required gate locally, in pre-commit, and in CI:

```sh
./scripts/check
```

Root ESLint also checks registry TypeScript, which needs the separate registry
dependencies. If `registry/node_modules` is absent, the gate installs its locked
dependencies with lifecycle scripts disabled before npm checks. An install
failure stops the gate. Existing dependency trees are reused; rerun the matching
`npm ci --ignore-scripts` command after dependency changes.

The gate checks crate boundaries and skill references, rustfmt, Clippy, Rust
tests and builds, rustdoc, unused dependencies, dependency advisories, licenses,
generated wire types, ESLint, Prettier, TypeScript, Ruff, mypy, and SDK fixtures.
It does not publish packages or call hosted models. Rust process and compiler
jobs are capped at two by the repository configuration.

`./scripts/check-registry` is the separate required gate for the registry. It
checks both workers, auth fixtures, schema, dependencies, and deployment dry runs.
Run it when changing the registry; CI runs both gates after human admission.

Use focused tests while editing. Run the required gate once the patch is ready.
A missing tool or stale lock is a failed gate, not a skipped check. The advisory
check needs access to the RustSec database. Report network or platform limits
as unverified evidence instead of describing a partial run as green.

## Shared workflows and source releases

Both canonical workflow callers and the release `standards-ref` pin reviewed
shared revision `6717b433c462e01abaed86e2118e4843827598cb`. Update all
three sites together through a protected PR. The shared repository owns the
workflow implementation, standards and release validator; this repository
owns its product gates and release metadata. Product repositories consume the
harness public APIs. No product runtime becomes a harness dependency.

This update replaces `c4f2873ef0eef5bc3c7d75cf71fa3489a911d134`. It retains
the complete two-pass Codex review guard, fixed Linux D-Bus setup, three-OS
quality matrix and separate registry gate. Linux quality installs `pkg-config`
and `libdbus-1-dev` before the existing gate, using a fixed trusted workflow
step. Existing product gate commands, permissions, secret names and human
admission rules are unchanged. Runtime sources, manifests, locks and public
SDK contracts do not change.

The target also supports CLI harness selection and an optional UI source pin.
This harness profile selects no sibling harness. It has no
`.github/ui-source.json`, so the optional UI checkout, source archive and
locked npm install stay inactive. Adding such a pin later needs its own
review; this update adds no UI dependency or package installation.

### Review source context

The [canonical source manifest](../.github/review-context/manifest.json) binds
the actual `c4f2873` to `6717` transition to full Git commit, tree and blob IDs,
original modes, sizes and SHA-256 digests. Generate both revisions only with
the shared `scripts/export_review_context.py`. Its 32 source files and one
manifest fit the combined 750,000-byte cap. They are nonexecutable offline
review data. Never import or execute them. Existing narrow Ruff and Prettier
exclusions apply only to `.github/review-context`; prior snapshots remain in
Git history. Shared snapshots retain `Copyright (c) 2026 ejmlabs contributors`
and the same permission and warranty terms as the [MIT license](../LICENSE).

Fresh exact-head human readiness and separate approval remain required before
costly work and merging. A changed caller must pass the existing Linux, macOS
and Windows quality jobs, registry and standards checks, two complete actual
Codex Sol 6.1 medium reviews, and the protected approval gates.

[PR #15](https://github.com/ejmlabs/ejm-harness/pull/15) adopted these pins on
protected main `1542f7c2c97d247df0d4744103d0e5e02660fbb0`.
Its [admitted run](https://github.com/ejmlabs/ejm-harness/actions/runs/39344288127)
passed all three OS quality jobs, registry, standards and two complete actual
Codex Sol 6.1 medium reviews with no findings. It merged with all protections
enforced. That run used the previous `c4f2873` shared caller. A fresh admitted
follow-up, [PR #16](https://github.com/ejmlabs/ejm-harness/pull/16), verified the
installed `6717b433` caller in
[run 39347428993](https://github.com/ejmlabs/ejm-harness/actions/runs/39347428993).
Its first attempt passed Linux, macOS, Windows, registry, standards and two
complete actual Codex Sol 6.1 medium reviews with no findings. The exact reviewed
tree merged normally as `039139129be2e957535478968c416e8c1d057fe8`, with
protections unchanged. This closes installed-caller acceptance, not the
remaining native key-service, host or release gates.

### Draft release validation

Follow the uniform [release policy](../RELEASING.md) for independent SemVer,
version metadata, dated notes and explicit publication. The repaired validator
compares original document and license bytes with both source archives,
preserving line endings. The earlier validator normalized expected text and
could reject an unchanged original CRLF notice.

The shared [draft pilot](https://github.com/ejmlabs/.github/actions/runs/39340565293)
verified exact source archives, checksums and provenance. A separate
[duplicate dispatch](https://github.com/ejmlabs/.github/actions/runs/39341034364)
refused its existing version before writes. The pilot created a source-only draft.
Those shared results do not establish a harness draft, binary, package or
deployment. Harness PR #16 supplies the separate installed-caller evidence.

The dated `0.1.0` [source notes](../CHANGELOG.md) are prepared for review.
No harness draft is announced. After their normal protected merge, freeze the
resulting main commit before requesting a source-only draft. Review both source
archives, original notice bytes, checksums and provenance before any publication.

A release dispatch permits only its first attempt. If it fails after reserving
a version, inspect the protected tag and draft before deciding on a new version.
Do not rerun the failed attempt, move a tag or replace assets. Installing these
callers creates no tag, draft, installer, package or deployment. Product release
writes and publication require separate authorization.

This contributor and release guidance changes no product API, public product
page or generated site output, so the public site needs no content or build
change. No public site publication or native platform certification is claimed.

## Plan changes

Read [the repository contribution policy](../CONTRIBUTING.md). Feature and bugfix
work starts with OpenSpec exploration or clarification, then a proposed change.
Implement tracked tasks through the apply workflow. Sync the specifications and
archive only after the required work and checks pass. Existing changes live in
`openspec/changes`.

## Design and ownership

The checked dependency rules and their reasons live in
[architecture-policy.toml](../scripts/architecture-policy.toml). Cargo's resolved
dependency names are checked, so renamed dependencies cannot bypass a boundary.

- `ejm-harness-exec` owns admission, leases, command supervision, and bounded output.
- `ejm-harness-auth` owns identity and credential renewal through injected protected storage.
- `ejm-harness` owns runs, events, persistence, context, and scheduling policy.
- `ejm-harness-coding` implements coding tools over the public engine and executor.
- `ejm-harness-worker` translates the versioned protocol to public harness calls.
- `ejm-harness-bench` measures deterministic workloads through public APIs.
- `ejm-harness-package-spec` owns portable manifests and bounded archive validation.
- `ejm-harness-packages` owns package installation, registry transport, and shared language hosts.
- `registry/` is a separate Rust Cloudflare Worker and private Better Auth Worker.

Keep each module responsible for one cohesive decision. Hide storage and
provider details behind the existing ports. Constructors receive concrete
configuration and the dependencies they use. Use small traits for genuine
substitution, and test the contract of each implementation. A dependency
injection container, generic factory, or trait for every struct adds work here.

Extend the existing driver through `Provider`, `Tool`, `ContextPolicy`, and
`RunStrategy` where those ports fit. Preserve defaults when adding options.
Keep terminal rendering, input, account UX, and product integrations in hosts.
Do not import frontend code into the engine or execution crates.

## Strict code policy

Workspace Clippy `all` and `pedantic` groups are denied. Production
`unwrap`, `expect`, `panic`, `todo`, `unimplemented`, and `dbg!` are denied.
Handle failures at the boundary and return useful context. Public fallible APIs
document their error conditions. Do not invent an error wrapper that discards
the source or turn an unknown failure into success.

Unsafe Rust is denied. Safe Rustix APIs handle native page-size, identity, and
process signals, so the checked exception list is empty. New unsafe code needs a
documented package gap, a concrete invariant, a platform test plan, and review of
the exception list. Sampling and process groups never become hard containment
merely because the call is native.

A lint exception belongs on the smallest item that needs it and includes a
reason tied to a stable contract or measured tradeoff. Do not allow lint groups,
lower their severity, or disable a tool to make a patch pass. Test modules may
allow `unwrap` and `expect` to fail at the assertion site. Generated wire files
are checked by their schema generator rather than edited by hand.

Keep dependencies small and maintained. Cargo Machete checks unused direct
dependencies. Cargo Deny checks advisories, sources, licenses, and duplicate
versions. Prefer dependency convergence. Any necessary version exception names
the exact crate and version and explains the upstream constraint. Broad tree
exceptions and permanent advisory ignores need a separate design decision.

### Use maintained packages for standard behavior

Use a trusted, maintained package for standard plumbing whenever one meets the
requirements.
Review its maintenance, security record, license, supported platforms, and
resource cost. Use existing dependencies when they already solve the problem.
Protocol parsers, HTTP servers, cryptography, file traversal, terminal rendering,
and formatters are package responsibilities.

Custom code belongs where the product differs: shared resource admission,
effect ownership, recovery, context policy, and the terminal workflow. A small
adapter can apply our bounds and cancellation around a library. It must not
grow into a second implementation of the library's protocol. Record a concrete
gap, alternatives, and evidence before making an exception.

## Updating decisions and skills

When a public contract, module boundary, resource guarantee, or measured
performance choice changes, update its owning code, check or test, documentation,
and relevant project skill in the same patch. Explain the reason, alternatives,
and evidence under the decision record below. Remove superseded advice.

An ordinary bug fix does not need a new rule. Add a rule when a demonstrated
failure or deliberate design choice makes it useful to future work. Keep skill
descriptions narrow so unrelated work does not load them. Link to the owning
contract instead of copying large sections. `scripts/check-policy.py` validates
discovery names, routing, references, lint inheritance, and dependency boundaries.
Its negative fixtures prove the gate rejects forbidden dependencies and misplaced
exceptions. Review still supplies the judgment that these checks cannot encode.

### Decision record

| Decision                                 | Reason                                                                                                        | Evidence and change rule                                                                                                                                                                                                                            |
| ---------------------------------------- | ------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Rust engine with thin language facades   | One implementation owns execution and recovery                                                                | Worker schema and cross-language fixtures must agree                                                                                                                                                                                                |
| Separate CLI repository                  | Other hosts reuse the same harness without terminal dependencies                                              | Checked crate graph rejects terminal dependencies                                                                                                                                                                                                   |
| One governor per harness                 | Per-agent limits multiply aggregate load                                                                      | Admission tests and deterministic managed-session benchmark                                                                                                                                                                                         |
| Explicit native capabilities             | Monitored usage differs from kernel enforcement                                                               | Prelaunch rejection tests and platform evidence in executor docs                                                                                                                                                                                    |
| Traits at external and policy boundaries | Hosts substitute behavior without copying the driver                                                          | Public provider, tool, context, and strategy contracts                                                                                                                                                                                              |
| 2026-10-18: maintained HTTP fixtures     | Test provider behavior without owning an HTTP server or parser                                                | Wiremock transport tests and fragmented SSE decoder tests                                                                                                                                                                                           |
| 2026-10-18: packages before plumbing     | Keep effort on distinct harness behavior and terminal workflows                                               | Owning skills require package review and documented exceptions                                                                                                                                                                                      |
| 2026-10-18: bounded `sse-core` decoder   | Retain field limits and cancellation without owning SSE framing                                               | Fragmented UTF-8, empty-data overflow, and provider HTTP fixtures                                                                                                                                                                                   |
| 2026-10-18: aggregate stream limits      | Bound comment traffic and ignored events before provider filtering                                            | Body-byte, decoded-event, exact-boundary, and cancellation fixtures                                                                                                                                                                                 |
| 2026-10-18: safe Rustix process APIs     | Use maintained native wrappers and remove project unsafe code                                                 | Executor cleanup fixtures and credential-store ownership tests                                                                                                                                                                                      |
| 2026-10-18: maintained SDK generators    | Keep standard schema rendering in maintained development packages                                             | Strict language checks and generated-contract freshness gate                                                                                                                                                                                        |
| 2026-10-18: `procfs-core` stat parser    | Preserve raw process start ticks without owning the stat format                                               | Bounded adapter fixtures for unusual names and malformed records                                                                                                                                                                                    |
| 2026-10-18: bounded observation masking  | Keep useful actions and failure evidence without another model call                                           | [Context, paging, provider, and recovery fixtures](harness-engineering.md)                                                                                                                                                                          |
| 2026-10-18: visible retrieval limits     | Prevent the model from treating partial results as complete                                                   | Coding tool limits and next-provider-request fixtures                                                                                                                                                                                               |
| 2026-10-18: worker stdin shutdown        | Exit after cleanup while the host keeps idle stdin open                                                       | SDK full-journal fixture retains run-wait and process-exit limits                                                                                                                                                                                   |
| 2026-10-18: portable package validation  | Native and Cloudflare hosts reject the same hostile archives                                                  | [Archive and integrity fixtures](packages.md)                                                                                                                                                                                                       |
| 2026-10-18: explicit version trust       | Installation and updates cannot activate new native code                                                      | Package manager trust and changed-content fixtures                                                                                                                                                                                                  |
| 2026-10-18: shared language hosts        | Package count and session count do not multiply interpreter processes                                         | Runtime sharing, cancellation, reload, and 100-session admission fixtures                                                                                                                                                                           |
| 2026-10-18: bounded source downloads     | Package tools could fill host disk before archive limits applied                                              | Maintained HTTP and ZIP/tar libraries, fixed public origins, source-shadowing and hostile-distribution fixtures                                                                                                                                     |
| 2026-10-18: Cloudflare registry          | Workers, D1, R2, and maintained auth cover the required service                                               | [Registry contract and security runbook](../registry/README.md)                                                                                                                                                                                     |
| 2026-10-18: atomic device consent        | Concurrent approval and denial must preserve the first owner decision                                         | [Real Better Auth race regression](../registry/README.md#authentication-sessions-and-browser-safety)                                                                                                                                                |
| 2026-10-20: independent package identity | Use `ejm.package.json`, remove the registry default, and require an explicit client only for registry sources | [Package contract](packages.md), archive/source fixtures, SDK fixtures, and the [curation record](../openspec/changes/archive/2026-10-20-separate-project-scope/public-curation.md) state the breaking format, origin isolation and release limits. |

For a new decision, add a dated row with the changed contract and a link to its
test or recorded measurement. Update an existing row when it is superseded.

The 2026-10-19 completion-sharing change uses the engine's existing Tokio watch
receiver for exact-run host subscriptions, including internal queued successors.
A receiver in each already bounded active entry preserves sender-drop errors;
it adds no task, channel or history map. Detailed completion records the
queued-successor decision under active admission ownership before notification,
so a fast local successor cannot disappear from that result. Summary-only wait
is unchanged. The public
[completion regression](../crates/ejm-harness/tests/run_completion.rs) covers
wrong identities and a blocked run-end observer. Its runtime result and the
normal local and hosted gates remain pending for this prepared source.
