# ejm-harness

## Overview

A reusable Rust engine for coding agents. Run sessions through one resource
governor, keep a durable event journal, and extend the engine from Rust,
TypeScript, or Python. [ejm-cli](https://github.com/ejmlabs/ejm-cli) and
[ejm-ide](https://github.com/ejmlabs/ejm-ide) are separate hosts. The engine runs
locally without a hosted workspace account or server.

## Status

This is development source at version 0.1.0. Package registry releases, prebuilt
workers, and a live extension registry have not been published. Local fixtures
use replay providers and make no paid model requests. See [the changelog](CHANGELOG.md)
and [release policy](RELEASING.md) for release status and gates.

## Requirements

- Rust 1.99.1 with rustfmt and Clippy. `rust-toolchain.toml` pins this toolchain.
- Node 24 or later for repository development and TypeScript extensions. The
  TypeScript SDK itself supports Node 20 or later with a compatible Rust worker.
- Python 3.11 or later for the Python SDK, and uv for its development tools.

Persistent credentials use macOS Keychain, Windows Credential Manager, or an
unlocked Linux Secret Service. Linux development builds also need `pkg-config`
and D-Bus development headers. The official Claude Code provider requires its
separately installed unmodified client.

Rust hosts do not require Node or Python. The [contributor guide](docs/contributing.md)
lists the tools needed for the full quality gate.

## Quick start

From a source checkout, build the worker and run a local TypeScript example:

```sh
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2
cargo build --locked -p ejm-harness-worker
npm ci --ignore-scripts
npm run build --workspace ejm-harness
export EJM_HARNESS_WORKER_PATH="$PWD/target/debug/ejm-harness-worker"
node examples/sdk_typescript.mjs
```

The default provider returns `Demo: ...` without credentials or model charges.
The SDK does not download a worker during installation.

## Usage

During source development, depend on `crates/ejm-harness` from a Rust project.
Add `ejm-harness-coding` when the host needs the standard file tools.

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

The replay provider replies with `Demo: Hello`. Hosted providers support Chat
Completions, Anthropic Messages, OpenAI Responses, and an official Claude Code
subscription adapter. Credentials and tool grants
remain host configuration. Use `Provider`, `Tool`, `ContextPolicy`, and `RunStrategy`
to substitute behavior at the engine boundaries.

- [TypeScript SDK](sdk/typescript/README.md)
- [Python SDK](sdk/python/README.md)
- [Host integration](docs/ide-integration.md)
- [Authentication](docs/auth.md)
- [Shared credential vault](docs/credential-vault.md)
- [Official Claude Code provider](docs/claude-code.md)
- [Host authentication guide](site/public/docs/host-auth/index.html)
- [Host authentication verification](docs/host-auth-verification.md)
- [Extension packages and trust](docs/packages.md)

## Development

Read [AGENTS.md](AGENTS.md) and [the technical contributor guide](docs/contributing.md).
Feature and bugfix work uses the initialized OpenSpec workflow.

```sh
./scripts/check
./scripts/check-registry
```

The workspace gate checks Rust, dependency policy, generated protocols, and both
SDKs. The separate registry gate covers the Cloudflare workers, schema, auth
fixtures, and registry dependencies. Gates do not publish or deploy anything.
Report failed or unavailable steps as unverified.

## Documentation

| Area                               | Guide                                                   |
| ---------------------------------- | ------------------------------------------------------- |
| Design and crate ownership         | [Architecture](docs/architecture.md)                    |
| Public host APIs                   | [Integration contract](docs/implementation-contract.md) |
| SDK protocol and recovery          | [SDK guide](docs/sdk.md)                                |
| Shared admission and native limits | [Execution guarantees](docs/executor.md)                |
| Context, recovery, and evaluation  | [Engineering decisions](docs/harness-engineering.md)    |
| Capacity measurements and limits   | [Benchmark evidence](docs/benchmarks.md)                |
| Undeployed registry service        | [Registry runbook](registry/README.md)                  |

## Contributing

Start with [the contribution policy](CONTRIBUTING.md), then follow
[the technical setup and quality rules](docs/contributing.md). Changes need
maintainer admission, current approval, and the required checks before merging.

## Security

Read [SECURITY.md](SECURITY.md) for supported versions and private vulnerability
reporting. Coding tools reject protected repository metadata and harness state,
including symlink aliases. A repository cannot grant itself a tool.

Native shell commands and enabled extension code run as the current user. They
are not sandboxed. Admission reservations and sampled process usage do not prove
kernel isolation. See [execution guarantees](docs/executor.md) before granting
native access.

## Support

Use [SUPPORT.md](SUPPORT.md) for questions, bug reports, and support scope. Keep
credentials and private session data out of public issues.

## License

The code is licensed under [MIT](LICENSE). Bundled third-party assets retain
their own license notices. Both SDK distributions include the MIT license text.
