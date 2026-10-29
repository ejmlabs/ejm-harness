use std::sync::Arc;

use anyhow::{Context, Result, ensure};
use ejm_harness_package_spec::RuntimeKind;
use serde_json::json;
use tokio_util::sync::CancellationToken;

use super::{
    InstalledPackage, PackageGrants, RuntimePool,
    state::{Language, Loaded, lock},
};
use crate::transport::Host;

const MAX_PACKAGES: usize = 128;

impl RuntimePool {
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
            let kind = lock(&pool.inner.state).kind_of(&package_id)?;
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

    /// Run one management operation at a time, bounded by a small queue and the caller's token.
    async fn manage<F, Fut>(&self, cancel: CancellationToken, operation: F) -> Result<()>
    where
        F: FnOnce(CancellationToken) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<()>> + Send,
    {
        self.ensure_open()?;
        let slot = self
            .inner
            .management_slots
            .clone()
            .try_acquire_owned()
            .context("package management queue is full")?;
        let cancel = cancel.child_token();
        let _cancel_on_drop = cancel.clone().drop_guard();
        let pool = self.clone();
        tokio::spawn(async move {
            let _slot = slot;
            let _management = tokio::select! {
                () = cancel.cancelled() => anyhow::bail!("package management cancelled"),
                guard = pool.inner.management.lock() => guard,
            };
            pool.ensure_open()?;
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
        let kind = installed.kind();
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
        let Some(kind) = kind else {
            return Ok(());
        };
        let result = async {
            let host = self.host(kind)?;
            self.import(&host, &id, cancel).await?;
            lock(&self.inner.state).mark_ready(&id)
        }
        .await;
        self.stop_on_error(kind, result).await
    }

    /// The language's shared host, spawned on first use.
    fn host(&self, kind: RuntimeKind) -> Result<Arc<Host>> {
        let mut state = lock(&self.inner.state);
        if let Some(language) = state.languages.get(&kind) {
            language.ensure_running()?;
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
            json!({
                "package_id": id,
                "root": package.installed.root,
                "entry": runtime.entry,
                "handlers": package.grants.handlers(&package.installed.manifest),
            })
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
        let kind = state.kind_of(id)?;
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
                .filter(|(_, package)| package.installed.kind() == Some(kind))
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
            ids.iter().try_for_each(|id| state.mark_ready(id))
        }
        .await;
        self.stop_on_error(kind, result).await
    }

    async fn stop(&self, kind: RuntimeKind) -> Result<()> {
        let host = {
            let mut state = lock(&self.inner.state);
            for package in state
                .packages
                .values_mut()
                .filter(|package| package.installed.kind() == Some(kind))
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

    /// A failed load or reload leaves the language host stopped, never half-running.
    async fn stop_on_error(&self, kind: RuntimeKind, result: Result<()>) -> Result<()> {
        if result.is_err() {
            self.stop(kind).await?;
        }
        result
    }
}
