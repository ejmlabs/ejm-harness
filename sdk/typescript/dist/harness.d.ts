import type { ConfigureParams, Event, GovernorSnapshot, HarnessSnapshot, PackageInfo } from "./protocol.js";
import { Session } from "./session.js";
import { Transport, type TransportOptions } from "./transport.js";
export type HarnessOptions = Omit<ConfigureParams, "tools"> & TransportOptions;
/** One persistent Rust worker, one shared governor, many sessions. */
export declare class Harness extends Transport {
    private constructor();
    static create(options?: HarnessOptions): Promise<Harness>;
    createSession(options?: {
        cwd?: string;
        title?: string;
    }): Promise<Session>;
    listSessions(): Promise<Session[]>;
    governor(): Promise<GovernorSnapshot>;
    status(): Promise<HarnessSnapshot>;
    listPackages(): Promise<PackageInfo[]>;
    /** Reload idle runtime code while keeping the configured manifest and grants. */
    reloadPackage(packageName: string): Promise<void>;
    invokePackageCommand(packageName: string, name: string, sessionId: string, args?: unknown): Promise<unknown>;
    /** Return widget data for the host to render. */
    renderPackageWidget(packageName: string, name: string, sessionId: string, args?: unknown): Promise<unknown>;
    events(sessionId: string, after?: number, limit?: number): Promise<Event[]>;
    /** Check the worker speaks this protocol and supports what the configuration needs. */
    private handshake;
}
