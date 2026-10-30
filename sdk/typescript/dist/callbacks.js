import { HarnessError, asError } from "./errors.js";
import { CLEANUP_TIMEOUT_MS, MAX_PENDING, MAX_PROGRESS_BYTES } from "./limits.js";
const MAX_TIMER_MS = 2_147_483_647;
/** Aborts the controller at the invocation deadline. Returns a function that stops the timers. */
function enforceDeadline(invocation, controller, fail) {
    let deadlineTimer;
    let cleanupTimer;
    const aborted = () => {
        clearTimeout(deadlineTimer);
        cleanupTimer ??= setTimeout(() => {
            fail(new HarnessError("cleanup_timeout", "tool did not finish cleanup within 5 seconds; its effect may be uncertain"));
        }, CLEANUP_TIMEOUT_MS);
    };
    controller.signal.addEventListener("abort", aborted, { once: true });
    const remaining = Math.max(0, invocation.deadline_ms - Date.now());
    if (remaining === 0)
        controller.abort(new HarnessError("timeout", "tool deadline exceeded"));
    else
        deadlineTimer = setTimeout(() => {
            controller.abort(new HarnessError("timeout", "tool deadline exceeded"));
        }, Math.min(remaining, MAX_TIMER_MS));
    return () => {
        clearTimeout(deadlineTimer);
        clearTimeout(cleanupTimer);
        controller.signal.removeEventListener("abort", aborted);
    };
}
/** Runs host tools the worker calls back, one abortable invocation per call id. */
export class ToolCallbacks {
    active = new Map();
    tools;
    send;
    fail;
    /** `fail` poisons the transport when a protocol or cleanup bound is broken. */
    constructor(tools, send, fail) {
        this.tools = tools;
        this.send = send;
        this.fail = fail;
    }
    /** Start a callback without awaiting it, so callbacks never block protocol cancellation. */
    start(request) {
        if (this.active.size >= MAX_PENDING) {
            this.fail(new HarnessError("busy", "callback capacity reached"));
            return;
        }
        const invocation = request.params;
        if (this.active.has(invocation.call_id)) {
            this.fail(new HarnessError("protocol", "duplicate callback call_id"));
            return;
        }
        const controller = new AbortController();
        this.active.set(invocation.call_id, controller);
        this.invoke(request.id, invocation, controller).catch((error) => {
            this.fail(asError(error));
        });
    }
    cancel(callId) {
        this.active.get(callId)?.abort(new HarnessError("cancelled", "tool cancelled by worker"));
    }
    abortAll(error) {
        for (const controller of this.active.values())
            controller.abort(error);
    }
    async invoke(id, invocation, controller) {
        const stopTimers = enforceDeadline(invocation, controller, this.fail);
        try {
            let response = await this.execute(id, invocation, controller);
            if (controller.signal.aborted)
                response = {
                    id,
                    error: { code: "cancelled", message: asError(controller.signal.reason).message },
                };
            // A response confirms that the host handler has settled, including cancellation cleanup.
            // After a failure send() throws and nothing is written.
            await this.send(response);
        }
        catch (error) {
            this.fail(asError(error));
        }
        finally {
            stopTimers();
            this.active.delete(invocation.call_id);
        }
    }
    async execute(id, invocation, controller) {
        try {
            const tool = this.tools.get(invocation.name);
            if (!tool)
                throw new HarnessError("tool", `unregistered tool ${invocation.name}`);
            const context = this.context(invocation, controller.signal);
            controller.signal.throwIfAborted();
            const result = await Promise.resolve().then(() => {
                controller.signal.throwIfAborted();
                return tool.execute(invocation.arguments, context);
            });
            return { id, result: typeof result === "string" ? { content: result } : result };
        }
        catch (error) {
            return { id, error: { code: "tool", message: asError(error).message.slice(0, 4096) } };
        }
    }
    context(invocation, signal) {
        return {
            sessionId: invocation.session_id,
            runId: invocation.run_id,
            callId: invocation.call_id,
            deadlineMs: invocation.deadline_ms,
            signal,
            progress: async (message) => {
                if (signal.aborted)
                    throw signal.reason;
                if (Buffer.byteLength(message) > MAX_PROGRESS_BYTES)
                    throw new HarnessError("frame_limit", "progress exceeds 16 KiB");
                await this.send({
                    method: "tool.progress",
                    params: { call_id: invocation.call_id, message },
                });
            },
        };
    }
}
