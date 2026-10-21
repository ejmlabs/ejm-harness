---
title: User guide
lead: Build from source, run a session, and add the provider and tools your host needs.
group: ejm-harness
order: 2
slug: user-guide
---

## Source requirements {#requirements}

Clone [ejmlabs/ejm-harness](https://github.com/ejmlabs/ejm-harness) and run these commands from its root. Keep the SDK and worker from the same checkout. Published packages and prebuilt workers are not available.

<!-- table: Source requirements -->

| Tool                 | Needed for                                                            |
| -------------------- | --------------------------------------------------------------------- |
| Rust 1.99.1          | Rust hosts and the worker. The repository pins this toolchain.        |
| Node 24 or later     | Repository development. The TypeScript SDK supports Node 20 or later. |
| Python 3.11 or later | The Python SDK. Contributor tools also use uv.                        |

Linux builds need `pkg-config` and D-Bus development headers for the native Secret Service adapter. On Debian or Ubuntu, install `pkg-config` and `libdbus-1-dev`. Use the native build tools required by the Rust target on other systems. Read [native process support](https://github.com/ejmlabs/ejm-harness/blob/main/docs/native-process-support.md) for platform verification limits.

Rust hosts [embed the crates](/docs/developer-guide/#rust) and do not need a worker. Language runtimes are needed only for the SDK or packages that use them.

## Build the worker {#build}

```sh
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2
cargo build --locked -p ejm-harness-worker
export EJM_HARNESS_WORKER_PATH="$PWD/target/debug/ejm-harness-worker"
```

On Windows, set `EJM_HARNESS_WORKER_PATH` to the built `ejm-harness-worker.exe`. The SDK accepts an explicit `workerPath` in TypeScript or `worker_path` in Python, then checks `EJM_HARNESS_WORKER_PATH`, then `PATH`. Installation does not download a worker.

### TypeScript setup {#typescript-setup}

```sh
npm ci --ignore-scripts
npm run build --workspace ejm-harness
```

### Python setup {#python-setup}

Load the SDK from this checkout with `PYTHONPATH=sdk/python`. No npm setup is needed. In PowerShell, set `$env:PYTHONPATH = "sdk/python"` before running Python.

## Run your first session {#first-run}

Keep the worker environment variable set. Choose an example below. Both use the offline `echo` provider, create a session, read text events and close the worker. They make no model request and grant no write or shell tools.

Save the TypeScript example as `quickstart.mjs` or the Python example as `quickstart.py` at the repository root.

<!-- tabs: SDK language -->

```js tab="TypeScript" label="quickstart.mjs"
import { Harness } from "./sdk/typescript/dist/index.js";

const harness = await Harness.create({
    state_dir: ".ejm-harness-quickstart-js",
    provider: { kind: "echo" },
});
try {
    const session = await harness.createSession({ title: "First session" });
    const run = await session.start("Hello");
    for await (const event of run) {
        if (event.type === "text_delta") process.stdout.write(event.text);
    }
    console.log(`\n${(await run.wait()).status}`);
} finally {
    await harness.close();
}
```

```python tab="Python" label="quickstart.py"
import asyncio
from ejm_harness import AsyncHarness

async def main() -> None:
    async with AsyncHarness(config={
        "state_dir": ".ejm-harness-quickstart-py",
        "provider": {"kind": "echo"},
    }) as harness:
        session = await harness.create_session(".", title="First session")
        run = await session.start("Hello")
        async for event in run:
            if event["type"] == "text_delta":
                print(event["text"], end="", flush=True)
        result = await run.wait()
        print(f"\n{result['status']}")

asyncio.run(main())
```

<!-- /tabs -->

Run `node quickstart.mjs` or `PYTHONPATH=sdk/python python3 quickstart.py`. The result is `Demo: Hello`, then `completed`. This checks the SDK, worker and journal; it does not inspect or edit a repository.

Python also provides a synchronous `Harness` context manager. See the [Python SDK guide](https://github.com/ejmlabs/ejm-harness/blob/main/sdk/python/README.md).

## Configure a provider {#providers}

Pass a `provider` object to the harness configuration. Creating a harness or session does not contact a model. Starting a real model run can incur provider charges. Read credentials from the host's protected configuration and keep them out of logs and session text.

<!-- table: Worker providers -->

| Kind        | Fields                                          | Use                                            |
| ----------- | ----------------------------------------------- | ---------------------------------------------- |
| `echo`      | None                                            | Offline demonstration. This is the default.    |
| `replay`    | `responses`                                     | Deterministic local responses and tool calls.  |
| `openai`    | `base_url`, `api_key`, `model`                  | A Chat Completions compatible endpoint.        |
| `responses` | `api_key`, `model`, optional `reasoning_effort` | OpenAI Responses at its fixed endpoint.        |
| `anthropic` | `api_key`, `model`                              | Anthropic Messages at its fixed endpoint.      |
| `package`   | `package`, `name`                               | A provider from an explicitly granted package. |

For example, replace the offline provider with this configuration after setting `OPENAI_API_KEY` and `OPENAI_MODEL`:

```js
const harness = await Harness.create({
    state_dir: ".ejm-harness-model",
    provider: {
        kind: "responses",
        api_key: process.env.OPENAI_API_KEY ?? "",
        model: process.env.OPENAI_MODEL ?? "",
    },
});
```

The Python configuration uses the same snake_case fields. Native Responses and Anthropic descriptors validate credentials and model names before a request starts. Responses streams with `store: false`; reasoning effort depends on the provider and model. See the [provider reference](https://github.com/ejmlabs/ejm-harness/blob/main/docs/sdk.md#provider-configuration).

The homepage plays a formatted offline recording from 19 October 2026. No worker or model runs in the browser. Its resource figures are admission reservations, not measured CPU or memory use; its unpinned worker does not establish exact source reproducibility.

## Handle credentials {#credentials}

SDK provider credentials pass to the worker over stdin, never through command arguments. The SDK has no subscription sign-in interface. Hosts own account controls and credential configuration. Rust hosts can use the [authentication APIs](/docs/host-auth/). Consult that contract before migrating local credentials or relying on native key storage.

## Grant tools explicitly {#grants}

Native tools start read-only. Set `allow_write: true` to enable edits and `allow_shell: true` to enable commands. Repository text cannot grant these permissions.

Use `defineTool` in TypeScript or a Python `Tool` through the SDK's `tools` option. Declare the input schema and effect; Rust validates arguments before execution. Registering a callback grants its declared effect.

Callbacks execute in the host language process. Keep Node handlers asynchronous. Python sync handlers use at most four threads and must check cancellation. Bound any external work and wait for cleanup.

> [!WARNING]
> Native commands, callbacks and enabled package code run as the current user. They are not a sandbox. A cancellation signal cannot undo an effect or stop an uncooperative Python thread.

## Control a run {#runs}

Iterate a `Run` to read durable events and call `wait()` for its terminal summary. Finished-before-subscribe runs remain readable. Runs expose `cancel()`, `steer(text)` and `queue(text)`; SDK handles target their own run ID.

Cancellation returns promptly while callbacks clean up. Their resource reservation and workspace effect lock stay held until work stops. A callback that fails to stop within five seconds poisons the transport and closes the worker. Stop uncooperative host work before opening another host for that workspace.

Sessions share one worker, provider, tool registry, governor and store. `harness.status()` reports agent and model work; `harness.governor()` reports tool reservations. Reservations are not kernel-enforced memory or CPU ceilings.

## Read saved history {#history}

Keep the state directory to preserve sessions. TypeScript uses `harness.listSessions()`; Python uses `harness.list_sessions()`. A session's `events(after, limit)` returns a bounded journal page. Advance from the last sequence number to continue reading.

Live notifications are hints; durable replay supplies ordered history. See [protocol limits](https://github.com/ejmlabs/ejm-harness/blob/main/docs/sdk.md#protocol-and-bounds) for frame, queue and event-page bounds.

## Recover interrupted work {#recovery}

The SDK does not retry an interrupted request or tool automatically. A timeout after sending can leave an uncertain effect and closes the transport. Keep the state directory, stop remaining host work and reopen it to inspect interrupted runs before taking new action.

If the journal cannot persist completion, the worker closes so `wait()` fails explicitly. Reopening records interrupted work before admitting effects. If the journal is full, increase `max_store_bytes` to permit recovery. History is not destructively compacted.

Use separate checkout directories for independent writers. Workspace effect locks do not make overlapping filesystem roots transactional or prevent an external editor from racing a write.

## Use packages {#packages}

Packages can expose tools, hooks, providers, commands and widgets. Installation does not grant execution. Enable only reviewed versions with explicit grants. Package code runs natively as the current user.

The public registry is not deployed. Use the [package guide](https://github.com/ejmlabs/ejm-harness/blob/main/docs/packages.md) for local packages, manifests, grants and runtime limits.

## Troubleshooting {#troubleshooting}

<!-- table: Common failures -->

| Problem                        | Next step                                                                    |
| ------------------------------ | ---------------------------------------------------------------------------- |
| Worker not found               | Build it, set its absolute path and keep SDK and worker revisions aligned.   |
| `Demo:` response               | The local echo provider is active. Configure a real provider for model work. |
| `busy`                         | Wait for admission or reduce incoming work.                                  |
| Transport closes               | Stop remaining callbacks, retain the state directory and inspect recovery.   |
| Journal is full                | Retain history and increase `max_store_bytes` before reopening.              |
| Native restriction unsupported | Consult the executor capability report; do not assume containment.           |

Report a reproducible problem in [GitHub issues](https://github.com/ejmlabs/ejm-harness/issues). Include source revision, platform and redacted errors. Do not attach tokens, credentials or private project content.
