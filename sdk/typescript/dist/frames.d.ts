import type { Notification, Response, ToolRequest } from "./protocol.js";
export type WorkerMessage = Notification | Response | ToolRequest;
export declare function encodeFrame(value: unknown): Buffer;
/** Reassembles newline-delimited worker frames from stdout chunks within the frame bound. */
export declare class FrameReader {
    private chunks;
    private bytes;
    /** True while a frame has started but not ended. */
    get partial(): boolean;
    reset(): void;
    /** Yield each complete frame as it is parsed, so the caller can stop between frames. */
    read(chunk: Buffer): Generator<WorkerMessage, void>;
    private take;
}
