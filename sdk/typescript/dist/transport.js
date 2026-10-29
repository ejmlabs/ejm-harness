import { spawn } from "node:child_process";
import { randomUUID } from "node:crypto";
import { ToolCallbacks } from "./callbacks.js";
import { HarnessError, asError } from "./errors.js";
import { EventFeed } from "./events.js";
import { FrameReader, encodeFrame } from "./frames.js";
import { MAX_OUTPUT, MAX_PENDING, SHUTDOWN_TIMEOUT_MS } from "./limits.js";
import { RequestQueue } from "./requests.js";
/**
 * One persistent worker process behind bounded JSONL framing. Any protocol violation, timeout, or
 * disconnect poisons the transport: pending work is rejected and the worker is killed.
 */
export class Transport {
    process;
    requests;
    callbacks;
    feed = new EventFeed();
    reader = new FrameReader();
    exited;
    failure;
    closing = false;
    closeTask;
    outputBytes = 0;
    constructor(options) {
        const tools = options.tools ?? [];
        const toolsByName = new Map(tools.map((tool) => [tool.definition.name, tool]));
        if (toolsByName.size !== tools.length || toolsByName.size > MAX_PENDING) {
            throw new HarnessError("tools", "tool names must be unique, with at most 64 tools");
        }
        const requestTimeoutMs = options.requestTimeoutMs ?? 30_000;
        if (!Number.isFinite(requestTimeoutMs) || requestTimeoutMs <= 0)
            throw new HarnessError("configuration", "requestTimeoutMs must be positive");
        const fail = (error) => {
            this.fail(error);
        };
        this.requests = new RequestQueue((frame) => this.sendFrame(frame), fail, requestTimeoutMs);
        this.callbacks = new ToolCallbacks(toolsByName, (value) => this.send(value), fail);
        this.process = spawn(options.workerPath ?? process.env["EJM_HARNESS_WORKER_PATH"] ?? "ejm-harness-worker", [], { stdio: ["pipe", "pipe", "pipe"] });
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
            this.fail(new HarnessError("disconnected", this.reader.partial ? "worker frame was truncated" : "worker output closed"));
        });
        // Drain diagnostics so a full stderr pipe cannot stall the protocol.
        this.process.stderr.resume();
    }
    close() {
        this.closeTask ??= this.shutdown();
        return this.closeTask;
    }
    async [Symbol.asyncDispose]() {
        await this.close();
    }
    /** Internal transport. Requests are never retried after a timeout or disconnect. */
    async request(method, params, duringClose = false) {
        this.assertOpen(duringClose);
        const id = randomUUID();
        const frame = encodeFrame({ id, method, params });
        return this.requests.submit(method, id, frame, duringClose || method === "run.cancel");
    }
    cursor(sessionId) {
        return this.feed.cursor(sessionId);
    }
    async changed(sessionId) {
        this.assertOpen();
        await this.feed.wait(sessionId);
        this.assertOpen();
    }
    /** Poison the transport with `error`, then wait for the worker to exit. */
    async abort(error) {
        this.fail(error);
        await this.exited;
    }
    async shutdown() {
        if (!this.failure) {
            this.closing = true;
            this.requests.rejectQueued(new HarnessError("closed", "harness is closing"));
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
                }, SHUTDOWN_TIMEOUT_MS);
            }),
        ]);
        if (timer)
            clearTimeout(timer);
        await this.exited;
    }
    assertOpen(duringClose = false) {
        if (this.failure)
            throw this.failure;
        if (this.closing && !duringClose)
            throw new HarnessError("closed", "harness is closing");
    }
    async send(value) {
        await this.sendFrame(encodeFrame(value));
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
        for (const message of this.reader.read(chunk)) {
            this.receive(message);
            this.assertOpen(true);
        }
    }
    receive(message) {
        if ("method" in message) {
            switch (message.method) {
                case "tool.invoke":
                    this.callbacks.start(message);
                    break;
                case "tool.cancel":
                    this.callbacks.cancel(message.params.call_id);
                    break;
                case "event":
                    this.feed.record(message.params);
                    break;
                default:
                    throw new HarnessError("protocol", "unknown worker message");
            }
        }
        else if (typeof message.id === "string")
            this.requests.settle(message);
        else
            throw new HarnessError("protocol", "unknown worker message");
    }
    fail(error) {
        if (this.failure)
            return;
        this.failure = error;
        this.requests.rejectAll(error);
        this.callbacks.abortAll(error);
        this.feed.wakeAll();
        this.reader.reset();
        if (!this.process.killed)
            this.process.kill();
    }
}
