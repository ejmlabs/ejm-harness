//! Bounded helper admission for a participant's staged native demand.

use std::{
    path::{Path, PathBuf},
    sync::mpsc,
    time::{Duration, Instant},
};

use anyhow::{Result, ensure};
use ejm_harness_exec::{
    HostSampler,
    native::{NativeOwnerAdmission, NativeOwnerPermit, NativeOwnerRequest},
};

use super::{CONTACT_TIME, execution_domain, registry::Registry};

pub(super) struct NativeAdmission {
    root: PathBuf,
    owner: String,
    wake: mpsc::SyncSender<()>,
}

impl NativeAdmission {
    pub(super) fn new(root: &Path, owner: &str, wake: mpsc::SyncSender<()>) -> Self {
        Self {
            root: root.into(),
            owner: owner.into(),
            wake,
        }
    }
}

impl NativeOwnerAdmission for NativeAdmission {
    fn domain_key(&self) -> Result<Option<String>> {
        // NativeOwner coalesces preparation before dependent reservations.
        // Keep old readiness guards bound to their original domain and epoch.
        Ok(Some(execution_domain(&HostSampler::default().sample())))
    }

    fn admit(&self, request: &NativeOwnerRequest) -> Result<NativeOwnerPermit> {
        let deadline = Instant::now() + CONTACT_TIME;
        let mut sampler = HostSampler::default();
        let mut registry = Registry::open(&self.root)?;
        loop {
            let observations = sampler.sample();
            ensure!(
                execution_domain(&observations) == request.domain_key(),
                "native helper resource domain changed; prepare capacity again"
            );
            ensure!(
                Instant::now() < deadline,
                "native helper capacity admission timed out"
            );
            if registry.admit_native_helper(&self.owner, request, &observations)? {
                return Ok(request.permit());
            }
            let _ = self.wake.try_send(());
            let remaining = deadline.saturating_duration_since(Instant::now());
            ensure!(
                !remaining.is_zero(),
                "native helper capacity admission timed out"
            );
            // Only a new helper generation takes this bounded wait. Ordinary
            // commands and status never visit the registry through this port.
            std::thread::sleep(remaining.min(Duration::from_millis(500)));
        }
    }
}
