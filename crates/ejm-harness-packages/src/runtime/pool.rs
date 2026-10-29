use std::{
    path::PathBuf,
    sync::{Arc, Mutex, Weak},
};

use anyhow::{Context, Result, ensure};
use ejm_harness::{Hook, Provider, Tool};
use ejm_harness_exec::{Governor, Reservation};
use ejm_harness_package_spec::RuntimeKind;
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::{
    RuntimeConfig,
    state::{Inner, Loaded, State, lock},
};
use crate::{adapters, transport::Host};

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

pub(crate) struct Callback {
    pub package_id: String,
    pub revision: uuid::Uuid,
    pub handler: String,
    pub args: Value,
    pub context: Value,
    pub cancel: CancellationToken,
    pub events: Option<mpsc::Sender<RuntimeEvent>>,
}

#[derive(Clone)]
pub struct RuntimePool {
    pub(super) inner: Arc<Inner>,
}

/// Counts one running callback against its package and language host until dropped.
struct Invocation {
    inner: Weak<Inner>,
    package: String,
    kind: RuntimeKind,
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
        let exports = &package.installed.manifest.exports.providers;
        let handler = granted_handler(
            "provider",
            &package.grants.providers,
            name,
            exports.iter().map(|export| (&export.name, &export.handler)),
        )?;
        Ok(adapters::provider(
            self.clone(),
            package_id.into(),
            package.revision,
            handler,
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
        let exports = &package.installed.manifest.exports.commands;
        let handler = granted_handler(
            "command",
            &package.grants.commands,
            name,
            exports.iter().map(|export| (&export.name, &export.handler)),
        )?;
        self.invoke_admitted(package_id, package.revision, &handler, args, context)
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
        let exports = &package.installed.manifest.exports.widgets;
        let handler = granted_handler(
            "widget",
            &package.grants.widgets,
            name,
            exports.iter().map(|export| (&export.name, &export.handler)),
        )?;
        self.invoke_admitted(package_id, package.revision, &handler, args, context)
            .await
    }

    pub(crate) async fn invoke<T: Send + 'static>(
        &self,
        callback: Callback,
        retained: T,
    ) -> Result<Value> {
        let (host, ownership) = self.begin(&callback)?;
        let params = json!({
            "package_id": callback.package_id,
            "handler": callback.handler,
            "args": callback.args,
            "context": callback.context,
        });
        host.call(
            "invoke",
            params,
            callback.cancel,
            callback.events,
            (retained, ownership),
        )
        .await
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

    pub(super) fn ensure_open(&self) -> Result<()> {
        ensure!(
            !self.inner.closed.is_cancelled(),
            "package runtime is shut down"
        );
        Ok(())
    }

    fn package(&self, id: &str) -> Result<Loaded> {
        self.ensure_open()?;
        lock(&self.inner.state).ready(id).cloned()
    }

    fn begin(&self, callback: &Callback) -> Result<(Arc<Host>, Invocation)> {
        self.ensure_open()?;
        let mut state = lock(&self.inner.state);
        let id = &callback.package_id;
        let package = state.ready(id)?;
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
        let kind = package.installed.kind().context("package has no runtime")?;
        let language = state
            .languages
            .get_mut(&kind)
            .context("package language host unavailable")?;
        language.ensure_running()?;
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
}

pub(crate) fn callback_reservation() -> Reservation {
    Reservation {
        processes: 0,
        ..Reservation::default()
    }
}

/// The handler of a declared export, once the host has granted its name.
fn granted_handler<'a>(
    kind: &str,
    granted: &[String],
    name: &str,
    declared: impl IntoIterator<Item = (&'a String, &'a String)>,
) -> Result<String> {
    ensure!(
        granted.iter().any(|grant| grant == name),
        "{kind} export is not granted"
    );
    declared
        .into_iter()
        .find(|(declared, _)| *declared == name)
        .map(|(_, handler)| handler.clone())
        .with_context(|| format!("{kind} export is not declared"))
}
