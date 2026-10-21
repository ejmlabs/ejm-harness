---
title: Developer guide
lead: Extend public engine ports and check a change across Rust and both SDKs.
group: ejm-harness
order: 3
slug: developer-guide
---

## Set up {#setup}

Read [CONTRIBUTING.md](https://github.com/ejmlabs/ejm-harness/blob/main/CONTRIBUTING.md), [AGENTS.md](https://github.com/ejmlabs/ejm-harness/blob/main/AGENTS.md) and the [technical contributor guide](https://github.com/ejmlabs/ejm-harness/blob/main/docs/contributing.md). Use Rust 1.99.1 with rustfmt and Clippy, Node 24 or later, Python 3.11 or later and uv.

Linux builds also need `pkg-config` and D-Bus development headers. The source guide describes native key-service requirements and platform limits.

```sh
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2
cargo install --locked --jobs 2 --version 0.20.2 cargo-deny
cargo install --locked --jobs 2 --version 0.9.2 cargo-machete
uv tool install pre-commit==4.3.0
npm ci --ignore-scripts
npm --prefix registry ci --ignore-scripts
uv sync --project sdk/python --group dev --locked
./scripts/install-hooks
```

Keep Cargo jobs at most two on constrained machines. If the checkout moves, refresh its Python environment with `uv sync --project sdk/python --group dev --locked --reinstall`.

## Find the owner {#map}

<!-- table: Repository ownership -->

| Path                              | Responsibility                                                          |
| --------------------------------- | ----------------------------------------------------------------------- |
| `crates/ejm-harness`              | Sessions, driver, providers, context, events, persistence and recovery. |
| `crates/ejm-harness-exec`         | Shared governor, admission, leases, commands and bounded output.        |
| `crates/ejm-harness-auth`         | Identity, credential sources, protected storage and renewal.            |
| `crates/ejm-harness-coding`       | Native file tools and explicitly granted shell tools.                   |
| `crates/ejm-harness-worker`       | Versioned JSONL bridge over the public engine.                          |
| `crates/ejm-harness-package-spec` | Portable manifests and archive validation.                              |
| `crates/ejm-harness-packages`     | Package installation, registry transport and shared language hosts.     |
| `sdk/typescript`, `sdk/python`    | Typed worker facades and callback lifecycles.                           |
| `registry/`                       | Public registry Worker and private Better Auth Worker.                  |
| `site/`, `docs/`                  | Public guides and source contracts.                                     |

ejm-cli and ejm-ide are separate hosts. They own terminal, editor, account and OS interactions. The engine must not depend on either host or a frontend framework. Shared presentation belongs in ejm-ui; consumers use its public exports.

## Embed the Rust engine {#rust}

Add a path dependency on `crates/ejm-harness` from your host project. This example also needs `anyhow` and Tokio with macros and runtime support. It uses the deterministic local replay provider and makes no model request.

```rust
use std::sync::Arc;
use ejm_harness::{Harness, HarnessConfig, ReplayProvider, SessionOptions};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let harness = Harness::new(
        HarnessConfig::default(),
        Arc::new(ReplayProvider::echo()),
        vec![],
    )?;
    let session = harness.create_session(SessionOptions {
        cwd: std::env::current_dir()?,
        title: "First session".into(),
    }).await?;
    let mut run = harness.start(&session.id, "Hello".into()).await?;
    println!("{}", run.wait().await?.text);
    harness.shutdown().await?;
    Ok(())
}
```

Use `ejm-harness-coding` for standard coding tools. Configure a real provider when the host should call a model. See the [integration contract](https://github.com/ejmlabs/ejm-harness/blob/main/docs/implementation-contract.md) for public types and error conditions.

<!-- table: Public extension ports -->

| Port            | Owns the choice                                         |
| --------------- | ------------------------------------------------------- |
| `Provider`      | Model transport and streaming.                          |
| `Tool`          | Schema, effect and cancellable execution.               |
| `ContextPolicy` | Selection of bounded model context.                     |
| `RunStrategy`   | The next bounded driver step.                           |
| `Hook`          | Ordered run, context, tool and completion interception. |

Use these ports without copying the driver. The engine keeps validation, grants, budgets and persistence when a host supplies an extension.

## Keep ownership shared {#ownership}

Sessions share the harness provider, tool registry, context policy, strategy, governor and store. Every tool, command and child agent must pass through the same governor. A lease keeps its reservation until actual work stops, including cancellation cleanup.

Model requests and local jobs have separate admission limits. HTTP model waiting does not allocate a process per session. Reservations and sampling do not establish hard containment; report each restriction as enforced, monitored or unsupported.

Mutating tools serialize by canonical session working directory. Independent writers need separate checkouts. Overlapping roots are not transactional, and external editors can still race file preconditions.

See [architecture](https://github.com/ejmlabs/ejm-harness/blob/main/docs/architecture.md) and [execution guarantees](https://github.com/ejmlabs/ejm-harness/blob/main/docs/executor.md) before changing this model. Use focused owner tests and affected consumer contracts when changing a public API.

## Preserve recovery {#recovery}

The bounded SQLite writer journals tool intent before invocation. Results close the call. Recovery records interrupted work and never repeats an uncertain effect.

Events have increasing sequence numbers. Broadcasts are hints; readers recover from lag through bounded journal pages. If completion cannot be persisted, stop admitting effects and surface failure. Do not hide storage failures or retry external work automatically.

## Keep the protocol aligned {#protocol}

Protocol v1 uses UTF-8 JSON Lines on stdin and stdout; diagnostics use stderr. The worker and SDKs bound requests, output, callbacks and replay pages. Custom callbacks keep their reservation and workspace lock until cleanup ends.

Rust protocol types are authoritative. After changing them, generate the schema and both language contracts together:

```sh
python3 scripts/generate-protocol.py
python3 scripts/generate-protocol.py --check
```

See the [SDK reference](https://github.com/ejmlabs/ejm-harness/blob/main/docs/sdk.md) for handshake, limits, cancellation and disconnect behavior. Tests use local echo or replay providers rather than paid models.

## Add a package {#extensions}

Packages can expose tools, hooks, providers, commands and widgets. Installation never grants execution. Preserve explicit version trust, runtime bounds and cancellation cleanup. Package code runs as the current user.

Use the [package contract](https://github.com/ejmlabs/ejm-harness/blob/main/docs/packages.md) for manifests, grants, archive validation and shared language hosts. The registry remains a separate service and is not yet deployed.

## Check the change {#checks}

```sh
./scripts/check
```

This is the required repository gate for hooks and CI. It checks crate boundaries, formatting, Clippy, Rust tests and builds, docs, dependency use and advisories, generated contracts, JavaScript and TypeScript, Python and SDK fixtures. It does not call hosted models or publish packages. Registry changes also require `./scripts/check-registry`.

Use focused checks while editing, then run the required gate. Report skipped or failed checks plainly. Platform source checks do not replace real native execution, key-service or hosted-provider acceptance.

## Contribute {#contribute}

Report a reproducible problem, improve a guide or discuss a code change in the [repository](https://github.com/ejmlabs/ejm-harness). Include source revision, platform and redacted error details. Read the contribution policy before implementation; feature and bugfix work follows OpenSpec.

Tests should protect behavior, a public contract or a concrete regression. Keep cleanup within scope and preserve other contributors' work. Never attach tokens, private project content or unredacted provider logs.

## Build this website {#website}

The site uses Vite, React and public ejm-ui packages for layout, tokens, motion, docs, demos and prerendering. There is no local design system. Copy stays in `site/content`; shared packages compile and render it.

Clone ejm-ui as a sibling checkout and use the exact revision in `.github/ui-source.json`. Install its locked root dependencies, then check this site from the harness repository root:

```sh
npm --prefix ../ejm-ui ci --ignore-scripts
npm --prefix site ci --ignore-scripts
npm --prefix site run check
npm --prefix site run budget
```

The build writes `site/dist`. `site/wrangler.jsonc` serves those static assets at `harness.ejmlabs.com`, separately from the registry. Public deployment requires the existing review and publication gates. Keep `/docs/host-auth/` and its published anchors working when changing navigation.
