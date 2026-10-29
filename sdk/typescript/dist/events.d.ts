import type { Event } from "./protocol.js";
/** The latest event sequence seen per session, and the iterators waiting for the next one. */
export declare class EventFeed {
    private cursors;
    private waiters;
    cursor(sessionId: string): number;
    record(event: Event): void;
    /** Resolve on the next event for the session, or after one wake interval. */
    wait(sessionId: string): Promise<void>;
    wakeAll(): void;
    private wake;
}
