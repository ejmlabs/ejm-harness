use std::{future::Future, path::Path, sync::Arc, time::Duration};

use anyhow::{Context, Result, ensure};
use ejm_harness_exec::{Governor, Reservation};
use ejm_harness_package_spec::MAX_ARCHIVE_BYTES;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use crate::{
    PackageManager, PackageRecord, RegistryClient,
    manager::{pack_directory_checked, read_bounded_checked},
};

mod archives;
mod download;

#[cfg(test)]
mod tests;

/// Install a pinned, bounded snapshot without executing package managers or package code.
///
/// GitHub commit archives, public npm packages with SHA-512 integrity, and portable
/// `PyPI` wheels are supported. Dependencies must be bundled or supplied by the host.
/// Explicit remote references always take precedence over repository filenames.
/// Registry references require an explicitly configured client. Other sources
/// need no registry configuration.
/// Blocking work retains admission until it stops, even after this future is dropped.
/// Native file calls cannot be interrupted. Cancellation can race the final atomic
/// registration; inspect the package list if the result is uncertain.
///
/// # Errors
/// Rejects unpinned or unsupported sources, unsafe archives, failed admission,
/// cancellation, and integrity failures. Installs start disabled with empty grants.
/// A deadline or cancellation error can return before an active native file call ends.
pub async fn install_source(
    manager: &PackageManager,
    source: &str,
    registry: Option<&RegistryClient>,
    governor: &Arc<Governor>,
    cancel: CancellationToken,
) -> Result<PackageRecord> {
    ensure!(
        source.len() <= 1024 && !source.chars().any(char::is_control),
        "invalid package source"
    );
    let manager = manager.clone();
    let source = source.to_owned();
    let registry = registry.cloned();
    let runtime = tokio::runtime::Handle::current();
    admitted_install(governor, cancel, move |control| {
        let bytes = runtime.block_on(control.run(resolve(&source, registry.as_ref(), control)))?;
        manager.install_checked(&bytes, &source, &|| control.check())
    })
    .await
}

// The blocking task owns admission, not the awaiting caller. A dropped caller
// cancels its child token; the task checks it before further work or registration.
async fn admitted_install(
    governor: &Arc<Governor>,
    cancel: CancellationToken,
    operation: impl FnOnce(&InstallControl) -> Result<PackageRecord> + Send + 'static,
) -> Result<PackageRecord> {
    let cancel = cancel.child_token();
    let _cancel_on_drop = cancel.clone().drop_guard();
    let control = InstallControl {
        cancel,
        deadline: Instant::now() + Duration::from_millis(governor.limits().max_job_ms),
    };
    let lease = control
        .run(governor.acquire(
            "package-install",
            Reservation {
                memory_bytes: 256 * 1024 * 1024,
                processes: 0,
                heavy: true,
                ..Reservation::default()
            },
            &control.cancel,
        ))
        .await?;
    let task_control = control.clone();
    let task = tokio::task::spawn_blocking(move || {
        let _lease = lease;
        task_control.check()?;
        operation(&task_control)
    });
    control
        .run(async { task.await.context("package installation task failed")? })
        .await
}

#[derive(Clone)]
struct InstallControl {
    cancel: CancellationToken,
    deadline: Instant,
}

impl InstallControl {
    fn check(&self) -> Result<()> {
        ensure!(
            Instant::now() < self.deadline,
            "package installation deadline exceeded"
        );
        ensure!(!self.cancel.is_cancelled(), "package install cancelled");
        Ok(())
    }

    async fn run<T>(&self, future: impl Future<Output = Result<T>>) -> Result<T> {
        tokio::select! {
            biased;
            () = tokio::time::sleep_until(self.deadline) => anyhow::bail!("package installation deadline exceeded"),
            () = self.cancel.cancelled() => anyhow::bail!("package install cancelled"),
            result = future => result,
        }
    }
}

async fn resolve(
    source: &str,
    registry: Option<&RegistryClient>,
    control: &InstallControl,
) -> Result<Vec<u8>> {
    if let Some(spec) = source.strip_prefix("git:") {
        return download::Downloads::new()?.git(spec, &control.cancel).await;
    }
    if let Some(spec) = source.strip_prefix("npm:") {
        return download::Downloads::new()?.npm(spec, &control.cancel).await;
    }
    if let Some(spec) = source.strip_prefix("pypi:") {
        return download::Downloads::new()?
            .pypi(spec, &control.cancel)
            .await;
    }
    if source.starts_with('@') {
        let registry =
            registry.context("registry package source requires a configured registry")?;
        let (name, version) = source
            .rsplit_once('@')
            .context("registry source requires @publisher/package@exact-version")?;
        let record = registry.version(name, version).await?;
        return registry.download(&record).await;
    }
    let local = Path::new(source.strip_prefix("local:").unwrap_or(source));
    control.check()?;
    ensure!(local.exists(), "local package path does not exist");
    control.check()?;
    if local.is_dir() {
        pack_directory_checked(local, &|| control.check())
    } else {
        read_bounded_checked(local, MAX_ARCHIVE_BYTES, &|| control.check())
    }
}
