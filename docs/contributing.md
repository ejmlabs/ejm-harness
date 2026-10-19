# Contributing to ejm-harness

ejm-harness is a reusable Rust engine. The CLI is a separate consumer. A host
chooses providers, tools, policies, and limits through public APIs. TypeScript
and Python clients own one persistent Rust worker each and adapt that protocol
to their language.

## Set up

Use Rust 1.99.1 with rustfmt and Clippy, Node 24 or later, Python 3.11 or later,
and uv. `rust-toolchain.toml` and workspace `rust-version` agree on Rust 1.99.1.
An older compiler is not a supported configuration. Cargo, npm, and Python
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

This source prerequisite starts from protected harness main
`ee1d2e3df5d77027be226d20830f6c5ea84d0d1d`. It updates both canonical
workflow `uses` pins and the release `standards-ref` together from
`5fae851502c2e3c31d4df9a44f8d56c2cb49e098` to reviewed central PR #11 implementation
`ae903129564926a14287f75d2012610c59cccb65`. The earlier `5309137` and
`0adffdf5` central targets are obsolete history. Product code, manifests,
locks, tools and gate commands stay at the protected base.

Central PR #11 passed required checks and exact-head human approval, then
merged through protected main as
`8c304c8cdfc8b6fe50f3e442507215e615944f2b`. The merge and reviewed head share
tree `91b7f5c0990e27f2e0fdfcce26aac18afdcf6b81`. The exact implementation
is retained at
`refs/heads/workflow-pins/ae903129564926a14287f75d2012610c59cccb65` under
active update/delete rule `25369476`, with no bypass actors and unchanged main
protections. Harness caller adoption and its hosted checks remain pending.

The retained central quality workflow installs `pkg-config` and
`libdbus-1-dev` with the runner's package manager only for the admitted
`ejm-harness` profile on Linux, before Rust tool setup and the existing gate.
The fixed step comes from the reviewed central workflow. It does not execute a
harness-local action or give product code installation authority. The central
CLI setup hook remains restricted to its existing CLI profile.

The separate host-auth API candidate needs these D-Bus development files for
its native Secret Service key adapter. Its Linux hosted run `118036420366`
failed to find `dbus-1.pc`. Passing local macOS checks do not verify that Linux
build. This caller prerequisite contains no new auth API, credential migration,
key-service behavior or dependency lock resolution.

Adopt the prerequisites in this order:

1. Verify the central merge and retention receipt above against the exact
   implementation at all three pin sites. The central prerequisite is complete.
   Keep its retained source; a later reviewed revision requires fresh checks,
   exact approval and protected retention.
2. Recheck all three caller pin sites against the retained implementation.
   Publish and admit the caller-only change through the existing human and
   protected merge gates. Each new head needs fresh decisions and checks.
3. Obtain exact-source Linux, macOS and Windows quality results for this caller.
   Then rebase the separately reviewed host-auth API source onto the adopted
   prerequisite and run fresh locked gates. A passing caller-only result does
   not prove the later API source or native key service.

Private CLI dependency checkout still requires reviewed harness source and a
dedicated, expiring Contents: read credential limited to `ejmlabs/ejm-harness`.
The user authorized private API publication and provided `EJM_HARNESS_TOKEN`;
successful CLI source preparation does not verify that credential's scope or
expiry. Validate those separately through protected secret handling. IDE access
and adoption require its owner's separate authorization. Keep workstation
credentials local and preserve all existing admission, approval and secret
boundaries.

The normal commit hook passed for caller head
`28401e93b79824d13bda697b805ceca05ea65d04`. Both admitted AI reports for
that head explicitly called review incomplete because the old and new external
workflow implementations were absent. Successful wrapper jobs and zero findings
do not complete review or establish acceptance.

The [shared workflow source evidence](ci/shared-workflow-review/README.md)
supplies exact plaintext copies of both called workflows and their readable
source. Its manifest binds original paths and full revisions to Git blob IDs
and SHA-256 digests, with byte-identical sources shared once and the MIT notice
preserved. These are review data, outside executable paths. Caller pins,
product code, manifests, locks, tools and gates stay unchanged.

The corrected head requires the normal hook, fresh admission, Linux/macOS/Windows
quality checks, two complete reviews, exact-head explicit approval and protected
adoption. This contributor CI guidance changes no product API, setup or runtime
behavior, so the public product page needs no change. Source preparation does
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
