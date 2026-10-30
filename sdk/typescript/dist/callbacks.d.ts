import type { ToolRequest } from "./protocol.js";
import type { Tool } from "./tools.js";
/** Runs host tools the worker calls back, one abortable invocation per call id. */
export declare class ToolCallbacks {
    private readonly active;
    private readonly tools;
    private readonly send;
    private readonly fail;
    /** `fail` poisons the transport when a protocol or cleanup bound is broken. */
    constructor(tools: ReadonlyMap<string, Tool<never>>, send: (value: unknown) => Promise<void>, fail: (error: Error) => void);
    /** Start a callback without awaiting it, so callbacks never block protocol cancellation. */
    start(request: ToolRequest): void;
    cancel(callId: string): void;
    abortAll(error: Error): void;
    private invoke;
    private execute;
    private context;
}
