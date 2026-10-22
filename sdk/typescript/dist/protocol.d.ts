/**
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "LearningState".
 */
export type LearningState = "cold" | "learned" | "unavailable";
/**
 * Capacity policy belongs to the local owner, never to generated content.
 *
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "CapacityMode".
 */
export type CapacityMode = "auto" | "fixed";
/**
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "ObservationAvailability".
 */
export type ObservationAvailability = "measured" | "estimated" | "unavailable" | "unsupported";
/**
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "ObservationSource".
 */
export type ObservationSource = "native_system" | "scheduler_availability" | "process_resource_limit" | "linux_cgroup_v2";
/**
 * Queue policy chosen by the owner, never by generated agent content.
 *
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "AdmissionPolicy".
 */
export type AdmissionPolicy = "fixed" | "automatic";
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
    method: "session.page";
    params: SessionQuery;
} | {
    method: "session.resolve";
    params: SessionResolveParams;
} | {
    method: "session.view";
    params: SessionViewParams;
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
    method: "events.page";
    params: EventsPageParams;
} | {
    method: "capacity.status";
    params: EmptyParams;
} | {
    method: "capacity.reset";
    params: EmptyParams;
} | {
    method: "governor.status";
    params: EmptyParams;
} | {
    method: "harness.status";
    params: EmptyParams;
} | {
    method: "scheduling.status";
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
 * `Monitored` is observation and reactive cleanup, not a hard OS ceiling.
 *
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "RestrictionState".
 */
export type RestrictionState = "enforced" | "monitored" | "unsupported";
/**
 * Collect every public wire type in one authoritative JSON Schema.
 */
export interface ProtocolSchema {
    capacity: CapacitySnapshot;
    capacity_reset: CapacityResetResult;
    event_page: EventPage;
    governor: GovernorSnapshot;
    harness: HarnessSnapshot;
    hello: HelloResult;
    notification: Notification;
    package_page: PackagePage;
    request: Request;
    response: Response;
    run_started: RunStarted;
    run_summary: RunSummary;
    scheduling: SchedulingSnapshot;
    session: Session;
    session_page: SessionPage;
    session_view: SessionView;
    tool_output: ToolOutput;
    tool_request: ToolRequest;
}
/**
 * A cheap cached snapshot. Reading it performs no probes or control work.
 *
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "CapacitySnapshot".
 */
export interface CapacitySnapshot {
    coordinated: boolean;
    hard_model_requests: number;
    hard_resources: ResourceLimits;
    learning: LearningStatus;
    mode: CapacityMode;
    model_running: number;
    model_target: number;
    model_waiting: number;
    models: LaneSnapshot;
    native: NativeSnapshot;
    native_helpers?: NativeHelperSnapshot;
    observations: HostObservations;
    operating: OperatingSnapshot;
    provider: ProviderSnapshot;
    reason?: string | null;
    scope: string;
    tools: LaneSnapshot;
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
 * via the `definition` "LearningStatus".
 */
export interface LearningStatus {
    profile_count: number;
    samples: number;
    state: LearningState;
}
/**
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "LaneSnapshot".
 */
export interface LaneSnapshot {
    cancelled: number;
    failed: number;
    preparation: TimingSummary;
    provider_wait: TimingSummary;
    queue: TimingSummary;
    service: TimingSummary;
    successful: number;
    successful_per_second: number;
}
/**
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "TimingSummary".
 */
export interface TimingSummary {
    p50_micros: number;
    p95_micros: number;
    samples: number;
}
/**
 * A cached view. Reading it never starts a helper or performs I/O.
 *
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "NativeSnapshot".
 */
export interface NativeSnapshot {
    active_local_scopes?: number;
    /**
     * `Monitored` is observation and reactive cleanup, not a hard OS ceiling.
     */
    descendant_cleanup?: "enforced" | "monitored" | "unsupported";
    /**
     * `Monitored` is observation and reactive cleanup, not a hard OS ceiling.
     */
    domain_placement?: "enforced" | "monitored" | "unsupported";
    epoch?: string | null;
    last_dead_epoch?: string | null;
    last_operation?: NativeOperation | null;
    /**
     * `Monitored` is observation and reactive cleanup, not a hard OS ceiling.
     */
    owner_death_cleanup?: "enforced" | "monitored" | "unsupported";
    /**
     * Confirmed owner deaths observed by this handle. This is not a job ledger.
     */
    owner_deaths?: number;
    /**
     * `Monitored` is observation and reactive cleanup, not a hard OS ceiling.
     */
    owner_exclusivity?: "enforced" | "monitored" | "unsupported";
    /**
     * `Monitored` is observation and reactive cleanup, not a hard OS ceiling.
     */
    owner_loss_detection?: "enforced" | "monitored" | "unsupported";
    /**
     * Last authenticated owner PID. This cached value does not establish liveness.
     */
    owner_pid?: number | null;
    /**
     * `Monitored` is observation and reactive cleanup, not a hard OS ceiling.
     */
    recovery?: "enforced" | "monitored" | "unsupported";
}
/**
 * An epoch and monotonic operation number. It never authorizes a replay.
 *
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "NativeOperation".
 */
export interface NativeOperation {
    epoch: string;
    id: number;
}
/**
 * Cached coarse helper commitments. Counts include pending and draining owners.
 */
export interface NativeHelperSnapshot {
    committed?: number;
    memory_bytes?: number;
    occupied?: number;
    pending?: number;
    /**
     * Process slots, including the helper runtime's threads on Linux.
     */
    processes?: number;
}
/**
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "HostObservations".
 */
export interface HostObservations {
    available_memory_bytes: Observation3;
    cpu_millis: Observation;
    cpu_usage_percent: Observation2;
    descriptor_limit: Observation31;
    /**
     * Unsupported or incomplete ancestor visibility prevents safe growth.
     */
    domain_visibility: "measured" | "estimated" | "unavailable" | "unsupported";
    domains: ResourceDomainObservation[];
    memory_bytes: Observation3;
    process_limit: Observation32;
}
/**
 * Missing values and stale samples remain distinct from a measured zero.
 *
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "Observation3".
 */
export interface Observation3 {
    age_ms: number;
    availability: ObservationAvailability;
    max_age_ms: number;
    source: ObservationSource;
    value?: number | null;
}
/**
 * Missing values and stale samples remain distinct from a measured zero.
 *
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "Observation".
 */
export interface Observation {
    age_ms: number;
    availability: ObservationAvailability;
    max_age_ms: number;
    source: ObservationSource;
    value?: number | null;
}
/**
 * Missing values and stale samples remain distinct from a measured zero.
 *
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "Observation2".
 */
export interface Observation2 {
    age_ms: number;
    availability: ObservationAvailability;
    max_age_ms: number;
    source: ObservationSource;
    value?: number | null;
}
/**
 * Missing values and stale samples remain distinct from a measured zero.
 */
export interface Observation31 {
    age_ms: number;
    availability: ObservationAvailability;
    max_age_ms: number;
    source: ObservationSource;
    value?: number | null;
}
/**
 * Visible ancestor constraints. IDs contain native scope identities, not paths.
 *
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "ResourceDomainObservation".
 */
export interface ResourceDomainObservation {
    available_memory_bytes?: number | null;
    available_processes?: number | null;
    cpu_millis?: number | null;
    id: string;
    max_processes?: number | null;
    memory_bytes?: number | null;
    parent?: string | null;
}
/**
 * Missing values and stale samples remain distinct from a measured zero.
 */
export interface Observation32 {
    age_ms: number;
    availability: ObservationAvailability;
    max_age_ms: number;
    source: ObservationSource;
    value?: number | null;
}
/**
 * Bounded local state, separate from the legacy [`GovernorSnapshot`] shape.
 *
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "OperatingSnapshot".
 */
export interface OperatingSnapshot {
    contact_remaining?: Duration | null;
    /**
     * False after the cached coordination contact deadline expires.
     */
    contact_valid: boolean;
    debt: OperatingTarget;
    generation: number;
    /**
     * Contexts and persistent hosts reserve memory without active job slots.
     */
    memory_leases: number;
    policy: AdmissionPolicy;
    target: OperatingTarget;
}
/**
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "Duration".
 */
export interface Duration {
    nanos: number;
    secs: number;
}
/**
 * Adjustable budgets beneath the governor's immutable hard limits.
 *
 * Zero is valid and parks admission for that resource. Shrinking a target does
 * not revoke running leases or erase reservations that exceed the new target.
 *
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "OperatingTarget".
 */
export interface OperatingTarget {
    cpu_millis: number;
    max_heavy_jobs: number;
    max_jobs: number;
    max_output_bytes: number;
    max_processes: number;
    memory_bytes: number;
}
/**
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "ProviderSnapshot".
 */
export interface ProviderSnapshot {
    blocked: boolean;
    cooldown_until_ms?: number | null;
    coordinated: boolean;
    domain?: string | null;
    in_flight: number;
    reason?: string | null;
    request_credits?: number | null;
    target: number;
    token_credits?: number | null;
    token_debt: number;
    waiting: number;
}
/**
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "CapacityResetResult".
 */
export interface CapacityResetResult {
    profiles_reset: number;
}
/**
 * Bounded replay through a captured global high-water boundary.
 *
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "EventPage".
 */
export interface EventPage {
    complete: boolean;
    /**
     * Global boundary represented by this page, or `through` when complete.
     */
    cursor: number;
    events: Event[];
    /**
     * The next single event exceeds the byte budget. Take a fresh snapshot.
     */
    omitted: boolean;
    /**
     * Last represented selected-session sequence, if this page returned one.
     */
    selected_cursor?: number | null;
    through: number;
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
    /**
     * Omission preserves the original fixed, process-local configuration.
     */
    capacity?: CapacityPolicy | null;
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
    provider_capacity?: ProviderQuotaConfig | null;
    resources?: ResourceLimits | null;
    run_timeout_ms?: number | null;
    state_dir?: string | null;
    system_prompt?: string | null;
    tools?: ToolDefinition[];
}
/**
 * Optional values are explicit hard ceilings. Omission is not a zero budget.
 *
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "CapacityPolicy".
 */
export interface CapacityPolicy {
    max_cpu_millis?: number | null;
    max_heavy_jobs?: number | null;
    max_jobs?: number | null;
    max_memory_bytes?: number | null;
    max_model_requests?: number | null;
    max_output_bytes?: number | null;
    max_processes?: number | null;
    max_sessions?: number | null;
    /**
     * Capacity policy belongs to the local owner, never to generated content.
     */
    mode?: "auto" | "fixed";
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
 * Owner-supplied quota identity. Never derive this UUID from credentials.
 *
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "ProviderQuotaConfig".
 */
export interface ProviderQuotaConfig {
    account_uuid: string;
    family?: string;
    max_in_flight?: number | null;
    requests?: ProviderRate | null;
    tokens?: ProviderRate | null;
}
/**
 * A locally configured sustained rate. Known rate charges are not concurrency
 * slots and survive local cancellation until their window expires.
 *
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "ProviderRate".
 */
export interface ProviderRate {
    limit: number;
    window_ms: number;
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
 * Case-insensitive title or identity prefix search, with bounded pages.
 *
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "SessionQuery".
 */
export interface SessionQuery {
    cursor?: SessionCursor | null;
    limit?: number;
    query?: string;
}
/**
 * Stable continuation for a session query. Keep it with the same query.
 *
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "SessionCursor".
 */
export interface SessionCursor {
    created_at_ms: number;
    id: string;
    query: string;
    upper_rowid: number;
}
/**
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "SessionResolveParams".
 */
export interface SessionResolveParams {
    prefix: string;
}
/**
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "SessionViewParams".
 */
export interface SessionViewParams {
    bounds?: SessionTailOptions;
    session_id: string;
}
/**
 * Hard payload bounds for a presentation tail or replay page.
 *
 * Zero in either field requests an authority-only tail. Replay pages require
 * positive bounds.
 * At most 1000 events and 2 MiB of serialized event payload are returned.
 */
export interface SessionTailOptions {
    max_bytes?: number;
    max_events?: number;
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
 * via the `definition` "EventsPageParams".
 */
export interface EventsPageParams {
    after?: number;
    bounds?: SessionTailOptions1;
    session_id: string;
    through: number;
}
/**
 * Hard payload bounds for a presentation tail or replay page.
 *
 * Zero in either field requests an authority-only tail. Replay pages require
 * positive bounds.
 * At most 1000 events and 2 MiB of serialized event payload are returned.
 */
export interface SessionTailOptions1 {
    max_bytes?: number;
    max_events?: number;
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
 * Cached local run counts. Each managed run occupies at most one listed phase.
 *
 * These counts describe engine work, not OS CPU use, native child liveness, or
 * a host-wide total. Reading them performs no journal or host observation I/O.
 *
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "SchedulingSnapshot".
 */
export interface SchedulingSnapshot {
    /**
     * Runs retained by the engine, including waiting and finishing runs.
     */
    managed_runs: number;
    /**
     * Runs preparing admitted context, hooks and a provider request.
     */
    model_preparing: number;
    /**
     * Runs inside a provider callback, including streamed-output backpressure.
     */
    model_running: number;
    /**
     * Runs waiting for a model slot, coordinated model credit or context memory.
     */
    model_waiting: number;
    /**
     * Runs waiting for provider selection or coordinated provider quota.
     */
    provider_waiting: number;
    /**
     * Managed runs outside the explicit waiting, preparation and service phases.
     * This includes journal and hook work; it is not an OS runnable-thread count.
     */
    runnable_runs: number;
    /**
     * Runs waiting to enter the bounded tool-dispatch pool.
     */
    tool_dispatch_waiting: number;
    /**
     * Runs waiting for a tool reservation in the shared governor.
     */
    tool_resource_waiting: number;
    /**
     * Runs inside a tool callback. Self-admitting tools may wait internally.
     * Callback completion does not prove that uncertain native work has exited.
     */
    tool_running: number;
    /**
     * Runs waiting for their workspace mutation lock.
     */
    workspace_waiting: number;
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
 * A page in descending creation-time and identity order.
 *
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "SessionPage".
 */
export interface SessionPage {
    next?: SessionCursor | null;
    sessions: Session[];
}
/**
 * One consistent journal boundary for authority and bounded presentation.
 *
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "SessionView".
 */
export interface SessionView {
    /**
     * Global durable high-water sequence at this snapshot.
     */
    durable_cursor: number;
    events: Event[];
    last_transition?: SessionRunTransition | null;
    omitted: boolean;
    /**
     * The active run only. Finished identity is in `last_transition`.
     */
    run_id?: string | null;
    /**
     * Last selected-session sequence, matching live predecessor tokens.
     */
    selected_cursor: number;
    session: Session;
}
/**
 * The latest committed run transition, independent of the visible tail.
 *
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "SessionRunTransition".
 */
export interface SessionRunTransition {
    outcome?: RunStatus | null;
    run_id?: string | null;
    sequence: number;
    status: SessionStatus;
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
/**
 * Cached coarse helper commitments. Counts include pending and draining owners.
 *
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "NativeHelperSnapshot".
 */
export interface NativeHelperSnapshot1 {
    committed?: number;
    memory_bytes?: number;
    occupied?: number;
    pending?: number;
    /**
     * Process slots, including the helper runtime's threads on Linux.
     */
    processes?: number;
}
/**
 * Hard payload bounds for a presentation tail or replay page.
 *
 * Zero in either field requests an authority-only tail. Replay pages require
 * positive bounds.
 * At most 1000 events and 2 MiB of serialized event payload are returned.
 *
 * This interface was referenced by `ProtocolSchema`'s JSON-Schema
 * via the `definition` "SessionTailOptions".
 */
export interface SessionTailOptions2 {
    max_bytes?: number;
    max_events?: number;
}
