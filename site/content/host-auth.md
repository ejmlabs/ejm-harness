---
title: Host authentication
lead: Account and credential contracts for Rust hosts.
group: ejm-harness
order: 4
slug: host-auth
---

## Availability {#status}

Rust hosts own account controls and use the authentication crate through its public APIs. The SDK worker accepts API-key provider configuration; it has no subscription sign-in interface. Start with the [authentication contract](https://github.com/ejmlabs/ejm-harness/blob/main/docs/auth.md) and the [user guide](/docs/user-guide/#credentials).

## Key service setup {#setup}

Native credential storage depends on the operating system's key service. Read the source guide for setup, errors and recovery before selecting a persistent or memory mode. Source tests do not certify every native key service.

## Credential storage {#vault}

The maintained [credential storage guide](https://github.com/ejmlabs/ejm-harness/blob/main/docs/credential-vault.md) defines storage APIs, privacy checks, locking and limits. Keep credentials out of arguments, logs and session text. Host adapters should use the shared contract rather than introduce another credential format.

## Migration and recovery {#migration}

Follow the [migration contract](https://github.com/ejmlabs/ejm-harness/blob/main/docs/auth.md) before opening older state. Stop older clients first. Retain recovery markers and inspect errors before restoring or replacing credentials. Do not assume a copied database is a portable credential backup.

## Official-client integration {#claude}

The [official-client source contract](https://github.com/ejmlabs/ejm-harness/blob/main/docs/claude-code.md) covers setup, account checks, process ownership and limits for hosts that choose this integration. Use the supported client and its own sign-in flow. Read its verification limits before relying on live account behavior.

## Provider changes {#provider}

Hosts change providers through the public engine contract while work is idle. Keep provider bindings nonsecret and preserve failure behavior. See [provider configuration](https://github.com/ejmlabs/ejm-harness/blob/main/docs/implementation-contract.md#engine-public-api).

## Run completion {#completion}

Keep the returned run handle and wait for its completion. A journal event does not establish that all host cleanup has ended. Hosts must still validate current authority before taking another action. Read the [run-handle contract](https://github.com/ejmlabs/ejm-harness/blob/main/docs/implementation-contract.md#engine-public-api).

## Verification {#verification}

The [verification record](https://github.com/ejmlabs/ejm-harness/blob/main/docs/host-auth-verification.md) retains source identities, prior failures and acceptance limits. Local fixtures, hosted checks, real sign-in, native platform acceptance, package publication and docs deployment are separate results.

This URL and its existing anchors remain available for links from earlier guides. Use the linked source contracts for complete API, migration and operational detail.
