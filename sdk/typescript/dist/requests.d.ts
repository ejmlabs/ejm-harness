import type { Response } from "./protocol.js";
/** Sends requests within the in-flight bound, queues the rest, and matches responses to them. */
export declare class RequestQueue {
    private readonly write;
    private readonly fail;
    private readonly timeoutMs;
    private readonly pending;
    private normalPending;
    private readonly normal;
    private readonly control;
    /** `fail` poisons the transport when a sent request times out. */
    constructor(write: (frame: Buffer) => Promise<void>, fail: (error: Error) => void, timeoutMs: number);
    /** Resolve with the worker's result. A control request never waits behind normal ones. */
    submit<T>(method: string, id: string, frame: Buffer, control: boolean): Promise<T>;
    settle(response: Response): void;
    rejectQueued(error: Error): void;
    rejectAll(error: Error): void;
    private canDispatch;
    private dispatch;
    private take;
    /** Dispatch queued requests while slots are free, control requests first. */
    private drain;
}
