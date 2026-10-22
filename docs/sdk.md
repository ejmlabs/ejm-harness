# ejm-harness SDKs

ejm-harness is a reusable Rust agent runtime. Its TypeScript and Python SDKs own one persistent `ejm-harness-worker` process and share that worker across sessions. ejm-cli is a separate application that links the same Rust libraries.

The SDKs have no runtime package dependencies. They require a compatible `ejm-harness-worker` binary on `PATH`, in `EJM_HARNESS_WORKER_PATH`, or at the explicit worker path. This source release does not bundle platform binaries or download executables during installation.

## Build and run

Set `OPENAI_API_KEY` and `OPENAI_MODEL` through the host's protected
configuration. The examples send requests to that model and use its billing.
From the ejm-harness repository:

```sh
cargo build --release -p ejm-harness-worker
export EJM_HARNESS_WORKER_PATH="$PWD/target/release/ejm-harness-worker"
npm ci
npm run build --workspace ejm-harness
node examples/sdk_typescript.mjs
PYTHONPATH=sdk/python python3 examples/sdk_async.py
```

PowerShell uses the same Cargo and npm commands, with Windows environment syntax
and the native worker suffix:

```powershell
cargo build --release -p ejm-harness-worker
$env:EJM_HARNESS_WORKER_PATH = "$PWD/target/release/ejm-harness-worker.exe"
npm ci
npm run build --workspace ejm-harness
node examples/sdk_typescript.mjs
$env:PYTHONPATH = "sdk/python"
python examples/sdk_async.py
```

The default executable is `ejm-harness-worker.exe` on Windows and
`ejm-harness-worker` on macOS and Linux. An explicit worker path takes precedence
over `EJM_HARNESS_WORKER_PATH`, then the default name on `PATH`. Keep an explicit
path unchanged, including any suffix. Native lifecycle acceptance is required on
each target; a macOS source check does not establish Windows or Linux support.

Node 20 or newer and Python 3.11 or newer are supported by the SDK source. Tests use the locally available runtimes. Building the Rust worker requires the toolchain listed in the workspace manifest.

## Prepare a native worker archive

`scripts/package.py` prepares local worker archives. It does not publish them or
change the SDK installation route. Build and smoke each target on its own native
host. For example, on an Apple Silicon Mac:

```sh
CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 cargo build --release --locked -p ejm-harness-worker
python3 scripts/package.py --binary target/release/ejm-harness-worker \
  --target aarch64-apple-darwin --output dist/workers --smoke
```

On Windows, run from a Visual Studio developer shell so `dumpbin` is available:

```powershell
$env:CARGO_BUILD_JOBS = "2"
$env:CARGO_INCREMENTAL = "0"
cargo build --release --locked -p ejm-harness-worker
python scripts/package.py --binary target/release/ejm-harness-worker.exe --target x86_64-pc-windows-msvc --output dist/workers --smoke
```

An Intel Mac uses `x86_64-apple-darwin`; Linux x64 GNU uses
`x86_64-unknown-linux-gnu`. Smoke requires the requested target to match
`rustc -vV`'s host. Native inspection uses `lipo` and `otool` on macOS, `readelf`
from binutils on Linux, and MSVC `dumpbin` on Windows. It checks the executable
architecture and records native dependency listings. Successful startup checks
the current host's loader and installed libraries, not older OS or glibc versions.

The archive contains the native worker, MIT license, third-party notices, and
provenance with source revision, dirty status, toolchain, target, lockfile hash,
and binary hash. File data streams through Python's standard archive libraries.
Archive names and contents have fixed timestamps; inputs and compression-library
versions must also match for byte-for-byte reproduction. Existing archives are
never overwritten. A `.sha256` file records the archive checksum.

With `--smoke`, the script extracts outside the checkout and uses private state
and workspace paths containing spaces and Unicode. It checks the handshake, a
local package-provider run, persisted history after restart, and shutdown while stdin stays
open. Success writes an adjacent `.smoke.json` tied to the archive hash. The
fixture creates a temporary Python provider outside the archive, needs Python
3.11 or newer on the worker's `PATH`, uses no credentials and makes no paid calls.
The fixture is not bundled in the archive. Omit `--smoke` only
to prepare an unverified artifact: its provenance states that architecture and
native behavior were not checked.

These are local preparation artifacts. This change does not establish signing,
notarization, publication, or acceptance on every target. Check the
[native evidence reference](native-process-support.md) for actual results. To use
a verified extracted worker, point `EJM_HARNESS_WORKER_PATH` at its absolute path.
Keep the matching SDK version and the platform's listed native libraries.

Configure an HTTP or trusted package provider explicitly. Omission returns a
`configuration` setup error before packages start or session state is created.
The worker rejects the removed `echo` and `replay` provider descriptors with
`invalid_request`; migrate them to a configured provider. Write and shell tools
require explicit grants. Credentials travel over stdin, never through command
arguments. The worker does not log them, and configuration debug output redacts
API keys.

## Provider configuration

Both SDKs accept the same `provider` configuration:

| `kind`      | Fields                                          | Transport                          |
| ----------- | ----------------------------------------------- | ---------------------------------- |
| `openai`    | `base_url`, `api_key`, `model`                  | OpenAI compatible Chat Completions |
| `responses` | `api_key`, `model`, optional `reasoning_effort` | Public OpenAI Responses            |
| `anthropic` | `api_key`, `model`                              | Public Anthropic Messages          |
| `package`   | `package`, `name`                               | Explicitly granted package export  |

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
    provider: {
        kind: "responses",
        api_key: process.env.OPENAI_API_KEY ?? "",
        model: process.env.OPENAI_MODEL ?? "",
    },
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
import os

from ejm_harness import AsyncHarness

async with AsyncHarness(config={
    "state_dir": ".ejm-harness",
    "provider": {
        "kind": "responses",
        "api_key": os.environ.get("OPENAI_API_KEY", ""),
        "model": os.environ.get("OPENAI_MODEL", ""),
    },
}) as harness:
    session = await harness.create_session(".")
    run = await session.start("Hello")
    async for event in run:
        if event["type"] == "text_delta":
            print(event["text"], end="", flush=True)
    result = await run.wait()
```

The sync API keeps its protocol reader on a private event loop:

```python
import os

from ejm_harness import Harness

with Harness(config={
    "state_dir": ".ejm-harness",
    "provider": {
        "kind": "responses",
        "api_key": os.environ.get("OPENAI_API_KEY", ""),
        "model": os.environ.get("OPENAI_MODEL", ""),
    },
}) as harness:
    session = harness.create_session(".")
    run = session.start("Hello")
    for event in run:
        if event["type"] == "text_delta":
            print(event["text"], end="", flush=True)
    result = run.wait()
```

Register a `Tool(definition, execute)` through the `tools` argument. Async handlers await `context.progress(message)` and may await `context.wait_cancelled()`. Sync handlers run in a pool of at most four threads, can call `context.report_progress(message)`, and must check `context.cancelled`. The SDK signals cancellation cooperatively and waits for the handler to stop before confirming cleanup. Python cannot forcibly stop a running thread. Host callbacks must cooperate with cancellation and bound their own external work.

Both APIs support session listing, durable event pages, run cancellation, steering, queued turns, and shared resource status. Use `harness.status()` for active agents, active and waiting model requests, and tool reservations. `harness.governor()` returns tool admission counts and reserved budgets.

For separate managed, runnable, waiting and callback counts, use
`await harness.schedulingStatus()` in TypeScript or `await
harness.scheduling_status()` with `AsyncHarness` in Python. The synchronous
Python `Harness` exposes the same method without `await`. These methods require
the additive `scheduling` worker capability and return `SchedulingSnapshot` for
both legacy and coordinated workers. An older worker returns an update error.
Legacy `status()` and its payload are unchanged. The read uses cached coherent
local counters without journal/host I/O or a scan of runs. Runnable includes
work outside the listed phases, such as hooks and journal writes; service counts
cover callback lifetime, including internal admission and backpressure. They
are not OS thread counts or proof of native descendant cleanup. See
[all fields](host-capacity.md#cached-scheduling-status).

## Capacity and bounded session views

Capacity coordination is opt-in in this source tree. Omit `capacity` to retain
the original fixed limits within one worker. Automatic SDK defaults remain
gated on native owner-crash recovery checks. Opted-in workers coordinate the
same host budget through the public Rust capacity context, prepared before
package imports and reused by the final harness. No separate scheduler runs
inside either SDK.

TypeScript accepts `capacity: { mode: "auto" }` in `Harness.create` options.
Python accepts `config={"capacity": {"mode": "auto"}}`. Use `mode: "fixed"`
for coordinated fixed targets. The optional policy ceilings are `max_sessions`,
`max_model_requests`, `max_jobs`, `max_memory_bytes`, `max_cpu_millis`,
`max_heavy_jobs`, `max_processes`, and `max_output_bytes`. A legacy explicit
`max_sessions`, `max_model_requests`, or `resources` value remains a ceiling;
when both forms are supplied, the smaller value wins. A ceiling does not
promise a grant. Other opted-in workers, host pressure, and observed system
limits can reduce admission. Unsupported scope, invalid ceilings, protected
state failures, and registration failures return a configuration error before
package imports. They do not silently switch to process-local limits.

`harness.capacityStatus()` in TypeScript and `harness.capacity_status()` in
Python return cached targets, hard ceilings, pressure observations, wait and
service timing, and learning status. They return `null` or `None` for legacy
composition. `resetCapacityLearning()` and `reset_capacity_learning()` return
the number of cleared aggregate profiles. Reset leaves current grants, provider
charges, credentials, and session journals intact. Explicit capacity options
and the new inspection methods require a worker that advertises the `capacity`
or `session_views` capability. Older workers return an actionable update error.

For an HTTP provider, optional `provider_capacity` configuration supplies an
`account_uuid` assigned and stored by the host, plus optional `family`,
`max_in_flight`, `requests`, and `tokens`. This requires an explicit `capacity`
mode and a worker with the `provider_capacity` capability. Reuse the UUID for
participating processes sharing the same account quota. Its default family is
`shared`, so changing model names does not split one account's budget. The Rust
worker combines that identity with the selected provider's origin; credentials
never form the identity. Each rate has a positive `limit` and a `window_ms`
between 1000 and 86400000. Package providers reject this HTTP
quota configuration.

Provider rates account for locally tracked requests and tokens from cooperating
processes. Other machines and external clients remain outside that ledger.
Unknown provider hints stay unknown. A local cancellation does not prove that
remote inference stopped, and uncertain rate charges remain until their window
expires. The SDK never retries a failed or uncertain request automatically.

Use `sessionsPage({ limit: 50 })` or `sessions_page({"limit": 50})` for the full
session set. Pass the returned `next` cursor with the same `query` until it is
absent. The worker returns at most 50 sessions per page and may reduce that
count to fit a frame. Cursors are opaque and belong to the original query.
`resolveSession(prefix)` and `resolve_session(prefix)` resolve an exact or
unambiguous identity in the full set. The retained `listSessions()` and
`list_sessions()` methods use the older list contract and may stop at 1000.

Read a consistent authority snapshot with `session.view()` or
`harness.sessionView(id)` in TypeScript; Python provides `session.view()` and
`harness.session_view(id)`. Its active `run_id`, latest durable transition, and
cursors describe current authority even when `events` omits history. Pass zero
for `max_events` or `max_bytes` in the bounds to request authority alone.
Otherwise the worker caps tails and replay pages at 200 events and 256 KiB of
serialized event payload. `omitted` reports excluded history. An individual
oversized event can also be omitted without changing run authority.

Capture `view.durable_cursor`, then use
`session.eventsPage({ after: 0, through: view.durable_cursor })` or
`session.events_page(view["durable_cursor"], after=0)` to replay through that
boundary. Advance `after` to the page's `cursor` until `complete` is true.
If `omitted` is true, the next individual event exceeds the byte bound and the
cursor does not advance past it. Increase `max_bytes` within the worker's
256 KiB cap and retry after the returned cursor. If it still cannot fit, read
a fresh `session.view()` and display its omission marker. Repeating an unchanged
request with the same cursor cannot make progress. Global sequence gaps are valid;
`selected_cursor` identifies selected-session continuity. Neither SDK retains
full event history in memory. These APIs bound host presentation work; they do
not establish thousand-agent throughput or native enforcement on every target.

## Effect grants and recovery

Native tools start read-only. `allow_write` and `allow_shell` enable the corresponding native effects. Registering a callback is an explicit host grant for that tool, including its declared effect. Repository contents cannot register tools or change these grants.

Custom callbacks execute in the host language process. Rust accounts for their declared tool admission reservation, but it does not isolate Python or JavaScript code or police arbitrary processes started by that code. Native command capability reports describe the actual operating system controls separately.

The SDK never retries a request or an interrupted callback automatically. Request deadlines include time waiting for admission. If a request times out after sending, its effect can be uncertain and the transport closes. A timeout before admission sends no effect. On worker disconnection, pending requests fail and callback cancellation is signaled. Reopening the same state directory exposes interrupted runs without invoking their tools again. Inspect the state before starting new work.

Run cancellation returns promptly while callback cleanup can continue. The worker keeps the callback's reservation and workspace effect lock during cleanup. A handler that does not stop within five seconds poisons the transport and closes the worker; that host cannot admit further work. This does not kill an uncooperative Python thread or an external process started by a host callback. Stop that work before opening a new host for the same workspace.

If the journal fills or cannot persist completion, the worker closes the transport so `wait()` fails explicitly. Keep the state directory. Reopening must durably record interrupted work before admitting new effects and may fail if the journal is still full. Increase `max_store_bytes` to allow recovery. The worker preserves history and does not destructively compact it.

## Protocol and bounds

Protocol v1 uses UTF-8 JSON Lines on stdin/stdout. Diagnostics use stderr, which SDKs drain without retaining an unbounded buffer.

| Limit                                           | Value                                            |
| ----------------------------------------------- | ------------------------------------------------ |
| One input or output frame                       | 1 MiB, including newline                         |
| Queued protocol output bytes, in each direction | 4 MiB                                            |
| Worker pending requests                         | 64                                               |
| Worker pending callbacks                        | 64                                               |
| SDK dispatched requests                         | 56 ordinary and 8 reserved for run controls      |
| SDK queued ordinary requests                    | 256 requests and 4 MiB                           |
| SDK queued control requests                     | 8 requests and 1 MiB                             |
| SDK callbacks awaiting actual cleanup           | 64                                               |
| Tool progress message                           | 16 KiB on the wire; Rust retains at most 2 KiB   |
| Event page                                      | Up to 64 events and about 512 KiB                |
| Session view or `events.page` event payload     | Up to 200 events and 256 KiB                     |
| Session query page                              | Up to 50 sessions, further bounded by frame size |
| Stalled worker output                           | Connection fails after 5 seconds                 |

Full event payloads are not buffered in the SDK. Live notifications wake iterators; durable replay supplies ordered event pages. A 250 ms replay fallback covers a dropped live hint. Bounded admission lets callers start and wait for 100 sessions concurrently without batching. Requests beyond the queue's count or byte limit return `busy`. Reserved control slots keep cancellation, steering, queued input, and shutdown responsive under ordinary request load.

Handshake `hello { protocol: 1 }` precedes one successful `configure`. Public methods include `session.create`, `session.list`, `session.page`, `session.resolve`, `session.view`, `run.start`, `run.cancel`, `run.steer`, `run.queue`, `events.list`, `events.page`, `capacity.status`, `capacity.reset`, `governor.status`, `harness.status`, `scheduling.status`, and `shutdown`. Run controls accept an optional `run_id`; SDKs always supply it. New view and capacity operations are additive protocol v1 capabilities.

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

Tests use the actual local worker with temporary deterministic package providers
and local HTTP fixtures. The production worker protocol has no fixture modes.
They cover streaming, late subscription, callbacks, progress, cancellation,
shutdown, crash recovery without repeated effects, malformed and oversized frames,
and 100 managed sessions. This does not establish hosted model quality, every
operating system's behavior, or performance on older hardware.
