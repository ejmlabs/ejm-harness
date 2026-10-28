use std::{fs::TryLockError, time::Duration};

use anyhow::Result;
use tokio::time::Instant;

use super::{
    NativeCleanupState, NativeOperation, NativeOperationState, NativeOwner, client,
    wire::{self, Advertisement, Completion},
};
use crate::governor::lock;

const PROBE_INTERVAL: Duration = Duration::from_millis(500);

/// Closing the lifetime connection asks a surviving owner to clean up. Keep the
/// caller's reservation until that owner confirms completion or its lock ends.
pub(super) async fn resolve(
    owner: &NativeOwner,
    ad: &Advertisement,
    operation: NativeOperation,
) -> Completion {
    let mut query = true;
    loop {
        if owner_died(owner, ad).await {
            return Completion::uncertain(NativeCleanupState::OwnerDied);
        }
        if query {
            match client::query(owner, ad, operation).await {
                Ok(NativeOperationState::Finished) => {
                    return Completion::uncertain(NativeCleanupState::Complete);
                }
                // The bounded result cache has expired. Further queries cannot
                // recover cleanup evidence and must not prevent idle shutdown.
                Ok(NativeOperationState::Expired) => query = false,
                _ => {}
            }
        }
        tokio::time::sleep(PROBE_INTERVAL).await;
    }
}

async fn owner_died(owner: &NativeOwner, ad: &Advertisement) -> bool {
    let mut checks = owner.inner.recovery.lock().await;
    if lock(&owner.activity).dead_epochs.contains(&ad.epoch) {
        return true;
    }
    if checks
        .last_probe
        .is_some_and(|(epoch, checked)| epoch == ad.epoch && checked.elapsed() < PROBE_INTERVAL)
    {
        return false;
    }
    checks.last_probe = Some((ad.epoch, Instant::now()));
    let root = owner.inner.root.clone();
    let domain = owner.inner.domain.clone();
    let original = ad.clone();
    let dead = tokio::task::spawn_blocking(move || -> Result<bool> {
        let directory = wire::directory(&root, &domain)?;
        if let Some(bytes) = directory.read_opt(wire::DISCOVERY, 4096)?
            && let Ok(current) = serde_json::from_slice::<Advertisement>(&bytes)
            && current.protocol == wire::PROTOCOL
            && current.domain == domain
            && current.epoch != original.epoch
        {
            // Only a replacement holding the same lifetime lock publishes
            // a new epoch in this protected directory.
            return Ok(true);
        }
        let ownership = wire::stable_lock(&directory, "owner.lock")?;
        match ownership.try_lock() {
            Ok(()) => {
                ownership.unlock()?;
                Ok(true)
            }
            Err(TryLockError::WouldBlock) => Ok(false),
            Err(TryLockError::Error(error)) => Err(error.into()),
        }
    })
    .await;
    if !matches!(dead, Ok(Ok(true))) {
        return false;
    }
    let mut activity = lock(&owner.activity);
    if !activity.dead_epochs.contains(&ad.epoch) {
        activity.dead_epochs.push_back(ad.epoch);
        if activity.dead_epochs.len() > wire::MAX_COMPLETED {
            activity.dead_epochs.pop_front();
        }
        activity.owner_deaths = activity.owner_deaths.saturating_add(1);
        activity.last_dead_epoch = Some(ad.epoch);
    }
    true
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        reason = "The bounded recovery fixture fails at the assertion site."
    )]

    use super::*;

    #[tokio::test]
    async fn unavailable_owner_retains_ownership_until_lifetime_lock_release() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap().join("state");
        let handle = NativeOwner::new(&root, "fixture").unwrap();
        let directory = wire::directory(&root, "fixture").unwrap();
        let ownership = wire::stable_lock(&directory, "owner.lock").unwrap();
        ownership.try_lock().unwrap();
        // An owner that hangs up at once keeps each query short; a connect to
        // port 0 hangs until the wire timeout on Windows.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move { while listener.accept().await.is_ok() {} });
        let ad = Advertisement {
            protocol: wire::PROTOCOL,
            epoch: uuid::Uuid::new_v4(),
            capability: uuid::Uuid::new_v4(),
            pid: std::process::id(),
            port,
            domain: "fixture".into(),
            accounted: false,
        };
        let operation = NativeOperation {
            epoch: ad.epoch,
            id: 1,
        };
        let mut pending = Box::pin(resolve(&handle, &ad, operation));
        assert!(
            tokio::time::timeout(Duration::from_millis(30), &mut pending)
                .await
                .is_err()
        );
        assert_eq!(handle.snapshot().owner_deaths, 0);
        ownership.unlock().unwrap();
        let completed = tokio::time::timeout(wire::TIMEOUT + PROBE_INTERVAL * 2, &mut pending)
            .await
            .unwrap();
        assert_eq!(completed.cleanup, NativeCleanupState::OwnerDied);
        assert!(completed.status().is_err());
        assert_eq!(handle.snapshot().owner_deaths, 1);
        // Other scopes from the same lost owner reuse the cached proof.
        assert!(owner_died(&handle, &ad).await);
        assert_eq!(handle.snapshot().owner_deaths, 1);
        assert_eq!(handle.snapshot().last_dead_epoch, Some(ad.epoch));
    }
}
