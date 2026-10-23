import { spawn } from "node:child_process";
import { randomUUID } from "node:crypto";
const MAX_FRAME = 1024 * 1024;
const MAX_PENDING = 64;
const MAX_CONTROL_PENDING = 8;
const MAX_NORMAL_PENDING = MAX_PENDING - MAX_CONTROL_PENDING;
const MAX_QUEUED = 256;
const MAX_OUTPUT = 4 * MAX_FRAME;
const CLEANUP_TIMEOUT_MS = 5_000;
const MAX_SESSION_HINTS = 1000;
const MAX_SESSION_WAITS = 4096;
export class HarnessError extends Error {
    code;
    constructor(code, message) {
        super(message);
        this.name = "HarnessError";
        this.code = code;
    }
}
/** Build the wire result returned by a host tool or package tool export. */
export function toolOutput(content, options = {}) {
    return {
        content,
        data: options.data ?? null,
        is_error: options.isError ?? false,
        artifact: options.artifact ?? null,
    };
}
/** Registering a tool grants the host's declared effect. Repository files grant nothing. */
export function defineTool(options) {
    return {
        definition: {
            name: options.name,
            description: options.description,
            input_schema: options.inputSchema,
            effect: options.effect ?? "read",
            version: options.version ?? "1",
        },
        execute: options.execute,
    };
}
/** One persistent Rust worker, one shared governor, many sessions. */
export class Harness {
    process;
    pending = new Map();
    normalPending = 0;
    queued = new Map();
    queuedControls = new Map();
    queuedBytes = 0;
    queuedControlBytes = 0;
    callbacks = new Map();
    tools;
    cursors = new Map();
    waiters = new Map();
    failure;
    closing = false;
    requestTimeoutMs;
    outputBytes = 0;
    closeTask;
    exited;
    chunks = [];
    inputBytes = 0;
    capabilities = new Set();
    constructor(options) {
        this.tools = new Map((options.tools ?? []).map((tool) => [tool.definition.name, tool]));
        if (this.tools.size !== (options.tools ?? []).length || this.tools.size > MAX_PENDING) {
            throw new HarnessError("tools", "tool names must be unique, with at most 64 tools");
        }
        this.requestTimeoutMs = options.requestTimeoutMs ?? 30_000;
        if (!Number.isFinite(this.requestTimeoutMs) || this.requestTimeoutMs <= 0)
            throw new HarnessError("configuration", "requestTimeoutMs must be positive");
        this.process = spawn(options.workerPath ??
            process.env["EJM_HARNESS_WORKER_PATH"] ??
            (process.platform === "win32" ? "ejm-harness-worker.exe" : "ejm-harness-worker"), [], { stdio: ["pipe", "pipe", "pipe"] });
        this.exited = new Promise((resolve) => {
            this.process.once("error", (error) => {
                this.fail(error);
                resolve();
            });
            this.process.once("close", (code, signal) => {
                this.fail(new HarnessError("disconnected", `worker exited (${String(signal ?? code)})`));
                resolve();
            });
        });
        this.process.stdout.on("data", (chunk) => {
            try {
                this.read(chunk);
            }
            catch (error) {
                this.fail(asError(error));
            }
        });
        this.process.stdin.on("error", (error) => {
            this.fail(error);
        });
        this.process.stdout.once("end", () => {
            this.fail(new HarnessError("disconnected", this.inputBytes ? "worker frame was truncated" : "worker output closed"));
        });
        // Drain diagnostics so a full stderr pipe cannot stall the protocol.
        this.process.stderr.resume();
    }
    static async create(options = {}) {
        if (process.platform === "darwin" && options.capacity === undefined) {
            options = { ...options, capacity: { mode: "auto" } };
        }
        const harness = new Harness(options);
        try {
            const hello = await harness.request("hello", { protocol: 1 });
            if (hello.protocol !== 1)
                throw new HarnessError("protocol", "worker protocol 1 is required");
            harness.capabilities = new Set(Array.isArray(hello.capabilities) ? hello.capabilities : []);
            if (options.capacity != null)
                harness.requireCapability("capacity");
            if (options.provider_capacity != null)
                harness.requireCapability("provider_capacity");
            if (((options.packages?.length ?? 0) > 0 || options.provider?.kind === "package") &&
                (!Array.isArray(hello.capabilities) || !hello.capabilities.includes("packages"))) {
                throw new HarnessError("protocol", "update ejm-harness-worker to a version that supports packages");
            }
            const config = { ...options };
            delete config.workerPath;
            delete config.requestTimeoutMs;
            await harness.request("configure", {
                ...config,
                tools: (options.tools ?? []).map((tool) => tool.definition),
            });
            return harness;
        }
        catch (error) {
            harness.fail(asError(error));
            await harness.exited;
            throw error;
        }
    }
    async createSession(options = {}) {
        const info = await this.request("session.create", {
            cwd: options.cwd ?? process.cwd(),
            title: options.title ?? "",
        });
        return new Session(this, info);
    }
    async listSessions() {
        return (await this.request("session.list", {})).map((info) => new Session(this, info));
    }
    /** Read a bounded page. Keep its opaque cursor with the same query. */
    async sessionsPage(query = {}) {
        this.requireCapability("session_views");
        return this.request("session.page", query);
    }
    async resolveSession(prefix) {
        this.requireCapability("session_views");
        return new Session(this, await this.request("session.resolve", { prefix }));
    }
    async sessionView(sessionId, bounds = {}) {
        this.requireCapability("session_views");
        return this.request("session.view", { session_id: sessionId, bounds });
    }
    async eventsPage(sessionId, options) {
        this.requireCapability("session_views");
        return this.request("events.page", { ...options, session_id: sessionId });
    }
    /** Null means this worker uses the legacy process-local fixed composition. */
    async capacityStatus() {
        this.requireCapability("capacity");
        return this.request("capacity.status", {});
    }
    async resetCapacityLearning() {
        this.requireCapability("capacity");
        return (await this.request("capacity.reset", {})).profiles_reset;
    }
    async governor() {
        return this.request("governor.status", {});
    }
    async status() {
        return this.request("harness.status", {});
    }
    /** Cached local engine phases; callback counts do not prove native child exit. */
    async schedulingStatus() {
        this.requireCapability("scheduling");
        return this.request("scheduling.status", {});
    }
    async listPackages() {
        const packages = [];
        let after = 0;
        for (;;) {
            const page = await this.request("package.list", { after, limit: 16 });
            packages.push(...page.packages);
            if (packages.length > 128)
                throw new HarnessError("protocol", "package list exceeds 128 entries");
            if (page.next == null)
                return packages;
            if (page.next <= after || page.next > 128)
                throw new HarnessError("protocol", "invalid package page cursor");
            after = page.next;
        }
    }
    /** Reload idle runtime code while keeping the configured manifest and grants. */
    async reloadPackage(packageName) {
        await this.request("package.reload", { package: packageName });
    }
    async invokePackageCommand(packageName, name, sessionId, args = {}) {
        return this.request("package.command", {
            package: packageName,
            name,
            session_id: sessionId,
            arguments: args,
        });
    }
    /** Return widget data for the host to render. */
    async renderPackageWidget(packageName, name, sessionId, args = {}) {
        return this.request("package.widget", {
            package: packageName,
            name,
            session_id: sessionId,
            arguments: args,
        });
    }
    async events(sessionId, after = 0, limit = 64) {
        return this.request("events.list", { session_id: sessionId, after, limit });
    }
    close() {
        if (this.closeTask)
            return this.closeTask;
        this.closeTask = (async () => {
            if (!this.failure) {
                this.closing = true;
                this.rejectQueued(new HarnessError("closed", "harness is closing"));
                this.request("shutdown", {}, true)
                    .catch(() => {
                    /* teardown still closes the child */
                })
                    .finally(() => {
                    this.process.stdin.end();
                })
                    .catch((error) => {
                    this.fail(asError(error));
                });
            }
            let timer;
            await Promise.race([
                this.exited,
                new Promise((resolve) => {
                    timer = setTimeout(() => {
                        this.process.kill("SIGKILL");
                        resolve();
                    }, 5_000);
                }),
            ]);
            if (timer)
                clearTimeout(timer);
            await this.exited;
        })();
        return this.closeTask;
    }
    async [Symbol.asyncDispose]() {
        await this.close();
    }
    /** Internal transport. Requests are never retried after a timeout or disconnect. */
    async request(method, params, duringClose = false) {
        this.assertOpen(duringClose);
        const id = randomUUID();
        const control = duringClose || method === "run.cancel";
        const frame = this.encode({ id, method, params });
        const ready = this.canDispatch(control);
        const queue = control ? this.queuedControls : this.queued;
        const queuedBytes = control ? this.queuedControlBytes : this.queuedBytes;
        if (!ready &&
            (queue.size >= (control ? MAX_CONTROL_PENDING : MAX_QUEUED) ||
                queuedBytes + frame.length > (control ? MAX_FRAME : MAX_OUTPUT)))
            throw new HarnessError("busy", control
                ? "control admission queue reached 8 frames or 1 MiB"
                : "request admission queue reached 256 frames or 4 MiB");
        return new Promise((resolve, reject) => {
            const timer = setTimeout(() => {
                const queued = queue.get(id);
                if (queued) {
                    queue.delete(id);
                    if (control)
                        this.queuedControlBytes -= queued.frame.length;
                    else
                        this.queuedBytes -= queued.frame.length;
                    reject(new HarnessError("timeout", `request ${method} timed out before admission`));
                }
                else if (this.pending.has(id)) {
                    this.fail(new HarnessError("timeout", `request ${method} timed out; its effect may be uncertain`));
                }
            }, this.requestTimeoutMs);
            const queued = {
                id,
                frame,
                control,
                resolve: (value) => {
                    resolve(value);
                },
                reject,
                timer,
            };
            if (ready)
                this.dispatchRequest(queued);
            else {
                queue.set(id, queued);
                if (control)
                    this.queuedControlBytes += frame.length;
                else
                    this.queuedBytes += frame.length;
            }
        });
    }
    canDispatch(control) {
        return (this.pending.size < MAX_PENDING &&
            (control
                ? this.pending.size - this.normalPending < MAX_CONTROL_PENDING
                : this.normalPending < MAX_NORMAL_PENDING));
    }
    dispatchRequest(request) {
        const { id, control, resolve, reject, timer } = request;
        this.pending.set(id, { control, resolve, reject, timer });
        if (!control)
            this.normalPending++;
        this.sendFrame(request.frame).catch((error) => {
            const pending = this.takePending(id);
            if (pending) {
                pending.reject(asError(error));
                this.drainRequests();
            }
        });
    }
    takePending(id) {
        const pending = this.pending.get(id);
        if (pending) {
            this.pending.delete(id);
            if (!pending.control)
                this.normalPending--;
            clearTimeout(pending.timer);
        }
        return pending;
    }
    drainRequests() {
        if (this.failure)
            return;
        for (;;) {
            const control = this.queuedControls.values().next().value;
            const queue = control && this.canDispatch(true) ? this.queuedControls : this.queued;
            const next = queue.values().next().value;
            if (!next || !this.canDispatch(next.control))
                return;
            queue.delete(next.id);
            if (next.control)
                this.queuedControlBytes -= next.frame.length;
            else
                this.queuedBytes -= next.frame.length;
            this.dispatchRequest(next);
        }
    }
    rejectQueued(error) {
        for (const queue of [this.queued, this.queuedControls]) {
            for (const request of queue.values()) {
                clearTimeout(request.timer);
                request.reject(error);
            }
            queue.clear();
        }
        this.queuedBytes = 0;
        this.queuedControlBytes = 0;
    }
    cursor(sessionId) {
        return this.cursors.get(sessionId) ?? 0;
    }
    async changed(sessionId) {
        this.assertOpen();
        if (!this.waiters.has(sessionId) && this.waiters.size >= MAX_SESSION_WAITS)
            throw new HarnessError("busy", "session iterator capacity exceeded");
        const waiters = this.waiters.get(sessionId) ?? new Set();
        this.waiters.set(sessionId, waiters);
        if (waiters.size >= MAX_PENDING)
            throw new HarnessError("busy", "too many concurrent iterators for one session");
        await new Promise((resolve) => {
            const done = () => {
                clearTimeout(timer);
                waiters.delete(done);
                if (!waiters.size)
                    this.waiters.delete(sessionId);
                resolve();
            };
            const timer = setTimeout(done, 250);
            waiters.add(done);
        });
        this.assertOpen();
    }
    assertOpen(duringClose = false) {
        if (this.failure)
            throw this.failure;
        if (this.closing && !duringClose)
            throw new HarnessError("closed", "harness is closing");
    }
    requireCapability(capability) {
        if (!this.capabilities.has(capability))
            throw new HarnessError("protocol", `update ejm-harness-worker to a version that supports ${capability}`);
    }
    encode(value) {
        const frame = Buffer.from(JSON.stringify(value) + "\n");
        if (frame.length > MAX_FRAME)
            throw new HarnessError("frame_limit", "protocol frame exceeds 1 MiB");
        return frame;
    }
    async send(value) {
        await this.sendFrame(this.encode(value));
    }
    async sendFrame(frame) {
        if (this.failure)
            throw this.failure;
        if (this.outputBytes + frame.length > MAX_OUTPUT)
            throw new HarnessError("busy", "worker output buffer reached 4 MiB");
        this.outputBytes += frame.length;
        await new Promise((resolve, reject) => {
            this.process.stdin.write(frame, (error) => {
                this.outputBytes -= frame.length;
                if (error)
                    reject(error);
                else
                    resolve();
            });
        });
    }
    read(chunk) {
        if (this.failure)
            return;
        let start = 0;
        while (start < chunk.length) {
            const newline = chunk.indexOf(10, start);
            const end = newline < 0 ? chunk.length : newline + 1;
            const part = chunk.subarray(start, end);
            this.inputBytes += part.length;
            if (this.inputBytes > MAX_FRAME)
                throw new HarnessError("frame_limit", "worker frame exceeds 1 MiB");
            this.chunks.push(part);
            if (newline >= 0) {
                const bytes = Buffer.concat(this.chunks, this.inputBytes);
                this.chunks = [];
                this.inputBytes = 0;
                const value = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes));
                if (typeof value !== "object" || value === null || Array.isArray(value))
                    throw new HarnessError("protocol", "worker frame must be an object");
                this.receive(value);
                this.assertOpen(true);
            }
            start = end;
        }
    }
    receive(value) {
        if ("method" in value && value.method === "tool.invoke") {
            if (this.callbacks.size >= MAX_PENDING) {
                this.fail(new HarnessError("busy", "callback capacity reached"));
                return;
            }
            const invocation = value.params;
            if (this.callbacks.has(invocation.call_id)) {
                this.fail(new HarnessError("protocol", "duplicate callback call_id"));
                return;
            }
            const controller = new AbortController();
            const task = this.invoke(value.id, invocation, controller).finally(() => this.callbacks.delete(invocation.call_id));
            this.callbacks.set(invocation.call_id, { controller, task });
        }
        else if ("method" in value && value.method === "tool.cancel") {
            this.callbacks
                .get(value.params.call_id)
                ?.controller.abort(new HarnessError("cancelled", "tool cancelled by worker"));
        }
        else if ("method" in value && value.method === "event") {
            const event = value.params;
            const sequence = Math.max(this.cursor(event.session_id), event.sequence);
            this.cursors.delete(event.session_id);
            if (this.cursors.size >= MAX_SESSION_HINTS) {
                const oldest = this.cursors.keys().next().value;
                if (oldest !== undefined)
                    this.cursors.delete(oldest);
            }
            this.cursors.set(event.session_id, sequence);
            for (const done of [...(this.waiters.get(event.session_id) ?? [])])
                done();
        }
        else if ("id" in value && typeof value.id === "string" && !("method" in value)) {
            const pending = this.takePending(value.id);
            if (!pending)
                return;
            if ("error" in value)
                pending.reject(new HarnessError(value.error.code, value.error.message));
            else if ("result" in value)
                pending.resolve(value.result);
            else
                pending.reject(new HarnessError("protocol", "response has no result or error"));
            this.drainRequests();
        }
        else
            throw new HarnessError("protocol", "unknown worker message");
    }
    async invoke(id, invocation, controller) {
        let deadlineTimer;
        let cleanupTimer;
        const aborted = () => {
            if (deadlineTimer)
                clearTimeout(deadlineTimer);
            cleanupTimer ??= setTimeout(() => {
                this.fail(new HarnessError("cleanup_timeout", "tool did not finish cleanup within 5 seconds; its effect may be uncertain"));
            }, CLEANUP_TIMEOUT_MS);
        };
        controller.signal.addEventListener("abort", aborted, { once: true });
        const remaining = Math.max(0, invocation.deadline_ms - Date.now());
        if (remaining === 0)
            controller.abort(new HarnessError("timeout", "tool deadline exceeded"));
        else
            deadlineTimer = setTimeout(() => {
                controller.abort(new HarnessError("timeout", "tool deadline exceeded"));
            }, Math.min(remaining, 2_147_483_647));
        let response;
        try {
            const tool = this.tools.get(invocation.name);
            if (!tool)
                throw new HarnessError("tool", `unregistered tool ${invocation.name}`);
            const context = {
                sessionId: invocation.session_id,
                runId: invocation.run_id,
                callId: invocation.call_id,
                deadlineMs: invocation.deadline_ms,
                signal: controller.signal,
                progress: async (message) => {
                    if (controller.signal.aborted)
                        throw controller.signal.reason;
                    if (Buffer.byteLength(message) > 16 * 1024)
                        throw new HarnessError("frame_limit", "progress exceeds 16 KiB");
                    await this.send({
                        method: "tool.progress",
                        params: { call_id: invocation.call_id, message },
                    });
                },
            };
            // The reader never awaits this promise, so callbacks cannot block protocol cancellation.
            controller.signal.throwIfAborted();
            const result = await Promise.resolve().then(() => {
                controller.signal.throwIfAborted();
                return tool.execute(invocation.arguments, context);
            });
            response = { id, result: typeof result === "string" ? { content: result } : result };
        }
        catch (error) {
            response = { id, error: { code: "tool", message: asError(error).message.slice(0, 4096) } };
        }
        try {
            if (controller.signal.aborted)
                response = {
                    id,
                    error: { code: "cancelled", message: asError(controller.signal.reason).message },
                };
            // A response confirms that the host handler has settled, including cancellation cleanup.
            if (!this.failure)
                await this.send(response);
        }
        catch (error) {
            this.fail(asError(error));
        }
        finally {
            if (deadlineTimer)
                clearTimeout(deadlineTimer);
            if (cleanupTimer)
                clearTimeout(cleanupTimer);
            controller.signal.removeEventListener("abort", aborted);
        }
    }
    fail(error) {
        if (this.failure)
            return;
        this.failure = error;
        for (const pending of this.pending.values()) {
            clearTimeout(pending.timer);
            pending.reject(error);
        }
        this.pending.clear();
        this.normalPending = 0;
        this.rejectQueued(error);
        for (const callback of this.callbacks.values())
            callback.controller.abort(error);
        for (const waiters of this.waiters.values())
            for (const done of [...waiters])
                done();
        this.waiters.clear();
        this.chunks = [];
        this.inputBytes = 0;
        if (!this.process.killed)
            this.process.kill();
    }
}
export class Session {
    info;
    harness;
    constructor(harness, info) {
        this.harness = harness;
        this.info = info;
    }
    get id() {
        return this.info.id;
    }
    async start(prompt, options = {}) {
        options.signal?.throwIfAborted();
        const after = this.harness.cursor(this.id);
        const started = await this.harness.request("run.start", {
            session_id: this.id,
            prompt,
        });
        return new Run(this.harness, started, after, options.signal);
    }
    async events(after = 0, limit = 64) {
        return this.harness.events(this.id, after, limit);
    }
    async view(bounds = {}) {
        return this.harness.sessionView(this.id, bounds);
    }
    async eventsPage(options) {
        return this.harness.eventsPage(this.id, options);
    }
    async invokePackageCommand(packageName, name, args = {}) {
        return this.harness.invokePackageCommand(packageName, name, this.id, args);
    }
    async renderPackageWidget(packageName, name, args = {}) {
        return this.harness.renderPackageWidget(packageName, name, this.id, args);
    }
}
export class Run {
    id;
    sessionId;
    harness;
    after;
    summary;
    result;
    signal;
    abort;
    constructor(harness, started, after = 0, signal) {
        this.harness = harness;
        this.id = started.run_id;
        this.sessionId = started.session_id;
        this.after = after;
        this.signal = signal;
        if (signal) {
            this.abort = () => {
                void this.cancel().catch(() => {
                    /* the reader owns disconnect handling */
                });
            };
            signal.addEventListener("abort", this.abort, { once: true });
            if (signal.aborted)
                this.abort();
            void this.wait()
                .catch(() => {
                /* wait() still exposes the failure to its caller */
            })
                .finally(() => {
                if (this.abort)
                    signal.removeEventListener("abort", this.abort);
            });
        }
    }
    async *[Symbol.asyncIterator]() {
        let after = this.after;
        for (;;) {
            const events = await this.harness.events(this.sessionId, after);
            if (!events.length) {
                await this.harness.changed(this.sessionId);
                continue;
            }
            for (const event of events) {
                after = event.sequence;
                if (event.run_id !== this.id)
                    continue;
                if (event.type === "run_finished") {
                    this.summary = event.summary;
                    if (this.signal && this.abort)
                        this.signal.removeEventListener("abort", this.abort);
                }
                yield event;
                if (event.type === "run_finished")
                    return;
            }
        }
    }
    async wait() {
        if (this.summary)
            return this.summary;
        this.result ??= (async () => {
            for await (const event of this)
                if (event.type === "run_finished")
                    return event.summary;
            throw new HarnessError("protocol", "run ended without a summary");
        })();
        return this.result;
    }
    async cancel() {
        if (!this.summary)
            await this.harness.request("run.cancel", { session_id: this.sessionId, run_id: this.id });
    }
    async steer(text) {
        await this.harness.request("run.steer", { session_id: this.sessionId, run_id: this.id, text });
    }
    async queue(text) {
        await this.harness.request("run.queue", { session_id: this.sessionId, run_id: this.id, text });
    }
}
function asError(error) {
    return error instanceof Error ? error : new Error(String(error));
}
