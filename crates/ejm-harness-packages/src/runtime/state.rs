use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard},
};

use anyhow::{Context, Result, ensure};
use ejm_harness_package_spec::RuntimeKind;
use tokio_util::sync::CancellationToken;

use super::{InstalledPackage, PackageGrants, RuntimeConfig};
use crate::transport::Host;

pub(super) struct Inner {
    pub(super) config: RuntimeConfig,
    pub(super) state: Mutex<State>,
    pub(super) management: tokio::sync::Mutex<()>,
    pub(super) management_slots: Arc<tokio::sync::Semaphore>,
    pub(super) closed: CancellationToken,
}

#[derive(Default)]
pub(super) struct State {
    pub(super) packages: HashMap<String, Loaded>,
    pub(super) languages: HashMap<RuntimeKind, Language>,
}

#[derive(Clone)]
pub(super) struct Loaded {
    pub(super) installed: InstalledPackage,
    pub(super) grants: PackageGrants,
    pub(super) revision: uuid::Uuid,
    pub(super) ready: bool,
    pub(super) active: usize,
}

pub(super) struct Language {
    pub(super) host: Arc<Host>,
    pub(super) active: usize,
    pub(super) blocked: bool,
}

impl State {
    /// A loaded package that has finished importing and has not been stopped.
    pub(super) fn ready(&self, id: &str) -> Result<&Loaded> {
        let package = self.packages.get(id).context("package is not loaded")?;
        ensure!(
            package.ready,
            "package is loading or stopped; reload explicitly"
        );
        Ok(package)
    }

    /// The language host a loaded package needs, if it declares a runtime.
    pub(super) fn kind_of(&self, id: &str) -> Result<Option<RuntimeKind>> {
        let package = self.packages.get(id).context("package is not loaded")?;
        Ok(package.installed.kind())
    }

    pub(super) fn mark_ready(&mut self, id: &str) -> Result<()> {
        self.packages
            .get_mut(id)
            .context("package disappeared")?
            .ready = true;
        Ok(())
    }
}

impl Language {
    pub(super) fn ensure_running(&self) -> Result<()> {
        ensure!(
            !self.blocked && !self.host.stopped(),
            "package language host is stopped; reload explicitly"
        );
        Ok(())
    }
}

pub(super) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
