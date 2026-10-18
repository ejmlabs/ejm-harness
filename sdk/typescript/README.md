# ejm-harness for TypeScript

A typed, dependency-free host for the persistent Rust `ejm-harness-worker`. One worker serves many sessions with a shared governor.

Build the worker and SDK from the repository root:

```sh
cargo build --release -p ejm-harness-worker
npm ci
npm run build --workspace ejm-harness
export EJM_HARNESS_WORKER_PATH="$PWD/target/release/ejm-harness-worker"
node examples/sdk_typescript.mjs
```

```ts
import { Harness } from "ejm-harness";

const harness = await Harness.create({ state_dir: ".ejm-harness" });
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

Default responses come from a demonstration provider and begin with `Demo:`. Configure a hosted provider for real model work. `defineTool<T>()` registers a typed host tool with JSON Schema validation, progress, and an `AbortSignal`. Runs support `wait`, `cancel`, `steer`, `queue`, and async iteration.

Load a trusted package by naming its root and granting its exports:

```ts
const harness = await Harness.create({
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

Requires Node 20 or newer and a compatible worker binary, supplied through `workerPath`, `EJM_HARNESS_WORKER_PATH`, or `PATH`. This source release includes no prebuilt workers and performs no installation-time downloads. See [the SDK guide](../../docs/sdk.md) for bounds, recovery, effect grants, and cooperative cancellation.
