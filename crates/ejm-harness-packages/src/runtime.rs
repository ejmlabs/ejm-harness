use std::{
    collections::{BTreeSet, HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard, Weak},
    time::Duration,
};

use anyhow::{Context, Result, ensure};
use ejm_harness::{Hook, Provider, Tool};
use ejm_harness_exec::{Governor, Reservation};
pub use ejm_harness_package_spec::RuntimeKind;
use ejm_harness_package_spec::{HookEvent, Manifest};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::{adapters, transport::Host};

const MAX_PACKAGES: usize = 128;

/// A validated installation. Loading it requires a separate, explicit host grant.
#[derive(Clone, Debug)]
pub struct InstalledPackage {
    pub manifest: Manifest,
    pub root: PathBuf,
}

/// Host-owned registration allowlists. They do not sandbox trusted native code.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct PackageGrants {
    pub tools: Vec<String>,
    pub hooks: Vec<HookEvent>,
    pub providers: Vec<String>,
    pub commands: Vec<String>,
    pub widgets: Vec<String>,
}

/// Fixed bounds for two lazy language hosts, never one process per agent.
#[derive(Clone, Debug)]
pub struct RuntimeConfig {
    pub node: PathBuf,
    pub python: PathBuf,
    pub max_pending: usize,
    pub max_frame_bytes: usize,
    pub max_buffer_bytes: usize,
    pub call_timeout: Duration,
    pub cleanup_timeout: Duration,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            node: PathBuf::from("node"),
            python: PathBuf::from("python3"),
            max_pending: 64,
            max_frame_bytes: 1024 * 1024,
            max_buffer_bytes: 4 * 1024 * 1024,
            call_timeout: Duration::from_secs(120),
            cleanup_timeout: Duration::from_secs(5),
        }
    }
}

impl RuntimeConfig {
    fn validate(&self) -> Result<()> {
        ensure!(
            !self.node.as_os_str().is_empty(),
            "Node executable is required"
        );
        ensure!(
            !self.python.as_os_str().is_empty(),
            "Python executable is required"
        );
        ensure!(
            (1..=64).contains(&self.max_pending),
            "runtime pending calls must be 1..64"
        );
        ensure!(
            (1024..=1024 * 1024).contains(&self.max_frame_bytes),
            "runtime frame limit must be 1 KiB..1 MiB"
        );
        ensure!(
            self.max_buffer_bytes >= self.max_frame_bytes
                && self.max_buffer_bytes <= 16 * 1024 * 1024,
            "runtime outbound buffer must fit a frame and stay within 16 MiB"
        );
        ensure!(
            !self.call_timeout.is_zero() && self.call_timeout <= Duration::from_secs(86_400),
            "runtime call timeout must be positive and at most one day"
        );
        ensure!(
            !self.cleanup_timeout.is_zero() && self.cleanup_timeout <= Duration::from_secs(30),
            "runtime cleanup timeout must be positive and at most 30 seconds"
        );
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub enum RuntimeEvent {
    Progress(String),
    Delta(String),
}

/// Runtime capabilities supplied by the host, never deserialized from package input.
#[derive(Clone)]
pub struct PackageContext {
    pub session_id: String,
    pub cwd: PathBuf,
    pub cancel: CancellationToken,
    pub governor: Arc<Governor>,
}

#[derive(Clone)]
pub struct RuntimePool {
    inner: Arc<Inner>,
}

struct Inner {
    config: RuntimeConfig,
    state: Mutex<State>,
    management: tokio::sync::Mutex<()>,
    management_slots: Arc<tokio::sync::Semaphore>,
    closed: CancellationToken,
}

#[derive(Default)]
struct State {
    packages: HashMap<String, Loaded>,
    languages: HashMap<RuntimeKind, Language>,
}

#[derive(Clone)]
struct Loaded {
    installed: InstalledPackage,
    grants: PackageGrants,
    revision: uuid::Uuid,
    ready: bool,
    active: usize,
}

struct Language {
    host: Arc<Host>,
    active: usize,
    blocked: bool,
}

pub(crate) struct Invocation {
    inner: Weak<Inner>,
    package: String,
    kind: RuntimeKind,
}

pub(crate) struct Callback {
    pub package_id: String,
    pub revision: uuid::Uuid,
    pub handler: String,
    pub args: Value,
    pub context: Value,
    pub cancel: CancellationToken,
    pub events: Option<mpsc::Sender<RuntimeEvent>>,
}

impl Drop for Invocation {
    fn drop(&mut self) {
        if let Some(inner) = self.inner.upgrade() {
            let mut state = lock(&inner.state);
            if let Some(package) = state.packages.get_mut(&self.package) {
                package.active = package.active.saturating_sub(1);
            }
            if let Some(language) = state.languages.get_mut(&self.kind) {
                language.active = language.active.saturating_sub(1);
            }
        }
    }
}

impl RuntimePool {
    /// Construct without starting a process or executing package code.
    ///
    /// # Errors
    /// Rejects unsupported queue, frame, timeout, or executable configuration.
    pub fn new(config: RuntimeConfig) -> Result<Self> {
        config.validate()?;
        Ok(Self {
            inner: Arc::new(Inner {
                config,
                state: Mutex::new(State::default()),
                management: tokio::sync::Mutex::new(()),
                management_slots: Arc::new(tokio::sync::Semaphore::new(16)),
                closed: CancellationToken::new(),
            }),
        })
    }

    /// Execute a trusted installation's declared entry and register only granted exports.
    ///
    /// # Errors
    /// Rejects invalid grants, duplicates, capacity, cancellation, or import failures.
    /// Import failure stops that language host; explicit reload is required to recover.
    pub async fn load(
        &self,
        installed: InstalledPackage,
        grants: PackageGrants,
        cancel: CancellationToken,
    ) -> Result<()> {
        installed.manifest.validate()?;
        grants.validate(&installed.manifest)?;
        let pool = self.clone();
        self.manage(cancel, async move |cancel| {
            pool.load_inner(installed, grants, cancel).await
        })
        .await
    }

    /// Return adapters for this installation's granted tools.
    ///
    /// # Errors
    /// Rejects a missing, loading, or failed installation.
    pub fn tools(&self, package_id: &str) -> Result<Vec<Arc<dyn Tool>>> {
        let package = self.package(package_id)?;
        Ok(package
            .installed
            .manifest
            .exports
            .tools
            .into_iter()
            .filter(|export| package.grants.tools.contains(&export.name))
            .map(|export| adapters::tool(self.clone(), package_id.into(), package.revision, export))
            .collect())
    }

    /// Return ordered adapters for this installation's granted hook events.
    ///
    /// # Errors
    /// Rejects a missing, loading, or failed installation.
    pub fn hooks(&self, package_id: &str) -> Result<Vec<Arc<dyn Hook>>> {
        let package = self.package(package_id)?;
        let hooks = package
            .installed
            .manifest
            .exports
            .hooks
            .into_iter()
            .filter(|export| package.grants.hooks.contains(&export.event))
            .collect();
        Ok(adapters::hooks(
            self.clone(),
            package_id.into(),
            package.revision,
            hooks,
        ))
    }

    /// Return a provider adapter after an explicit named grant.
    ///
    /// # Errors
    /// Rejects undeclared or ungranted exports and unavailable installations.
    pub fn provider(&self, package_id: &str, name: &str) -> Result<Arc<dyn Provider>> {
        let package = self.package(package_id)?;
        ensure!(
            package.grants.providers.iter().any(|grant| grant == name),
            "provider export is not granted"
        );
        let export = package
            .installed
            .manifest
            .exports
            .providers
            .into_iter()
            .find(|export| export.name == name)
            .context("provider export is not declared")?;
        Ok(adapters::provider(
            self.clone(),
            package_id.into(),
            package.revision,
            export.handler,
        ))
    }

    /// Invoke a granted command using the same governor as tools and agents.
    ///
    /// # Errors
    /// Rejects missing grants, admission, cancellation, unavailable hosts, or callback errors.
    pub async fn invoke_command(
        &self,
        package_id: &str,
        name: &str,
        args: Value,
        context: PackageContext,
    ) -> Result<Value> {
        let package = self.package(package_id)?;
        ensure!(
            package.grants.commands.iter().any(|grant| grant == name),
            "command export is not granted"
        );
        let export = package
            .installed
            .manifest
            .exports
            .commands
            .into_iter()
            .find(|export| export.name == name)
            .context("command export is not declared")?;
        self.invoke_admitted(package_id, package.revision, &export.handler, args, context)
            .await
    }

    /// Invoke a granted widget using shared admission. The host owns widget rendering.
    ///
    /// # Errors
    /// Rejects missing grants, admission, cancellation, unavailable hosts, or callback errors.
    pub async fn render_widget(
        &self,
        package_id: &str,
        name: &str,
        args: Value,
        context: PackageContext,
    ) -> Result<Value> {
        let package = self.package(package_id)?;
        ensure!(
            package.grants.widgets.iter().any(|grant| grant == name),
            "widget export is not granted"
        );
        let export = package
            .installed
            .manifest
            .exports
            .widgets
            .into_iter()
            .find(|export| export.name == name)
            .context("widget export is not declared")?;
        self.invoke_admitted(package_id, package.revision, &export.handler, args, context)
            .await
    }

    /// Remove a package and rebuild its idle language host to reclaim module state.
    ///
    /// # Errors
    /// Rejects active callbacks, missing packages, cancellation, or reload failures.
    pub async fn unload(&self, package_id: &str, cancel: CancellationToken) -> Result<()> {
        let package_id = package_id.to_owned();
        let pool = self.clone();
        self.manage(cancel, async move |cancel| {
            let kind = pool.remove(&package_id)?;
            if let Some(kind) = kind {
                pool.rebuild(kind, cancel).await?;
            }
            Ok(())
        })
        .await
    }

    /// Reap and rebuild an idle language host, refreshing all of its authorized packages.
    ///
    /// # Errors
    /// Rejects active callbacks, missing packages, cancellation, or reload failures.
    /// Failed reloads remain stopped and are never retried automatically.
    pub async fn reload(&self, package_id: &str, cancel: CancellationToken) -> Result<()> {
        let package_id = package_id.to_owned();
        let pool = self.clone();
        self.manage(cancel, async move |cancel| {
            let kind = {
                let state = lock(&pool.inner.state);
                let package = state
                    .packages
                    .get(&package_id)
                    .context("package is not loaded")?;
                package
                    .installed
                    .manifest
                    .runtime
                    .as_ref()
                    .map(|runtime| runtime.kind)
            };
            if let Some(kind) = kind {
                pool.rebuild(kind, cancel).await?;
            }
            Ok(())
        })
        .await
    }

    /// Stop both hosts and await actual process exit before releasing callback ownership.
    ///
    /// # Errors
    /// Reports cleanup still pending; keep the Tokio runtime alive until it completes.
    pub async fn shutdown(&self) -> Result<()> {
        self.inner.closed.cancel();
        let pool = self.clone();
        tokio::spawn(async move {
            let _management = pool.inner.management.lock().await;
            let hosts = {
                let mut state = lock(&pool.inner.state);
                for package in state.packages.values_mut() {
                    package.ready = false;
                }
                for language in state.languages.values_mut() {
                    language.blocked = true;
                }
                state
                    .languages
                    .values()
                    .map(|language| language.host.clone())
                    .collect::<Vec<_>>()
            };
            let mut failure = None;
            for host in hosts {
                if let Err(error) = host.shutdown().await {
                    failure.get_or_insert(error);
                }
            }
            failure.map_or(Ok(()), Err)
        })
        .await
        .context("package shutdown supervisor failed")?
    }

    pub(crate) async fn invoke<T: Send + 'static>(
        &self,
        callback: Callback,
        retained: T,
    ) -> Result<Value> {
        let (host, ownership) = self.begin(&callback)?;
        host.call("invoke", json!({"package_id":callback.package_id,"handler":callback.handler,"args":callback.args,"context":callback.context}), callback.cancel, callback.events, (retained, ownership)).await
    }

    async fn invoke_admitted(
        &self,
        package_id: &str,
        revision: uuid::Uuid,
        handler: &str,
        args: Value,
        context: PackageContext,
    ) -> Result<Value> {
        let lease = context
            .governor
            .acquire(&context.session_id, callback_reservation(), &context.cancel)
            .await?;
        self.invoke(
            Callback {
                package_id: package_id.into(),
                revision,
                handler: handler.into(),
                args,
                context: json!({"session_id":context.session_id,"cwd":context.cwd}),
                cancel: context.cancel,
                events: None,
            },
            lease,
        )
        .await
    }

    fn package(&self, id: &str) -> Result<Loaded> {
        ensure!(
            !self.inner.closed.is_cancelled(),
            "package runtime is shut down"
        );
        let state = lock(&self.inner.state);
        let package = state.packages.get(id).context("package is not loaded")?;
        ensure!(
            package.ready,
            "package is loading or stopped; reload explicitly"
        );
        Ok(package.clone())
    }

    fn begin(&self, callback: &Callback) -> Result<(Arc<Host>, Invocation)> {
        ensure!(
            !self.inner.closed.is_cancelled(),
            "package runtime is shut down"
        );
        let mut state = lock(&self.inner.state);
        let id = &callback.package_id;
        let package = state.packages.get(id).context("package is not loaded")?;
        ensure!(
            package.ready,
            "package is loading or stopped; reload explicitly"
        );
        ensure!(
            package.revision == callback.revision,
            "package adapter is stale; register the current installation"
        );
        ensure!(
            package
                .grants
                .handlers(&package.installed.manifest)
                .contains(&callback.handler),
            "package handler is not granted"
        );
        let kind = package
            .installed
            .manifest
            .runtime
            .as_ref()
            .context("package has no runtime")?
            .kind;
        let language = state
            .languages
            .get_mut(&kind)
            .context("package language host unavailable")?;
        ensure!(
            !language.blocked && !language.host.stopped(),
            "package language host is stopped; reload explicitly"
        );
        language.active += 1;
        let host = language.host.clone();
        state
            .packages
            .get_mut(id)
            .context("package disappeared")?
            .active += 1;
        Ok((
            host,
            Invocation {
                inner: Arc::downgrade(&self.inner),
                package: id.clone(),
                kind,
            },
        ))
    }

    async fn manage<F, Fut>(&self, cancel: CancellationToken, operation: F) -> Result<()>
    where
        F: FnOnce(CancellationToken) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<()>> + Send,
    {
        ensure!(
            !self.inner.closed.is_cancelled(),
            "package runtime is shut down"
        );
        let slot = self
            .inner
            .management_slots
            .clone()
            .try_acquire_owned()
            .context("package management queue is full")?;
        let cancel = cancel.child_token();
        let _cancel_on_drop = CancelOnDrop(cancel.clone());
        let pool = self.clone();
        tokio::spawn(async move {
            let _slot = slot;
            let _management = tokio::select! {
                () = cancel.cancelled() => anyhow::bail!("package management cancelled"),
                guard = pool.inner.management.lock() => guard,
            };
            ensure!(
                !pool.inner.closed.is_cancelled(),
                "package runtime is shut down"
            );
            operation(cancel).await
        })
        .await
        .context("package management supervisor failed")?
    }

    async fn load_inner(
        &self,
        mut installed: InstalledPackage,
        grants: PackageGrants,
        cancel: CancellationToken,
    ) -> Result<()> {
        ensure!(!cancel.is_cancelled(), "package management cancelled");
        installed.root = tokio::fs::canonicalize(&installed.root)
            .await
            .context("resolve package root")?;
        ensure!(
            tokio::fs::metadata(&installed.root).await?.is_dir(),
            "package root is not a directory"
        );
        ensure!(!cancel.is_cancelled(), "package management cancelled");
        let id = installed.manifest.name.clone();
        let kind = installed
            .manifest
            .runtime
            .as_ref()
            .map(|runtime| runtime.kind);
        {
            let mut state = lock(&self.inner.state);
            ensure!(
                !state.packages.contains_key(&id),
                "package is already loaded; reload explicitly"
            );
            ensure!(
                state.packages.len() < MAX_PACKAGES,
                "runtime exceeds 128 loaded packages"
            );
            state.packages.insert(
                id.clone(),
                Loaded {
                    installed,
                    grants,
                    revision: uuid::Uuid::new_v4(),
                    ready: kind.is_none(),
                    active: 0,
                },
            );
        }
        if let Some(kind) = kind {
            let result = async {
                let host = self.host(kind)?;
                self.import(&host, &id, cancel).await?;
                lock(&self.inner.state)
                    .packages
                    .get_mut(&id)
                    .context("package disappeared")?
                    .ready = true;
                Ok(())
            }
            .await;
            if result.is_err() {
                self.stop(kind).await?;
            }
            result
        } else {
            Ok(())
        }
    }

    fn host(&self, kind: RuntimeKind) -> Result<Arc<Host>> {
        let mut state = lock(&self.inner.state);
        if let Some(language) = state.languages.get(&kind) {
            ensure!(
                !language.blocked && !language.host.stopped(),
                "package language host is stopped; reload explicitly"
            );
            return Ok(language.host.clone());
        }
        let host = Host::spawn(
            kind,
            self.inner.config.clone(),
            self.inner.closed.child_token(),
        )?;
        state.languages.insert(
            kind,
            Language {
                host: host.clone(),
                active: 0,
                blocked: false,
            },
        );
        Ok(host)
    }

    async fn import(&self, host: &Host, id: &str, cancel: CancellationToken) -> Result<()> {
        let params = {
            let state = lock(&self.inner.state);
            let package = state.packages.get(id).context("package disappeared")?;
            let runtime = package
                .installed
                .manifest
                .runtime
                .as_ref()
                .context("package has no runtime")?;
            json!({"package_id":id,"root":package.installed.root,"entry":runtime.entry,"handlers":package.grants.handlers(&package.installed.manifest)})
        };
        let result = host.call("load", params, cancel, None, ()).await?;
        ensure!(
            result["package_id"] == id,
            "language host returned a different package identity"
        );
        Ok(())
    }

    fn remove(&self, id: &str) -> Result<Option<RuntimeKind>> {
        let mut state = lock(&self.inner.state);
        let package = state.packages.get(id).context("package is not loaded")?;
        let kind = package
            .installed
            .manifest
            .runtime
            .as_ref()
            .map(|runtime| runtime.kind);
        if let Some(kind) = kind
            && let Some(language) = state.languages.get_mut(&kind)
        {
            ensure!(
                language.active == 0,
                "language host has active callbacks; wait before unload"
            );
            language.blocked = true;
        }
        state.packages.remove(id);
        Ok(kind)
    }

    async fn rebuild(&self, kind: RuntimeKind, cancel: CancellationToken) -> Result<()> {
        {
            let mut state = lock(&self.inner.state);
            if let Some(language) = state.languages.get_mut(&kind) {
                ensure!(
                    language.active == 0,
                    "language host has active callbacks; wait before reload"
                );
                language.blocked = true;
            }
        }
        self.stop(kind).await?;
        let mut ids = {
            let mut state = lock(&self.inner.state);
            state.languages.remove(&kind);
            state
                .packages
                .iter()
                .filter(|(_, package)| package_kind(package) == Some(kind))
                .map(|(id, _)| id.clone())
                .collect::<Vec<_>>()
        };
        ids.sort();
        if ids.is_empty() {
            return Ok(());
        }
        let result = async {
            let host = self.host(kind)?;
            for id in &ids {
                self.import(&host, id, cancel.clone()).await?;
            }
            let mut state = lock(&self.inner.state);
            for id in &ids {
                state
                    .packages
                    .get_mut(id)
                    .context("package disappeared")?
                    .ready = true;
            }
            Ok(())
        }
        .await;
        if result.is_err() {
            self.stop(kind).await?;
        }
        result
    }

    async fn stop(&self, kind: RuntimeKind) -> Result<()> {
        let host = {
            let mut state = lock(&self.inner.state);
            for package in state
                .packages
                .values_mut()
                .filter(|package| package_kind(package) == Some(kind))
            {
                package.ready = false;
            }
            state.languages.get_mut(&kind).map(|language| {
                language.blocked = true;
                language.host.clone()
            })
        };
        if let Some(host) = host {
            host.shutdown().await?;
        }
        Ok(())
    }
}

impl PackageGrants {
    pub(crate) fn validate(&self, manifest: &Manifest) -> Result<()> {
        let exports = &manifest.exports;
        for (grants, declared) in [
            (
                &self.tools,
                exports
                    .tools
                    .iter()
                    .map(|export| &export.name)
                    .collect::<HashSet<_>>(),
            ),
            (
                &self.providers,
                exports
                    .providers
                    .iter()
                    .map(|export| &export.name)
                    .collect(),
            ),
            (
                &self.commands,
                exports.commands.iter().map(|export| &export.name).collect(),
            ),
            (
                &self.widgets,
                exports.widgets.iter().map(|export| &export.name).collect(),
            ),
        ] {
            let mut unique = HashSet::new();
            for grant in grants {
                ensure!(
                    declared.contains(grant),
                    "grant is not declared in package manifest: {grant}"
                );
                ensure!(unique.insert(grant), "duplicate package grant");
            }
        }
        let mut hooks = HashSet::new();
        for event in &self.hooks {
            ensure!(
                exports.hooks.iter().any(|hook| hook.event == *event),
                "hook grant is not declared"
            );
            ensure!(hooks.insert(event), "duplicate hook grant");
        }
        Ok(())
    }

    fn handlers(&self, manifest: &Manifest) -> Vec<String> {
        let exports = &manifest.exports;
        let handlers = exports
            .tools
            .iter()
            .filter(|export| self.tools.contains(&export.name))
            .map(|export| &export.handler)
            .chain(
                exports
                    .hooks
                    .iter()
                    .filter(|export| self.hooks.contains(&export.event))
                    .map(|export| &export.handler),
            )
            .chain(
                exports
                    .providers
                    .iter()
                    .filter(|export| self.providers.contains(&export.name))
                    .map(|export| &export.handler),
            )
            .chain(
                exports
                    .commands
                    .iter()
                    .filter(|export| self.commands.contains(&export.name))
                    .map(|export| &export.handler),
            )
            .chain(
                exports
                    .widgets
                    .iter()
                    .filter(|export| self.widgets.contains(&export.name))
                    .map(|export| &export.handler),
            );
        handlers
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }
}

pub(crate) fn callback_reservation() -> Reservation {
    Reservation {
        processes: 0,
        ..Reservation::default()
    }
}

fn package_kind(package: &Loaded) -> Option<RuntimeKind> {
    package
        .installed
        .manifest
        .runtime
        .as_ref()
        .map(|runtime| runtime.kind)
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

struct CancelOnDrop(CancellationToken);

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}
