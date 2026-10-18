# ejm-harness SDKs

ejm-harness is a reusable Rust agent runtime. Its TypeScript and Python SDKs own one persistent `ejm-harness-worker` process and share that worker across sessions. ejm-cli is a separate application that links the same Rust libraries.

The SDKs have no runtime package dependencies. They require a compatible `ejm-harness-worker` binary on `PATH`, in `EJM_HARNESS_WORKER_PATH`, or at the explicit worker path. This source release does not bundle platform binaries or download executables during installation.

## Build and run

From the ejm-harness repository:

```sh
cargo build --release -p ejm-harness-worker
export EJM_HARNESS_WORKER_PATH="$PWD/target/release/ejm-harness-worker"
npm ci
npm run build --workspace ejm-harness
node examples/sdk_typescript.mjs
PYTHONPATH=sdk/python python3 examples/sdk_async.py
```

Node 20 or newer and Python 3.11 or newer are supported by the SDK source. Tests use the locally available runtimes. Building the Rust worker requires the toolchain listed in the workspace manifest.

The default provider is an explicit demonstration provider. It returns `Demo: ...` and does not inspect or edit a repository. To run a coding agent, configure a provider and opt in to write or shell tools as needed. Credentials travel over stdin, never through command arguments. The worker does not log them, and configuration debug output redacts API keys.

## Provider configuration

Both SDKs accept the same `provider` configuration:

| `kind`      | Fields                                          | Transport                          |
| ----------- | ----------------------------------------------- | ---------------------------------- |
| `echo`      | None                                            | Local demonstration                |
| `replay`    | `responses`                                     | Local recorded responses           |
| `openai`    | `base_url`, `api_key`, `model`                  | OpenAI compatible Chat Completions |
| `responses` | `api_key`, `model`, optional `reasoning_effort` | Public OpenAI Responses            |
| `anthropic` | `api_key`, `model`                              | Public Anthropic Messages          |

`responses` uses `https://api.openai.com/v1` with `store: false` and streaming enabled. It retains bounded encrypted reasoning between tool turns. `anthropic` uses `https://api.anthropic.com/v1`. These two variants have fixed endpoints and use API keys. The worker does not provide subscription sign-in UI.

`reasoning_effort` accepts `none`, `minimal`, `low`, `medium`, `high`, `xhigh`, `max`, or `ultra`. A provider or model can support a narrower set. An omitted value uses the provider default. The two native variants reject an empty model, empty or malformed API key, or unknown effort before a request can start. Creating a harness or session does not contact a model provider.

For example, pass this object as `provider` in the TypeScript SDK:

```ts
{
    kind: "responses",
    api_key: process.env.OPENAI_API_KEY ?? "",
    model: process.env.OPENAI_MODEL ?? "",
    reasoning_effort: "high",
}
```

Use the same fields in a Python dictionary. For Anthropic, set `kind: "anthropic"` and supply an Anthropic API key and model. Read credentials from the host's protected configuration, and keep them out of application logs and session data.

## TypeScript

```ts
import { Harness } from "ejm-harness";

const harness = await Harness.create({
    state_dir: ".ejm-harness",
    provider: { kind: "echo" },
});
try {
    const session = await harness.createSession({ cwd: process.cwd() });
    const run = await session.start("Hello");
    for await (const event of run) {
        if (event.type === "text_delta") process.stdout.write(event.text);
    }
    const result = await run.wait();
    console.log(result.status);
} finally {
    await harness.close();
}
```

`Session.start(prompt, { signal })` accepts an `AbortSignal`. Runs also expose `cancel()`, `steer(text)`, and `queue(text)`. Each sends the run ID so a stale handle cannot control a later run in that session. `wait()` and iteration replay durable events, including a run that finished before the caller subscribed. A run's terminal summary remains available after `wait()`.

Use `defineTool<Arguments>({ name, description, inputSchema, effect, execute })` for host tools. The handler receives typed arguments and a context with `sessionId`, `runId`, `callId`, `deadlineMs`, `signal`, and async `progress(message)`. JSON Schema validation occurs in Rust before execution. Return a string or `{ content, data?, is_error?, artifact? }`.

Callbacks run separately from the protocol reader and receive cancellation without waiting for the handler to finish. Their tool reservation and workspace effect lock remain held until the handler has actually stopped. Keep JavaScript handlers asynchronous. CPU work must use a host worker thread or an appropriate external executor. An `AbortSignal` cannot stop synchronous JavaScript or undo an effect.

## Python

The async API fits an existing event loop:

```python
from ejm_harness import AsyncHarness

async with AsyncHarness(config={"state_dir": ".ejm-harness"}) as harness:
    session = await harness.create_session(".")
    run = await session.start("Hello")
    async for event in run:
        if event["type"] == "text_delta":
            print(event["text"], end="", flush=True)
    result = await run.wait()
```

The sync API keeps its protocol reader on a private event loop:

```python
from ejm_harness import Harness

with Harness(config={"state_dir": ".ejm-harness"}) as harness:
    session = harness.create_session(".")
    run = session.start("Hello")
    for event in run:
        if event["type"] == "text_delta":
            print(event["text"], end="", flush=True)
    result = run.wait()
```

Register a `Tool(definition, execute)` through the `tools` argument. Async handlers await `context.progress(message)` and may await `context.wait_cancelled()`. Sync handlers run in a pool of at most four threads, can call `context.report_progress(message)`, and must check `context.cancelled`. The SDK signals cancellation cooperatively and waits for the handler to stop before confirming cleanup. Python cannot forcibly stop a running thread. Host callbacks must cooperate with cancellation and bound their own external work.

Both APIs support session listing, durable event pages, run cancellation, steering, queued turns, and shared resource status. Use `harness.status()` for active agents, active and waiting model requests, and tool reservations. `harness.governor()` returns tool admission counts and reserved budgets.

## Effect grants and recovery

Native tools start read-only. `allow_write` and `allow_shell` enable the corresponding native effects. Registering a callback is an explicit host grant for that tool, including its declared effect. Repository contents cannot register tools or change these grants.

Custom callbacks execute in the host language process. Rust accounts for their declared tool admission reservation, but it does not isolate Python or JavaScript code or police arbitrary processes started by that code. Native command capability reports describe the actual operating system controls separately.

The SDK never retries a request or an interrupted callback automatically. Request deadlines include time waiting for admission. If a request times out after sending, its effect can be uncertain and the transport closes. A timeout before admission sends no effect. On worker disconnection, pending requests fail and callback cancellation is signaled. Reopening the same state directory exposes interrupted runs without invoking their tools again. Inspect the state before starting new work.

Run cancellation returns promptly while callback cleanup can continue. The worker keeps the callback's reservation and workspace effect lock during cleanup. A handler that does not stop within five seconds poisons the transport and closes the worker; that host cannot admit further work. This does not kill an uncooperative Python thread or an external process started by a host callback. Stop that work before opening a new host for the same workspace.

If the journal fills or cannot persist completion, the worker closes the transport so `wait()` fails explicitly. Keep the state directory. Reopening must durably record interrupted work before admitting new effects and may fail if the journal is still full. Increase `max_store_bytes` to allow recovery. The worker preserves history and does not destructively compact it.

## Protocol and bounds

Protocol v1 uses UTF-8 JSON Lines on stdin/stdout. Diagnostics use stderr, which SDKs drain without retaining an unbounded buffer.

| Limit                                           | Value                                          |
| ----------------------------------------------- | ---------------------------------------------- |
| One input or output frame                       | 1 MiB, including newline                       |
| Queued protocol output bytes, in each direction | 4 MiB                                          |
| Worker pending requests                         | 64                                             |
| Worker pending callbacks                        | 64                                             |
| SDK dispatched requests                         | 56 ordinary and 8 reserved for run controls    |
| SDK queued ordinary requests                    | 256 requests and 4 MiB                         |
| SDK queued control requests                     | 8 requests and 1 MiB                           |
| SDK callbacks awaiting actual cleanup           | 64                                             |
| Tool progress message                           | 16 KiB on the wire; Rust retains at most 2 KiB |
| Event page                                      | Up to 64 events and about 512 KiB              |
| Stalled worker output                           | Connection fails after 5 seconds               |

Full event payloads are not buffered in the SDK. Live notifications wake iterators; durable replay supplies ordered event pages. A 250 ms replay fallback covers a dropped live hint. Bounded admission lets callers start and wait for 100 sessions concurrently without batching. Requests beyond the queue's count or byte limit return `busy`. Reserved control slots keep cancellation, steering, queued input, and shutdown responsive under ordinary request load.

Handshake `hello { protocol: 1 }` precedes one successful `configure`. Public methods are `session.create`, `session.list`, `run.start`, `run.cancel`, `run.steer`, `run.queue`, `events.list`, `governor.status`, `harness.status`, and `shutdown`. Run controls accept an optional `run_id`; SDKs always supply it.

The worker can send `tool.invoke` requests while a normal request is pending. Hosts return a normal result or error envelope only after the handler has stopped, including cancellation cleanup. Cancellation uses `tool.cancel`, and host progress uses `tool.progress`. A dropped execution future sends cancellation and retains a bounded callback record with its reservation and effect lock until the host confirms cleanup. If a cancellation notice cannot be queued, the connection closes to cancel all host callbacks.

## Generated contracts and checks

Rust wire types in `crates/ejm-harness-worker/src/protocol.rs` and the public Rust harness types are authoritative. After changing them:

```sh
python3 scripts/generate-protocol.py
python3 scripts/generate-protocol.py --check
```

Generation uses Cargo, `json-schema-to-typescript`, `datamodel-code-generator`,
Prettier, and Ruff. The generators are pinned development dependencies installed
by `npm ci` and `uv sync --project sdk/python --group dev --locked`. They update
the JSON Schema and both language contracts together. Thin adapters preserve
the SDK's public type names and formatting. Handwritten SDK facades use those
generated types.

Tests use the actual local worker with deterministic echo or replay providers. They cover streaming, late subscription, callbacks, progress, cancellation, shutdown, crash recovery without repeated effects, malformed and oversized frames, and 100 managed sessions. This does not establish hosted model quality, every operating system's behavior, or performance on older hardware.
