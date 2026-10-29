import type { Effect, ToolDefinition, ToolOutput } from "./protocol.js";
export interface ToolContext {
    sessionId: string;
    runId: string;
    callId: string;
    deadlineMs: number;
    signal: AbortSignal;
    progress(message: string): Promise<void>;
}
/** Context passed to named package exports. Metadata depends on the export kind. */
export interface ExtensionContext {
    readonly session_id?: string;
    readonly run_id?: string;
    readonly cwd?: string;
    readonly call_id?: string;
    readonly step?: number;
    readonly deadline_ms?: number;
    readonly context_max_bytes?: number;
    readonly signal: AbortSignal;
    progress(message: string): Promise<void>;
    delta(text: string): Promise<void>;
}
/** Build the wire result returned by a host tool or package tool export. */
export declare function toolOutput(content: string, options?: {
    data?: unknown;
    isError?: boolean;
    artifact?: string | null;
}): ToolOutput;
export interface Tool<T = unknown> {
    definition: ToolDefinition;
    execute: (args: T, context: ToolContext) => ToolOutput | string | Promise<ToolOutput | string>;
}
/** Registering a tool grants the host's declared effect. Repository files grant nothing. */
export declare function defineTool<T>(options: {
    name: string;
    description: string;
    inputSchema: unknown;
    effect?: Effect;
    version?: string;
    execute: (args: T, context: ToolContext) => ToolOutput | string | Promise<ToolOutput | string>;
}): Tool<T>;
