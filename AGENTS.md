# ejm-harness

Reusable open source Rust engine, coding tools, executor, worker, and language
SDKs. ejm-cli lives in its own sibling repository.

- Never use em dashes. Use plain, direct prose.
- Keep this open source project independent of unrelated proprietary products.
  Do not include their names, domains, credentials, contracts, assets, or metadata
  in source, docs, fixtures, generated files, or distributed archives.
- Read [the contributor contract](docs/contributing.md) before changing architecture or quality policy.
- Keep Rust ownership and admission in the harness. SDKs adapt the same public worker protocol.
- Use one shared governor for every tool, command, and child agent in a harness instance.
- Bound queues, output, context, retries, steps, artifacts, and process lifetimes.
- Never replay an uncertain effect after recovery.
- Report native restrictions as enforced, monitored, or unsupported. Native execution is not a sandbox.
- Public traits are dependency injection ports. Add a trait where an external boundary or real variation needs one.
- Inherit workspace lints. Do not relax groups or add production panic shortcuts.
- Use trusted, maintained packages for standard behavior. Write custom code
  for ejm-harness's distinct behavior, such as admission, recovery, and context
  policy. Document any exception with the package alternatives and evidence.
  Do not handroll protocol parsing, cryptography, or test servers.
- Run `./scripts/check` before handing off completed work. Use focused checks while editing.
- Keep Cargo jobs at most two on constrained machines.
- Use replay or local HTTP fixtures. Tests must not call paid models.
- Do not publish packages, upload artifacts, or deploy without explicit authorization.
- Feature and bugfix work follows the initialized OpenSpec workflow: explore or clarify,
  propose, apply, sync specs, then archive after required verification.

## Repository map

| Path                              | Owns                                                                             |
| --------------------------------- | -------------------------------------------------------------------------------- |
| `crates/ejm-harness`              | Sessions, agent loop, providers, context, events, persistence, recovery          |
| `crates/ejm-harness-exec`         | One governor, admission, leases, command supervision, bounded output             |
| `crates/ejm-harness-auth`         | Provider sign-in, identity, protected credential storage, renewal                |
| `crates/ejm-harness-coding`       | Native read, list, search, write, patch, and optional shell tools                |
| `crates/ejm-harness-worker`       | Versioned JSONL protocol bridge for the language SDKs                            |
| `crates/ejm-harness-packages`     | Package install, registry client, shared Node and Python hosts                   |
| `crates/ejm-harness-package-spec` | Portable manifests and bounded archive validation                                |
| `crates/ejm-harness-bench`        | Deterministic concurrency and command workloads                                  |
| `sdk/typescript`, `sdk/python`    | Typed language facades over the worker. Wire types are generated                 |
| `registry/`                       | Cloudflare extension registry and its Better Auth Worker                         |
| `site/`                           | Static harness.ejmlabs.com marketing site                                        |
| `docs/`                           | Contracts, decisions, evidence. `docs/contributing.md` holds the decision record |
| `scripts/`                        | The gate, policy checker, generators, benchmark, and hook installer              |

## Names

| Thing                  | Name                                                                                      |
| ---------------------- | ----------------------------------------------------------------------------------------- |
| Program and main crate | `ejm-harness`                                                                             |
| Other crates           | `ejm-harness-exec`, `-auth`, `-coding`, `-packages`, `-package-spec`, `-worker`, `-bench` |
| Rust modules           | `ejm_harness`, `ejm_harness_exec`, and so on                                              |
| Worker binary          | `ejm-harness-worker`                                                                      |
| Environment variables  | `EJM_HARNESS_*`, for example `EJM_HARNESS_WORKER_PATH`                                    |
| State directory        | `.ejm-harness`                                                                            |
| SDK packages           | npm `ejm-harness`, Python `ejm_harness`                                                   |
| Public page            | "harness by ejmlabs" at harness.ejmlabs.com                                               |
| Hosts                  | `ejm-cli` at `../ejm-cli` and `ejm-ide` at `../ejm-ide`, not part of this repo            |
| Package manifest       | `ejm.package.json`; registry clients require an explicit origin                           |

## Commands

```sh
./scripts/check                          # the required gate, same in hooks and CI
cargo test -p ejm-harness-exec           # focused tests for one crate
cargo clippy --workspace --all-targets --locked -- -D warnings
python3 scripts/check-policy.py          # crate boundaries and skill routing
python3 scripts/generate-protocol.py --check
cargo build -p ejm-harness-worker        # SDK tests need this worker
```

Set `CARGO_INCREMENTAL=0` and `CARGO_BUILD_JOBS=2` for full gate runs. If the disk is
full, remove this repo's `target/debug/incremental` first and never another project's files.
After moving the repo folder, run `uv sync --project sdk/python --group dev --locked --reinstall`.

## Working agreements

- Other repositories depend on these crates by path: ejm-cli and ejm-ide. Do not edit them
  from here. Tell the owning session about any removed, renamed, or retyped public item, and
  check that they still build. See the hosts skill.
- Other sessions may be editing this repository at the same time, notably `site/`. Check
  `git status` before editing or committing, and commit only paths you changed.
- A failed or skipped gate step is unverified, not green. Say which commands ran and on
  which platform. Never bypass the pre-commit hook with `--no-verify`.
- Fix the cause of a lint, deny, or type finding. A dependency exception is the last
  resort and names the exact crate, version, and upstream constraint.
- Claims about performance, platforms, or availability cite evidence and its limits. This is
  a source release. Nothing is published or packaged yet.

## Project skills

Load the skill for the part of the system being changed. Load more than one only
when the change crosses those boundaries.

- [harness-engine](.agents/skills/harness-engine/SKILL.md): driver, providers, policies, history, and recovery.
- [harness-auth](.agents/skills/harness-auth/SKILL.md): provider sign-in, credential storage, identity, renewal, and logout.
- [harness-execution](.agents/skills/harness-execution/SKILL.md): admission, command supervision, output, and native guarantees.
- [harness-coding](.agents/skills/harness-coding/SKILL.md): native file tools, shell tool, path safety, grants, and output limits.
- [harness-sdk](.agents/skills/harness-sdk/SKILL.md): worker protocol, generated types, and TypeScript or Python facades.
- [harness-packages](.agents/skills/harness-packages/SKILL.md): portable manifests, installation, grants, language hosts, and extension adapters.
- [harness-registry](.agents/skills/harness-registry/SKILL.md): Cloudflare registry, publisher accounts, immutable versions, login, moderation, and security checks.
- [harness-performance](.agents/skills/harness-performance/SKILL.md): latency, memory, concurrency, and measured claims.
- [harness-testing](.agents/skills/harness-testing/SKILL.md): choosing and running focused tests and fixtures, and reporting what a run proves.
- [harness-review](.agents/skills/harness-review/SKILL.md): reviewing a patch for correctness, bounds, recovery, boundaries, and honest claims.
- [harness-dependencies](.agents/skills/harness-dependencies/SKILL.md): toolchain and tool pins, locks, cargo-deny exceptions, and local build problems.
- [harness-hosts](.agents/skills/harness-hosts/SKILL.md): public API and rename coordination with ejm-cli and ejm-ide.
- [harness-site](.agents/skills/harness-site/SKILL.md): the static harness.ejmlabs.com site, the ejmlabs design kit, headers, and deployment.
- [harness-maintenance](.agents/skills/harness-maintenance/SKILL.md): architecture decisions, strict gates, and updating these rules from evidence.

## OpenSpec skills

These six generated workflow skills retain OpenSpec's upstream format. The policy
gate checks their metadata, routing, and local references. Harness-owned skills
also follow the repository's concise authoring rules.

- [openspec-explore](.agents/skills/openspec-explore/SKILL.md): investigate and clarify a change.
- [openspec-propose](.agents/skills/openspec-propose/SKILL.md): create its planning artifacts.
- [openspec-update-change](.agents/skills/openspec-update-change/SKILL.md): revise an existing plan.
- [openspec-apply-change](.agents/skills/openspec-apply-change/SKILL.md): implement and verify tracked tasks.
- [openspec-sync-specs](.agents/skills/openspec-sync-specs/SKILL.md): update the main specifications.
- [openspec-archive-change](.agents/skills/openspec-archive-change/SKILL.md): archive completed work.

## Done means

The change works for a user, `./scripts/check` passed (say if you did not run it, and why),
docs and skills match the behavior, host repositories still build if a public item changed,
and the report states what was verified and what was not. Commit, push, publish, and deploy
only when the user asks.
