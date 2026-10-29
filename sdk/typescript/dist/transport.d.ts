import { type ChildProcessWithoutNullStreams } from "node:child_process";
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
export declare abstract class Transport {
    readonly process: ChildProcessWithoutNullStreams;
    private readonly requests;
    private readonly callbacks;
    private readonly feed;
    private readonly reader;
    private readonly exited;
    private failure?;
    private closing;
    private closeTask?;
    private outputBytes;
    protected constructor(options: TransportOptions);
    close(): Promise<void>;
    [Symbol.asyncDispose](): Promise<void>;
    /** Internal transport. Requests are never retried after a timeout or disconnect. */
    request<T = unknown>(method: string, params: unknown, duringClose?: boolean): Promise<T>;
    cursor(sessionId: string): number;
    changed(sessionId: string): Promise<void>;
    /** Poison the transport with `error`, then wait for the worker to exit. */
    protected abort(error: Error): Promise<void>;
    private shutdown;
    private assertOpen;
    private send;
    private sendFrame;
    private read;
    private receive;
    private fail;
}
