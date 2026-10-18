import { type ChildProcessWithoutNullStreams } from "node:child_process";
import type { ConfigureParams, Effect, Event, GovernorSnapshot, RunStarted, RunSummary, Session as SessionInfo, HarnessSnapshot, PackageInfo, ToolDefinition, ToolOutput } from "./protocol.js";
export type * from "./protocol.js";
export declare class HarnessError extends Error {
    readonly code: string;
    constructor(code: string, message: string);
}
export interface ToolContext {
    sessionId: string;
    runId: string;
    callId: string;
    deadlineMs: number;
    signal: AbortSignal;
    progress(message: string): Promise<void>;
}
/** Context passed to named package exports. Metadata depends on the export kind. */
export interface ExtensionContext {
    readonly session_id?: string;
    readonly run_id?: string;
    readonly cwd?: string;
    readonly call_id?: string;
    readonly step?: number;
    readonly deadline_ms?: number;
    readonly context_max_bytes?: number;
    readonly signal: AbortSignal;
    progress(message: string): Promise<void>;
    delta(text: string): Promise<void>;
}
/** Build the wire result returned by a host tool or package tool export. */
export declare function toolOutput(content: string, options?: {
    data?: unknown;
    isError?: boolean;
    artifact?: string | null;
}): ToolOutput;
export interface Tool<T = unknown> {
    definition: ToolDefinition;
    execute: (args: T, context: ToolContext) => ToolOutput | string | Promise<ToolOutput | string>;
}
/** Registering a tool grants the host's declared effect. Repository files grant nothing. */
export declare function defineTool<T>(options: {
    name: string;
    description: string;
    inputSchema: unknown;
    effect?: Effect;
    version?: string;
    execute: (args: T, context: ToolContext) => ToolOutput | string | Promise<ToolOutput | string>;
}): Tool<T>;
export type HarnessOptions = Omit<ConfigureParams, "tools"> & {
    tools?: readonly Tool<never>[];
    workerPath?: string;
    requestTimeoutMs?: number;
};
/** One persistent Rust worker, one shared governor, many sessions. */
export declare class Harness {
    readonly process: ChildProcessWithoutNullStreams;
    private pending;
    private normalPending;
    private queued;
    private queuedControls;
    private queuedBytes;
    private queuedControlBytes;
    private callbacks;
    private tools;
    private cursors;
    private waiters;
    private failure?;
    private closing;
    private requestTimeoutMs;
    private outputBytes;
    private closeTask?;
    private exited;
    private chunks;
    private inputBytes;
    private constructor();
    static create(options?: HarnessOptions): Promise<Harness>;
    createSession(options?: {
        cwd?: string;
        title?: string;
    }): Promise<Session>;
    listSessions(): Promise<Session[]>;
    governor(): Promise<GovernorSnapshot>;
    status(): Promise<HarnessSnapshot>;
    listPackages(): Promise<PackageInfo[]>;
    /** Reload idle runtime code while keeping the configured manifest and grants. */
    reloadPackage(packageName: string): Promise<void>;
    invokePackageCommand(packageName: string, name: string, sessionId: string, args?: unknown): Promise<unknown>;
    /** Return widget data for the host to render. */
    renderPackageWidget(packageName: string, name: string, sessionId: string, args?: unknown): Promise<unknown>;
    events(sessionId: string, after?: number, limit?: number): Promise<Event[]>;
    close(): Promise<void>;
    [Symbol.asyncDispose](): Promise<void>;
    /** Internal transport. Requests are never retried after a timeout or disconnect. */
    request<T = unknown>(method: string, params: unknown, duringClose?: boolean): Promise<T>;
    private canDispatch;
    private dispatchRequest;
    private takePending;
    private drainRequests;
    private rejectQueued;
    cursor(sessionId: string): number;
    changed(sessionId: string): Promise<void>;
    private assertOpen;
    private encode;
    private send;
    private sendFrame;
    private read;
    private receive;
    private invoke;
    private fail;
}
export declare class Session {
    readonly info: SessionInfo;
    private harness;
    constructor(harness: Harness, info: SessionInfo);
    get id(): string;
    start(prompt: string, options?: {
        signal?: AbortSignal;
    }): Promise<Run>;
    events(after?: number, limit?: number): Promise<Event[]>;
    invokePackageCommand(packageName: string, name: string, args?: unknown): Promise<unknown>;
    renderPackageWidget(packageName: string, name: string, args?: unknown): Promise<unknown>;
}
export declare class Run implements AsyncIterable<Event> {
    readonly id: string;
    readonly sessionId: string;
    private harness;
    private after;
    private summary?;
    private result?;
    private signal;
    private abort?;
    constructor(harness: Harness, started: RunStarted, after?: number, signal?: AbortSignal);
    [Symbol.asyncIterator](): AsyncIterator<Event>;
    wait(): Promise<RunSummary>;
    cancel(): Promise<void>;
    steer(text: string): Promise<void>;
    queue(text: string): Promise<void>;
}
