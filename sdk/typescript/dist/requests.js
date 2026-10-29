import { HarnessError, asError } from "./errors.js";
import { MAX_CONTROL_PENDING, MAX_FRAME, MAX_NORMAL_PENDING, MAX_OUTPUT, MAX_PENDING, MAX_QUEUED, } from "./limits.js";
/** Where a request waits for an in-flight slot, bounded by frame count and bytes. */
class Lane {
    control;
    fullMessage;
    queue = new Map();
    maxFrames;
    maxBytes;
    bytes = 0;
    constructor(control, maxFrames, maxBytes, fullMessage) {
        this.control = control;
        this.maxFrames = maxFrames;
        this.maxBytes = maxBytes;
        this.fullMessage = fullMessage;
    }
    get head() {
        return this.queue.values().next().value;
    }
    hasRoom(frame) {
        return this.queue.size < this.maxFrames && this.bytes + frame.length <= this.maxBytes;
    }
    add(request) {
        this.queue.set(request.id, request);
        this.bytes += request.frame.length;
    }
    remove(id) {
        const request = this.queue.get(id);
        if (request) {
            this.queue.delete(id);
            this.bytes -= request.frame.length;
        }
        return request;
    }
    clear() {
        const requests = [...this.queue.values()];
        this.queue.clear();
        this.bytes = 0;
        return requests;
    }
}
/** Sends requests within the in-flight bound, queues the rest, and matches responses to them. */
export class RequestQueue {
    write;
    fail;
    timeoutMs;
    pending = new Map();
    normalPending = 0;
    normal = new Lane(false, MAX_QUEUED, MAX_OUTPUT, "request admission queue reached 256 frames or 4 MiB");
    control = new Lane(true, MAX_CONTROL_PENDING, MAX_FRAME, "control admission queue reached 8 frames or 1 MiB");
    /** `fail` poisons the transport when a sent request times out. */
    constructor(write, fail, timeoutMs) {
        this.write = write;
        this.fail = fail;
        this.timeoutMs = timeoutMs;
    }
    /** Resolve with the worker's result. A control request never waits behind normal ones. */
    submit(method, id, frame, control) {
        const lane = control ? this.control : this.normal;
        const ready = this.canDispatch(control);
        if (!ready && !lane.hasRoom(frame))
            throw new HarnessError("busy", lane.fullMessage);
        return new Promise((resolve, reject) => {
            const timer = setTimeout(() => {
                if (lane.remove(id)) {
                    reject(new HarnessError("timeout", `request ${method} timed out before admission`));
                }
                else if (this.pending.has(id)) {
                    this.fail(new HarnessError("timeout", `request ${method} timed out; its effect may be uncertain`));
                }
            }, this.timeoutMs);
            const request = {
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
                this.dispatch(request);
            else
                lane.add(request);
        });
    }
    settle(response) {
        const pending = this.take(response.id);
        if (!pending)
            return;
        if ("error" in response)
            pending.reject(new HarnessError(response.error.code, response.error.message));
        else if ("result" in response)
            pending.resolve(response.result);
        else
            pending.reject(new HarnessError("protocol", "response has no result or error"));
        this.drain();
    }
    rejectQueued(error) {
        for (const lane of [this.normal, this.control]) {
            for (const request of lane.clear()) {
                clearTimeout(request.timer);
                request.reject(error);
            }
        }
    }
    rejectAll(error) {
        for (const pending of this.pending.values()) {
            clearTimeout(pending.timer);
            pending.reject(error);
        }
        this.pending.clear();
        this.normalPending = 0;
        this.rejectQueued(error);
    }
    canDispatch(control) {
        return (this.pending.size < MAX_PENDING &&
            (control
                ? this.pending.size - this.normalPending < MAX_CONTROL_PENDING
                : this.normalPending < MAX_NORMAL_PENDING));
    }
    dispatch(request) {
        const { id, control, resolve, reject, timer } = request;
        this.pending.set(id, { control, resolve, reject, timer });
        if (!control)
            this.normalPending++;
        this.write(request.frame).catch((error) => {
            const pending = this.take(id);
            if (pending) {
                pending.reject(asError(error));
                this.drain();
            }
        });
    }
    take(id) {
        const pending = this.pending.get(id);
        if (pending) {
            this.pending.delete(id);
            if (!pending.control)
                this.normalPending--;
            clearTimeout(pending.timer);
        }
        return pending;
    }
    /** Dispatch queued requests while slots are free, control requests first. */
    drain() {
        for (;;) {
            const lane = [this.control, this.normal].find((candidate) => candidate.head && this.canDispatch(candidate.control));
            const next = lane?.head;
            if (!lane || !next)
                return;
            lane.remove(next.id);
            this.dispatch(next);
        }
    }
}
