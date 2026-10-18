use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, anyhow, bail, ensure};
use async_trait::async_trait;
use ejm_harness::{
    AnthropicConfig, AnthropicProvider, Harness, HarnessConfig, OpenAiConfig, OpenAiProvider,
    Provider, ReplayProvider, ResponsesConfig, ResponsesProvider, Tool, ToolCall, ToolContext,
    ToolDefinition, ToolOutput,
};
use ejm_harness_auth::{SecretString, StaticCredential};
use ejm_harness_exec::{Lease, Reservation};
use ejm_harness_package_spec::{MANIFEST_FILE, MAX_MANIFEST_BYTES, Manifest};
use ejm_harness_packages::{
    InstalledPackage, PackageContext, RuntimeConfig, RuntimePool, read_bounded,
};
use serde::Serialize;
use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt},
    sync::{OwnedSemaphorePermit, Semaphore, mpsc, oneshot},
    task::JoinSet,
};
use tokio_util::sync::CancellationToken;

use crate::protocol::{
    ClientCall, ConfigureParams, HelloResult, MAX_FRAME_BYTES, Notification, PROTOCOL_VERSION,
    PackageConfig, PackageInfo, PackagePage, ProviderConfig, Request, Response, ResponsePayload,
    RunStarted, ToolCancellation, ToolInvocation, ToolMethod, ToolProgress, ToolRequest,
};

const MAX_PENDING: usize = 64;
const OUTPUT_BYTES: usize = 4 * MAX_FRAME_BYTES;
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);
const CLEANUP_TIMEOUT: Duration = Duration::from_secs(5);
const CONFIGURE_TIMEOUT: Duration = Duration::from_secs(20);

struct Frame {
    bytes: Vec<u8>,
    _permit: OwnedSemaphorePermit,
}

#[derive(Clone)]
struct Outbound {
    tx: mpsc::Sender<Frame>,
    bytes: Arc<Semaphore>,
    disconnected: CancellationToken,
}

impl Outbound {
    fn send_now(&self, value: impl Serialize) {
        let frame = (|| -> Result<Frame> {
            let mut bytes = serde_json::to_vec(&value)?;
            ensure!(
                bytes.len() < MAX_FRAME_BYTES,
                "outbound frame exceeds 1 MiB"
            );
            bytes.push(b'\n');
            let permit = self
                .bytes
                .clone()
                .try_acquire_many_owned(u32::try_from(bytes.len())?)?;
            Ok(Frame {
                bytes,
                _permit: permit,
            })
        })();
        // Cancellation must reach the host, or the connection must fail all its callbacks.
        let sent = frame.is_ok_and(|frame| self.tx.try_send(frame).is_ok());
        if !sent {
            self.disconnected.cancel();
        }
    }

    async fn send(&self, value: impl Serialize) -> Result<()> {
        let mut bytes = serde_json::to_vec(&value)?;
        ensure!(
            bytes.len() < MAX_FRAME_BYTES,
            "outbound frame exceeds 1 MiB"
        );
        bytes.push(b'\n');
        let operation = async {
            let permit = self
                .bytes
                .clone()
                .acquire_many_owned(u32::try_from(bytes.len())?)
                .await?;
            self.tx
                .send(Frame {
                    bytes,
                    _permit: permit,
                })
                .await
                .map_err(|_| anyhow!("worker output closed"))
        };
        tokio::select! {
            () = self.disconnected.cancelled() => bail!("worker disconnected"),
            result = tokio::time::timeout(WRITE_TIMEOUT, operation) => {
                if result.is_err() { self.disconnected.cancel(); }
                result.context("worker output stalled")?
            }
        }
    }
}

struct Pending {
    call_id: String,
    context: ToolContext,
    reply: Option<oneshot::Sender<Result<ToolOutput>>>,
    cancelled_at: Option<Instant>,
    _lease: Lease,
    _execution: Box<dyn Send + Sync>,
}

struct Broker {
    pending: Mutex<HashMap<String, Pending>>,
    outbound: Outbound,
    closed: CancellationToken,
    timeout_ms: u64,
}

struct PendingRegistration {
    broker: Arc<Broker>,
    id: String,
    call_id: String,
}

impl Drop for PendingRegistration {
    fn drop(&mut self) {
        let notify = {
            let mut pending = self
                .broker
                .pending
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(entry) = pending.get_mut(&self.id) {
                entry.reply.take();
                let notify = entry.cancelled_at.is_none();
                entry.cancelled_at.get_or_insert_with(Instant::now);
                notify
            } else {
                false
            }
        };
        // Keep the lease and effect guard until the host confirms actual termination.
        if notify {
            self.broker
                .outbound
                .send_now(Notification::ToolCancel(ToolCancellation {
                    call_id: self.call_id.clone(),
                }));
        }
    }
}

impl Broker {
    fn fail_pending(&self) {
        self.closed.cancel();
        let pending = std::mem::take(
            &mut *self
                .pending
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );
        for (_, call) in pending {
            if let Some(reply) = call.reply {
                let _ = reply.send(Err(anyhow!("worker disconnected before tool completed")));
            }
        }
    }

    fn cleanup_expired(&self) -> bool {
        self.pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .values()
            .any(|pending| {
                pending
                    .cancelled_at
                    .is_some_and(|at| at.elapsed() >= CLEANUP_TIMEOUT)
            })
    }

    fn resolve(&self, response: Response) {
        let pending = self
            .pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&response.id);
        if let Some(pending) = pending {
            let result = match response.payload {
                ResponsePayload::Success { result } => {
                    serde_json::from_value(result).context("invalid callback tool output")
                }
                ResponsePayload::Failure { error } => {
                    Err(anyhow!("callback {}: {}", error.code, error.message))
                }
            };
            if let Some(reply) = pending.reply {
                let _ = reply.send(result);
            }
        }
    }

    fn progress(&self, progress: &ToolProgress) -> Option<ToolContext> {
        self.pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .values()
            .find(|pending| pending.call_id == progress.call_id && pending.cancelled_at.is_none())
            .map(|pending| pending.context.clone())
    }

    async fn invoke(self: &Arc<Self>, call: ToolCall, context: ToolContext) -> Result<ToolOutput> {
        ensure!(!self.closed.is_cancelled(), "worker is closing");
        let lease = tokio::select! {
            biased;
            () = self.closed.cancelled() => bail!("worker is closing"),
            lease = context.governor.acquire(&context.session_id, Reservation::default(), &context.cancel) => lease?,
        };
        ensure!(!self.closed.is_cancelled(), "worker is closing");
        let execution = context.retain_execution();
        let id = format!("tool:{}", uuid::Uuid::new_v4());
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self
                .pending
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            ensure!(pending.len() < MAX_PENDING, "callback capacity reached");
            ensure!(
                !pending.values().any(|entry| entry.call_id == call.id),
                "duplicate active callback call_id"
            );
            pending.insert(
                id.clone(),
                Pending {
                    call_id: call.id.clone(),
                    context: context.clone(),
                    reply: Some(tx),
                    cancelled_at: None,
                    _lease: lease,
                    _execution: Box::new(execution),
                },
            );
        }
        let _registration = PendingRegistration {
            broker: self.clone(),
            id: id.clone(),
            call_id: call.id.clone(),
        };
        let request = ToolRequest {
            id: id.clone(),
            method: ToolMethod::Invoke,
            params: ToolInvocation {
                session_id: context.session_id.clone(),
                run_id: context.run_id.clone(),
                call_id: call.id.clone(),
                name: call.name,
                arguments: call.arguments,
                deadline_ms: timestamp_ms().saturating_add(self.timeout_ms),
            },
        };
        async {
            self.outbound.send(request).await?;
            tokio::select! {
                biased;
                () = context.cancel.cancelled() => bail!("callback cancelled"),
                () = self.closed.cancelled() => bail!("worker closed during callback"),
                () = self.outbound.disconnected.cancelled() => bail!("client disconnected during callback"),
                result = tokio::time::timeout(Duration::from_millis(self.timeout_ms), rx) => {
                    result.context("callback timed out")?.context("callback channel closed")?
                }
            }
        }.await
    }
}

struct CallbackTool {
    definition: ToolDefinition,
    broker: Arc<Broker>,
}

#[async_trait]
impl Tool for CallbackTool {
    fn definition(&self) -> ToolDefinition {
        self.definition.clone()
    }

    fn reservation(&self) -> Option<Reservation> {
        None
    }

    async fn execute(&self, call: ToolCall, context: ToolContext) -> Result<ToolOutput> {
        self.broker.invoke(call, context).await
    }
}

#[derive(Clone)]
struct PackageHost {
    pool: RuntimePool,
    entries: Arc<Vec<PackageInfo>>,
    starts: Arc<tokio::sync::RwLock<()>>,
}

async fn load_packages(
    configs: Vec<PackageConfig>,
    cancel: CancellationToken,
) -> Result<PackageHost> {
    ensure!(
        configs.len() <= 128,
        "at most 128 packages may be configured"
    );
    let pool = RuntimePool::new(RuntimeConfig::default())?;
    let loaded = tokio::time::timeout(CONFIGURE_TIMEOUT, async {
        let mut entries = Vec::with_capacity(configs.len());
        for config in configs {
            ensure!(
                config.root.as_os_str().len() <= 4096,
                "package root exceeds 4 KiB"
            );
            let root = tokio::fs::canonicalize(&config.root)
                .await
                .context("resolve configured package root")?;
            let bytes = read_bounded(&root.join(MANIFEST_FILE), MAX_MANIFEST_BYTES)?;
            let manifest = Manifest::from_bytes(&bytes)?;
            let installed = InstalledPackage {
                manifest: manifest.clone(),
                root: root.clone(),
            };
            pool.load(installed, config.grants.clone(), cancel.child_token())
                .await?;
            entries.push(PackageInfo {
                root,
                manifest,
                grants: config.grants,
            });
        }
        Ok::<_, anyhow::Error>(entries)
    })
    .await
    .unwrap_or_else(|_| Err(anyhow!("package configuration exceeded 20 seconds")));
    match loaded {
        Ok(entries) => Ok(PackageHost {
            pool,
            entries: Arc::new(entries),
            starts: Arc::new(tokio::sync::RwLock::new(())),
        }),
        Err(error) => {
            pool.shutdown()
                .await
                .context("clean up failed package configuration")?;
            Err(error)
        }
    }
}

async fn configure(
    params: ConfigureParams,
    outbound: Outbound,
) -> Result<(Harness, Arc<Broker>, PackageHost)> {
    ensure!(
        params.tools.len() <= MAX_PENDING,
        "at most 64 callback tools may be registered"
    );
    let config = harness_config(&params);
    let packages = load_packages(params.packages, outbound.disconnected.child_token()).await?;
    let broker = Arc::new(Broker {
        pending: Mutex::new(HashMap::new()),
        outbound,
        closed: CancellationToken::new(),
        timeout_ms: config.run_timeout_ms,
    });
    let configured = async {
        let provider = match params.provider.unwrap_or(ProviderConfig::Echo) {
            ProviderConfig::Package { package, name } => packages.pool.provider(&package, &name)?,
            provider => configure_provider(provider)?,
        };
        let mut tools = ejm_harness_coding::tools(ejm_harness_coding::CodingConfig {
            allow_write: params.allow_write,
            allow_shell: params.allow_shell,
            artifact_dir: config.state_dir.join("artifacts"),
        })?;
        for definition in params.tools {
            tools.push(Arc::new(CallbackTool {
                definition,
                broker: broker.clone(),
            }));
        }
        let mut hooks = vec![];
        for package in packages.entries.iter() {
            tools.extend(packages.pool.tools(&package.manifest.name)?);
            hooks.extend(packages.pool.hooks(&package.manifest.name)?);
        }
        Harness::with_hooks(config, provider, tools, hooks)
    }
    .await;
    match configured {
        Ok(harness) => Ok((harness, broker, packages)),
        Err(error) => {
            packages
                .pool
                .shutdown()
                .await
                .context("clean up invalid harness configuration")?;
            Err(error)
        }
    }
}

fn harness_config(params: &ConfigureParams) -> HarnessConfig {
    let mut config = HarnessConfig::default();
    if let Some(value) = &params.state_dir {
        config.state_dir.clone_from(value);
    }
    if let Some(value) = &params.resources {
        config.resources.clone_from(value);
    }
    if let Some(value) = params.max_sessions {
        config.max_sessions = value;
    }
    if let Some(value) = params.max_model_requests {
        config.max_model_requests = value;
    }
    if let Some(value) = params.max_steps {
        config.max_steps = value;
    }
    if let Some(value) = params.context_max_bytes {
        config.context_max_bytes = value;
    }
    if let Some(value) = params.run_timeout_ms {
        config.run_timeout_ms = value;
    }
    if let Some(value) = params.max_repeated_calls {
        config.max_repeated_calls = value;
    }
    if let Some(value) = params.max_input_tokens {
        config.max_input_tokens = value;
    }
    if let Some(value) = params.max_output_tokens {
        config.max_output_tokens = value;
    }
    if let Some(value) = params.max_store_bytes {
        config.max_store_bytes = value;
    }
    if let Some(value) = &params.system_prompt {
        config.system_prompt.clone_from(value);
    }
    config
}

fn configure_provider(config: ProviderConfig) -> Result<Arc<dyn Provider>> {
    if let ProviderConfig::Responses { api_key, .. } | ProviderConfig::Anthropic { api_key, .. } =
        &config
    {
        ensure!(!api_key.is_empty(), "API key is required");
        ensure!(
            api_key.len() <= 64 * 1024 && api_key.bytes().all(|byte| byte.is_ascii_graphic()),
            "API key must contain at most 64 KiB of visible ASCII characters"
        );
    }
    Ok(match config {
        ProviderConfig::Echo => Arc::new(ReplayProvider::echo()),
        ProviderConfig::Replay { responses } => Arc::new(ReplayProvider::new(responses)),
        ProviderConfig::Openai {
            base_url,
            api_key,
            model,
        } => Arc::new(OpenAiProvider::new(OpenAiConfig {
            base_url,
            api_key,
            model,
        })?),
        ProviderConfig::Responses {
            api_key,
            model,
            reasoning_effort,
        } => Arc::new(ResponsesProvider::new(
            ResponsesConfig {
                model,
                reasoning_effort,
            },
            Arc::new(StaticCredential::new(SecretString::new(api_key))),
        )?),
        ProviderConfig::Anthropic { api_key, model } => {
            Arc::new(AnthropicProvider::new(AnthropicConfig {
                base_url: "https://api.anthropic.com/v1".into(),
                api_key,
                model,
            })?)
        }
        ProviderConfig::Package { .. } => {
            bail!("package providers require explicit package configuration")
        }
    })
}

async fn dispatch(
    harness: &Harness,
    packages: &PackageHost,
    call: ClientCall,
    outbound: &Outbound,
    closing: &CancellationToken,
) -> Result<Value> {
    Ok(match call {
        ClientCall::SessionCreate(options) => {
            serde_json::to_value(harness.create_session(options).await?)?
        }
        ClientCall::SessionList(_) => serde_json::to_value(harness.list_sessions().await?)?,
        ClientCall::RunStart(params) => {
            let _start = packages.starts.read().await;
            let mut run = harness.start(&params.session_id, params.prompt).await?;
            let result = serde_json::to_value(RunStarted {
                run_id: run.id.clone(),
                session_id: run.session_id.clone(),
            })?;
            let harness = harness.clone();
            let outbound = outbound.clone();
            let closing = closing.clone();
            tokio::spawn(async move {
                let outcome = run.wait().await;
                if (outcome.is_err() || harness.is_stopped()) && !closing.is_cancelled() {
                    outbound.disconnected.cancel();
                }
            });
            result
        }
        ClientCall::RunCancel(params) => {
            if let Some(run_id) = params.run_id {
                harness.cancel_run(&params.session_id, &run_id).await?;
            } else {
                harness.cancel(&params.session_id).await?;
            }
            json!({"cancelled":true})
        }
        ClientCall::RunSteer(params) => {
            if let Some(run_id) = params.run_id {
                harness
                    .steer_run(&params.session_id, &run_id, params.text)
                    .await?;
            } else {
                harness.steer(&params.session_id, params.text).await?;
            }
            json!({"accepted":true})
        }
        ClientCall::RunQueue(params) => {
            let _start = packages.starts.read().await;
            if let Some(run_id) = params.run_id {
                harness
                    .enqueue_for_run(&params.session_id, &run_id, params.text)
                    .await?;
            } else {
                harness.enqueue(&params.session_id, params.text).await?;
            }
            json!({"accepted":true})
        }
        ClientCall::EventsList(params) => {
            let mut events = harness
                .events(&params.session_id, params.after, params.limit.clamp(1, 64))
                .await?;
            while serde_json::to_vec(&events)?.len() > MAX_FRAME_BYTES / 2 {
                ensure!(events.len() > 1, "event exceeds worker frame capacity");
                events.pop();
            }
            serde_json::to_value(events)?
        }
        ClientCall::GovernorStatus(_) => serde_json::to_value(harness.governor().snapshot())?,
        ClientCall::HarnessStatus(_) => serde_json::to_value(harness.status().await)?,
        call @ (ClientCall::PackageList(_)
        | ClientCall::PackageReload(_)
        | ClientCall::PackageCommand(_)
        | ClientCall::PackageWidget(_)) => {
            dispatch_package(harness, packages, call, closing).await?
        }
        _ => bail!("method is invalid in this state"),
    })
}

async fn dispatch_package(
    harness: &Harness,
    packages: &PackageHost,
    call: ClientCall,
    closing: &CancellationToken,
) -> Result<Value> {
    match call {
        ClientCall::PackageList(params) => {
            ensure!(
                params.after <= packages.entries.len(),
                "package page cursor is out of range"
            );
            let end = params
                .after
                .saturating_add(params.limit.clamp(1, 16))
                .min(packages.entries.len());
            serde_json::to_value(PackagePage {
                packages: packages.entries[params.after..end].to_vec(),
                next: (end < packages.entries.len()).then_some(end),
            })
            .map_err(Into::into)
        }
        ClientCall::PackageReload(params) => {
            let _reload = packages.starts.write().await;
            ensure!(
                harness.status().await.active_agents == 0,
                "wait for active runs before reloading packages"
            );
            let frozen = packages
                .entries
                .iter()
                .find(|package| package.manifest.name == params.package)
                .context("package is not configured")?;
            let current = Manifest::from_bytes(&read_bounded(
                &frozen.root.join(MANIFEST_FILE),
                MAX_MANIFEST_BYTES,
            )?)?;
            ensure!(
                current == frozen.manifest,
                "package manifest changed; configure a new worker to review exports and grants"
            );
            packages
                .pool
                .reload(&params.package, closing.child_token())
                .await?;
            Ok(json!({"reloaded":true}))
        }
        call @ (ClientCall::PackageCommand(_) | ClientCall::PackageWidget(_)) => {
            let widget = matches!(&call, ClientCall::PackageWidget(_));
            let (ClientCall::PackageCommand(params) | ClientCall::PackageWidget(params)) = call
            else {
                bail!("invalid package invocation");
            };
            let _invocation = packages.starts.read().await;
            let session = harness.get_session(&params.session_id).await?;
            let context = PackageContext {
                session_id: session.id,
                cwd: session.cwd,
                cancel: closing.child_token(),
                governor: harness.governor(),
            };
            if widget {
                packages
                    .pool
                    .render_widget(&params.package, &params.name, params.arguments, context)
                    .await
            } else {
                packages
                    .pool
                    .invoke_command(&params.package, &params.name, params.arguments, context)
                    .await
            }
        }
        _ => bail!("invalid package method"),
    }
}

/// Read without allowing an unterminated or oversized input line to grow memory.
async fn read_frame(reader: &mut (impl AsyncBufRead + Unpin)) -> Result<Option<Vec<u8>>> {
    let mut frame = Vec::new();
    loop {
        let chunk = reader.fill_buf().await?;
        if chunk.is_empty() {
            ensure!(frame.is_empty(), "unterminated JSON frame");
            return Ok(None);
        }
        let newline = chunk.iter().position(|byte| *byte == b'\n');
        let length = newline.map_or(chunk.len(), |index| index + 1);
        ensure!(
            frame.len() + length <= MAX_FRAME_BYTES,
            "inbound frame exceeds 1 MiB"
        );
        frame.extend_from_slice(&chunk[..length]);
        reader.consume(length);
        if newline.is_some() {
            frame.pop();
            if frame.last() == Some(&b'\r') {
                frame.pop();
            }
            return Ok(Some(frame));
        }
    }
}

struct Connection {
    outbound: Outbound,
    capacity: Arc<Semaphore>,
    progress_capacity: Arc<Semaphore>,
    handlers: JoinSet<()>,
    hello: bool,
    harness: Option<Harness>,
    broker: Option<Arc<Broker>>,
    packages: Option<PackageHost>,
    event_task: Option<tokio::task::JoinHandle<()>>,
    explicit_shutdown: bool,
}

impl Connection {
    async fn read(&mut self, reader: &mut (impl AsyncBufRead + Unpin)) -> Result<()> {
        loop {
            while self.handlers.try_join_next().is_some() {}
            let frame = tokio::select! {
                () = self.outbound.disconnected.cancelled() => bail!("client output disconnected"),
                result = read_frame(reader) => result?,
            };
            let Some(frame) = frame else {
                break;
            };
            let value: Value = serde_json::from_slice(&frame).context("invalid JSON frame")?;
            if value.get("method").is_none() {
                let response: Response =
                    serde_json::from_value(value).context("invalid callback response")?;
                if let Some(broker) = &self.broker {
                    broker.resolve(response);
                }
                continue;
            }
            if value.get("id").is_none() {
                if let Notification::ToolProgress(progress) =
                    serde_json::from_value(value).context("invalid notification")?
                {
                    ensure!(
                        progress.message.len() <= 16 * 1024,
                        "tool progress exceeds 16 KiB"
                    );
                    if let Some(context) = self
                        .broker
                        .as_ref()
                        .and_then(|broker| broker.progress(&progress))
                        && let Ok(permit) = self.progress_capacity.clone().try_acquire_owned()
                    {
                        self.handlers.spawn(async move {
                            let _permit = permit;
                            let _ = context.progress(&progress.call_id, progress.message).await;
                        });
                    }
                }
                continue;
            }
            let id = value
                .get("id")
                .and_then(Value::as_str)
                .context("request id must be a string")?
                .to_owned();
            ensure!(id.len() <= 128, "request id exceeds 128 bytes");
            let request: Request = if let Ok(request) = serde_json::from_value(value) {
                request
            } else {
                self.outbound
                    .send(Response::error(
                        id,
                        "invalid_request",
                        "unknown method or invalid params",
                    ))
                    .await?;
                continue;
            };
            self.request(request).await?;
            if self.explicit_shutdown {
                break;
            }
        }
        Ok(())
    }

    async fn request(&mut self, request: Request) -> Result<()> {
        let id = request.id;
        match request.call {
            ClientCall::Hello(params) => {
                if self.hello || params.protocol != PROTOCOL_VERSION {
                    self.outbound
                        .send(Response::error(
                            id,
                            "protocol",
                            "hello requires protocol 1 and must be called once",
                        ))
                        .await?;
                } else {
                    self.hello = true;
                    self.outbound
                        .send(Response::ok(
                            id,
                            HelloResult {
                                protocol: PROTOCOL_VERSION,
                                version: env!("CARGO_PKG_VERSION").into(),
                                capabilities: [
                                    "tools",
                                    "cancel",
                                    "steer",
                                    "queue",
                                    "replay",
                                    "run_controls",
                                    "packages",
                                    "package_provider",
                                    "package_hooks",
                                ]
                                .map(String::from)
                                .to_vec(),
                            },
                        )?)
                        .await?;
                }
            }
            ClientCall::Configure(params) if self.hello && self.harness.is_none() => {
                self.configure(id, *params).await?;
            }
            ClientCall::Shutdown(_) if self.hello => {
                if let Some(broker) = &self.broker {
                    broker.closed.cancel();
                }
                self.outbound
                    .send(Response::ok(id, json!({"closed":true}))?)
                    .await?;
                self.explicit_shutdown = true;
            }
            call => self.dispatch(id, call).await?,
        }
        Ok(())
    }

    async fn configure(&mut self, id: String, params: ConfigureParams) -> Result<()> {
        match configure(params, self.outbound.clone()).await {
            Ok((configured, callbacks, packages)) => {
                let mut events = configured.subscribe();
                let output = self.outbound.clone();
                let watchdog = callbacks.clone();
                let journal = configured.clone();
                self.event_task = Some(tokio::spawn(async move {
                    let mut cleanup = tokio::time::interval(Duration::from_millis(100));
                    loop {
                        let event = tokio::select! {
                            () = output.disconnected.cancelled() => break,
                            _ = cleanup.tick() => {
                                if watchdog.cleanup_expired() || (journal.is_stopped() && !watchdog.closed.is_cancelled()) { output.disconnected.cancel(); break; }
                                continue;
                            }
                            event = events.recv() => event,
                        };
                        match event {
                            Ok(event) => {
                                if output.send(Notification::Event(event)).await.is_err() {
                                    break;
                                }
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                        }
                    }
                }));
                self.harness = Some(configured);
                self.broker = Some(callbacks);
                self.packages = Some(packages);
                self.outbound
                    .send(Response::ok(id, json!({"configured":true}))?)
                    .await?;
            }
            Err(error) => {
                self.outbound
                    .send(Response::error(id, "configuration", error.to_string()))
                    .await?;
            }
        }
        Ok(())
    }

    async fn dispatch(&mut self, id: String, call: ClientCall) -> Result<()> {
        let Some(configured) = &self.harness else {
            return self
                .outbound
                .send(Response::error(
                    id,
                    "state",
                    "call hello and configure before session methods",
                ))
                .await;
        };
        if matches!(call, ClientCall::Configure(_)) {
            return self
                .outbound
                .send(Response::error(id, "state", "worker is already configured"))
                .await;
        }
        let Ok(permit) = self.capacity.clone().try_acquire_owned() else {
            return self
                .outbound
                .send(Response::error(
                    id,
                    "busy",
                    "at most 64 requests may be pending",
                ))
                .await;
        };
        let output = self.outbound.clone();
        let configured = configured.clone();
        let packages = self
            .packages
            .as_ref()
            .context("configured packages missing")?
            .clone();
        let closing = self
            .broker
            .as_ref()
            .context("configured broker missing")?
            .closed
            .clone();
        self.handlers.spawn(async move {
            let _permit = permit;
            let response = match dispatch(&configured, &packages, call, &output, &closing).await {
                Ok(value) => Response::ok(id.clone(), value).unwrap_or_else(|error| {
                    Response::error(id.clone(), "serialization", error.to_string())
                }),
                Err(error) => Response::error(id.clone(), "harness", error.to_string()),
            };
            if output.send(response).await.is_err() {
                let _ = output
                    .send(Response::error(
                        id,
                        "frame_limit",
                        "response exceeds worker frame capacity",
                    ))
                    .await;
            }
        });
        Ok(())
    }

    async fn close(&mut self) {
        if !self.explicit_shutdown {
            self.outbound.disconnected.cancel();
        }
        if let Some(harness) = &self.harness {
            harness.request_shutdown();
        }
        if let Some(broker) = &self.broker {
            broker.fail_pending();
        }
        if let Some(harness) = &self.harness {
            let _ = harness.shutdown().await;
        }
        if let Some(packages) = &self.packages {
            let _ = packages.pool.shutdown().await;
        }
        self.handlers.abort_all();
        while self.handlers.join_next().await.is_some() {}
        if let Some(task) = self.event_task.take() {
            task.abort();
            let _ = task.await;
        }
    }
}

/// Serve one host. Stdout carries protocol frames only.
///
/// # Errors
/// Returns an error for invalid input, a disconnected transport, or a stalled output pipe.
pub async fn serve<R, W>(mut reader: R, mut writer: W) -> Result<()>
where
    R: AsyncBufRead + Unpin + Send,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let (tx, mut rx) = mpsc::channel::<Frame>(MAX_PENDING);
    let disconnected = CancellationToken::new();
    let outbound = Outbound {
        tx,
        bytes: Arc::new(Semaphore::new(OUTPUT_BYTES)),
        disconnected: disconnected.clone(),
    };
    let writer_task = tokio::spawn(async move {
        while let Some(frame) = rx.recv().await {
            let write = async {
                writer.write_all(&frame.bytes).await?;
                writer.flush().await
            };
            tokio::select! {
                () = disconnected.cancelled() => break,
                result = tokio::time::timeout(WRITE_TIMEOUT, write) => {
                    if !matches!(result, Ok(Ok(()))) { disconnected.cancel(); break; }
                }
            }
        }
    });
    let mut connection = Connection {
        outbound,
        capacity: Arc::new(Semaphore::new(MAX_PENDING)),
        progress_capacity: Arc::new(Semaphore::new(4)),
        handlers: JoinSet::new(),
        hello: false,
        harness: None,
        broker: None,
        packages: None,
        event_task: None,
        explicit_shutdown: false,
    };
    let outcome = connection.read(&mut reader).await;
    connection.close().await;
    drop(connection);
    let _ = tokio::time::timeout(WRITE_TIMEOUT, writer_task).await;
    outcome
}

fn timestamp_ms() -> u64 {
    u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
    )
    .unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ejm_harness::{Effect, ModelResponse, RunStatus, SessionOptions};

    #[test]
    fn native_provider_configuration_is_local_and_validated() -> Result<()> {
        for value in [
            json!({"kind":"responses","api_key":"fixture-key","model":"fixture-model"}),
            json!({"kind":"responses","api_key":"fixture-key","model":"fixture-model","reasoning_effort":"high"}),
            json!({"kind":"anthropic","api_key":"fixture-key","model":"fixture-model"}),
        ] {
            configure_provider(serde_json::from_value(value)?)?;
        }
        for value in [
            json!({"kind":"responses","api_key":"","model":"fixture-model"}),
            json!({"kind":"anthropic","api_key":" ","model":"fixture-model"}),
            json!({"kind":"responses","api_key":"private-fixture-key\r\n","model":"fixture-model"}),
            json!({"kind":"anthropic","api_key":"private-fixture-key\n","model":"fixture-model"}),
            json!({"kind":"responses","api_key":"private-fixture-key","model":" "}),
            json!({"kind":"anthropic","api_key":"private-fixture-key","model":""}),
            json!({"kind":"responses","api_key":"private-fixture-key","model":"fixture-model","reasoning_effort":"unknown"}),
        ] {
            let error = configure_provider(serde_json::from_value(value)?)
                .err()
                .context("invalid provider configuration was accepted")?;
            assert!(!error.to_string().contains("private-fixture-key"));
        }
        Ok(())
    }

    #[test]
    fn provider_configuration_debug_redacts_api_keys() -> Result<()> {
        for kind in ["openai", "responses", "anthropic"] {
            let provider = json!({
                "kind": kind,
                "api_key": "private-fixture-key",
                "model": "fixture-model",
                "base_url": "https://user:private-fixture-key@fixture.invalid/v1"
            });
            let params: ConfigureParams = serde_json::from_value(json!({"provider": provider}))?;
            let debug = format!("{params:?}");
            assert!(!debug.contains("private-fixture-key"));
            assert!(debug.contains("[REDACTED]"));
        }
        Ok(())
    }

    #[tokio::test]
    async fn frame_reader_rejects_oversize_and_truncated_input() -> Result<()> {
        let bytes = vec![b'x'; MAX_FRAME_BYTES + 1];
        assert!(read_frame(&mut std::io::Cursor::new(bytes)).await.is_err());
        assert!(
            read_frame(&mut std::io::Cursor::new(b"{}".to_vec()))
                .await
                .is_err()
        );
        assert_eq!(
            read_frame(&mut std::io::Cursor::new(b"{}\r\n".to_vec())).await?,
            Some(b"{}".to_vec())
        );
        Ok(())
    }

    #[tokio::test]
    async fn cancelled_callbacks_retain_admission_until_host_confirms_cleanup() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let (tx, mut rx) = mpsc::channel(MAX_PENDING);
        let outbound = Outbound {
            tx,
            bytes: Arc::new(Semaphore::new(OUTPUT_BYTES)),
            disconnected: CancellationToken::new(),
        };
        let definition = ToolDefinition {
            name: "host_tool".into(),
            description: "test callback".into(),
            input_schema: json!({"type":"object"}),
            effect: Effect::Read,
            version: "1".into(),
        };
        let params = ConfigureParams {
            state_dir: Some(directory.path().join("state")),
            provider: Some(ProviderConfig::Replay {
                responses: vec![ModelResponse {
                    tool_calls: vec![ToolCall {
                        id: "provider-call".into(),
                        name: "host_tool".into(),
                        arguments: json!({}),
                    }],
                    ..ModelResponse::default()
                }],
            }),
            tools: vec![definition],
            run_timeout_ms: Some(10_000),
            ..ConfigureParams::default()
        };
        let (harness, broker, _packages) = configure(params, outbound).await?;
        let session = harness
            .create_session(SessionOptions {
                cwd: directory.path().to_owned(),
                title: String::new(),
            })
            .await?;
        let mut run = harness.start(&session.id, "invoke callback".into()).await?;
        let frame = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await?
            .context("missing tool invocation")?;
        let request: ToolRequest = serde_json::from_slice(&frame.bytes)?;
        assert_eq!(
            broker
                .pending
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .len(),
            1
        );
        harness.cancel_run(&session.id, &run.id).await?;
        let summary = tokio::time::timeout(Duration::from_secs(2), run.wait()).await??;
        assert_eq!(summary.status, RunStatus::Cancelled);
        let frame = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await?
            .context("missing tool cancellation")?;
        let notification: Notification = serde_json::from_slice(&frame.bytes)?;
        assert!(
            matches!(notification, Notification::ToolCancel(ToolCancellation { call_id }) if call_id == request.params.call_id)
        );
        assert_eq!(
            broker
                .pending
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .len(),
            1
        );
        assert_eq!(harness.governor().snapshot().running, 1);
        broker.resolve(Response::error(
            request.id,
            "cancelled",
            "host cleanup completed",
        ));
        assert!(
            broker
                .pending
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .is_empty()
        );
        assert_eq!(harness.governor().snapshot().running, 0);
        harness.shutdown().await?;
        Ok(())
    }

    #[tokio::test]
    async fn saturated_cancellation_output_disconnects_instead_of_leaking_callback() -> Result<()> {
        let (tx, _rx) = mpsc::channel(1);
        let outbound = Outbound {
            tx,
            bytes: Arc::new(Semaphore::new(OUTPUT_BYTES)),
            disconnected: CancellationToken::new(),
        };
        outbound.send(json!({"first":true})).await?;
        outbound.send_now(Notification::ToolCancel(ToolCancellation {
            call_id: "call".into(),
        }));
        assert!(outbound.disconnected.is_cancelled());
        Ok(())
    }

    async fn rpc(
        reader: &mut (impl AsyncBufRead + Unpin),
        writer: &mut (impl AsyncWrite + Unpin),
        method: &str,
        params: Value,
    ) -> Result<Value> {
        let id = uuid::Uuid::new_v4().to_string();
        let request = serde_json::to_vec(&json!({"id":id,"method":method,"params":params}))?;
        writer.write_all(&request).await?;
        writer.write_all(b"\n").await?;
        let frame = tokio::time::timeout(Duration::from_secs(2), read_frame(reader))
            .await??
            .context("missing response")?;
        let response: Value = serde_json::from_slice(&frame)?;
        assert_eq!(response["id"], id);
        Ok(response)
    }

    #[tokio::test]
    async fn handshake_and_configuration_are_ordered_and_shutdown_drains() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let (client, server) = tokio::io::duplex(16 * 1024);
        let (server_read, server_write) = tokio::io::split(server);
        let task = tokio::spawn(serve(tokio::io::BufReader::new(server_read), server_write));
        let (client_read, mut client_write) = tokio::io::split(client);
        let mut reader = tokio::io::BufReader::new(client_read);
        assert_eq!(
            rpc(&mut reader, &mut client_write, "session.list", json!({})).await?["error"]["code"],
            "state"
        );
        assert_eq!(
            rpc(
                &mut reader,
                &mut client_write,
                "hello",
                json!({"protocol":2})
            )
            .await?["error"]["code"],
            "protocol"
        );
        assert_eq!(
            rpc(
                &mut reader,
                &mut client_write,
                "hello",
                json!({"protocol":1})
            )
            .await?["result"]["protocol"],
            1
        );
        assert_eq!(
            rpc(
                &mut reader,
                &mut client_write,
                "configure",
                json!({"state_dir":directory.path()})
            )
            .await?["result"]["configured"],
            true
        );
        assert_eq!(
            rpc(&mut reader, &mut client_write, "configure", json!({})).await?["error"]["code"],
            "state"
        );
        assert_eq!(
            rpc(&mut reader, &mut client_write, "shutdown", json!({})).await?["result"]["closed"],
            true
        );
        tokio::time::timeout(Duration::from_secs(2), task).await???;
        Ok(())
    }
}
