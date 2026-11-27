# ejm-harness

## Overview

ejm-harness is a reusable Rust engine for coding agents. Run sessions through
one shared resource governor, write an event log to disk, and extend the engine
from Rust, TypeScript, or Python. [ejm-cli](https://github.com/ejmlabs/ejm-cli)
and [ejm-ide](https://github.com/ejmlabs/ejm-ide) are separate hosts. The engine
runs locally without a hosted workspace account or server.

## Status

This repository holds development source at version 0.1.1. Published packages,
prebuilt workers, and a live extension registry do not exist yet. Local test
fixtures use replay providers and make no paid model requests. Read
[the changelog](CHANGELOG.md) and [the release policy](RELEASING.md) for release
status and gates.

## Requirements

- Rust 1.99.1 with rustfmt and Clippy. `rust-toolchain.toml` pins this toolchain.
- Node 24 or later for repository work and TypeScript extensions. The TypeScript
  SDK runs on Node 20 or later with a compatible Rust worker.
- Python 3.11 or later for the Python SDK, plus uv for development tools.

Saved credentials use macOS Keychain, Windows Credential Manager, or an unlocked
Linux Secret Service. Linux builds also need `pkg-config` and D-Bus development
headers. To use the official Claude Code provider, install its unmodified client
separately.

Rust hosts do not need Node or Python. The [contributor guide](docs/contributing.md)
lists every tool the full quality gate needs.

## Quick start

Build the worker from a source checkout and run the local TypeScript example:

```sh
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2
cargo build --locked -p ejm-harness-worker
npm ci --ignore-scripts
npm run build --workspace ejm-harness
export EJM_HARNESS_WORKER_PATH="$PWD/target/debug/ejm-harness-worker"
node examples/sdk_typescript.mjs
```

The default replay provider answers `Demo: ...` with no credentials or paid
model requests. Installing the SDK does not download a worker.

## Usage

From source, depend on `crates/ejm-harness` in your Rust project. Add
`ejm-harness-coding` if your host needs standard file tools.

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

The replay provider answers `Demo: Hello`. Hosted providers support Chat
Completions, Anthropic Messages, OpenAI Responses, and an official Claude Code
subscription adapter. The host supplies credentials and tool grants. Use
`Provider`, `Tool`, `ContextPolicy`, and `RunStrategy` to swap behavior at engine
boundaries.

- [TypeScript SDK](sdk/typescript/README.md)
- [Python SDK](sdk/python/README.md)
- [Host integration](docs/ide-integration.md)
- [Authentication](docs/auth.md)
- [Shared credential vault](docs/credential-vault.md)
- [Official Claude Code provider](docs/claude-code.md)
- [Host authentication guide](https://harness.ejmlabs.com/docs/host-auth/)
- [Host authentication verification](docs/host-auth-verification.md)
- [Extension packages and trust](docs/packages.md)

## Development

Read [AGENTS.md](AGENTS.md) and [the contributor guide](docs/contributing.md).
Feature and bugfix work follows the OpenSpec workflow.

```sh
./scripts/check
./scripts/check-registry
```

`./scripts/check` checks Rust code, dependency policy, generated protocols, and
both SDKs. `./scripts/check-registry` checks the Cloudflare workers, schema,
auth fixtures, and registry dependencies. Neither gate publishes or deploys
anything. Report failed or unavailable steps as unverified.

## Documentation

| Area                               | Guide                                                   |
| ---------------------------------- | ------------------------------------------------------- |
| Design and crate ownership         | [Architecture](docs/architecture.md)                    |
| Public host APIs                   | [Integration contract](docs/implementation-contract.md) |
| SDK protocol and recovery          | [SDK guide](docs/sdk.md)                                |
| Shared admission and native limits | [Execution guarantees](docs/executor.md)                |
| Context, recovery, and evaluation  | [Engineering decisions](docs/harness-engineering.md)    |
| Measured capacity and limits       | [Benchmark evidence](docs/benchmarks.md)                |
| Undeployed registry service        | [Registry runbook](registry/README.md)                  |

## Contributing

Start with [the contribution policy](CONTRIBUTING.md), then follow
[the setup and quality rules](docs/contributing.md). Each change needs
maintainer admission, current approval, and passing checks before it merges.

## Security

Read [SECURITY.md](SECURITY.md) for supported versions and how to report security
bugs privately. Coding tools reject protected repository metadata and harness
state, including symlinks. A repository cannot grant itself a tool.

Native shell commands and enabled extensions run as the current user without
kernel sandboxing. Resource reservations and sampled process usage do not prove
kernel isolation. Read [execution guarantees](docs/executor.md) before you grant
native access.

## Support

Use [SUPPORT.md](SUPPORT.md) for questions, bug reports, and support scope. Keep
credentials and private session data out of public issues.

## License

This code uses the [MIT](LICENSE) license. Bundled third-party files keep their
own license notices. Both SDK packages include the MIT license text.
