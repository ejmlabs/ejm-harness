mod driver;
mod hooks;
mod tool;

pub use tool::{Tool, ToolContext};

use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use anyhow::{Context, Result, ensure};
use ejm_harness_exec::Governor;
use tokio::sync::{Mutex, Notify, Semaphore, broadcast, mpsc, watch};
use tokio_util::sync::CancellationToken;

use crate::{
    ContextPolicy, Event, EventKind, HarnessConfig, Hook, Provider, RecentContext, RunStrategy,
    RunSummary, Session, SessionOptions, SessionStatus, ToolDefinition, ToolLoop, store::Store,
    types::timestamp_ms,
};

const MAX_PROMPT: usize = 16 * 1024;
const MAX_CONTROLS: usize = 8;
const MAX_PROVIDER_BINDING: usize = 1024;

#[derive(Clone)]
pub struct Harness {
    inner: Arc<Inner>,
}

struct Inner {
    config: HarnessConfig,
    provider: Mutex<ProviderSelection>,
    tools: BTreeMap<String, Arc<dyn Tool>>,
    definitions: Vec<ToolDefinition>,
    validators: HashMap<String, jsonschema::Validator>,
    context: Arc<dyn ContextPolicy>,
    strategy: Arc<dyn RunStrategy>,
    hooks: Vec<Arc<dyn Hook>>,
    store: Store,
    governor: Arc<Governor>,
    model_slots: Arc<Semaphore>,
    events: broadcast::Sender<Event>,
    active: Arc<Mutex<HashMap<String, ActiveRun>>>,
    stopped: CancellationToken,
    changed: Notify,
    // Effects in one working directory are serialized across sessions. Dormant directories use no lock.
    writes: std::sync::Mutex<HashMap<PathBuf, std::sync::Weak<Mutex<()>>>>,
    model_waiters: AtomicUsize,
}

struct ActiveRun {
    id: String,
    cancel: CancellationToken,
    done: watch::Receiver<Option<RunCompletion>>,
    steering: mpsc::Sender<String>,
    queued: VecDeque<String>,
}

struct ProviderSelection {
    provider: Option<Arc<dyn Provider>>,
    binding_initialized: bool,
}

/// One run's completion and its driver's local queue decision.
#[derive(Clone)]
pub struct RunCompletion {
    pub summary: RunSummary,
    /// Whether the driver handed off to an internally queued successor.
    /// A false value does not assert that the session is still idle.
    pub queued_successor: bool,
}

/// An observer and cancellation handle for one run. Clones share the same run.
/// Dropping a handle does not cancel work.
#[derive(Clone)]
pub struct RunHandle {
    pub id: String,
    pub session_id: String,
    cancel: CancellationToken,
    done: watch::Receiver<Option<RunCompletion>>,
}

impl RunHandle {
    pub fn cancel(&self) {
        self.cancel.cancel();
    }
    /// Wait for durable completion and bounded run-end observers.
    ///
    /// Another locally queued run may already be current when this returns.
    ///
    /// # Errors
    /// Returns an error if the run driver stops without a summary.
    pub async fn wait(&mut self) -> Result<RunSummary> {
        Ok(self.wait_completion().await?.summary)
    }

    /// Wait for completion and the driver's atomic queued-successor decision.
    ///
    /// Hosts must still validate their own target, revocation and admission.
    /// The result is retained even if a queued successor has already finished.
    ///
    /// # Errors
    /// Returns an error if the run driver stops without a summary.
    pub async fn wait_completion(&mut self) -> Result<RunCompletion> {
        loop {
            if let Some(completion) = self.done.borrow_and_update().clone() {
                return Ok(completion);
            }
            self.done
                .changed()
                .await
                .context("run driver stopped without a summary")?;
        }
    }
}

impl Harness {
    /// Create a harness with recent context and the default tool loop.
    ///
    /// # Errors
    /// Rejects invalid budgets, tool schemas, or an unavailable state directory.
    pub fn new(
        config: HarnessConfig,
        provider: Arc<dyn Provider>,
        tools: Vec<Arc<dyn Tool>>,
    ) -> Result<Self> {
        Self::with_policies(
            config,
            provider,
            tools,
            Arc::new(RecentContext),
            Arc::new(ToolLoop),
        )
    }

    /// Create a harness with host supplied context and execution policies.
    ///
    /// # Errors
    /// Rejects invalid budgets, tool schemas, or an unavailable state directory.
    pub fn with_policies(
        config: HarnessConfig,
        provider: Arc<dyn Provider>,
        tools: Vec<Arc<dyn Tool>>,
        context: Arc<dyn ContextPolicy>,
        strategy: Arc<dyn RunStrategy>,
    ) -> Result<Self> {
        Self::with_policies_and_hooks(config, provider, tools, context, strategy, vec![])
    }

    /// Create a harness with ordered hooks and the default context and tool policies.
    ///
    /// # Errors
    /// Rejects invalid budgets, tool schemas, hooks, or an unavailable state directory.
    pub fn with_hooks(
        config: HarnessConfig,
        provider: Arc<dyn Provider>,
        tools: Vec<Arc<dyn Tool>>,
        hooks: Vec<Arc<dyn Hook>>,
    ) -> Result<Self> {
        Self::with_policies_and_hooks(
            config,
            provider,
            tools,
            Arc::new(RecentContext),
            Arc::new(ToolLoop),
            hooks,
        )
    }

    /// Create a host that may configure its provider after sign-in.
    ///
    /// A missing provider rejects turns without persisting their prompts. Use
    /// [`Self::configure_provider`] before starting work to bind durable reasoning
    /// to the selected provider, account, model and endpoint.
    /// Starting with an unbound provider invalidates any earlier durable binding
    /// and its opaque continuation before admitting the turn.
    ///
    /// # Errors
    /// Rejects invalid budgets, tool schemas, hooks, or an unavailable state directory.
    pub fn with_optional_provider(
        config: HarnessConfig,
        provider: Option<Arc<dyn Provider>>,
        tools: Vec<Arc<dyn Tool>>,
        hooks: Vec<Arc<dyn Hook>>,
    ) -> Result<Self> {
        Self::build(
            config,
            provider,
            tools,
            Arc::new(RecentContext),
            Arc::new(ToolLoop),
            hooks,
        )
    }

    /// Create a harness with host policies and hooks in registration order.
    ///
    /// # Errors
    /// Rejects invalid budgets, tool schemas, hooks, or an unavailable state directory.
    pub fn with_policies_and_hooks(
        config: HarnessConfig,
        provider: Arc<dyn Provider>,
        tools: Vec<Arc<dyn Tool>>,
        context: Arc<dyn ContextPolicy>,
        strategy: Arc<dyn RunStrategy>,
        hooks: Vec<Arc<dyn Hook>>,
    ) -> Result<Self> {
        Self::build(config, Some(provider), tools, context, strategy, hooks)
    }

    fn build(
        config: HarnessConfig,
        provider: Option<Arc<dyn Provider>>,
        tools: Vec<Arc<dyn Tool>>,
        context: Arc<dyn ContextPolicy>,
        strategy: Arc<dyn RunStrategy>,
        hooks: Vec<Arc<dyn Hook>>,
    ) -> Result<Self> {
        ensure!(hooks.len() <= 32, "at most 32 hooks may be registered");
        ensure!(
            (1..=1000).contains(&config.max_sessions),
            "max_sessions must be between 1 and 1000"
        );
        ensure!(
            (1..=128).contains(&config.max_model_requests),
            "max_model_requests must be between 1 and 128"
        );
        ensure!(
            (4096..=1024 * 1024).contains(&config.context_max_bytes),
            "context budget must be between 4 KiB and 1 MiB"
        );
        ensure!(
            (1..=256).contains(&config.max_steps) && config.max_repeated_calls > 0,
            "invalid step or repeat budget"
        );
        ensure!(
            (1..=86_400_000).contains(&config.run_timeout_ms),
            "invalid run deadline"
        );
        ensure!(
            config.system_prompt.len() <= MAX_PROMPT
                && config.max_input_tokens > 0
                && config.max_output_tokens > 0,
            "invalid system prompt or token budget"
        );
        ensure!(tools.len() <= 64, "at most 64 tools may be registered");
        let mut registry = BTreeMap::new();
        let mut definitions = vec![];
        let mut validators = HashMap::new();
        for tool in tools {
            let definition = tool.definition();
            ensure!(
                !definition.name.is_empty()
                    && definition.name.len() <= 64
                    && definition
                        .name
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c)),
                "invalid tool name"
            );
            ensure!(
                !definition.version.is_empty() && definition.version.len() <= 128,
                "invalid tool version"
            );
            ensure!(
                serde_json::to_vec(&definition)?.len() <= 16 * 1024,
                "tool definition exceeds 16 KiB"
            );
            ensure!(
                !registry.contains_key(&definition.name),
                "duplicate tool name: {}",
                definition.name
            );
            let validator = jsonschema::validator_for(&definition.input_schema)
                .map_err(|e| anyhow::anyhow!("invalid schema for {}: {e}", definition.name))?;
            validators.insert(definition.name.clone(), validator);
            registry.insert(definition.name.clone(), tool);
            definitions.push(definition);
        }
        ensure!(
            serde_json::to_vec(&definitions)?.len() <= 64 * 1024,
            "tool definitions exceed 64 KiB"
        );
        let governor = Arc::new(Governor::new(config.resources.clone())?);
        let store = Store::open(&config.state_dir, config.max_store_bytes)?;
        let (events, _) = broadcast::channel(128);
        let slots = config.max_model_requests;
        Ok(Self {
            inner: Arc::new(Inner {
                config,
                provider: Mutex::new(ProviderSelection {
                    provider,
                    binding_initialized: false,
                }),
                tools: registry,
                definitions,
                validators,
                context,
                strategy,
                hooks,
                store,
                governor,
                model_slots: Arc::new(Semaphore::new(slots)),
                events,
                active: Arc::new(Mutex::new(HashMap::new())),
                stopped: CancellationToken::new(),
                changed: Notify::new(),
                writes: std::sync::Mutex::new(HashMap::new()),
                model_waiters: AtomicUsize::new(0),
            }),
        })
    }

    /// Replace or remove the provider only while every managed run is idle.
    ///
    /// `binding` is a stable, nonsecret identity covering provider, account, model
    /// and endpoint. It must contain 1 to 1024 printable UTF-8 bytes. A changed
    /// binding clears opaque continuation while retaining readable history and
    /// tool evidence. The same binding preserves continuation across restart.
    ///
    /// After admission, configuration completes even if the caller drops this
    /// future. Run admission remains locked through the journal transaction and
    /// provider replacement, so cancellation cannot expose a partial change.
    ///
    /// # Errors
    /// Rejects invalid bindings, active or queued runs, shutdown, and journal
    /// failures. A rejected change leaves the previous provider configured.
    pub async fn configure_provider(
        &self,
        provider: Option<Arc<dyn Provider>>,
        binding: &str,
    ) -> Result<()> {
        ensure!(
            !binding.trim().is_empty()
                && binding.len() <= MAX_PROVIDER_BINDING
                && !binding.chars().any(char::is_control),
            "provider binding must contain 1 to 1024 printable UTF-8 bytes"
        );
        let admission = self.inner.active.clone().lock_owned().await;
        ensure!(!self.inner.stopped.is_cancelled(), "harness is shut down");
        ensure!(
            admission.is_empty(),
            "wait for all managed runs and queued turns to finish before changing providers"
        );
        let harness = self.clone();
        let binding = binding.to_owned();
        tokio::spawn(async move {
            let _admission = admission;
            harness
                .inner
                .store
                .configure_provider_binding(Some(binding))
                .await?;
            *harness.inner.provider.lock().await = ProviderSelection {
                provider,
                binding_initialized: true,
            };
            Ok::<(), anyhow::Error>(())
        })
        .await
        .context("provider configuration task stopped")?
    }

    /// Report shutdown or a journal failure that stopped further effects.
    #[must_use]
    pub fn is_stopped(&self) -> bool {
        self.inner.stopped.is_cancelled()
    }

    /// Subscribe to bounded live events. Replay the journal after a lag error.
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.inner.events.subscribe()
    }
    /// Share this harness resource governor with host tools and child work.
    #[must_use]
    pub fn governor(&self) -> Arc<Governor> {
        self.inner.governor.clone()
    }
    pub async fn status(&self) -> crate::HarnessSnapshot {
        crate::HarnessSnapshot {
            active_agents: self.inner.active.lock().await.len(),
            model_requests: self.inner.config.max_model_requests
                - self.inner.model_slots.available_permits(),
            waiting_model_requests: self.inner.model_waiters.load(Ordering::Relaxed),
            tools: self.inner.governor.snapshot(),
        }
    }
    /// List up to 1000 recent sessions.
    ///
    /// # Errors
    /// Returns an error if the journal cannot be read.
    pub async fn list_sessions(&self) -> Result<Vec<Session>> {
        self.inner.store.list_sessions().await
    }
    /// Read a session by its durable identity, including older sessions.
    ///
    /// # Errors
    /// Returns an error for an unknown session or if the journal cannot be read.
    pub async fn get_session(&self, session_id: &str) -> Result<Session> {
        self.inner.store.session(session_id).await
    }
    /// Read a bounded page of durable events after a sequence number.
    ///
    /// # Errors
    /// Returns an error if the journal cannot be read or decoded.
    pub async fn events(&self, session_id: &str, after: u64, limit: usize) -> Result<Vec<Event>> {
        self.inner.store.events(session_id, after, limit).await
    }

    /// Create a session rooted in an existing directory.
    ///
    /// # Errors
    /// Rejects unavailable directories, invalid titles, shutdown, or journal failures.
    pub async fn create_session(&self, options: SessionOptions) -> Result<Session> {
        ensure!(!self.inner.stopped.is_cancelled(), "harness is shut down");
        let cwd = std::fs::canonicalize(if options.cwd.as_os_str().is_empty() {
            PathBuf::from(".")
        } else {
            options.cwd
        })
        .context("working directory is unavailable")?;
        ensure!(cwd.is_dir(), "working directory is not a directory");
        ensure!(options.title.len() <= 256, "session title exceeds limit");
        self.inner
            .store
            .create_session(Session {
                id: uuid::Uuid::new_v4().to_string(),
                title: options.title,
                cwd,
                created_at_ms: timestamp_ms(),
                status: SessionStatus::Idle,
            })
            .await
    }

    /// Start a turn. Dropping its handle does not cancel the turn.
    ///
    /// # Errors
    /// Rejects invalid prompts, a missing provider, busy sessions, exhausted agent
    /// capacity, or shutdown.
    pub async fn start(&self, session_id: &str, prompt: String) -> Result<RunHandle> {
        validate_prompt(&prompt)?;
        let session = self.inner.store.session(session_id).await?;
        let mut active = self.inner.active.lock().await;
        ensure!(!self.inner.stopped.is_cancelled(), "harness is shut down");
        let mut selection = self.inner.provider.lock().await;
        ensure!(
            selection.provider.is_some(),
            "no model provider is configured; sign in or select a provider before sending a prompt"
        );
        ensure!(
            !active.contains_key(session_id),
            "session already has a running turn; use steer or queue"
        );
        ensure!(
            active.len() < self.inner.config.max_sessions,
            "managed agent limit reached"
        );
        if !selection.binding_initialized {
            self.inner.store.configure_provider_binding(None).await?;
            selection.binding_initialized = true;
        }
        drop(selection);
        let id = uuid::Uuid::new_v4().to_string();
        let cancel = self.inner.stopped.child_token();
        let (steering, receiver) = mpsc::channel(MAX_CONTROLS);
        let (done, result) = watch::channel(None);
        active.insert(
            session_id.into(),
            ActiveRun {
                id: id.clone(),
                cancel: cancel.clone(),
                done: result.clone(),
                steering,
                queued: VecDeque::new(),
            },
        );
        let harness = self.clone();
        let run_id = id.clone();
        let task_cancel = cancel.clone();
        tokio::spawn(async move {
            harness
                .session_driver(session, run_id, prompt, task_cancel, receiver, done)
                .await;
        });
        Ok(RunHandle {
            id,
            session_id: session_id.into(),
            cancel,
            done: result,
        })
    }

    /// Subscribe to the exact current run, including its bounded run-end cleanup.
    ///
    /// Returns `None` when the named session and run are not the current pair.
    /// This is not evidence of success. Use durable replay to inspect old results.
    /// Waiting includes run-end observers; journaled completion alone does not.
    pub async fn subscribe_run(&self, session_id: &str, run_id: &str) -> Option<RunHandle> {
        let active = self.inner.active.lock().await;
        let run = active.get(session_id).filter(|run| run.id == run_id)?;
        Some(RunHandle {
            id: run.id.clone(),
            session_id: session_id.into(),
            cancel: run.cancel.clone(),
            done: run.done.clone(),
        })
    }

    /// Add user direction to the active turn before its next tool execution.
    ///
    /// # Errors
    /// Rejects invalid prompts, inactive turns, full controls, or journal failures.
    pub async fn steer(&self, session_id: &str, text: String) -> Result<()> {
        self.steer_inner(session_id, None, text).await
    }

    /// Steer only the named active run. Stale handles cannot steer a later run.
    ///
    /// # Errors
    /// Rejects stale runs, invalid prompts, full controls, or journal failures.
    pub async fn steer_run(&self, session_id: &str, run_id: &str, text: String) -> Result<()> {
        self.steer_inner(session_id, Some(run_id), text).await
    }

    async fn steer_inner(
        &self,
        session_id: &str,
        expected: Option<&str>,
        text: String,
    ) -> Result<()> {
        validate_prompt(&text)?;
        let active = self.inner.active.lock().await;
        let run = active
            .get(session_id)
            .context("session has no active turn")?;
        ensure!(
            expected.is_none_or(|id| id == run.id),
            "run has ended; control was not applied to another turn"
        );
        ensure!(!run.cancel.is_cancelled(), "turn is stopping");
        ensure!(
            run.queued.len() + run.steering.max_capacity() - run.steering.capacity() < MAX_CONTROLS,
            "control queue is full"
        );
        let permit = run
            .steering
            .try_reserve()
            .context("steering queue is full")?;
        self.emit(session_id, None, EventKind::Steered { text: text.clone() })
            .await?;
        permit.send(text);
        Ok(())
    }

    /// Queue a turn, or start it immediately if the session is idle.
    ///
    /// # Errors
    /// Rejects invalid prompts, full controls, stopping turns, or journal failures.
    pub async fn enqueue(&self, session_id: &str, text: String) -> Result<()> {
        self.enqueue_inner(session_id, None, text).await
    }

    /// Queue work only while the named run is still active.
    ///
    /// # Errors
    /// Rejects stale runs, invalid prompts, full controls, or journal failures.
    pub async fn enqueue_for_run(
        &self,
        session_id: &str,
        run_id: &str,
        text: String,
    ) -> Result<()> {
        self.enqueue_inner(session_id, Some(run_id), text).await
    }

    async fn enqueue_inner(
        &self,
        session_id: &str,
        expected: Option<&str>,
        text: String,
    ) -> Result<()> {
        validate_prompt(&text)?;
        let mut active = self.inner.active.lock().await;
        if let Some(run) = active.get_mut(session_id) {
            ensure!(
                expected.is_none_or(|id| id == run.id),
                "run has ended; control was not applied to another turn"
            );
            ensure!(
                run.queued.len() + run.steering.max_capacity() - run.steering.capacity()
                    < MAX_CONTROLS,
                "control queue is full"
            );
            ensure!(!run.cancel.is_cancelled(), "turn is stopping");
            self.emit(session_id, None, EventKind::Queued { text: text.clone() })
                .await?;
            run.queued.push_back(text);
            Ok(())
        } else {
            ensure!(expected.is_none(), "run has ended");
            drop(active);
            self.start(session_id, text).await.map(|_| ())
        }
    }

    /// Cancel an active session and discard its queued turns.
    ///
    /// # Errors
    /// Reserved for host control failures. Cancelling an idle session succeeds.
    pub async fn cancel(&self, session_id: &str) -> Result<()> {
        self.cancel_inner(session_id, None).await
    }

    /// Cancel only the named run, leaving any newer run untouched.
    ///
    /// # Errors
    /// Rejects a run identity that differs from the active turn.
    pub async fn cancel_run(&self, session_id: &str, run_id: &str) -> Result<()> {
        self.cancel_inner(session_id, Some(run_id)).await
    }

    async fn cancel_inner(&self, session_id: &str, expected: Option<&str>) -> Result<()> {
        if let Some(run) = self.inner.active.lock().await.get_mut(session_id) {
            ensure!(
                expected.is_none_or(|id| id == run.id),
                "run has ended; control was not applied to another turn"
            );
            run.queued.clear();
            run.cancel.cancel();
        }
        Ok(())
    }

    /// Stop admission and request cancellation without waiting for cleanup.
    /// Call [`Self::shutdown`] to wait for all retained resources to become idle.
    pub fn request_shutdown(&self) {
        self.inner.stopped.cancel();
    }

    /// Stop accepting runs, cancel active work, and wait for resource cleanup.
    ///
    /// # Errors
    /// Reserved for shutdown failures.
    pub async fn shutdown(&self) -> Result<()> {
        self.request_shutdown();
        loop {
            let changed = self.inner.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if self.inner.active.lock().await.is_empty() {
                break;
            }
            changed.await;
        }
        self.inner.governor.wait_idle().await;
        Ok(())
    }

    async fn emit(&self, session: &str, run: Option<&str>, kind: EventKind) -> Result<()> {
        let event = self
            .inner
            .store
            .append(Event {
                sequence: 0,
                session_id: session.into(),
                run_id: run.map(Into::into),
                timestamp_ms: timestamp_ms(),
                kind,
            })
            .await?;
        let _ = self.inner.events.send(event);
        Ok(())
    }
}

fn validate_prompt(prompt: &str) -> Result<()> {
    ensure!(
        !prompt.trim().is_empty() && prompt.len() <= MAX_PROMPT,
        "prompt must contain 1 to 16384 UTF-8 bytes"
    );
    Ok(())
}

struct ModelWaiter<'a>(&'a AtomicUsize);
impl Drop for ModelWaiter<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}
