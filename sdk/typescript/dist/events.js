import { HarnessError } from "./errors.js";
import { MAX_PENDING } from "./limits.js";
const MAX_SESSIONS = 1000;
const WAKE_INTERVAL_MS = 250;
/** The latest event sequence seen per session, and the iterators waiting for the next one. */
export class EventFeed {
    cursors = new Map();
    waiters = new Map();
    cursor(sessionId) {
        return this.cursors.get(sessionId) ?? 0;
    }
    record(event) {
        if (!this.cursors.has(event.session_id) && this.cursors.size >= MAX_SESSIONS)
            throw new HarnessError("protocol", "session capacity exceeded");
        this.cursors.set(event.session_id, Math.max(this.cursor(event.session_id), event.sequence));
        this.wake(event.session_id);
    }
    /** Resolve on the next event for the session, or after one wake interval. */
    async wait(sessionId) {
        const waiters = this.waiters.get(sessionId) ?? new Set();
        this.waiters.set(sessionId, waiters);
        if (waiters.size >= MAX_PENDING)
            throw new HarnessError("busy", "too many concurrent iterators for one session");
        await new Promise((resolve) => {
            const done = () => {
                clearTimeout(timer);
                waiters.delete(done);
                if (!waiters.size)
                    this.waiters.delete(sessionId);
                resolve();
            };
            const timer = setTimeout(done, WAKE_INTERVAL_MS);
            waiters.add(done);
        });
    }
    wakeAll() {
        for (const sessionId of [...this.waiters.keys()])
            this.wake(sessionId);
    }
    wake(sessionId) {
        for (const done of [...(this.waiters.get(sessionId) ?? [])])
            done();
    }
}
