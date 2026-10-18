# ejm-harness

A reusable Rust engine for coding agents. Run many sessions through one resource
governor, keep a durable event journal, and extend the engine from Rust,
TypeScript, or Python.

[ejm-cli](../ejm-cli/README.md) is the separate reference terminal app. Neither
project requires a Gerry Workspace account or server.

## Build

Requires the pinned Rust toolchain. The repository contains source packages;
registry releases and prebuilt worker binaries have not been published.

```sh
cargo build --release --workspace --locked
cargo test --workspace --locked
```

The main packages are:

| Package              | Responsibility                                                         |
| -------------------- | ---------------------------------------------------------------------- |
| `ejm-harness`        | Sessions, agent loop, providers, context, events, recovery             |
| `ejm-harness-exec`   | Shared resource admission and command supervision                      |
| `ejm-harness-auth`   | Sign-in, verified identity, credential storage, and serialized renewal |
| `ejm-harness-coding` | Native read, list, search, write, patch, and optional shell tools      |
| `ejm-harness-worker` | Persistent JSONL bridge for language SDKs                              |
| `sdk/typescript`     | Typed async API and TypeScript tool callbacks                          |
| `sdk/python`         | Sync and async Python API and tool callbacks                           |
| `ejm-harness-bench`  | Deterministic concurrency and command workloads                        |

## Embed in Rust

During source development, depend on `crates/ejm-harness` from your own Cargo
project. Add `ejm-harness-coding` when your host needs the standard coding tools.

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
    title: "First agent".into(),
}).await?;
let mut run = harness.start(&session.id, "Hello".into()).await?;
println!("{}", run.wait().await?.text);
harness.shutdown().await?;
    Ok(())
}
```

The replay provider replies with `Demo: Hello` and makes no network calls.
`OpenAiProvider` supports Chat Completions compatible streaming APIs, including
DeepSeek endpoints. `AnthropicProvider` supports the Messages streaming API.
`ResponsesProvider` supports native OpenAI Responses through API keys or ChatGPT
sign-in via `ejm-harness-auth`. Real provider credentials remain host configuration.
See [host integration](docs/ide-integration.md) and [authentication](docs/auth.md).
The [harness engineering decisions](docs/harness-engineering.md) record the
source review, context and recovery contracts, and evaluation limits.

Use `Provider`, `Tool`, `ContextPolicy`, and `RunStrategy` to substitute behavior
at the engine boundaries. The engine retains control of grants, budgets,
validation, and journaling. A repository cannot grant itself a tool.

## TypeScript and Python

Both SDKs share one persistent Rust worker across their sessions. Custom tools
remain ordinary functions in the host language, with schema validation,
progress, and cooperative cancellation. There is no Node or Python dependency
for Rust hosts or ejm-cli.

- [TypeScript quick start](sdk/typescript/README.md)
- [Python quick start](sdk/python/README.md)
- [SDK contracts, bounds, and recovery](docs/sdk.md)

## Keep the host responsive

One hundred managed agents is the target workload, using hosted models on four
CPU cores and 8 GB RAM. Agents waiting on models do not need a shell or thread
each. Model requests and local work have separate admission limits.

Defaults allow eight model requests, four local jobs, and one heavy job. Local
jobs share CPU, memory, process, output, queue, and duration reservations.
Native file tools avoid spawning a shell for common repository operations.
Normal Bash semantics are preserved when shell access is granted.

Admission reservations are not kernel isolation. Native execution runs as the
current user. The executor reports restrictions as enforced, monitored, or
unsupported and rejects required unsupported protections before launch.
See [execution guarantees](docs/executor.md) and
[benchmark methods and evidence](docs/benchmarks.md). Results from a modern
machine do not prove the old-laptop target or superiority over another agent.

## Development

Read [AGENTS.md](AGENTS.md) and [the contribution guide](docs/contributing.md).
Project skills under `.agents/skills` route engine, execution, SDK, performance,
and maintenance work. `./scripts/check` is the shared local, hook, and CI gate.
It denies Rust warnings and Clippy all/pedantic, checks architecture, locks,
dependencies, generated protocols, language types, formatting, and fixtures.

- [Architecture and design decisions](docs/architecture.md)
- [Public integration contract](docs/implementation-contract.md)
- [MIT license](LICENSE)
