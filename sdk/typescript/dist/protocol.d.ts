/**
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "Notification".
 */
export type Notification = {
    method: "event";
    params: Event;
} | {
    method: "tool.cancel";
    params: ToolCancellation;
} | {
    method: "tool.progress";
    params: ToolProgress;
};
/**
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "Event".
 */
export type Event = {
    run_id?: string | null;
    sequence: number;
    session_id: string;
    timestamp_ms: number;
} & Event1;
export type Event1 = {
    prompt: string;
    type: "run_started";
} | {
    step: number;
    type: "model_started";
} | {
    text: string;
    type: "text_delta";
} | {
    call: ToolCall;
    type: "tool_queued";
} | {
    call: ToolCall;
    type: "tool_started";
} | {
    call_id: string;
    output: ToolOutput;
    type: "tool_finished";
} | {
    type: "usage";
    usage: Usage;
} | {
    summary: RunSummary;
    type: "run_finished";
} | {
    text: string;
    type: "steered";
} | {
    text: string;
    type: "queued";
} | {
    call_id: string;
    message: string;
    type: "progress";
} | {
    after_bytes: number;
    before_bytes: number;
    type: "context_trimmed";
};
/**
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "RunStatus".
 */
export type RunStatus = "completed" | "cancelled" | "failed" | "interrupted" | "budget_exhausted";
/**
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "HookEvent".
 */
export type HookEvent = "run_start" | "before_model" | "before_tool" | "after_tool" | "run_end";
/**
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "ToolEffect".
 */
export type ToolEffect = "read" | "write" | "external";
/**
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "RuntimeKind".
 */
export type RuntimeKind = "node" | "python";
/**
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "Request".
 */
export type Request = {
    id: string;
} & Request1;
export type Request1 = {
    method: "hello";
    params: HelloParams;
} | {
    method: "configure";
    params: ConfigureParams;
} | {
    method: "session.create";
    params: SessionOptions;
} | {
    method: "session.list";
    params: EmptyParams;
} | {
    method: "run.start";
    params: StartParams;
} | {
    method: "run.cancel";
    params: SessionParams;
} | {
    method: "run.steer";
    params: TextParams;
} | {
    method: "run.queue";
    params: TextParams;
} | {
    method: "events.list";
    params: EventsParams;
} | {
    method: "governor.status";
    params: EmptyParams;
} | {
    method: "harness.status";
    params: EmptyParams;
} | {
    method: "package.list";
    params: PackageListParams;
} | {
    method: "package.reload";
    params: PackageParams;
} | {
    method: "package.command";
    params: PackageInvokeParams;
} | {
    method: "package.widget";
    params: PackageInvokeParams;
} | {
    method: "shutdown";
    params: EmptyParams;
};
/**
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "ProviderConfig".
 */
export type ProviderConfig = {
    kind: "echo";
} | {
    kind: "replay";
    responses: ModelResponse[];
} | {
    api_key: string;
    base_url: string;
    kind: "openai";
    model: string;
} | {
    api_key: string;
    kind: "responses";
    model: string;
    reasoning_effort?: string | null;
} | {
    api_key: string;
    kind: "anthropic";
    model: string;
} | {
    kind: "package";
    name: string;
    package: string;
};
/**
 * Provider state that must survive a stateless tool exchange and journal replay.
 *
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "ProviderContinuation".
 */
export type ProviderContinuation = {
    provider: "open_ai_responses";
    reasoning: unknown[];
};
/**
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "Effect".
 */
export type Effect = "read" | "write" | "external";
/**
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "Response".
 */
export type Response = Response1 & {
    id: string;
};
export type Response1 = {
    result: unknown;
} | {
    error: ProtocolError;
};
/**
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "SessionStatus".
 */
export type SessionStatus = "idle" | "running" | "interrupted";
/**
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "ToolMethod".
 */
export type ToolMethod = "tool.invoke";
/**
 * Collect every public wire type in one authoritative JSON Schema.
 */
export interface ProtocolSchema {
    governor: GovernorSnapshot;
    harness: HarnessSnapshot;
    hello: HelloResult;
    notification: Notification;
    package_page: PackagePage;
    request: Request;
    response: Response;
    run_started: RunStarted;
    run_summary: RunSummary;
    session: Session;
    tool_output: ToolOutput;
    tool_request: ToolRequest;
}
/**
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "GovernorSnapshot".
 */
export interface GovernorSnapshot {
    heavy_running: number;
    queued: number;
    reserved_cpu_millis: number;
    reserved_memory_bytes: number;
    reserved_output_bytes: number;
    reserved_processes: number;
    running: number;
}
/**
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "HarnessSnapshot".
 */
export interface HarnessSnapshot {
    active_agents: number;
    model_requests: number;
    tools: GovernorSnapshot;
    waiting_model_requests: number;
}
/**
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "HelloResult".
 */
export interface HelloResult {
    capabilities: string[];
    protocol: number;
    version: string;
}
/**
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "ToolCall".
 */
export interface ToolCall {
    arguments: unknown;
    id: string;
    name: string;
}
/**
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "ToolOutput".
 */
export interface ToolOutput {
    artifact?: string | null;
    content: string;
    data?: unknown;
    is_error?: boolean;
}
/**
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "Usage".
 */
export interface Usage {
    /**
     * Reported subset of input tokens served from the provider's cache.
     */
    cached_input_tokens?: number | null;
    input_tokens: number;
    output_tokens: number;
    /**
     * Reported subset of output tokens spent on reasoning.
     */
    reasoning_output_tokens?: number | null;
}
/**
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "RunSummary".
 */
export interface RunSummary {
    error?: string | null;
    run_id: string;
    status: RunStatus;
    steps: number;
    text: string;
    usage: Usage;
}
/**
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "ToolCancellation".
 */
export interface ToolCancellation {
    call_id: string;
}
/**
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "ToolProgress".
 */
export interface ToolProgress {
    call_id: string;
    message: string;
}
/**
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "PackagePage".
 */
export interface PackagePage {
    next?: number | null;
    packages: PackageInfo[];
}
/**
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "PackageInfo".
 */
export interface PackageInfo {
    grants: PackageGrants;
    manifest: Manifest;
    root: string;
}
/**
 * Host-owned registration allowlists. They do not sandbox trusted native code.
 *
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "PackageGrants".
 */
export interface PackageGrants {
    commands?: string[];
    hooks?: HookEvent[];
    providers?: string[];
    tools?: string[];
    widgets?: string[];
}
/**
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "Manifest".
 */
export interface Manifest {
    description: string;
    exports?: Exports;
    name: string;
    runtime?: Runtime | null;
    schema_version: number;
    version: string;
}
export interface Exports {
    commands?: CommandExport[];
    hooks?: HookExport[];
    prompts?: string[];
    providers?: NamedExport[];
    skills?: string[];
    themes?: string[];
    tools?: ToolExport[];
    widgets?: NamedExport[];
}
/**
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "CommandExport".
 */
export interface CommandExport {
    description: string;
    handler: string;
    name: string;
}
/**
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "HookExport".
 */
export interface HookExport {
    event: HookEvent;
    handler: string;
}
/**
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "NamedExport".
 */
export interface NamedExport {
    handler: string;
    name: string;
}
/**
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "ToolExport".
 */
export interface ToolExport {
    description: string;
    effect: ToolEffect;
    handler: string;
    input_schema: unknown;
    name: string;
    version: string;
}
/**
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "Runtime".
 */
export interface Runtime {
    entry: string;
    kind: RuntimeKind;
}
/**
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "HelloParams".
 */
export interface HelloParams {
    protocol: number;
}
/**
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "ConfigureParams".
 */
export interface ConfigureParams {
    allow_shell?: boolean;
    allow_write?: boolean;
    context_max_bytes?: number | null;
    max_input_tokens?: number | null;
    max_model_requests?: number | null;
    max_output_tokens?: number | null;
    max_repeated_calls?: number | null;
    max_sessions?: number | null;
    max_steps?: number | null;
    max_store_bytes?: number | null;
    packages?: PackageConfig[];
    provider?: ProviderConfig | null;
    resources?: ResourceLimits | null;
    run_timeout_ms?: number | null;
    state_dir?: string | null;
    system_prompt?: string | null;
    tools?: ToolDefinition[];
}
/**
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "PackageConfig".
 */
export interface PackageConfig {
    grants: PackageGrants;
    root: string;
}
/**
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "ModelResponse".
 */
export interface ModelResponse {
    content?: string;
    continuation?: ProviderContinuation | null;
    tool_calls?: ToolCall[];
    usage?: Usage1;
}
export interface Usage1 {
    /**
     * Reported subset of input tokens served from the provider's cache.
     */
    cached_input_tokens?: number | null;
    input_tokens: number;
    output_tokens: number;
    /**
     * Reported subset of output tokens spent on reasoning.
     */
    reasoning_output_tokens?: number | null;
}
/**
 * Aggregate admission budgets shared by every session and executor.
 *
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "ResourceLimits".
 */
export interface ResourceLimits {
    cpu_millis?: number;
    max_artifact_bytes?: number;
    max_heavy_jobs?: number;
    max_job_ms?: number;
    max_jobs?: number;
    max_output_bytes?: number;
    max_processes?: number;
    max_queue?: number;
    memory_bytes?: number;
}
/**
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "ToolDefinition".
 */
export interface ToolDefinition {
    description: string;
    effect: Effect;
    input_schema: unknown;
    name: string;
    version: string;
}
/**
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "SessionOptions".
 */
export interface SessionOptions {
    cwd: string;
    title?: string;
}
/**
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "EmptyParams".
 */
export interface EmptyParams {
    [k: string]: unknown;
}
/**
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "StartParams".
 */
export interface StartParams {
    prompt: string;
    session_id: string;
}
/**
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "SessionParams".
 */
export interface SessionParams {
    run_id?: string | null;
    session_id: string;
}
/**
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "TextParams".
 */
export interface TextParams {
    run_id?: string | null;
    session_id: string;
    text: string;
}
/**
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "EventsParams".
 */
export interface EventsParams {
    after?: number;
    limit?: number;
    session_id: string;
}
/**
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "PackageListParams".
 */
export interface PackageListParams {
    after?: number;
    limit?: number;
}
/**
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "PackageParams".
 */
export interface PackageParams {
    package: string;
}
/**
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "PackageInvokeParams".
 */
export interface PackageInvokeParams {
    arguments?: {
        [k: string]: unknown;
    };
    name: string;
    package: string;
    session_id: string;
}
/**
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "ProtocolError".
 */
export interface ProtocolError {
    code: string;
    message: string;
}
/**
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "RunStarted".
 */
export interface RunStarted {
    run_id: string;
    session_id: string;
}
/**
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "Session".
 */
export interface Session {
    created_at_ms: number;
    cwd: string;
    id: string;
    status: SessionStatus;
    title: string;
}
/**
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "ToolRequest".
 */
export interface ToolRequest {
    id: string;
    method: ToolMethod;
    params: ToolInvocation;
}
/**
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "ToolInvocation".
 */
export interface ToolInvocation {
    arguments: unknown;
    call_id: string;
    deadline_ms: number;
    name: string;
    run_id: string;
    session_id: string;
}
/**
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "Exports".
 */
export interface Exports1 {
    commands?: CommandExport[];
    hooks?: HookExport[];
    prompts?: string[];
    providers?: NamedExport[];
    skills?: string[];
    themes?: string[];
    tools?: ToolExport[];
    widgets?: NamedExport[];
}
