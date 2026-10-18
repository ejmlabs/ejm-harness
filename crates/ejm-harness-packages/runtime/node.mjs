import { on } from "node:events";
import { realpath, stat } from "node:fs/promises";
import { relative, resolve, isAbsolute, sep } from "node:path";
import { createInterface } from "node:readline";
import { Transform } from "node:stream";
import { pipeline } from "node:stream/promises";
import { pathToFileURL } from "node:url";

const MAX_FRAME = 1024 * 1024;
const MAX_ACTIVE = 64;
const MAX_PACKAGES = 128;
const MAX_LOADS = 512;
const MAX_PENDING_BYTES = 2 * MAX_FRAME;
const protocolWrite = process.stdout.write.bind(process.stdout);
process.stdout.write = process.stderr.write.bind(process.stderr);
process.stdout.on("error", () => {});
process.stderr.on("error", () => {});

// Only this writer retains protocol stdout. Native extensions are trusted code.
class Output {
  tail = Promise.resolve();
  pending = 0;
  bytes = 0;
  waiters = new Set();
  failure;

  async send(frame, terminal = false) {
    const data = Buffer.from(`${JSON.stringify(frame)}\n`);
    if (data.length - 1 > MAX_FRAME) throw new Error("output frame exceeds 1 MiB");
    while (this.pending >= MAX_ACTIVE || this.bytes + data.length > MAX_PENDING_BYTES) {
      if (this.failure) throw this.failure;
      if (!terminal) throw new Error("extension output queue is full");
      await new Promise((resolve) => this.waiters.add(resolve));
    }
    if (this.failure) throw this.failure;
    this.pending += 1;
    this.bytes += data.length;
    const done = this.tail.then(
      () =>
        new Promise((resolve, reject) =>
          protocolWrite(data, (error) => {
            if (error) reject(error);
            else resolve();
          }),
        ),
    );
    this.tail = done.catch((error) => {
      this.failure = error;
    });
    try {
      await done;
    } finally {
      this.pending -= 1;
      this.bytes -= data.length;
      for (const wake of this.waiters) wake();
      this.waiters.clear();
    }
  }
}

const output = new Output();
const packages = new Map();
const active = new Map();
let nonce = 0;
let closing = false;
let closeTask;
let exitCode = 0;

function object(value, name) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`${name} must be an object`);
  }
  return value;
}

function text(value, name, limit = 256) {
  if (typeof value !== "string" || !value || Buffer.byteLength(value) > limit) {
    throw new Error(`${name} must be a nonempty string of at most ${limit} bytes`);
  }
  return value;
}

function errorMessage(error) {
  return Buffer.from(error instanceof Error ? error.message : "extension request failed")
    .subarray(0, 2048)
    .toString("utf8");
}

async function load(params, signal) {
  const packageId = text(params.package_id, "package_id");
  if (packages.has(packageId)) throw new Error("package is already loaded or loading");
  if (packages.size >= MAX_PACKAGES) throw new Error("package limit reached");
  if (nonce >= MAX_LOADS)
    throw new Error("host load limit reached; restart the idle language host");
  if (!Array.isArray(params.handlers) || params.handlers.length > 256) {
    throw new Error("handlers must be an array of at most 256 names");
  }
  const handlers = params.handlers.map((name) => text(name, "handler"));
  if (new Set(handlers).size !== handlers.length) throw new Error("duplicate handler name");
  const generation = ++nonce;
  const record = { handlers: null, active: 0 };
  packages.set(packageId, record);
  try {
    const root = await realpath(text(params.root, "root", MAX_FRAME));
    if (!(await stat(root)).isDirectory()) throw new Error("root must be a directory");
    const entry = await realpath(resolve(root, text(params.entry, "entry", MAX_FRAME)));
    const child = relative(root, entry);
    if (!child || child === ".." || child.startsWith(`..${sep}`) || isAbsolute(child)) {
      throw new Error("entry must be a file within the package root");
    }
    if (!(await stat(entry)).isFile()) throw new Error("entry must be a file");
    signal.throwIfAborted();
    const url = pathToFileURL(entry);
    url.searchParams.set("ejm_harness_load", String(generation));
    const module = await import(url.href);
    signal.throwIfAborted();
    record.handlers = new Map(
      handlers.map((name) => {
        if (typeof module[name] !== "function") throw new Error(`handler is not callable: ${name}`);
        return [name, module[name]];
      }),
    );
    return { package_id: packageId, handlers };
  } catch (error) {
    packages.delete(packageId);
    throw error;
  }
}

function unload(params) {
  const packageId = text(params.package_id, "package_id");
  const record = packages.get(packageId);
  if (!record) throw new Error("package is not loaded");
  if (!record.handlers || record.active)
    throw new Error("package is loading or has active requests");
  packages.delete(packageId);
  return null;
}

async function invoke(params, request) {
  const record = packages.get(text(params.package_id, "package_id"));
  const handler = record?.handlers?.get(text(params.handler, "handler"));
  if (!handler) throw new Error("package handler is not loaded");
  const fields = object(params.context ?? {}, "context");
  const emit = async (method, value, limit) => {
    request.controller.signal.throwIfAborted();
    if (!request.running) throw new Error("invocation has finished");
    if (typeof value !== "string" || Buffer.byteLength(value) > limit) {
      throw new Error(`${method} must be a string of at most ${limit} bytes`);
    }
    const key = method === "progress" ? "message" : "text";
    await output.send({ method, params: { id: request.id, [key]: value } });
  };
  const context = Object.freeze({
    ...fields,
    signal: request.controller.signal,
    progress: (message) => emit("progress", message, 2048),
    delta: (delta) => emit("delta", delta, 64 * 1024),
  });
  record.active += 1;
  try {
    request.controller.signal.throwIfAborted();
    const result = await handler(params.args ?? null, context);
    request.controller.signal.throwIfAborted();
    return result ?? null;
  } finally {
    request.running = false;
    record.active -= 1;
  }
}

function close(exclude) {
  if (closeTask) return closeTask;
  closing = true;
  const pending = [...active.values()].filter((request) => request.id !== exclude);
  for (const request of pending) request.controller.abort(new Error("request cancelled"));
  closeTask = (async () => {
    let timer;
    const finished = await Promise.race([
      Promise.allSettled(pending.map((request) => request.task)).then(() => true),
      new Promise((resolve) => {
        timer = setTimeout(() => resolve(false), 4000);
      }),
    ]);
    clearTimeout(timer);
    if (!finished) {
      exitCode = 1;
      throw new Error("active requests did not finish during shutdown");
    }
  })();
  return closeTask;
}

function start(frame) {
  const request = { id: frame.id, controller: new AbortController(), running: true, task: null };
  const task = Promise.resolve()
    .then(async () => {
      let response;
      try {
        const params = object(frame.params ?? {}, "params");
        request.controller.signal.throwIfAborted();
        let result;
        switch (frame.method) {
          case "load":
            result = await load(params, request.controller.signal);
            break;
          case "unload":
            result = unload(params);
            break;
          case "invoke":
            result = await invoke(params, request);
            break;
          default:
            throw new Error("unknown method");
        }
        response = { id: frame.id, result };
        // Serialize before queuing so an invalid extension result becomes a normal error.
        if (Buffer.byteLength(JSON.stringify(response)) > MAX_FRAME) {
          throw new Error("output frame exceeds 1 MiB");
        }
      } catch (error) {
        response = { id: frame.id, error: { message: errorMessage(error) } };
      }
      await output.send(response, true);
    })
    .finally(() => {
      request.running = false;
      active.delete(frame.id);
    });
  request.task = task;
  active.set(frame.id, request);
  task.catch((error) => {
    exitCode = 1;
    process.stderr.write(`${errorMessage(error)}\n`);
    process.stdin.destroy();
  });
  return task;
}

// Readline owns framing; this adapter applies bounds before it buffers a line.
class InputBounds extends Transform {
  bytes = 0;
  carriage = false;
  utf8 = new TextDecoder("utf-8", { fatal: true });

  _transform(chunk, encoding, callback) {
    try {
      if (encoding !== "buffer") chunk = Buffer.from(chunk, encoding);
      this.utf8.decode(chunk, { stream: true });
      if (this.carriage && chunk[0] !== 10) throw new Error("bare carriage return in input");
      this.carriage = false;
      for (let index = chunk.indexOf(13); index !== -1; index = chunk.indexOf(13, index + 1)) {
        if (index === chunk.length - 1) this.carriage = true;
        else if (chunk[index + 1] !== 10) throw new Error("bare carriage return in input");
      }
      let start = 0;
      for (let end = chunk.indexOf(10); end !== -1; end = chunk.indexOf(10, start)) {
        this.bytes += end - start;
        if (this.bytes > MAX_FRAME) throw new Error("input frame exceeds 1 MiB");
        this.bytes = 0;
        start = end + 1;
      }
      this.bytes += chunk.length - start;
      if (this.bytes > MAX_FRAME) throw new Error("input frame exceeds 1 MiB");
      callback(null, chunk);
    } catch (error) {
      callback(error);
    }
  }

  _flush(callback) {
    try {
      this.utf8.decode();
      if (this.bytes) throw new Error("incomplete input frame");
      callback();
    } catch (error) {
      callback(error);
    }
  }
}

async function* frames() {
  const bounded = new InputBounds();
  const lines = createInterface({ input: bounded, crlfDelay: Infinity, terminal: false });
  let failure;
  const delivery = pipeline(process.stdin, bounded).catch((error) => {
    failure = error;
    lines.close();
  });
  try {
    // The default readline iterator can retain 1,024 complete lines.
    for await (const [line] of on(lines, "line", {
      close: ["close"],
      highWaterMark: 1,
      lowWaterMark: 1,
    })) {
      yield JSON.parse(line);
    }
    await delivery;
    if (failure) throw failure;
  } finally {
    lines.close();
    bounded.destroy();
    await delivery;
  }
}

for (const signal of ["SIGINT", "SIGTERM"]) {
  process.once(signal, () => {
    exitCode = signal === "SIGINT" ? 130 : 143;
    process.stdin.destroy();
  });
}

try {
  for await (const value of frames()) {
    const frame = object(value, "request");
    if (frame.method === "cancel" && frame.id === undefined) {
      const id = text(object(frame.params, "params").id, "id");
      active.get(id)?.controller.abort(new Error("request cancelled"));
      continue;
    }
    text(frame.id, "id");
    if (active.has(frame.id)) throw new Error("duplicate active request id");
    if (frame.method === "shutdown") {
      let response;
      try {
        await close();
        response = { id: frame.id, result: null };
      } catch (error) {
        response = { id: frame.id, error: { message: errorMessage(error) } };
      }
      await output.send(response, true);
      break;
    }
    if (closing || active.size >= MAX_ACTIVE) {
      await output.send(
        { id: frame.id, error: { message: "request limit reached or host is closing" } },
        true,
      );
      continue;
    }
    start(frame);
  }
} catch (error) {
  if (!exitCode) {
    exitCode = 1;
    process.stderr.write(`${errorMessage(error)}\n`);
  }
} finally {
  try {
    await close();
  } catch (error) {
    process.stderr.write(`${errorMessage(error)}\n`);
  }
  process.exit(exitCode);
}
