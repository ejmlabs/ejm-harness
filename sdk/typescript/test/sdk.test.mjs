import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { Harness, defineTool, toolOutput } from "../dist/index.js";

const workerPath =
  process.env.EJM_HARNESS_WORKER_PATH ??
  fileURLToPath(new URL("../../../target/debug/ejm-harness-worker", import.meta.url));
const toolDefinition = {
  name: "sdk_lookup",
  description: "Look up a test value",
  effect: "read",
  version: "1",
  inputSchema: {
    type: "object",
    properties: { value: { type: "string" } },
    required: ["value"],
    additionalProperties: false,
  },
};
const replay = (callId = "sdk-call") => ({
  kind: "replay",
  responses: [
    { tool_calls: [{ id: callId, name: "sdk_lookup", arguments: { value: "hello" } }] },
    { content: "finished" },
  ],
});
const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

async function within(promise, timeoutMs) {
  let timer;
  try {
    return await Promise.race([
      promise,
      new Promise((_resolve, reject) => {
        timer = setTimeout(
          () => reject(new Error(`operation exceeded ${timeoutMs} ms`)),
          timeoutMs,
        );
      }),
    ]);
  } finally {
    clearTimeout(timer);
  }
}

async function fixture(options = {}) {
  const state = await mkdtemp(path.join(os.tmpdir(), "ejm-harness-sdk-ts-"));
  const harness = await Harness.create({ workerPath, state_dir: state, ...options });
  return {
    harness,
    state,
    close: async () => {
      await harness.close();
      await rm(state, { recursive: true, force: true });
    },
  };
}

test("package configuration rejects workers that do not advertise package support", async () => {
  const request = Harness.prototype.request;
  Harness.prototype.request = async function (method, ...args) {
    const result = await request.call(this, method, ...args);
    return method === "hello" ? { ...result, capabilities: [] } : result;
  };
  try {
    await assert.rejects(
      Harness.create({ workerPath, packages: [{ root: "/unused-package", grants: {} }] }),
      { code: "protocol", message: /update ejm-harness-worker/ },
    );
  } finally {
    Harness.prototype.request = request;
  }
});

test("trusted packages share tools, hooks, provider, commands, and widgets through one runtime", async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), "ejm-harness-package-sdk-ts-"));
  const manifest = {
    schema_version: 1,
    name: "@fixture/sdk-node",
    version: "1.0.0",
    description: "Offline SDK package fixture",
    runtime: { kind: "node", entry: "entry.mjs" },
    exports: {
      tools: [
        {
          name: "package_lookup",
          description: "Offline lookup",
          version: "1.0.0",
          input_schema: {
            type: "object",
            properties: { value: { type: "string" } },
            required: ["value"],
            additionalProperties: false,
          },
          effect: "read",
          handler: "lookup",
        },
      ],
      providers: [{ name: "model", handler: "model" }],
      hooks: [
        { event: "run_start", handler: "prepare" },
        { event: "before_tool", handler: "gate" },
        { event: "after_tool", handler: "observe" },
      ],
      commands: [{ name: "count", description: "Inspect shared runtime state", handler: "count" }],
      widgets: [{ name: "status", handler: "widget" }],
    },
  };
  await writeFile(path.join(root, "ejm.package.json"), JSON.stringify(manifest));
  await writeFile(
    path.join(root, "entry.mjs"),
    `
let calls = 0, commands = 0;
export function prepare(args) { return args.prompt + '|hook'; }
export function gate(args) { return {action:'execute',arguments:{value:args.call.arguments.value+'|gate'}}; }
export function observe(args) { return {...args.output,content:args.output.content+'|after'}; }
export async function lookup(args, ctx) { calls++; await ctx.progress('package progress'); return {content:args.value,data:{pid:process.pid}}; }
export function model(request) {
  const last = request.messages.at(-1);
  return last.role === 'tool' ? {content:'finished:'+last.content} : {tool_calls:[{id:'package-call',name:'package_lookup',arguments:{value:last.content}}]};
}
export function count(args,ctx) { return {calls,commands:++commands,pid:process.pid,session_id:ctx.session_id}; }
export function widget() { return {kind:'text',content:'calls:'+calls}; }
`,
  );
  let harness;
  try {
    harness = await Harness.create({
      workerPath,
      state_dir: path.join(root, "state"),
      packages: [
        {
          root,
          grants: {
            tools: ["package_lookup"],
            providers: ["model"],
            hooks: ["run_start", "before_tool", "after_tool"],
            commands: ["count"],
            widgets: ["status"],
          },
        },
      ],
      provider: { kind: "package", package: manifest.name, name: "model" },
    });
    const session = await harness.createSession({ cwd: root });
    const listed = await harness.listPackages();
    assert.equal(listed[0].manifest.name, manifest.name);
    const run = await session.start("request");
    const events = [];
    for await (const event of run) events.push(event);
    assert.equal((await run.wait()).text, "finished:request|hook|gate|after");
    assert.ok(
      events.some((event) => event.type === "progress" && event.message === "package progress"),
    );
    const tool = events.find((event) => event.type === "tool_finished");
    const first = await session.invokePackageCommand(manifest.name, "count");
    const second = await session.invokePackageCommand(manifest.name, "count");
    assert.equal(first.calls, 1);
    assert.equal(first.commands, 1);
    assert.equal(second.commands, 2);
    assert.equal(first.pid, tool.output.data.pid);
    assert.equal(first.pid, second.pid);
    assert.equal(first.session_id, session.id);
    assert.deepEqual(await session.renderPackageWidget(manifest.name, "status"), {
      kind: "text",
      content: "calls:1",
    });
    await assert.rejects(session.invokePackageCommand(manifest.name, "ungranted"), /not granted/);
    await within(
      (async () => {
        while ((await harness.status()).active_agents) await new Promise(setImmediate);
      })(),
      2000,
    );
    await harness.reloadPackage(manifest.name);
    const refreshed = await session.invokePackageCommand(manifest.name, "count");
    assert.equal(refreshed.calls, 0);
    assert.equal(refreshed.commands, 1);
    assert.notEqual(refreshed.pid, first.pid);
    const resumed = await session.start("again");
    assert.equal((await resumed.wait()).text, "finished:again|hook|gate|after");
    await within(
      (async () => {
        while ((await harness.status()).active_agents) await new Promise(setImmediate);
      })(),
      2000,
    );
    await writeFile(
      path.join(root, "ejm.package.json"),
      JSON.stringify({ ...manifest, version: "1.0.1" }),
    );
    await assert.rejects(harness.reloadPackage(manifest.name), /manifest changed/);
  } finally {
    await harness?.close();
    await rm(root, { recursive: true, force: true });
  }
});

test("native API-key providers configure without making model requests", async () => {
  for (const provider of [
    {
      kind: "responses",
      api_key: "fixture-key-never-sent",
      model: "fixture-model",
      reasoning_effort: "high",
    },
    { kind: "anthropic", api_key: "fixture-key-never-sent", model: "fixture-model" },
  ]) {
    const f = await fixture({ provider });
    try {
      await f.harness.createSession({ cwd: f.state });
      assert.equal((await f.harness.status()).active_agents, 0);
    } finally {
      await f.close();
    }
  }
});

test("native provider configuration errors keep credentials private", async () => {
  const state = await mkdtemp(path.join(os.tmpdir(), "ejm-harness-sdk-ts-"));
  try {
    for (const provider of [
      {
        kind: "responses",
        api_key: "private-fixture-key",
        model: "fixture-model",
        reasoning_effort: "unknown",
      },
      { kind: "anthropic", api_key: "private-fixture-key\n", model: "fixture-model" },
    ]) {
      await assert.rejects(Harness.create({ workerPath, state_dir: state, provider }), (error) => {
        assert.equal(error.code, "configuration");
        assert.ok(!error.message.includes("private-fixture-key"));
        return true;
      });
    }
  } finally {
    await rm(state, { recursive: true, force: true });
  }
});

test("streaming and late wait replay a completed run", async () => {
  const f = await fixture();
  try {
    const session = await f.harness.createSession({ cwd: f.state });
    const run = await session.start("hello from TypeScript");
    await delay(60);
    const events = [];
    for await (const event of run) events.push(event);
    assert.ok(events.some((event) => event.type === "text_delta"));
    assert.equal(events.at(-1).type, "run_finished");
    assert.equal((await run.wait()).text, "Demo: hello from TypeScript");
    assert.equal((await f.harness.listSessions())[0].id, session.id);
    assert.equal((await f.harness.status()).active_agents, 0);
  } finally {
    await f.close();
  }
});

test("callback tools and progress use a responsive reverse channel", async () => {
  let called = 0;
  const tool = defineTool({
    ...toolDefinition,
    execute: async (args, context) => {
      called++;
      assert.equal(args.value, "hello");
      await context.progress("looked up value");
      return toolOutput(`value: ${args.value}`, { data: { found: true } });
    },
  });
  const f = await fixture({ provider: replay(), tools: [tool] });
  try {
    const run = await (await f.harness.createSession({ cwd: f.state })).start("look up");
    const events = [];
    for await (const event of run) events.push(event);
    assert.equal((await run.wait()).text, "finished");
    assert.equal(called, 1);
    assert.ok(
      events.some((event) => event.type === "progress" && event.message === "looked up value"),
    );
    assert.ok(events.some((event) => event.type === "tool_finished" && event.output.data.found));
  } finally {
    await f.close();
  }
});

test("cancel reaches a blocked callback and releases its capacity after cleanup", async () => {
  let entered;
  const invoked = new Promise((resolve) => {
    entered = resolve;
  });
  let cancelled = false;
  const tool = defineTool({
    ...toolDefinition,
    execute: async (_args, context) => {
      entered();
      await new Promise((resolve) =>
        context.signal.addEventListener(
          "abort",
          () => {
            cancelled = true;
            resolve();
          },
          { once: true },
        ),
      );
      return "cancelled";
    },
  });
  const f = await fixture({ provider: replay(), tools: [tool] });
  try {
    const run = await (await f.harness.createSession({ cwd: f.state })).start("wait");
    await invoked;
    await run.cancel();
    assert.equal((await run.wait()).status, "cancelled");
    await delay(20);
    assert.equal(cancelled, true);
    assert.equal((await f.harness.governor()).running, 0);
  } finally {
    await f.close();
  }
});

test("cancelled callbacks retain admission until a handler that ignores abort settles", async () => {
  let entered;
  const invoked = new Promise((resolve) => {
    entered = resolve;
  });
  let releaseCleanup;
  const cleanup = new Promise((resolve) => {
    releaseCleanup = resolve;
  });
  let firstContext;
  let calls = 0;
  const tool = defineTool({
    ...toolDefinition,
    effect: "write",
    execute: async (_args, context) => {
      calls++;
      if (calls === 1) {
        firstContext = context;
        entered();
        await cleanup;
      }
      return "settled";
    },
  });
  const firstCall = replay("first").responses[0];
  const nextCall = replay("next").responses[0];
  const f = await fixture({
    provider: { kind: "replay", responses: [firstCall, nextCall, { content: "finished" }] },
    tools: [tool],
    resources: { max_jobs: 1, max_heavy_jobs: 1 },
  });
  try {
    const session = await f.harness.createSession({ cwd: f.state });
    const first = await session.start("wait for cleanup");
    await invoked;
    await first.cancel();
    assert.equal((await first.wait()).status, "cancelled");
    assert.equal(firstContext.signal.aborted, true);
    assert.equal((await f.harness.governor()).running, 1);

    const next = await session.start("wait for the previous effect");
    await delay(80);
    assert.equal(calls, 1);
    releaseCleanup();
    assert.equal((await next.wait()).status, "completed");
    assert.equal(calls, 2);
    assert.equal((await f.harness.governor()).running, 0);
  } finally {
    releaseCleanup();
    await f.close();
  }
});

test("an unresponsive cancelled handler poisons the transport within its cleanup bound", async () => {
  let entered;
  const invoked = new Promise((resolve) => {
    entered = resolve;
  });
  const tool = defineTool({
    ...toolDefinition,
    execute: async () => {
      entered();
      await new Promise(() => {});
      return "never settled";
    },
  });
  const f = await fixture({ provider: replay(), tools: [tool] });
  try {
    const run = await (await f.harness.createSession({ cwd: f.state })).start("ignore abort");
    await invoked;
    const exited = once(f.harness.process, "close");
    await run.cancel();
    assert.equal((await run.wait()).status, "cancelled");
    await within(exited, 7_000);
    await assert.rejects(f.harness.governor(), /cleanup|uncertain|closed|exited|disconnected/);
  } finally {
    await f.close();
  }
});

test("disconnect cancels callbacks, and reopening never repeats the uncertain effect", async () => {
  let entered;
  const invoked = new Promise((resolve) => {
    entered = resolve;
  });
  let cancelled = false;
  let calls = 0;
  const tool = defineTool({
    ...toolDefinition,
    effect: "write",
    execute: async (_args, context) => {
      calls++;
      entered();
      await new Promise((resolve) =>
        context.signal.addEventListener(
          "abort",
          () => {
            cancelled = true;
            resolve();
          },
          { once: true },
        ),
      );
      return "uncertain";
    },
  });
  const f = await fixture({ provider: replay(), tools: [tool] });
  let resumed;
  try {
    const session = await f.harness.createSession({ cwd: f.state });
    const run = await session.start("wait");
    await invoked;
    const waiting = run.wait();
    f.harness.process.kill("SIGKILL");
    await assert.rejects(waiting, /closed|exited|disconnected/);
    await delay(20);
    assert.equal(cancelled, true);
    resumed = await Harness.create({ workerPath, state_dir: f.state, tools: [tool] });
    const sessions = await resumed.listSessions();
    assert.equal(sessions[0].id, session.id);
    const events = await resumed.events(session.id);
    assert.ok(
      events.some(
        (event) => event.type === "run_finished" && event.summary.status === "interrupted",
      ),
    );
    await delay(20);
    assert.equal(calls, 1);
  } finally {
    if (resumed) await resumed.close();
    await f.close();
  }
});

test("a poisoned transport ignores buffered and later callback requests", async () => {
  let calls = 0;
  const tool = defineTool({
    ...toolDefinition,
    execute: () => {
      calls++;
      return "unexpected";
    },
  });
  const f = await fixture({ tools: [tool] });
  try {
    const invocation = (callId) => ({
      id: `tool:${callId}`,
      method: "tool.invoke",
      params: {
        session_id: "late-session",
        run_id: "late-run",
        call_id: callId,
        name: "sdk_lookup",
        arguments: { value: "late" },
        deadline_ms: Date.now() + 10_000,
      },
    });
    const frames = [invocation("duplicate"), invocation("duplicate"), invocation("buffered")];
    f.harness.process.stdout.emit(
      "data",
      Buffer.from(frames.map((frame) => JSON.stringify(frame) + "\n").join("")),
    );
    f.harness.process.stdout.emit("data", Buffer.from(JSON.stringify(invocation("later")) + "\n"));
    await delay(0);
    assert.equal(calls, 0);
    await assert.rejects(f.harness.status(), /duplicate callback/);
  } finally {
    await f.close();
  }
});

test("one worker manages 100 sessions without one process per agent", async () => {
  const f = await fixture({ max_sessions: 100 });
  try {
    const sessions = [];
    for (let i = 0; i < 100; i++)
      sessions.push(await f.harness.createSession({ cwd: f.state, title: `Agent ${i}` }));
    const pid = f.harness.process.pid;
    const runs = await Promise.all(sessions.map((session) => session.start(`hello ${session.id}`)));
    const summaries = await Promise.all(runs.map((run) => run.wait()));
    assert.equal(summaries.length, 100);
    assert.ok(summaries.every((summary) => summary.status === "completed"));
    assert.equal((await f.harness.listSessions()).length, 100);
    assert.equal(f.harness.process.pid, pid);
  } finally {
    await f.close();
  }
});

test("admission bounds an oversized burst while preserving cancellation capacity", async () => {
  const f = await fixture();
  try {
    const session = await f.harness.createSession({ cwd: f.state });
    const burst = Array.from({ length: 400 }, () => f.harness.status());
    const cancelling = f.harness.request("run.cancel", { session_id: session.id });
    const results = await Promise.allSettled(burst);
    const rejected = results.filter((result) => result.status === "rejected");
    assert.ok(rejected.length > 0);
    assert.ok(rejected.every((result) => result.reason.code === "busy"));
    assert.ok(results.filter((result) => result.status === "fulfilled").length >= 100);
    await cancelling;
    assert.equal((await f.harness.status()).active_agents, 0);
  } finally {
    await f.close();
  }
});

test("a full journal fails waiting runs promptly and closes the transport", async () => {
  const f = await fixture({ max_store_bytes: 1024 * 1024 });
  try {
    const session = await f.harness.createSession({ cwd: f.state });
    const exited = once(f.harness.process, "close");
    await assert.rejects(
      async () => {
        for (let index = 0; index < 30; index++) {
          const run = await session.start("x".repeat(16_000));
          await within(run.wait(), 2_000);
        }
      },
      { name: "HarnessError" },
    );
    await within(exited, 1_000);
    await assert.rejects(f.harness.status(), { name: "HarnessError" });
  } finally {
    await f.close();
  }
});

test("malformed and oversized raw frames close the worker", async () => {
  for (const frame of ["{invalid}\n", "x".repeat(1024 * 1024 + 1)]) {
    const child = spawn(workerPath, [], { stdio: ["pipe", "pipe", "pipe"] });
    child.stdout.resume();
    child.stderr.resume();
    child.stdin.on("error", () => {});
    const exit = once(child, "close");
    child.stdin.end(frame);
    const [code] = await exit;
    assert.notEqual(code, 0);
  }
});
