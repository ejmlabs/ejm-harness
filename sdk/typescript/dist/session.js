import { HarnessError } from "./errors.js";
export class Session {
    info;
    harness;
    constructor(harness, info) {
        this.harness = harness;
        this.info = info;
    }
    get id() {
        return this.info.id;
    }
    async start(prompt, options = {}) {
        options.signal?.throwIfAborted();
        const after = this.harness.cursor(this.id);
        const started = await this.harness.request("run.start", {
            session_id: this.id,
            prompt,
        });
        return new Run(this.harness, started, after, options.signal);
    }
    async events(after = 0, limit = 64) {
        return this.harness.events(this.id, after, limit);
    }
    async invokePackageCommand(packageName, name, args = {}) {
        return this.harness.invokePackageCommand(packageName, name, this.id, args);
    }
    async renderPackageWidget(packageName, name, args = {}) {
        return this.harness.renderPackageWidget(packageName, name, this.id, args);
    }
}
export class Run {
    id;
    sessionId;
    harness;
    after;
    summary;
    result;
    constructor(harness, started, after = 0, signal) {
        this.harness = harness;
        this.id = started.run_id;
        this.sessionId = started.session_id;
        this.after = after;
        if (signal)
            this.cancelOnAbort(signal);
    }
    async *[Symbol.asyncIterator]() {
        let after = this.after;
        for (;;) {
            const events = await this.harness.events(this.sessionId, after);
            if (!events.length) {
                await this.harness.changed(this.sessionId);
                continue;
            }
            for (const event of events) {
                after = event.sequence;
                if (event.run_id !== this.id)
                    continue;
                if (event.type === "run_finished")
                    this.summary = event.summary;
                yield event;
                if (event.type === "run_finished")
                    return;
            }
        }
    }
    async wait() {
        if (this.summary)
            return this.summary;
        this.result ??= (async () => {
            for await (const event of this)
                if (event.type === "run_finished")
                    return event.summary;
            throw new HarnessError("protocol", "run ended without a summary");
        })();
        return this.result;
    }
    async cancel() {
        if (!this.summary)
            await this.harness.request("run.cancel", this.target());
    }
    async steer(text) {
        await this.harness.request("run.steer", { ...this.target(), text });
    }
    async queue(text) {
        await this.harness.request("run.queue", { ...this.target(), text });
    }
    target() {
        return { session_id: this.sessionId, run_id: this.id };
    }
    /** Cancel the run when the signal aborts, until the run finishes. */
    cancelOnAbort(signal) {
        const abort = () => {
            void this.cancel().catch(() => {
                /* the reader owns disconnect handling */
            });
        };
        signal.addEventListener("abort", abort, { once: true });
        if (signal.aborted)
            abort();
        void this.wait()
            .catch(() => {
            /* wait() still exposes the failure to its caller */
        })
            .finally(() => {
            signal.removeEventListener("abort", abort);
        });
    }
}
