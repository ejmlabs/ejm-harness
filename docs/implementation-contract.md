# Implementation contract

This is the integration contract for the initial implementation. Rust source types become authoritative once present. SDK wire types are generated from the worker schema.

## Packages and ownership

- `ejm-harness-exec`: resource governor, command execution, capabilities, abuse tests.
- `ejm-harness-auth`: sign-in, verified identity, injected credential storage, renewal, and logout.
- `ejm-harness`: generic engine, providers, events, persistence, context and strategy traits.
- `ejm-harness-coding`: native coding tools and shell tool, reuses `ejm-harness-exec`.
- The separate `ejm-cli` project consumes public engine, coding, and executor crates.
- `ejm-harness-worker`: persistent bidirectional JSONL worker, same public harness.
- `sdk/typescript`, `sdk/python`: worker lifecycle, typed runs, tool callbacks, cancellation.

## Executor public API

`ResourceLimits`, `Reservation`, `Governor`, `Lease`, `GovernorSnapshot`, `Executor`, `CommandRequest`, `CommandOutput`, `ExecutorCapabilities`, `RequiredCapabilities` are exported from `ejm_harness_exec`.

- `ResourceLimits: Default + Clone + Serialize + Deserialize`: `max_jobs: usize`, `max_heavy_jobs: usize`, `cpu_millis: u32`, `memory_bytes: u64`, `max_processes: u32`, `max_queue: usize`, `max_output_bytes: usize`, `max_artifact_bytes: u64`, `max_job_ms: u64`.
- `Reservation: Default + Clone + Serialize + Deserialize`: `cpu_millis: u32`, `memory_bytes: u64`, `processes: u32`, `output_bytes: usize`, `heavy: bool`.
- `Governor::new(ResourceLimits) -> anyhow::Result<Self>`; `Governor::snapshot(&self) -> GovernorSnapshot`; `Governor::acquire(self: &Arc<Self>, session_id: &str, reservation: Reservation, cancel: &CancellationToken) -> impl Future<Output=Result<Lease>>`. RAII lease releases reservations. `GovernorSnapshot` includes `running: usize`, `queued: usize`, `heavy_running: usize`, `reserved_cpu_millis: u32`, `reserved_memory_bytes: u64`, `reserved_processes: u32`, `reserved_output_bytes: usize`.
- `Executor::new(governor: Arc<Governor>, artifact_root: PathBuf) -> Result<Self>`; `Executor::capabilities(&self) -> ExecutorCapabilities`; `Executor::run(&self, request: CommandRequest, cancel: CancellationToken) -> impl Future<Output=Result<CommandOutput>>`.
- `CommandRequest: Default + Clone + Serialize + Deserialize`: `session_id: String`, `program: String`, `args: Vec<String>`, `cwd: PathBuf`, `env: BTreeMap<String,String>`, `timeout_ms: u64`, `reservation: Reservation`, `required: RequiredCapabilities`.
- `CommandOutput: Clone + Serialize + Deserialize`: `exit_code: Option<i32>`, `stdout: String`, `stderr: String`, `truncated: bool`, `cancelled: bool`, `timed_out: bool`, `artifact: Option<PathBuf>`, `duration_ms: u64`, `peak_memory_bytes: u64`, `peak_processes: u32`.
- All new fields on requests use defaults. Requested unsupported restrictions fail before launch. Native execution is never called a security sandbox. Linux hard control and external isolation can be optional backends with explicit requirements.

`ejm_harness_exec::process::spawn_owned(tokio::process::Command) -> std::io::Result<OwnedChild>`
is the lower-level child owner for supervised host processes.
`OwnedChild` exposes `id`, `take_stdin`, `take_stdout`, `take_stderr`, `wait`,
`try_wait`, `start_kill`, and async `kill`. Root status does not mean descendant
cleanup is complete. Retain the child, pipe tasks, and host leases until `kill`
finishes, including after normal root exit. Unix uses process groups; Windows
uses suspended Job assignment and confirms empty Job membership before returning.
Use the host's existing Tokio runtime. Read the
[native ownership contract](native-process-support.md) for errors, limits, and
the creation flags controlled by the Windows adapter.

## Engine public API

`Harness::new(config: HarnessConfig, provider: Arc<dyn Provider>, tools: Vec<Arc<dyn Tool>>) -> Result<Self>`; harness is Clone, owns one shared governor and store.

`Harness::with_optional_provider(config: HarnessConfig, provider: Option<Arc<dyn Provider>>, tools: Vec<Arc<dyn Tool>>, hooks: Vec<Arc<dyn Hook>>) -> Result<Self>` allows an interactive host to open before sign-in. Without a provider, submitting or queuing a turn fails before the prompt or run-start event is persisted.

`Harness::configure_provider(&self, provider: Option<Arc<dyn Provider>>, binding: &str) -> async Result<()>` changes or removes the shared provider only while all managed runs and queued turns are idle. The host supplies a stable, nonsecret binding of 1 to 1024 printable UTF-8 bytes covering provider, account, model and endpoint. A changed binding atomically clears opaque message continuation while retaining readable conversation and tool evidence; the same binding preserves continuation across restart. Invalid configuration or a journal failure retains the prior provider and binding. Once admitted, configuration completes even if its caller drops the future.

Providers supplied through constructors are unbound until explicitly configured. Before their first admitted turn, an earlier durable binding and its opaque continuation are cleared together. Journals that have never been explicitly bound retain legacy continuation behavior.

`HarnessConfig: Default + Clone`: `state_dir: PathBuf`, `resources: ResourceLimits`, `max_sessions: usize`, `max_model_requests: usize`, `context_max_bytes: usize`, `max_steps: u32`, `max_repeated_calls: u32`, `run_timeout_ms: u64`, `system_prompt: String`, `max_store_bytes: u64`. Core validates all limits.

`SessionOptions: Default + Clone + Serialize + Deserialize`: `cwd: PathBuf`, `title: String`.

`Session: Clone + Serialize + Deserialize`: `id: String`, `title: String`, `cwd: PathBuf`, `created_at_ms: u64`, `status: SessionStatus`.

- `Harness::create_session(&self, SessionOptions) -> async Result<Session>`
- `Harness::list_sessions(&self) -> async Result<Vec<Session>>`
- `Harness::start(&self, session_id: &str, prompt: String) -> async Result<RunHandle>`
- `Harness::steer(&self, session_id: &str, text: String) -> async Result<()>`
- `Harness::enqueue(&self, session_id: &str, text: String) -> async Result<()>`
- `Harness::cancel(&self, session_id: &str) -> async Result<()>`
- `Harness::subscribe(&self) -> broadcast::Receiver<Event>`; bounded live hints. On lag replay from durable `events`.
- `Harness::events(&self, session_id: &str, after: u64, limit: usize) -> async Result<Vec<Event>>`
- `Harness::governor(&self) -> Arc<Governor>`
- `Harness::shutdown(&self) -> async Result<()>` cancels runs and waits for resource cleanup.

`RunHandle: Clone`: public `id: String`, `session_id: String`; `cancel(&self)`; `wait(&mut self) -> async Result<RunSummary>`; `wait_completion(&mut self) -> async Result<RunCompletion>`.

`RunCompletion: Clone`: public `summary: RunSummary`, `queued_successor: bool`. Detailed completion records whether the driver handed off to a local queued successor, decided after draining late steering and applying continuation rules under active admission ownership, before notifying waiters. That decision survives the successor finishing. `wait` returns the same summary. A false flag does not assert current idleness or authorize another action; hosts still check their authority and target.

`Harness::subscribe_run(&self, session_id: &str, run_id: &str) -> async Option<RunHandle>` observes only that exact current run, including an internally queued successor and its bounded run-end cleanup. `None` means absent or superseded, not successful completion. Use durable replay for old outcomes. A durable `RunFinished` event and session `Idle` can precede run-end observers; `wait` includes those observers. Handles share one completion result and cancellation token. Dropping or cloning a handle does not cancel work; `cancel` on any clone targets that same run. Retained receivers preserve the error when a driver ends without a summary. A new local queued run may already be current when a wait returns, so hosts must still validate admission.

`RunSummary`: `run_id: String`, `status: RunStatus` (`completed`, `cancelled`, `failed`, `interrupted`, `budget_exhausted`), `steps: u32`, `text: String`, `usage: Usage`, `error: Option<String>`.

`Event`: `sequence: u64`, `session_id: String`, `run_id: Option<String>`, `timestamp_ms: u64`, flattened `kind: EventKind`. `EventKind` is tagged `type` with snake_case variants: `run_started {prompt}`, `model_started {step}`, `text_delta {text}`, `tool_queued {call}`, `tool_started {call}`, `tool_finished {call_id, output}`, `usage {usage}`, `run_finished {summary}`, `steered {text}`, `queued {text}`, `progress {call_id, message}`, `context_trimmed {before_bytes, after_bytes}`.

Provider exports:

- `Provider` async trait: `async fn stream(&self, request: ModelRequest, cancel: CancellationToken, deltas: mpsc::Sender<ModelDelta>) -> Result<ModelResponse>`.
- `ModelRequest`: `system: String`, `messages: Vec<Message>`, `tools: Vec<ToolDefinition>`, `max_output_tokens: u32`.
- `Message`: `role: Role` (`user`, `assistant`, `tool`), `content: String`, `tool_calls: Vec<ToolCall>`, `tool_call_id: Option<String>`, `continuation: Option<ProviderContinuation>`, `is_error: bool`, `artifact: Option<PathBuf>`. New metadata defaults when absent in older journals. Use `Message::tool(call_id, &output)` for tool results so status and artifact references reach built-in providers.
- `ModelDelta::Text(String)`.
- `ModelResponse`: `content: String`, `tool_calls: Vec<ToolCall>`, `usage: Usage`, `continuation: Option<ProviderContinuation>`.
- `Usage: Default`: `input_tokens: u64`, `output_tokens: u64`, `cached_input_tokens: Option<u64>`, `reasoning_output_tokens: Option<u64>`. Optional counts are reported subsets, not extra tokens. Aggregation sums only reported detail.
- `ReplayProvider::new(responses: Vec<ModelResponse>) -> Self`; deterministic runs, no model charge. `ReplayProvider::echo()` for smoke usage.
- `OpenAiProvider::new(config: OpenAiConfig) -> Result<Self>`; config `base_url: String`, `api_key: String`, `model: String`. Chat Completions SSE, compatible with hosted DeepSeek and other compatible endpoints. `AnthropicProvider` is also included.
- `ResponsesProvider::new(ResponsesConfig, Arc<dyn CredentialSource>) -> Result<Self>` uses the public OpenAI Responses API, per-request model/effort, encrypted stateless continuation, and optional redacted HTTP capture. See [the host integration contract](ide-integration.md).
- `ClaudeCodeProvider::new(ClaudeCodeConfig, Executor) -> Result<Self>` delegates structured inference to an installed official Claude Code client. Use an executor from the harness's shared governor and `ejm_harness_auth::ClaudeCodeClient` for subscription status/sign-in. Read [the official-client contract](claude-code.md) before composing it. This Rust adapter is not a worker provider descriptor.

Tools:

- `Tool` async trait: `fn definition(&self) -> ToolDefinition`; `async fn execute(&self, call: ToolCall, context: ToolContext) -> Result<ToolOutput>`.
- `ToolDefinition`: `name: String`, `description: String`, `input_schema: Value`, `effect: Effect` (`read`, `write`, `external`), `version: String`.
- `ToolCall`: `id: String`, `name: String`, `arguments: Value`.
- `ToolOutput: Default`: `content: String`, `data: Value`, `is_error: bool`, `artifact: Option<PathBuf>`.
- `ToolContext: Clone`: `session_id: String`, `run_id: String`, `cwd: PathBuf`, `cancel: CancellationToken`, `governor: Arc<Governor>`; `progress(&self, call_id: &str, message: String) -> async Result<()>`.
- Registered tools are grants. CLI defaults to read-only tools. `--allow-write` and `--allow-shell` enable effects for this CLI process. SDK host opts in to registering each tool. Model output cannot grant tools.
- `ejm_harness_coding::tools(config: CodingConfig) -> Result<Vec<Arc<dyn Tool>>>`; `CodingConfig {allow_write: bool, allow_shell: bool, artifact_dir: PathBuf}` with Default.

Extensibility: public ContextPolicy and RunStrategy traits and `Harness::with_policies` enable bounded preparation and step choices without copying the driver. No plugin executes implicitly from repository contents.

## Worker protocol v1

One UTF-8 JSON object per line. Maximum inbound frame 1 MiB. Stdout is protocol only; diagnostics use stderr. Each client owns one persistent worker shared across sessions. Read loop must remain responsive while handlers run.

Client request: `{id: string, method: string, params: object}`. Response: `{id, result: value}` or `{id, error: {code: string, message: string}}`.

Handshake first: `hello {protocol: 1}` -> `{protocol: 1, version: "0.1.0", capabilities: ["tools", "cancel", "steer", "queue", "replay"]}`.

`configure {state_dir?, provider?: ProviderConfig, tools?: ToolDefinition[], allow_write?: bool, allow_shell?: bool, resources?: ResourceLimits, max_sessions?, max_model_requests?, max_steps?, context_max_bytes?, run_timeout_ms?, max_repeated_calls?, max_input_tokens?, max_output_tokens?, max_store_bytes?, system_prompt?}` -> `{configured: true}`. Exactly once before sessions. Default uses echo and read tools. Never log api_key. Worker registers callbacks as Rust Tool implementations.

`ProviderConfig` is a tagged union:

- `{kind: "echo"}` for offline smoke use.
- `{kind: "replay", responses: ModelResponse[]}` for deterministic fixtures.
- `{kind: "openai", base_url, api_key, model}` for compatible Chat Completions APIs, including DeepSeek.
- `{kind: "responses", api_key, model, reasoning_effort?}` for the public OpenAI Responses API.
- `{kind: "anthropic", api_key, model}` for Anthropic's public Messages API.

Responses and Anthropic use fixed public endpoints. The worker accepts API keys
for these providers. Hosts that need subscription sign-in use the Rust auth API
and supply a `CredentialSource`; browser and account UX belong to the host.

`session.create {cwd, title?}` -> Session; `session.list {}` -> Session[]; `run.start {session_id,prompt}` -> `{run_id,session_id}`; `run.cancel {session_id,run_id?}` -> `{cancelled:true}`; `run.steer {session_id,run_id?,text}` -> `{accepted:true}`; `run.queue {session_id,run_id?,text}` -> `{accepted:true}`; `events.list {session_id,after?,limit?}` -> Event[]; `governor.status {}` -> snapshot; `harness.status {}` -> `HarnessSnapshot`; `shutdown {}` -> `{closed:true}` then drain and exit.

Notification: `{method:"event",params:Event}`.

Reverse request: `{id:"tool:<uuid>",method:"tool.invoke",params:{session_id,run_id,call_id,name,arguments,deadline_ms}}`; client responds with normal response envelope and ToolOutput. Cancellation notification `{method:"tool.cancel",params:{call_id}}`; progress notification client -> worker `{method:"tool.progress",params:{call_id,message}}`. Pending invocations fail explicitly on disconnect, timeout, or cancellation. They are never replayed automatically.

The worker emits `run_finished` for completion. SDK `Run` is async iterable, has `wait`, `cancel`, `steer`, `queue`; `wait` handles finished-before-subscribe races through buffered terminal results or replay.

## Acceptance and evidence

Prove engine runs, streaming, native and callback tools, history/resume, cancellation, turn queue/steering, exact resource admission and release, noisy commands, child cleanup, disconnected callbacks, bounded events and slow consumers, CLI terminal recovery, and 100 managed sessions with deterministic fixtures. Real hosted API requests require credentials and are not part of tests. Cross-platform enforcement and old-laptop claims require separate evidence.
