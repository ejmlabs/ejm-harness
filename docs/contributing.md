# Contributing to ejm-harness

ejm-harness is a reusable Rust engine. The CLI is a separate consumer. A host
chooses providers, tools, policies, and limits through public APIs. TypeScript
and Python clients own one persistent Rust worker each and adapt that protocol
to their language.

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

## Shared CI caller prerequisite

Protected harness main `09b779ee0452dda07b7f52503be22e65fe238373` already
adopts shared revision `aa4144fa978d711a5e009b384718b4cf27cf87bf` at both
canonical workflow `uses` sites and the release `standards-ref`. That revision
requires complete actual AI reviews and owns the bounded static source
exporter. Preserve those controls and the checked standards-plan history.

The next caller-only prerequisite selects retained combined implementation
`c4f2873ef0eef5bc3c7d75cf71fa3489a911d134` at all three sites together.
Its 16 canonical workflow, script, review and template blobs match protected
shared main `d00d8bbc7231485eb918008c562d835f394e82ea`. The exact
`refs/heads/workflow-pins/c4f2873ef0eef5bc3c7d75cf71fa3489a911d134`
ref has active update/deletion protection under rule `25369476`, with no bypass
actors. This revision combines the existing completion guard with the
previously reviewed setup from `ae903129564926a14287f75d2012610c59cccb65`;
selecting the older setup revision alone would remove the newer guard.

The combined quality workflow installs `pkg-config` and `libdbus-1-dev` with
the runner's package manager only for the admitted `ejm-harness` profile on
Linux, before Rust tools and the existing gate. This fixed step comes from the
trusted shared workflow. It executes no harness-local action and gives product
code no installation authority. The CLI local setup hook remains restricted to
its CLI profile. Product runtime, manifests, locks, tools and gate commands
stay at the protected harness base.

The separate host-auth API candidate needs these D-Bus development files for
its native Secret Service adapter: Linux job `118036420366` failed to find
`dbus-1.pc`. This caller prerequisite contains no auth API, credential
migration, native key-service change or dependency resolution. Passing local
macOS or caller-only checks does not verify that later Linux build or the
native key service.

Adopt the prerequisites in this order:

1. Verify the combined source and exact protected retention at all three pin
   sites. Preserve the current caller ancestry through a normal merge of
   protected `09b779`; do not replace newer controls with `ae903`.
2. Run the normal hook and admit the new exact caller head through the existing
   human and protected gates. Require Linux, macOS and Windows quality,
   registry, standards, two complete actual AI reviews and separate explicit
   approval. Prior-head decisions and checks do not approve this head.
3. After protected caller adoption, reconcile and readmit the separate API and
   vault source on the actual protected parent. Run fresh locked platform
   gates. Caller adoption does not establish API or native certification.

Private CLI dependency checkout still requires reviewed harness source and a
dedicated, expiring Contents: read credential limited to `ejmlabs/ejm-harness`.
The user authorized private API publication and provided `EJM_HARNESS_TOKEN`;
successful CLI source preparation does not verify that credential's scope or
expiry. Validate those separately through protected secret handling. IDE access
and adoption require its owner's separate authorization. Keep workstation
credentials local and preserve all existing admission, approval and secret
boundaries.

Both admitted AI reports for the earlier PR #13 head
`28401e93b79824d13bda697b805ceca05ea65d04` explicitly called review
incomplete because the old and new shared workflow sources were absent.
Successful wrappers and zero findings did not complete review. Head
`8b29b6c0aa944ff51611a377a067180b8fd6c940` supplied exact source context;
both actual reports in [run 39274768967](https://github.com/ejmlabs/ejm-harness/actions/runs/39274768967)
bind that head and report `complete: true` with no findings. Those reports cover
that old head, not this reconciliation.

The [canonical source manifest](../.github/review-context/manifest.json) binds
the actual `aa414` to `c4f2873` transition to full Git commit, tree and blob
IDs, original file modes, sizes and SHA-256 digests. Generate the 32 source
files and manifest only with the reviewed shared exporter and enforce its
combined 750,000-byte cap. Shared snapshots retain
`Copyright (c) 2026 ejmlabs contributors`; their permission and warranty text
is identical to the [repository's MIT license](../LICENSE). Snapshots are
nonexecutable offline review data; never import or execute them. The existing narrow Ruff and Prettier exclusions
apply only to `.github/review-context`. The superseded source pair and duplicate
`docs/ci/shared-workflow-review` evidence remain available in Git history.

This contributor CI guidance changes no product API or public product-page
workflow, so the public site needs no content change. Source preparation does
not establish a new full gate, hosted acceptance, native key-service acceptance
or public site publication.

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

| Decision                                 | Reason                                                                | Evidence and change rule                                                                                        |
| ---------------------------------------- | --------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------- |
| Rust engine with thin language facades   | One implementation owns execution and recovery                        | Worker schema and cross-language fixtures must agree                                                            |
| Separate CLI repository                  | Other hosts reuse the same harness without terminal dependencies      | Checked crate graph rejects terminal dependencies                                                               |
| One governor per harness                 | Per-agent limits multiply aggregate load                              | Admission tests and deterministic managed-session benchmark                                                     |
| Explicit native capabilities             | Monitored usage differs from kernel enforcement                       | Prelaunch rejection tests and platform evidence in executor docs                                                |
| Traits at external and policy boundaries | Hosts substitute behavior without copying the driver                  | Public provider, tool, context, and strategy contracts                                                          |
| 2026-10-18: maintained HTTP fixtures     | Test provider behavior without owning an HTTP server or parser        | Wiremock transport tests and fragmented SSE decoder tests                                                       |
| 2026-10-18: packages before plumbing     | Keep effort on distinct harness behavior and terminal workflows       | Owning skills require package review and documented exceptions                                                  |
| 2026-10-18: bounded `sse-core` decoder   | Retain field limits and cancellation without owning SSE framing       | Fragmented UTF-8, empty-data overflow, and provider HTTP fixtures                                               |
| 2026-10-18: aggregate stream limits      | Bound comment traffic and ignored events before provider filtering    | Body-byte, decoded-event, exact-boundary, and cancellation fixtures                                             |
| 2026-10-18: safe Rustix process APIs     | Use maintained native wrappers and remove project unsafe code         | Executor cleanup fixtures and credential-store ownership tests                                                  |
| 2026-10-18: maintained SDK generators    | Keep standard schema rendering in maintained development packages     | Strict language checks and generated-contract freshness gate                                                    |
| 2026-10-18: `procfs-core` stat parser    | Preserve raw process start ticks without owning the stat format       | Bounded adapter fixtures for unusual names and malformed records                                                |
| 2026-10-18: bounded observation masking  | Keep useful actions and failure evidence without another model call   | [Context, paging, provider, and recovery fixtures](harness-engineering.md)                                      |
| 2026-10-18: visible retrieval limits     | Prevent the model from treating partial results as complete           | Coding tool limits and next-provider-request fixtures                                                           |
| 2026-10-18: worker stdin shutdown        | Exit after cleanup while the host keeps idle stdin open               | SDK full-journal fixture retains run-wait and process-exit limits                                               |
| 2026-10-18: portable package validation  | Native and Cloudflare hosts reject the same hostile archives          | [Archive and integrity fixtures](packages.md)                                                                   |
| 2026-10-18: explicit version trust       | Installation and updates cannot activate new native code              | Package manager trust and changed-content fixtures                                                              |
| 2026-10-18: shared language hosts        | Package count and session count do not multiply interpreter processes | Runtime sharing, cancellation, reload, and 100-session admission fixtures                                       |
| 2026-10-18: bounded source downloads     | Package tools could fill host disk before archive limits applied      | Maintained HTTP and ZIP/tar libraries, fixed public origins, source-shadowing and hostile-distribution fixtures |
| 2026-10-18: Cloudflare registry          | Workers, D1, R2, and maintained auth cover the required service       | [Registry contract and security runbook](../registry/README.md)                                                 |
| 2026-10-18: atomic device consent        | Concurrent approval and denial must preserve the first owner decision | [Real Better Auth race regression](../registry/README.md#authentication-sessions-and-browser-safety)            |

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
