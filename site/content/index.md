---
title: Documentation
lead: Build an agent host, add tools, and keep work bounded.
group: ejm-harness
order: 1
slug:
---

## Choose your path {#choose}

ejm-harness is a Rust engine for coding agents, open source under [MIT](https://github.com/ejmlabs/ejm-harness/blob/main/LICENSE). Rust hosts embed the libraries. TypeScript and Python hosts share one persistent Rust worker across sessions.

- [User guide](/docs/user-guide/): build from source, run a session, configure providers and recover saved work.
- [Developer guide](/docs/developer-guide/): extend the engine and run contributor checks.
- [Host authentication](/docs/host-auth/): find the Rust account and credential contracts.

For a terminal application, use [ejm-cli](https://cli.ejmlabs.com/docs/). For a desktop editor, use [ejm-ide](https://ide.ejmlabs.com/docs/).

## What is available {#availability}

The repository contains the Rust crates, worker and language SDKs. Build from source with the pinned toolchain. Prebuilt workers and published crates, npm and Python packages are not available. The extension registry is not live.

No ejmlabs account or server is required. The local `echo` provider is the default; it returns demonstration text without calling a model. `replay` supplies deterministic responses for tests. Real model work needs a configured provider and credentials.

## Run locally {#first-run}

1. Clone [ejmlabs/ejm-harness](https://github.com/ejmlabs/ejm-harness).
2. Follow the [source requirements](/docs/user-guide/#requirements) for your host.
3. Run the [offline first session](/docs/user-guide/#first-run).
4. Add a [real provider](/docs/user-guide/#providers) and grant only the tools your host needs.

## One engine, three languages {#languages}

Rust hosts use `Harness`, `Provider` and `Tool` directly. TypeScript and Python use the same versioned worker protocol. The SDKs have no runtime package dependencies and do not download executables during installation.

<!-- table: Language interfaces -->

| Language   | Start here                                          |
| ---------- | --------------------------------------------------- |
| Rust       | [Embed the engine](/docs/developer-guide/#rust)     |
| TypeScript | [Build the SDK](/docs/user-guide/#typescript-setup) |
| Python     | [Load the SDK](/docs/user-guide/#python-setup)      |

## Know the boundaries {#boundaries}

The engine owns admission, run limits, tool validation and durable history. Hosts supply credentials, tool grants and their own interface. Repository content cannot grant tools.

Native commands, callbacks and enabled package code run as the current user. They are not sandboxed. Restrictions are reported as enforced, monitored or unsupported. Read the [execution guarantees](https://github.com/ejmlabs/ejm-harness/blob/main/docs/executor.md) before relying on a restriction.

Recovery records interrupted work and never repeats a tool whose outcome is uncertain. [Inspect saved state](/docs/user-guide/#recovery) before starting new work.

## References {#references}

<!-- table: Source contracts -->

| Need                      | Reference                                                                                               |
| ------------------------- | ------------------------------------------------------------------------------------------------------- |
| Integrate a Rust host     | [Public API contract](https://github.com/ejmlabs/ejm-harness/blob/main/docs/implementation-contract.md) |
| Use an SDK                | [Protocol, bounds and recovery](https://github.com/ejmlabs/ejm-harness/blob/main/docs/sdk.md)           |
| Add a package             | [Manifests, grants and runtime](https://github.com/ejmlabs/ejm-harness/blob/main/docs/packages.md)      |
| Add sign-in               | [Authentication contract](https://github.com/ejmlabs/ejm-harness/blob/main/docs/auth.md)                |
| Understand design choices | [Engine guide](https://github.com/ejmlabs/ejm-harness/blob/main/docs/harness-engineering.md)            |
| Contribute                | [Setup and quality checks](https://github.com/ejmlabs/ejm-harness/blob/main/docs/contributing.md)       |
