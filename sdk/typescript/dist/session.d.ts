import type { Harness } from "./harness.js";
import type { Event, RunStarted, RunSummary, Session as SessionInfo } from "./protocol.js";
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
    constructor(harness: Harness, started: RunStarted, after?: number, signal?: AbortSignal);
    [Symbol.asyncIterator](): AsyncIterator<Event>;
    wait(): Promise<RunSummary>;
    cancel(): Promise<void>;
    steer(text: string): Promise<void>;
    queue(text: string): Promise<void>;
    private target;
    /** Cancel the run when the signal aborts, until the run finishes. */
    private cancelOnAbort;
}
