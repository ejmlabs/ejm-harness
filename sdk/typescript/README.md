# ejm-harness for TypeScript

<a href="https://harness.ejmlabs.com/"><img src="https://harness.ejmlabs.com/android-chrome-512.png" alt="Harness" width="96" height="96" /></a>

A typed, dependency-free host for the persistent Rust `ejm-harness-worker`. One worker serves many sessions with a shared governor.

Build the worker and SDK from the repository root:

```sh
cargo build --release -p ejm-harness-worker
npm ci --ignore-scripts
npm run build --workspace ejm-harness
export EJM_HARNESS_WORKER_PATH="$PWD/target/release/ejm-harness-worker"
node examples/sdk_typescript.mjs
```

In PowerShell, set
`$env:EJM_HARNESS_WORKER_PATH = "$PWD/target/release/ejm-harness-worker.exe"`.
The default name on `PATH` is `ejm-harness-worker.exe` on Windows and
`ejm-harness-worker` on macOS and Linux. Explicit `workerPath` and environment
paths are used as supplied. The
[SDK guide](https://github.com/ejmlabs/ejm-harness/blob/main/docs/sdk.md) records
native setup and platform verification limits.

Set `OPENAI_API_KEY` and `OPENAI_MODEL` through your host configuration before
running the example. It sends requests to that model and uses its billing.

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
    const session = await harness.createSession();
    const run = await session.start("Hello");
    for await (const event of run) {
        if (event.type === "text_delta") process.stdout.write(event.text);
    }
    console.log((await run.wait()).status);
} finally {
    await harness.close();
}
```

Configure an HTTP or trusted package provider explicitly. Missing configuration
returns a setup error. The worker no longer accepts `echo` or `replay` provider
descriptors. `defineTool<T>()` registers a typed host tool with JSON Schema validation,
progress, and an `AbortSignal`. Runs support `wait`, `cancel`, `steer`, `queue`, and
async iteration.

Load a trusted package by naming its root and granting its exports:

```ts
const harness = await Harness.create({
    provider: {
        kind: "responses",
        api_key: process.env.OPENAI_API_KEY ?? "",
        model: process.env.OPENAI_MODEL ?? "",
    },
    packages: [
        {
            root: "/path/to/trusted-package",
            grants: { tools: ["check_project"], commands: ["report"], widgets: ["status"] },
        },
    ],
});
const session = await harness.createSession();
console.log(await harness.listPackages());
console.log(await session.invokePackageCommand("@owner/package", "report", {}));
console.log(await session.renderPackageWidget("@owner/package", "status", {}));
await harness.close();
```

Package hooks use the same run loop as Rust hooks. A package provider requires an explicit provider grant and `provider: { kind: "package", package: "@owner/package", name: "model" }`. Packages share one process per language. TypeScript package sources require Node 24 or newer on the worker's `PATH`. Package code runs with native host access. Grants select registered exports and do not contain its operating system access. `reloadPackage()` requires idle runs and the same manifest and grants; changes to registration require a new worker. Widget methods return data for the caller to render.

Package authors export named functions from their entry module. `ExtensionContext` types the second argument, with snake_case metadata, `signal`, `progress()`, and `delta()`. `toolOutput(content, { data, isError, artifact })` builds a tool result. Bundle dependencies with the package; installation does not run npm or a custom bootstrap.

One hundred runs can wait concurrently without caller batching. The SDK admits at most 56 ordinary requests and reserves eight worker slots for cancellation and shutdown. Ordinary requests wait in a queue of at most 256 frames and 4 MiB. Controls have a separate queue of at most eight frames and 1 MiB. Request deadlines include admission time. A sent request timeout closes the transport because its effect may still be running.

Callback cancellation is cooperative. A callback holds its capacity and effect guard until its handler settles, including cleanup. If it does not settle within five seconds after cancellation, the transport closes and rejects further work. Deadlines and cancellation require a responsive Node event loop. JavaScript cannot forcibly stop a host handler, and uncertain effects are never retried automatically.

Requires Node 20 or newer and a compatible worker binary, supplied through `workerPath`, `EJM_HARNESS_WORKER_PATH`, or `PATH`. This source release includes no prebuilt workers and performs no installation-time downloads. See [the SDK guide](https://github.com/ejmlabs/ejm-harness/blob/main/docs/sdk.md) for bounds, recovery, effect grants, and cooperative cancellation.

## Capacity and session views

On macOS, `Harness.create` selects automatic shared capacity by default.
Elsewhere, omission retains legacy limits pending native acceptance. Pass
`capacity: { mode: "auto" }` to select auto, `{ mode: "fixed" }` for coordinated
fixed targets, or `capacity: null` for legacy composition. Optional policy
ceilings include `max_sessions`, `max_model_requests`, `max_jobs`, and
`max_memory_bytes`; explicit legacy limits still apply. `capacityStatus()`
returns cached host targets and observations, or `null` for legacy composition.
`resetCapacityLearning()` clears aggregate profiles and returns their count;
it leaves grants, credentials, and sessions intact.

`await harness.schedulingStatus()` returns cached local managed, runnable,
waiting and callback counts through the additive `scheduling` worker capability.
It works with both legacy and coordinated workers, and leaves `status()`
unchanged. Runnable includes hooks and journal work; callback counts can include
internal admission and do not prove native cleanup. See
[field meanings](https://github.com/ejmlabs/ejm-harness/blob/main/docs/host-capacity.md#cached-scheduling-status).

HTTP providers can also set `provider_capacity` with a stable host-assigned
`account_uuid`, optional quota `family` (default `shared`), and optional
`max_in_flight`, `requests`, and `tokens` limits. Rates use `{ limit, window_ms }`.
This requires explicit coordinated capacity and a current worker. Reuse the UUID
across participating processes sharing one account quota; never derive it from
credentials. Unknown provider hints remain unknown, and external clients are
outside this local accounting.

Use `sessionsPage({ limit: 50 })` and its opaque `next` cursor to browse the
full session set, and `resolveSession(prefix)` for an unambiguous identity.
`session.view({ max_events: 0 })` reads current authority without transcript
history. `session.eventsPage({ through, after })` replays through a captured
durable cursor, bounded by 200 events and 256 KiB per page. Advance by the
returned `cursor` until `complete`; inspect `omitted` for excluded history.
An oversized event leaves the cursor before that event. Increase `max_bytes`
within 256 KiB, or use a fresh view and show its omission marker.
These additive methods require a current worker. See the
[SDK guide](https://github.com/ejmlabs/ejm-harness/blob/main/docs/sdk.md#capacity-and-bounded-session-views)
for all ceilings, failure behavior, and recovery limits.

## License

Licensed under [MIT](LICENSE). The package includes the full license text.
