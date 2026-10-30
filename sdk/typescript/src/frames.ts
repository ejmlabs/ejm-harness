import { HarnessError } from "./errors.js";
import { MAX_FRAME } from "./limits.js";
import type { Notification, Response, ToolRequest } from "./protocol.js";

export type WorkerMessage = Notification | Response | ToolRequest;

export function encodeFrame(value: unknown): Buffer {
  const frame = Buffer.from(JSON.stringify(value) + "\n");
  if (frame.length > MAX_FRAME)
    throw new HarnessError("frame_limit", "protocol frame exceeds 1 MiB");
  return frame;
}

/** Reassembles newline-delimited worker frames from stdout chunks within the frame bound. */
export class FrameReader {
  private chunks: Buffer[] = [];
  private bytes = 0;

  /** True while a frame has started but not ended. */
  get partial(): boolean {
    return this.bytes > 0;
  }

  reset(): void {
    this.chunks = [];
    this.bytes = 0;
  }

  /** Yield each complete frame as it is parsed, so the caller can stop between frames. */
  *read(chunk: Buffer): Generator<WorkerMessage, void> {
    let start = 0;
    while (start < chunk.length) {
      const newline = chunk.indexOf(10, start);
      const end = newline < 0 ? chunk.length : newline + 1;
      const part = chunk.subarray(start, end);
      this.bytes += part.length;
      if (this.bytes > MAX_FRAME)
        throw new HarnessError("frame_limit", "worker frame exceeds 1 MiB");
      this.chunks.push(part);
      if (newline >= 0) yield this.take();
      start = end;
    }
  }

  private take(): WorkerMessage {
    const bytes = Buffer.concat(this.chunks, this.bytes);
    this.reset();
    const value: unknown = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes));
    if (typeof value !== "object" || value === null || Array.isArray(value))
      throw new HarnessError("protocol", "worker frame must be an object");
    return value as WorkerMessage;
  }
}
