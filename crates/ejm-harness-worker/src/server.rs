use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, anyhow, bail, ensure};
use async_trait::async_trait;
use ejm_harness::{
    AnthropicConfig, AnthropicProvider, CapacityContext, CapacityPolicy, Harness, HarnessConfig,
    OpenAiConfig, OpenAiProvider, Provider, ProviderCapacity, QuotaDomain, ResponsesConfig,
    ResponsesProvider, SessionTailOptions, Tool, ToolCall, ToolContext, ToolDefinition, ToolOutput,
    reset_capacity_learning,
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
    CapacityResetResult, ClientCall, ConfigureParams, EventsParams, HelloResult, MAX_FRAME_BYTES,
    Notification, PROTOCOL_VERSION, PackageConfig, PackageInfo, PackagePage, ProviderConfig,
    Request, Response, ResponsePayload, RunStarted, ToolCancellation, ToolInvocation, ToolMethod,
    ToolProgress, ToolRequest,
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

enum PreparedProvider {
    Native(Arc<dyn Provider>),
    Package { package: String, name: String },
}

async fn load_packages(
    configs: Vec<PackageConfig>,
    cancel: CancellationToken,
    capacity: Option<&CapacityContext>,
) -> Result<PackageHost> {
    ensure!(
        configs.len() <= 128,
        "at most 128 packages may be configured"
    );
    let pool = match capacity {
        Some(capacity) => RuntimePool::with_capacity(RuntimeConfig::default(), capacity.clone())?,
        None => RuntimePool::new(RuntimeConfig::default())?,
    };
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
    configure_with(params, outbound, CapacityContext::prepare).await
}

async fn configure_with<F>(
    params: ConfigureParams,
    outbound: Outbound,
    prepare: F,
) -> Result<(Harness, Arc<Broker>, PackageHost)>
where
    F: FnOnce(&mut HarnessConfig, CapacityPolicy) -> Result<CapacityContext> + Send + 'static,
{
    ensure!(
        params.tools.len() <= MAX_PENDING,
        "at most 64 callback tools may be registered"
    );
    let mut config = harness_config(&params);
    let quota = provider_capacity(&params)?;
    let policy = capacity_policy(&params);
    let provider = match params
        .provider
        .context("provider is required; configure an HTTP or trusted package provider")?
    {
        ProviderConfig::Package { package, name } => PreparedProvider::Package { package, name },
        provider => PreparedProvider::Native(configure_provider(provider, quota)?),
    };
    let capacity = if let Some(policy) = policy {
        let prepared = tokio::task::spawn_blocking(move || {
            let capacity = prepare(&mut config, policy)?;
            Ok::<_, anyhow::Error>((config, capacity))
        });
        let (prepared_config, capacity) = tokio::select! {
            () = outbound.disconnected.cancelled() => bail!("capacity configuration cancelled"),
            result = prepared => result.context("prepare worker capacity")??,
        };
        config = prepared_config;
        Some(capacity)
    } else {
        None
    };
    let packages = load_packages(
        params.packages,
        outbound.disconnected.child_token(),
        capacity.as_ref(),
    )
    .await?;
    let broker = Arc::new(Broker {
        pending: Mutex::new(HashMap::new()),
        outbound,
        closed: CancellationToken::new(),
        timeout_ms: config.run_timeout_ms,
    });
    let configured = async {
        let provider = match provider {
            PreparedProvider::Native(provider) => provider,
            PreparedProvider::Package { package, name } => {
                packages.pool.provider(&package, &name)?
            }
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
        match capacity {
            Some(capacity) => {
                Harness::with_capacity(config, Some(provider), tools, hooks, capacity)
            }
            None => Harness::with_hooks(config, provider, tools, hooks),
        }
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

fn provider_capacity(params: &ConfigureParams) -> Result<Option<ProviderCapacity>> {
    let Some(quota) = &params.provider_capacity else {
        return Ok(None);
    };
    ensure!(
        params.capacity.is_some(),
        "provider_capacity requires an explicit capacity mode"
    );
    let origin = match &params.provider {
        Some(ProviderConfig::Openai { base_url, .. }) => base_url.as_str(),
        Some(ProviderConfig::Responses { .. }) => "https://api.openai.com",
        Some(ProviderConfig::Anthropic { .. }) => "https://api.anthropic.com",
        _ => bail!("provider_capacity requires an HTTP provider"),
    };
    Ok(Some(ProviderCapacity {
        domain: Some(QuotaDomain::new(
            origin,
            &quota.account_uuid,
            &quota.family,
        )?),
        max_in_flight: quota.max_in_flight,
        requests: quota.requests,
        tokens: quota.tokens,
    }))
}

fn capacity_policy(params: &ConfigureParams) -> Option<CapacityPolicy> {
    fn ceiling<T: Ord>(policy: Option<T>, legacy: Option<T>) -> Option<T> {
        match (policy, legacy) {
            (Some(policy), Some(legacy)) => Some(policy.min(legacy)),
            (policy, legacy) => policy.or(legacy),
        }
    }
    let mut policy = params.capacity?;
    // Legacy explicit limits remain ceilings when a client opts into coordination.
    policy.max_sessions = ceiling(policy.max_sessions, params.max_sessions);
    policy.max_model_requests = ceiling(policy.max_model_requests, params.max_model_requests);
    if let Some(resources) = &params.resources {
        policy.max_jobs = ceiling(policy.max_jobs, Some(resources.max_jobs));
        policy.max_memory_bytes = ceiling(policy.max_memory_bytes, Some(resources.memory_bytes));
        policy.max_cpu_millis = ceiling(policy.max_cpu_millis, Some(resources.cpu_millis));
        policy.max_heavy_jobs = ceiling(policy.max_heavy_jobs, Some(resources.max_heavy_jobs));
        policy.max_processes = ceiling(policy.max_processes, Some(resources.max_processes));
        policy.max_output_bytes =
            ceiling(policy.max_output_bytes, Some(resources.max_output_bytes));
    }
    Some(policy)
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

fn configure_provider(
    config: ProviderConfig,
    capacity: Option<ProviderCapacity>,
) -> Result<Arc<dyn Provider>> {
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
        ProviderConfig::Openai {
            base_url,
            api_key,
            model,
        } => Arc::new(
            OpenAiProvider::new(OpenAiConfig {
                base_url,
                api_key,
                model,
            })?
            .with_capacity(capacity.unwrap_or_default())?,
        ),
        ProviderConfig::Responses {
            api_key,
            model,
            reasoning_effort,
        } => Arc::new(
            ResponsesProvider::new(
                ResponsesConfig {
                    model,
                    reasoning_effort,
                },
                Arc::new(StaticCredential::new(SecretString::new(api_key))),
            )?
            .with_capacity(capacity.unwrap_or_default())?,
        ),
        ProviderConfig::Anthropic { api_key, model } => Arc::new(
            AnthropicProvider::new(AnthropicConfig {
                base_url: "https://api.anthropic.com/v1".into(),
                api_key,
                model,
            })?
            .with_capacity(capacity.unwrap_or_default())?,
        ),
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
        ClientCall::SessionPage(query) => session_page(harness, query).await?,
        ClientCall::SessionResolve(params) => {
            serde_json::to_value(harness.resolve_session(&params.prefix).await?)?
        }
        ClientCall::SessionView(params) => serde_json::to_value(
            harness
                .session_view(&params.session_id, worker_bounds(params.bounds))
                .await?,
        )?,
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
        ClientCall::EventsList(params) => events_list(harness, params).await?,
        ClientCall::EventsPage(params) => serde_json::to_value(
            harness
                .events_page(
                    &params.session_id,
                    params.after,
                    params.through,
                    worker_bounds(params.bounds),
                )
                .await?,
        )?,
        ClientCall::CapacityStatus(_) => serde_json::to_value(harness.capacity_snapshot())?,
        ClientCall::CapacityReset(_) => {
            let profiles_reset = tokio::task::spawn_blocking(reset_capacity_learning)
                .await
                .context("reset capacity learning")??;
            serde_json::to_value(CapacityResetResult { profiles_reset })?
        }
        ClientCall::GovernorStatus(_) => serde_json::to_value(harness.governor().snapshot())?,
        ClientCall::HarnessStatus(_) => serde_json::to_value(harness.status().await)?,
        ClientCall::SchedulingStatus(_) => serde_json::to_value(harness.scheduling_snapshot())?,
        call @ (ClientCall::PackageList(_)
        | ClientCall::PackageReload(_)
        | ClientCall::PackageCommand(_)
        | ClientCall::PackageWidget(_)) => {
            dispatch_package(harness, packages, call, closing).await?
        }
        _ => bail!("method is invalid in this state"),
    })
}

async fn events_list(harness: &Harness, params: EventsParams) -> Result<Value> {
    let mut events = harness
        .events(&params.session_id, params.after, params.limit.clamp(1, 64))
        .await?;
    while serde_json::to_vec(&events)?.len() > MAX_FRAME_BYTES / 2 {
        ensure!(events.len() > 1, "event exceeds worker frame capacity");
        events.pop();
    }
    Ok(serde_json::to_value(events)?)
}

async fn session_page(harness: &Harness, mut query: ejm_harness::SessionQuery) -> Result<Value> {
    query.limit = query.limit.clamp(1, 50);
    loop {
        let page = harness.sessions_page(query.clone()).await?;
        if serde_json::to_vec(&page)?.len() <= MAX_FRAME_BYTES / 2 {
            return Ok(serde_json::to_value(page)?);
        }
        ensure!(query.limit > 1, "session exceeds worker frame capacity");
        query.limit = query.limit.div_ceil(2);
    }
}

fn worker_bounds(bounds: SessionTailOptions) -> SessionTailOptions {
    SessionTailOptions {
        max_events: bounds.max_events.min(200),
        max_bytes: bounds.max_bytes.min(256 * 1024),
    }
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
                                    "capacity",
                                    "provider_capacity",
                                    "session_views",
                                    "scheduling",
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
    use wiremock::{Mock, MockServer, ResponseTemplate, matchers::any};

    fn native_fixture() -> ProviderConfig {
        ProviderConfig::Openai {
            base_url: "http://127.0.0.1:1/v1".into(),
            api_key: String::new(),
            model: "fixture-model".into(),
        }
    }

    async fn provider_fixture(response: ModelResponse) -> (MockServer, ProviderConfig) {
        let server = MockServer::start().await;
        let calls = response.tool_calls.iter().enumerate().map(|(index, call)| {
            json!({"index":index,"id":call.id,"function":{"name":call.name,"arguments":call.arguments.to_string()}})
        }).collect::<Vec<_>>();
        let body = format!(
            "data: {}\n\ndata: [DONE]\n\n",
            json!({"choices":[{"delta":{"content":response.content,"tool_calls":calls},"finish_reason":"stop"}]})
        );
        Mock::given(any())
            .respond_with(ResponseTemplate::new(200).set_body_raw(body, "text/event-stream"))
            .expect(1)
            .mount(&server)
            .await;
        let provider = ProviderConfig::Openai {
            base_url: format!("{}/v1", server.uri()),
            api_key: String::new(),
            model: "fixture-model".into(),
        };
        (server, provider)
    }

    #[tokio::test]
    async fn provider_setup_is_required_before_packages_or_journals() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let state = directory.path().join("state");
        let (tx, _rx) = mpsc::channel(MAX_PENDING);
        let outbound = Outbound {
            tx,
            bytes: Arc::new(Semaphore::new(OUTPUT_BYTES)),
            disconnected: CancellationToken::new(),
        };
        let params = serde_json::from_value(json!({
            "state_dir": state,
            "capacity": {"mode":"fixed"},
            "packages": [{"root":"/unused-provider-fixture", "grants":{}}]
        }))?;
        let error = configure(params, outbound)
            .await
            .err()
            .context("missing provider was accepted")?;
        assert!(error.to_string().contains("provider is required"));
        assert!(!state.exists());
        for kind in ["echo", "replay"] {
            assert!(
                serde_json::from_value::<ConfigureParams>(json!({
                    "provider": {"kind":kind,"responses":[{"content":"removed"}]}
                }))
                .is_err()
            );
        }
        Ok(())
    }

    #[test]
    fn coordinated_configuration_preserves_legacy_explicit_ceilings() -> Result<()> {
        for mode in ["fixed", "auto"] {
            let params: ConfigureParams = serde_json::from_value(json!({
                "max_sessions": 20,
                "max_model_requests": 3,
                "resources": {
                    "max_jobs":3,"max_heavy_jobs":2,"cpu_millis":1500,
                    "memory_bytes":268_435_456,"max_processes":12,"max_output_bytes":196_608
                },
                "capacity": {
                    "mode":mode,"max_sessions":40,"max_model_requests":2,"max_jobs":5,
                    "max_heavy_jobs":1,"max_cpu_millis":2000,"max_memory_bytes":134_217_728,
                    "max_processes":16,"max_output_bytes":65_536
                }
            }))?;
            let policy = capacity_policy(&params).context("missing capacity policy")?;
            assert_eq!(policy.max_sessions, Some(20));
            assert_eq!(policy.max_model_requests, Some(2));
            assert_eq!(policy.max_jobs, Some(3));
            assert_eq!(policy.max_heavy_jobs, Some(1));
            assert_eq!(policy.max_cpu_millis, Some(1_500));
            assert_eq!(policy.max_memory_bytes, Some(128 * 1024 * 1024));
            assert_eq!(policy.max_processes, Some(12));
            assert_eq!(policy.max_output_bytes, Some(64 * 1024));
        }
        assert!(capacity_policy(&ConfigureParams::default()).is_none());
        Ok(())
    }

    #[test]
    fn provider_quota_scope_uses_host_identity_and_provider_origin() -> Result<()> {
        let account = uuid::Uuid::new_v4().to_string();
        let make = |model: &str, base_url: &str| -> Result<ConfigureParams> {
            Ok(serde_json::from_value(json!({
                "capacity": {"mode":"fixed"},
                "provider": {"kind":"openai","base_url":base_url,"api_key":"fixture-key","model":model},
                "provider_capacity": {"account_uuid":account,"max_in_flight":2,"requests":{"limit":10,"window_ms":60000}}
            }))?)
        };
        let first = make("first-model", "https://fixture.invalid/v1")?;
        let quota = provider_capacity(&first)?.context("missing quota policy")?;
        let domain = quota
            .domain
            .as_ref()
            .context("missing quota identity")?
            .clone();
        assert_eq!(quota.max_in_flight, Some(2));
        let provider =
            configure_provider(first.provider.context("missing provider")?, Some(quota))?;
        assert_eq!(provider.capacity().domain, Some(domain.clone()));
        let other = provider_capacity(&make("second-model", "https://fixture.invalid/v2")?)?
            .context("missing shared model quota")?;
        assert_eq!(other.domain, Some(domain.clone()));
        let remote = provider_capacity(&make("first-model", "https://other.invalid/v1")?)?
            .context("missing separate origin quota")?;
        assert_ne!(remote.domain, Some(domain));
        let mut missing_mode = make("first-model", "https://fixture.invalid/v1")?;
        missing_mode.capacity = None;
        assert!(provider_capacity(&missing_mode).is_err());
        let mut invalid_rate = make("first-model", "https://fixture.invalid/v1")?;
        invalid_rate
            .provider_capacity
            .as_mut()
            .context("missing quota config")?
            .max_in_flight = Some(0);
        assert!(
            configure_provider(
                invalid_rate.provider.clone().context("missing provider")?,
                provider_capacity(&invalid_rate)?,
            )
            .is_err()
        );
        Ok(())
    }

    #[tokio::test]
    async fn invalid_capacity_is_rejected_before_package_import() -> Result<()> {
        let (tx, _rx) = mpsc::channel(MAX_PENDING);
        let outbound = Outbound {
            tx,
            bytes: Arc::new(Semaphore::new(OUTPUT_BYTES)),
            disconnected: CancellationToken::new(),
        };
        let params = serde_json::from_value(json!({
            "provider": native_fixture(),
            "capacity": {"mode":"fixed", "max_model_requests":0},
            "packages": [{"root":"/unused-capacity-fixture", "grants":{}}]
        }))?;
        let error = configure(params, outbound)
            .await
            .err()
            .context("invalid capacity was accepted")?;
        assert!(error.to_string().contains("model ceiling"));
        Ok(())
    }

    fn legacy_fixed_config(
        state: &std::path::Path,
        provider: &ProviderConfig,
    ) -> Result<ConfigureParams> {
        Ok(serde_json::from_value(json!({
            "state_dir":state,
            "provider":provider,
            "max_sessions":2,
            "max_model_requests":1,
            "resources": {
                "max_jobs":2,"max_heavy_jobs":1,"cpu_millis":750,
                "memory_bytes":134_217_728,"max_processes":4,"max_queue":47,
                "max_output_bytes":65_536,"max_artifact_bytes":1_048_576,"max_job_ms":2000
            }
        }))?)
    }

    #[tokio::test]
    async fn worker_pages_keep_authority_when_history_is_omitted() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let (_provider, provider) = provider_fixture(ModelResponse {
            content: "offline fixture".into(),
            ..ModelResponse::default()
        })
        .await;
        let (tx, _rx) = mpsc::channel(MAX_PENDING);
        let outbound = Outbound {
            tx,
            bytes: Arc::new(Semaphore::new(OUTPUT_BYTES)),
            disconnected: CancellationToken::new(),
        };
        let params = legacy_fixed_config(&directory.path().join("state"), &provider)?;
        let resources = params
            .resources
            .clone()
            .context("missing fixed resources")?;
        let (harness, _broker, packages) = configure(params, outbound.clone()).await?;
        assert_eq!(
            serde_json::to_value(harness.governor().limits())?,
            serde_json::to_value(resources)?
        );
        let first = harness
            .create_session(SessionOptions {
                cwd: directory.path().to_owned(),
                title: "first".into(),
            })
            .await?;
        let second = harness
            .create_session(SessionOptions {
                cwd: directory.path().to_owned(),
                title: "second".into(),
            })
            .await?;
        let mut run = harness.start(&first.id, "offline fixture".into()).await?;
        let summary = tokio::time::timeout(Duration::from_secs(2), run.wait()).await??;
        let closing = CancellationToken::new();
        let call = |method: &str, params: Value| {
            serde_json::from_value::<ClientCall>(json!({"method":method,"params":params}))
        };
        let view = dispatch(
            &harness,
            &packages,
            call(
                "session.view",
                json!({"session_id":first.id,"bounds":{"max_events":0}}),
            )?,
            &outbound,
            &closing,
        )
        .await?;
        assert_eq!(view["events"], json!([]));
        assert_eq!(view["omitted"], true);
        assert!(view["run_id"].is_null());
        assert_eq!(view["last_transition"]["run_id"], run.id);
        assert_eq!(
            view["last_transition"]["outcome"],
            serde_json::to_value(summary.status)?
        );
        let page = dispatch(
            &harness,
            &packages,
            call("session.page", json!({"limit":1}))?,
            &outbound,
            &closing,
        )
        .await?;
        assert_eq!(page["sessions"].as_array().map(Vec::len), Some(1));
        let following = dispatch(
            &harness,
            &packages,
            call("session.page", json!({"limit":1,"cursor":page["next"]}))?,
            &outbound,
            &closing,
        )
        .await?;
        assert_ne!(page["sessions"][0]["id"], following["sessions"][0]["id"]);
        let resolved = dispatch(
            &harness,
            &packages,
            call("session.resolve", json!({"prefix":second.id}))?,
            &outbound,
            &closing,
        )
        .await?;
        assert_eq!(resolved["id"], second.id);
        let events = dispatch(
            &harness,
            &packages,
            call("events.page", json!({"session_id":first.id,"through":view["durable_cursor"],"bounds":{"max_events":1}}))?,
            &outbound,
            &closing,
        )
        .await?;
        assert_eq!(events["events"].as_array().map(Vec::len), Some(1));
        assert_eq!(events["complete"], false);
        assert!(harness.capacity_snapshot().is_none());
        harness.shutdown().await?;
        packages.pool.shutdown().await?;
        Ok(())
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[tokio::test]
    async fn explicit_worker_capacity_enforces_ceilings_and_drains_callbacks() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let (_provider, provider) = provider_fixture(ModelResponse {
            tool_calls: vec![ToolCall {
                id: "held".into(),
                name: "held".into(),
                arguments: json!({}),
            }],
            ..ModelResponse::default()
        })
        .await;
        let root = directory.path().canonicalize()?.join("capacity");
        let (tx, mut rx) = mpsc::channel(MAX_PENDING);
        let outbound = Outbound {
            tx,
            bytes: Arc::new(Semaphore::new(OUTPUT_BYTES)),
            disconnected: CancellationToken::new(),
        };
        let params: ConfigureParams = serde_json::from_value(json!({
            "state_dir": directory.path().join("state"),
            "capacity": {"mode":"fixed", "max_sessions":1,"max_model_requests":1,"max_jobs":1},
            "tools": [{"name":"held","description":"hold fixture","version":"1","effect":"read","input_schema":{"type":"object"}}],
            "provider": provider
        }))?;
        let (harness, broker, packages) =
            configure_with(params, outbound, move |config, policy| {
                CapacityContext::prepare_isolated(config, policy, &root)
            })
            .await?;
        let capacity = harness
            .capacity_snapshot()
            .context("missing shared capacity")?;
        assert_eq!(capacity.hard_model_requests, 1);
        assert_eq!(capacity.hard_resources.max_jobs, 1);
        let first = harness
            .create_session(SessionOptions {
                cwd: directory.path().to_owned(),
                title: String::new(),
            })
            .await?;
        let second = harness
            .create_session(SessionOptions {
                cwd: directory.path().to_owned(),
                title: String::new(),
            })
            .await?;
        let mut run = harness.start(&first.id, "hold callback".into()).await?;
        let frame = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await?
            .context("missing governed callback")?;
        let request: ToolRequest = serde_json::from_slice(&frame.bytes)?;
        assert!(
            harness
                .start(&second.id, "exceeds ceiling".into())
                .await
                .is_err()
        );
        harness.cancel_run(&first.id, &run.id).await?;
        broker.resolve(Response::error(
            request.id,
            "cancelled",
            "fixture cleanup complete",
        ));
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), run.wait())
                .await??
                .status,
            RunStatus::Cancelled
        );
        harness.shutdown().await?;
        packages.pool.shutdown().await?;
        assert_eq!(harness.governor().snapshot().running, 0);
        Ok(())
    }

    #[test]
    fn native_provider_configuration_is_local_and_validated() -> Result<()> {
        for value in [
            json!({"kind":"responses","api_key":"fixture-key","model":"fixture-model"}),
            json!({"kind":"responses","api_key":"fixture-key","model":"fixture-model","reasoning_effort":"high"}),
            json!({"kind":"anthropic","api_key":"fixture-key","model":"fixture-model"}),
        ] {
            configure_provider(serde_json::from_value(value)?, None)?;
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
            let error = configure_provider(serde_json::from_value(value)?, None)
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
        let (_provider, provider) = provider_fixture(ModelResponse {
            tool_calls: vec![ToolCall {
                id: "provider-call".into(),
                name: "host_tool".into(),
                arguments: json!({}),
            }],
            ..ModelResponse::default()
        })
        .await;
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
            provider: Some(provider),
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
                json!({"state_dir":directory.path(),"provider":native_fixture()})
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
