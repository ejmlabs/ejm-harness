//! Startup and retirement waits use caller bounds, without command admission or effects.

use std::{
    fs::{File, TryLockError},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use anyhow::{Context, Result, ensure};
use tokio::time::{sleep, timeout};
use tokio_util::sync::CancellationToken;

use super::super::{
    NativeCommand, NativeOwner, NativeOwnerAdmission, NativeOwnerPermit, NativeOwnerRequest, wire,
};
use crate::{CommandRequest, Executor, Governor, GovernorSnapshot, ResourceLimits};

struct Admissions(AtomicUsize);

impl NativeOwnerAdmission for Admissions {
    fn admit(&self, request: &NativeOwnerRequest) -> Result<NativeOwnerPermit> {
        self.0.fetch_add(1, Ordering::AcqRel);
        Ok(request.permit())
    }
}

#[tokio::test]
#[ignore = "requires EJM_NATIVE_STARTUP_HELPER pointing to native_owner_helper"]
async fn startup_lock_wait_preserves_readiness_and_executor_bounds() -> Result<()> {
    let helper = PathBuf::from(
        std::env::var_os("EJM_NATIVE_STARTUP_HELPER").context("set EJM_NATIVE_STARTUP_HELPER")?,
    );
    ensure!(helper.is_absolute(), "test helper path must be absolute");
    let temporary = tempfile::tempdir()?;
    let root = temporary.path().canonicalize()?.join("state");
    let directory = wire::directory(&root, "startup-wait")?;
    let startup = wire::stable_lock(&directory, "startup.lock")?;
    let ownership = wire::stable_lock(&directory, "owner.lock")?;
    startup.try_lock()?;
    let result = timeout(Duration::from_secs(20), exercise(&root, &helper, &startup))
        .await
        .context("startup contention fixture exceeded its deadline")
        .and_then(std::convert::identity);
    // Every failed contract check still drops readiness and releases contention.
    drop(startup);
    let cleanup = timeout(wire::IDLE + Duration::from_secs(3), async {
        loop {
            match ownership.try_lock() {
                Ok(()) => return Ok::<_, anyhow::Error>(()),
                Err(TryLockError::WouldBlock) => sleep(Duration::from_millis(20)).await,
                Err(TryLockError::Error(error)) => return Err(error.into()),
            }
        }
    })
    .await
    .context("native helper did not release lifetime ownership")
    .and_then(std::convert::identity);
    if let Err(error) = cleanup {
        let retained = temporary.keep();
        return Err(error.context(format!(
            "startup result: {result:?}; cleanup unverified; retained {}",
            retained.display()
        )));
    }
    result
}

async fn exercise(root: &Path, helper: &Path, startup: &File) -> Result<()> {
    let admissions = Arc::new(Admissions(AtomicUsize::new(0)));
    let owner = NativeOwner::new(root, "startup-wait")?
        .with_helper_executable(helper)
        .with_admission(admissions.clone());
    let peer = NativeOwner::new(root, "startup-wait")?
        .with_helper_executable(helper)
        .with_admission(admissions.clone());
    let governor = Arc::new(Governor::new(ResourceLimits::default())?);
    let executor =
        Executor::new(governor.clone(), root.join("artifacts"))?.with_native_owner(owner.clone());
    caller_bounds(&executor, &owner, &governor, root).await?;
    let readiness = async { tokio::try_join!(owner.ready(), peer.ready()) };
    tokio::pin!(readiness);
    ensure!(
        timeout(wire::TIMEOUT + Duration::from_secs(1), &mut readiness)
            .await
            .is_err(),
        "startup contention completed before the caller released it"
    );
    ensure!(
        admissions.0.load(Ordering::Acquire) == 0,
        "helper admission bypassed the startup lock"
    );
    ensure!(
        governor.snapshot() == GovernorSnapshot::default(),
        "readiness retained command reservations"
    );
    startup.unlock()?;
    let (ready, shared) = timeout(wire::TIMEOUT + Duration::from_secs(2), &mut readiness)
        .await
        .context("readiness did not recover after startup lock release")??;
    ensure!(
        ready.generation() == shared.generation()
            && owner.snapshot().owner_pid.is_some()
            && owner.snapshot().owner_pid == peer.snapshot().owner_pid
            && admissions.0.load(Ordering::Acquire) == 1,
        "contending readiness requests did not reuse one helper"
    );
    ensure!(
        !root.join("must-not-launch").exists(),
        "a cancelled or expired command launched after contention ended"
    );
    let generation = ready.generation();
    drop(ready);
    drop(shared);
    retiring_owner_bounds(&executor, &owner, &governor, root, &admissions, generation).await?;
    Ok(())
}

async fn retiring_owner_bounds(
    executor: &Executor,
    owner: &NativeOwner,
    governor: &Governor,
    root: &Path,
    admissions: &Admissions,
    generation: uuid::Uuid,
) -> Result<()> {
    let directory = wire::directory(root, "startup-wait")?;
    let ownership = wire::stable_lock(&directory, "owner.lock")?;
    timeout(wire::IDLE + Duration::from_secs(3), async {
        loop {
            match ownership.try_lock() {
                Ok(()) => return Ok::<_, anyhow::Error>(()),
                Err(TryLockError::WouldBlock) => sleep(Duration::from_millis(20)).await,
                Err(TryLockError::Error(error)) => return Err(error.into()),
            }
        }
    })
    .await
    .context("initial helper did not retire")??;
    // Model the interval after endpoint closure and before witness release.
    // Port zero cannot contact a listener, independent of scheduling or reuse.
    let bytes = directory
        .read_opt(wire::DISCOVERY, 4096)?
        .context("retired helper did not publish discovery")?;
    let mut stale: wire::Advertisement = serde_json::from_slice(&bytes)?;
    stale.port = 0;
    directory.write(wire::DISCOVERY, &serde_json::to_vec(&stale)?)?;
    *crate::governor::lock(&owner.inner.cached) = Some(stale);
    caller_bounds(executor, owner, governor, root).await?;
    let readiness = owner.ready();
    tokio::pin!(readiness);
    ensure!(
        timeout(Duration::from_millis(100), &mut readiness)
            .await
            .is_err(),
        "readiness failed or completed while the old witness was held"
    );
    ensure!(
        admissions.0.load(Ordering::Acquire) == 1
            && owner.snapshot().last_operation.is_none()
            && governor.snapshot() == GovernorSnapshot::default()
            && !root.join("must-not-launch").exists(),
        "retirement waiting bypassed ownership or retained cancelled work"
    );
    drop(ownership);
    let ready = timeout(wire::TIMEOUT + Duration::from_secs(2), &mut readiness)
        .await
        .context("readiness did not recover after real witness release")??;
    ensure!(
        ready.generation() != generation && admissions.0.load(Ordering::Acquire) == 2,
        "readiness did not start exactly one successor after witness release"
    );
    let marker = root.join("successor-effect");
    let mut child = ready
        .spawn(NativeCommand::new("/usr/bin/touch")?.args([marker.as_os_str()]))
        .await?;
    ensure!(child.wait().await?.success(), "successor command failed");
    child.kill().await?;
    ensure!(
        marker.exists() && !root.join("must-not-launch").exists(),
        "succession lost useful work or launched cancelled work"
    );
    Ok(())
}

async fn caller_bounds(
    executor: &Executor,
    owner: &NativeOwner,
    governor: &Governor,
    root: &Path,
) -> Result<()> {
    let marker = root.join("must-not-launch");
    let mut request = CommandRequest {
        program: "/usr/bin/touch".into(),
        args: vec![marker.to_string_lossy().into_owned()],
        cwd: root.to_owned(),
        timeout_ms: 15_000,
        ..CommandRequest::default()
    };
    let cancel = CancellationToken::new();
    let pending = executor.run(request.clone(), cancel.clone());
    tokio::pin!(pending);
    tokio::select! {
        result = &mut pending => anyhow::bail!("command completed during startup contention: {result:?}"),
        result = timeout(Duration::from_secs(3), async {
            while governor.snapshot().queued != 1 || owner.inner.startup.try_lock().is_ok() {
                sleep(Duration::from_millis(1)).await;
            }
        }) => result.context("command did not reach native readiness")?,
    }
    ensure!(
        governor.snapshot()
            == GovernorSnapshot {
                queued: 1,
                ..GovernorSnapshot::default()
            }
            && governor.admission_demand().memory_bytes > 0,
        "native readiness consumed execution reservations"
    );
    cancel.cancel();
    let cancelled = timeout(Duration::from_secs(3), &mut pending).await??;
    ensure!(
        cancelled.cancelled
            && !cancelled.timed_out
            && cancelled.exit_code.is_none()
            && governor.snapshot() == GovernorSnapshot::default()
            && !marker.exists(),
        "cancellation retained admission or launched the command"
    );
    request.timeout_ms = 100;
    let expired = timeout(
        Duration::from_secs(3),
        executor.run(request, CancellationToken::new()),
    )
    .await??;
    ensure!(
        expired.timed_out
            && !expired.cancelled
            && expired.exit_code.is_none()
            && governor.snapshot() == GovernorSnapshot::default()
            && !marker.exists(),
        "deadline retained admission or launched the command"
    );
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn startup_exit_retries_only_clean_exit_and_remains_bounded() -> Result<()> {
    for (program, attempts, diagnostic) in [
        ("/usr/bin/true", 2, "repeatedly retired"),
        ("/usr/bin/false", 1, "failed before startup"),
    ] {
        let temporary = tempfile::tempdir()?;
        let root = temporary.path().canonicalize()?.join("state");
        let admissions = Arc::new(Admissions(AtomicUsize::new(0)));
        let owner = NativeOwner::new(&root, "startup-exit")?
            .with_helper_executable(Path::new(program))
            .with_admission(admissions.clone());
        let result = timeout(Duration::from_secs(5), owner.ready()).await?;
        let error = result
            .err()
            .context("empty helper unexpectedly became ready")?;
        ensure!(
            error.to_string().contains(diagnostic),
            "unexpected startup failure: {error}"
        );
        ensure!(
            admissions.0.load(Ordering::Acquire) == attempts,
            "wrong startup retry count"
        );
        wire::stable_lock(&wire::directory(&root, "startup-exit")?, "owner.lock")?.try_lock()?;
    }
    Ok(())
}
