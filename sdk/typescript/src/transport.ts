import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process";
import { randomUUID } from "node:crypto";
import { ToolCallbacks } from "./callbacks.js";
import { HarnessError, asError } from "./errors.js";
import { EventFeed } from "./events.js";
import { FrameReader, encodeFrame, type WorkerMessage } from "./frames.js";
import { MAX_OUTPUT, MAX_PENDING, SHUTDOWN_TIMEOUT_MS } from "./limits.js";
import { RequestQueue } from "./requests.js";
import type { Tool } from "./tools.js";

export interface TransportOptions {
  tools?: readonly Tool<never>[];
  workerPath?: string;
  requestTimeoutMs?: number;
}

/**
 * One persistent worker process behind bounded JSONL framing. Any protocol violation, timeout, or
 * disconnect poisons the transport: pending work is rejected and the worker is killed.
 */
export abstract class Transport {
  readonly process: ChildProcessWithoutNullStreams;
  private readonly requests: RequestQueue;
  private readonly callbacks: ToolCallbacks;
  private readonly feed = new EventFeed();
  private readonly reader = new FrameReader();
  private readonly exited: Promise<void>;
  private failure?: Error;
  private closing = false;
  private closeTask?: Promise<void>;
  private outputBytes = 0;

  protected constructor(options: TransportOptions) {
    const tools = options.tools ?? [];
    const toolsByName = new Map(tools.map((tool) => [tool.definition.name, tool]));
    if (toolsByName.size !== tools.length || toolsByName.size > MAX_PENDING) {
      throw new HarnessError("tools", "tool names must be unique, with at most 64 tools");
    }
    const requestTimeoutMs = options.requestTimeoutMs ?? 30_000;
    if (!Number.isFinite(requestTimeoutMs) || requestTimeoutMs <= 0)
      throw new HarnessError("configuration", "requestTimeoutMs must be positive");
    const fail = (error: Error): void => {
      this.fail(error);
    };
    this.requests = new RequestQueue((frame) => this.sendFrame(frame), fail, requestTimeoutMs);
    this.callbacks = new ToolCallbacks(toolsByName, (value) => this.send(value), fail);
    this.process = spawn(
      options.workerPath ?? process.env["EJM_HARNESS_WORKER_PATH"] ?? "ejm-harness-worker",
      [],
      { stdio: ["pipe", "pipe", "pipe"] },
    );
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
    this.process.stdout.on("data", (chunk: Buffer) => {
      try {
        this.read(chunk);
      } catch (error) {
        this.fail(asError(error));
      }
    });
    this.process.stdin.on("error", (error) => {
      this.fail(error);
    });
    this.process.stdout.once("end", () => {
      this.fail(
        new HarnessError(
          "disconnected",
          this.reader.partial ? "worker frame was truncated" : "worker output closed",
        ),
      );
    });
    // Drain diagnostics so a full stderr pipe cannot stall the protocol.
    this.process.stderr.resume();
  }

  close(): Promise<void> {
    this.closeTask ??= this.shutdown();
    return this.closeTask;
  }

  async [Symbol.asyncDispose](): Promise<void> {
    await this.close();
  }

  /** Internal transport. Requests are never retried after a timeout or disconnect. */
  async request<T = unknown>(method: string, params: unknown, duringClose = false): Promise<T> {
    this.assertOpen(duringClose);
    const id = randomUUID();
    const frame = encodeFrame({ id, method, params });
    return this.requests.submit<T>(method, id, frame, duringClose || method === "run.cancel");
  }

  cursor(sessionId: string): number {
    return this.feed.cursor(sessionId);
  }

  async changed(sessionId: string): Promise<void> {
    this.assertOpen();
    await this.feed.wait(sessionId);
    this.assertOpen();
  }

  /** Poison the transport with `error`, then wait for the worker to exit. */
  protected async abort(error: Error): Promise<void> {
    this.fail(error);
    await this.exited;
  }

  private async shutdown(): Promise<void> {
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
        .catch((error: unknown) => {
          this.fail(asError(error));
        });
    }
    let timer: ReturnType<typeof setTimeout> | undefined;
    await Promise.race([
      this.exited,
      new Promise<void>((resolve) => {
        timer = setTimeout(() => {
          this.process.kill("SIGKILL");
          resolve();
        }, SHUTDOWN_TIMEOUT_MS);
      }),
    ]);
    if (timer) clearTimeout(timer);
    await this.exited;
  }

  private assertOpen(duringClose = false): void {
    if (this.failure) throw this.failure;
    if (this.closing && !duringClose) throw new HarnessError("closed", "harness is closing");
  }

  private async send(value: unknown): Promise<void> {
    await this.sendFrame(encodeFrame(value));
  }

  private async sendFrame(frame: Buffer): Promise<void> {
    if (this.failure) throw this.failure;
    if (this.outputBytes + frame.length > MAX_OUTPUT)
      throw new HarnessError("busy", "worker output buffer reached 4 MiB");
    this.outputBytes += frame.length;
    await new Promise<void>((resolve, reject) => {
      this.process.stdin.write(frame, (error) => {
        this.outputBytes -= frame.length;
        if (error) reject(error);
        else resolve();
      });
    });
  }

  private read(chunk: Buffer): void {
    if (this.failure) return;
    for (const message of this.reader.read(chunk)) {
      this.receive(message);
      this.assertOpen(true);
    }
  }

  private receive(message: WorkerMessage): void {
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
    } else if (typeof message.id === "string") this.requests.settle(message);
    else throw new HarnessError("protocol", "unknown worker message");
  }

  private fail(error: Error): void {
    if (this.failure) return;
    this.failure = error;
    this.requests.rejectAll(error);
    this.callbacks.abortAll(error);
    this.feed.wakeAll();
    this.reader.reset();
    if (!this.process.killed) this.process.kill();
  }
}
