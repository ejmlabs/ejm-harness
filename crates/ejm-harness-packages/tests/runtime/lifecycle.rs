use super::*;

#[path = "crash_support.rs"]
mod crash_support;

use crash_support::{HelperWitness, Roots, registry_key, snapshot_diagnostics, wait_helpers};
use ejm_harness_exec::{ProcessIdentity, process_identity};
use std::os::unix::process::ExitStatusExt;

const RESIDENT_BYTES: u64 = 64 * 1024 * 1024;
const IMPORT_BYTES: u64 = 16 * 1024 * 1024;

impl Fixture {
    fn write_import_gate(&self, fail: bool, previous: Option<u32>) -> Result<()> {
        let started = serde_json::to_string(&self.root.path().join("import-started"))?;
        let release = serde_json::to_string(&self.root.path().join("import-release"))?;
        let source = match self.kind {
            RuntimeKind::Node => {
                let previous = previous.map_or_else(String::new, |pid| {
                    format!(
                        "try {{ process.kill({pid}, 0); throw new Error('old interpreter still exists'); }} \
                         catch (error) {{ if (error.code !== 'ESRCH') throw error; }}\n"
                    )
                });
                let failure = if fail {
                    "throw new Error('fixture import rejected');\n"
                } else {
                    ""
                };
                format!(
                    "import {{ writeFileSync as recordImport, existsSync as gateExists }} from 'node:fs';\n\
                     {previous}\
                     recordImport({started}, String(process.pid));\n\
                     while (!gateExists({release})) await new Promise(resolve => setTimeout(resolve, 5));\n\
                     {failure}{NODE_SOURCE}"
                )
            }
            RuntimeKind::Python => {
                let previous = previous.map_or_else(String::new, |pid| {
                    format!(
                        "try:\n    import_os.kill({pid}, 0)\n\
                         except ProcessLookupError:\n    pass\n\
                         else:\n    raise RuntimeError('old interpreter still exists')\n"
                    )
                });
                let failure = if fail {
                    "raise RuntimeError('fixture import rejected')\n"
                } else {
                    ""
                };
                format!(
                    "import os as import_os, time as import_time\n\
                     from pathlib import Path as ImportPath\n\
                     {previous}\
                     ImportPath({started}).write_text(str(import_os.getpid()))\n\
                     while not ImportPath({release}).exists():\n    import_time.sleep(0.005)\n\
                     {failure}{PYTHON_SOURCE}"
                )
            }
        };
        self.write_entry(&source)
    }

    fn write_entry(&self, source: &str) -> Result<()> {
        let runtime = self.installed.manifest.runtime.as_ref().unwrap();
        std::fs::write(self.root.path().join(&runtime.entry), source)?;
        Ok(())
    }

    fn restore_entry(&self) -> Result<()> {
        self.write_entry(match self.kind {
            RuntimeKind::Node => NODE_SOURCE,
            RuntimeKind::Python => PYTHON_SOURCE,
        })
    }

    fn release_import(&self) -> Result<()> {
        std::fs::write(self.root.path().join("import-release"), "continue")?;
        Ok(())
    }

    async fn import_pid(&self) -> Result<u32> {
        Ok(wait_file(&self.root.path().join("import-started"))
            .await?
            .parse()?)
    }
}

fn start_import(
    pool: &RuntimePool,
    fixture: &Fixture,
    reload: bool,
    cancel: CancellationToken,
) -> tokio::task::JoinHandle<Result<()>> {
    let pool = pool.clone();
    let installed = fixture.installed.clone();
    tokio::spawn(async move {
        if reload {
            pool.reload(&installed.manifest.name, cancel).await
        } else {
            pool.load(installed, grants(), cancel).await
        }
    })
}

fn assert_import_reserved(governor: &Governor, baseline: u64, hosts: u32) {
    let usage = governor.snapshot();
    assert_eq!(usage.running, 1);
    assert_eq!(usage.reserved_processes, hosts);
    assert_eq!(
        usage.reserved_memory_bytes,
        baseline
            + u64::from(hosts) * RESIDENT_BYTES
            + IMPORT_BYTES
            + u64::try_from(usage.reserved_output_bytes).unwrap()
    );
}

async fn wait_reservations_released(governor: &Governor, baseline: u64) -> Result<()> {
    timeout(BOUND, async {
        loop {
            let usage = governor.snapshot();
            if usage.running == 0
                && usage.queued == 0
                && usage.reserved_processes == 0
                && usage.reserved_memory_bytes == baseline
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .context("package cleanup retained its reservation")?;
    Ok(())
}

async fn assert_cleaned(governor: &Governor, baseline: u64, pids: &[u32]) -> Result<()> {
    wait_reservations_released(governor, baseline).await?;
    assert!(pids.iter().all(|pid| !live(*pid)));
    Ok(())
}

#[tokio::test]
async fn coordinated_import_and_reload_failures_clean_up_before_releasing_capacity() -> Result<()> {
    for kind in LANGUAGES {
        for reload in [false, true] {
            let (_directory, pool, capacity) = coordinated_pool(RuntimeConfig::default(), 2)?;
            let governor = capacity.governor();
            let baseline = governor.snapshot().reserved_memory_bytes;
            let fixture = Fixture::new(kind, "failed-import")?;
            let previous = if reload {
                load(&pool, &fixture).await?;
                Some(u32::try_from(
                    status(&pool, &fixture, governor.clone()).await?["pid"]
                        .as_u64()
                        .context("package host PID")?,
                )?)
            } else {
                None
            };
            fixture.write_import_gate(true, previous)?;
            let loading = start_import(&pool, &fixture, reload, CancellationToken::new());
            let pid = fixture.import_pid().await?;
            assert!(live(pid));
            assert_import_reserved(&governor, baseline, 1);
            assert!(previous.is_none_or(|old| !live(old)));
            fixture.release_import()?;
            let error = timeout(BOUND, loading)
                .await??
                .expect_err("failed import was registered");
            assert!(format!("{error:#}").contains("fixture import rejected"));
            assert_cleaned(&governor, baseline, &[pid]).await?;
            assert!(pool.provider(fixture.id(), "model").is_err());

            fixture.restore_entry()?;
            timeout(BOUND, pool.reload(fixture.id(), CancellationToken::new())).await??;
            status(&pool, &fixture, governor.clone()).await?;
            timeout(BOUND, pool.shutdown()).await??;
            assert_cleaned(&governor, baseline, &[]).await?;
        }
    }
    Ok(())
}

#[tokio::test]
async fn coordinated_cancelled_and_dropped_imports_keep_ownership_until_cleanup() -> Result<()> {
    for kind in LANGUAGES {
        for dropped in [false, true] {
            let (_directory, pool, capacity) = coordinated_pool(
                RuntimeConfig {
                    cleanup_timeout: Duration::from_millis(250),
                    ..RuntimeConfig::default()
                },
                2,
            )?;
            let governor = capacity.governor();
            let baseline = governor.snapshot().reserved_memory_bytes;
            let fixture = Fixture::new(kind, "cancelled-import")?;
            fixture.write_import_gate(false, None)?;
            let cancel = CancellationToken::new();
            let loading = start_import(&pool, &fixture, false, cancel.clone());
            let pid = fixture.import_pid().await?;
            assert_import_reserved(&governor, baseline, 1);
            if dropped {
                loading.abort();
                assert!(timeout(BOUND, loading).await?.unwrap_err().is_cancelled());
            } else {
                cancel.cancel();
                assert!(!loading.is_finished());
                assert_import_reserved(&governor, baseline, 1);
                assert!(timeout(BOUND, loading).await??.is_err());
            }
            assert_cleaned(&governor, baseline, &[pid]).await?;
            timeout(BOUND, pool.shutdown()).await??;
        }
    }
    Ok(())
}

#[tokio::test]
async fn coordinated_existing_host_waits_for_import_admission_before_effects() -> Result<()> {
    for kind in LANGUAGES {
        let (_directory, pool, capacity) = coordinated_pool(RuntimeConfig::default(), 1)?;
        let governor = capacity.governor();
        let baseline = governor.snapshot().reserved_memory_bytes;
        let first = Fixture::new(kind, "existing-host")?;
        load(&pool, &first).await?;
        let pid = u32::try_from(
            status(&pool, &first, governor.clone()).await?["pid"]
                .as_u64()
                .context("package host PID")?,
        )?;
        let busy = governor
            .acquire(
                "occupied",
                ejm_harness_exec::Reservation {
                    memory_bytes: IMPORT_BYTES,
                    processes: 0,
                    ..ejm_harness_exec::Reservation::default()
                },
                &CancellationToken::new(),
            )
            .await?;
        let second = Fixture::new(kind, "waiting-import")?;
        second.write_import_gate(false, None)?;
        let cancel = CancellationToken::new();
        let loading = start_import(&pool, &second, false, cancel.clone());
        timeout(BOUND, async {
            while governor.snapshot().queued == 0 && !loading.is_finished() {
                tokio::task::yield_now().await;
            }
        })
        .await?;
        assert!(!loading.is_finished());
        assert!(!second.root.path().join("import-started").exists());
        assert!(live(pid));
        cancel.cancel();
        assert!(timeout(BOUND, loading).await??.is_err());
        drop(busy);
        assert_cleaned(&governor, baseline, &[pid]).await?;
        assert!(!second.root.path().join("import-started").exists());
        timeout(BOUND, pool.shutdown()).await??;
    }
    Ok(())
}

#[tokio::test]
async fn first_package_stages_the_complete_peak_before_native_startup() -> Result<()> {
    let (_directory, pool, capacity) = coordinated_pool(RuntimeConfig::default(), 1)?;
    let governor = capacity.governor();
    let baseline = governor.snapshot().reserved_memory_bytes;
    let occupied = governor
        .acquire_resident(
            "occupied-memory",
            governor.limits().memory_bytes - baseline - RESIDENT_BYTES,
            0,
            &CancellationToken::new(),
        )
        .await?;
    let fixture = Fixture::new(RuntimeKind::Python, "first-host-wait")?;
    fixture.write_import_gate(false, None)?;
    let cancel = CancellationToken::new();
    let loading = start_import(&pool, &fixture, false, cancel.clone());
    timeout(BOUND, async {
        while governor.snapshot().queued == 0 && !loading.is_finished() {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await?;
    assert!(capacity.native_owner().snapshot().owner_pid.is_none());
    assert!(!loading.is_finished());
    let usage = governor.snapshot();
    assert_eq!(
        usage.reserved_memory_bytes,
        governor.limits().memory_bytes - RESIDENT_BYTES
    );
    assert_eq!(usage.queued, 1);
    assert_eq!(usage.reserved_processes, 0);
    assert_eq!(usage.running, 0);
    assert!(!fixture.root.path().join("import-started").exists());

    cancel.cancel();
    assert!(timeout(BOUND, loading).await??.is_err());
    assert_eq!(governor.snapshot().queued, 0);
    assert!(!fixture.root.path().join("import-started").exists());
    drop(occupied);
    fixture.release_import()?;
    timeout(BOUND, pool.reload(fixture.id(), CancellationToken::new())).await??;
    let pid = fixture.import_pid().await?;
    assert!(live(pid));
    timeout(BOUND, pool.shutdown()).await??;
    assert_cleaned(&governor, baseline, &[pid]).await?;
    Ok(())
}

#[tokio::test]
async fn failed_final_composition_retains_package_hosts_until_explicit_shutdown() -> Result<()> {
    let (directory, pool, capacity) = coordinated_pool(RuntimeConfig::default(), 2)?;
    let governor = capacity.governor();
    let baseline = governor.snapshot().reserved_memory_bytes;
    let mut pids = Vec::new();
    let mut provider = None;
    let fixtures = [
        Fixture::new(RuntimeKind::Node, "composition-node")?,
        Fixture::new(RuntimeKind::Python, "composition-python")?,
    ];
    for fixture in &fixtures {
        load(&pool, fixture).await?;
        pids.push(u32::try_from(
            status(&pool, fixture, governor.clone()).await?["pid"]
                .as_u64()
                .context("package host PID")?,
        )?);
        provider = Some(pool.provider(fixture.id(), "model")?);
    }
    let state_dir = directory.path().join("invalid-state-root");
    std::fs::write(&state_dir, "a file cannot hold the session journal")?;
    let config = ejm_harness::HarnessConfig {
        state_dir,
        resources: capacity.snapshot().hard_resources,
        ..Default::default()
    };
    let result = ejm_harness::Harness::with_capacity(config, provider, vec![], vec![], capacity);
    assert!(result.is_err());
    assert_eq!(governor.snapshot().running, 0);
    assert_eq!(governor.snapshot().reserved_processes, 2);
    assert_eq!(
        governor.snapshot().reserved_memory_bytes,
        baseline + 2 * RESIDENT_BYTES
    );
    assert!(pids.iter().all(|pid| live(*pid)));
    timeout(BOUND, pool.shutdown()).await??;
    assert_cleaned(&governor, baseline, &pids).await?;
    Ok(())
}

#[tokio::test]
async fn shutdown_cancels_import_and_cleans_other_idle_language() -> Result<()> {
    for kind in LANGUAGES {
        let (_directory, pool, capacity) = coordinated_pool(RuntimeConfig::default(), 2)?;
        let governor = capacity.governor();
        let baseline = governor.snapshot().reserved_memory_bytes;
        let idle = Fixture::new(
            match kind {
                RuntimeKind::Node => RuntimeKind::Python,
                RuntimeKind::Python => RuntimeKind::Node,
            },
            "shutdown-idle",
        )?;
        load(&pool, &idle).await?;
        let idle_pid = u32::try_from(
            status(&pool, &idle, governor.clone()).await?["pid"]
                .as_u64()
                .context("package host PID")?,
        )?;
        let importing = Fixture::new(kind, "shutdown-import")?;
        importing.write_import_gate(false, None)?;
        let loading = start_import(&pool, &importing, false, CancellationToken::new());
        let import_pid = importing.import_pid().await?;
        assert_import_reserved(&governor, baseline, 2);
        timeout(BOUND, pool.shutdown()).await??;
        assert!(timeout(BOUND, loading).await??.is_err());
        assert_cleaned(&governor, baseline, &[idle_pid, import_pid]).await?;
    }
    Ok(())
}

struct PausedOwner(rustix::process::Pid);

impl PausedOwner {
    fn new(capacity: &ejm_harness::CapacityContext) -> Result<Self> {
        let pid = capacity
            .native_owner()
            .snapshot()
            .owner_pid
            .context("authenticated native owner PID")?;
        let pid = rustix::process::Pid::from_raw(i32::try_from(pid)?)
            .context("positive native owner PID")?;
        rustix::process::kill_process(pid, rustix::process::Signal::STOP)?;
        Ok(Self(pid))
    }
}

impl Drop for PausedOwner {
    fn drop(&mut self) {
        let _ = rustix::process::kill_process(self.0, rustix::process::Signal::CONT);
    }
}

#[tokio::test]
async fn shutdown_timeout_keeps_both_language_hosts_charged_until_owner_resumes() -> Result<()> {
    let (_directory, pool, capacity) = coordinated_pool(
        RuntimeConfig {
            cleanup_timeout: Duration::from_millis(100),
            ..RuntimeConfig::default()
        },
        2,
    )?;
    let governor = capacity.governor();
    let baseline = governor.snapshot().reserved_memory_bytes;
    let fixtures = [
        Fixture::new(RuntimeKind::Node, "paused-node")?,
        Fixture::new(RuntimeKind::Python, "paused-python")?,
    ];
    let mut pids = Vec::new();
    for fixture in &fixtures {
        load(&pool, fixture).await?;
        pids.push(u32::try_from(
            status(&pool, fixture, governor.clone()).await?["pid"]
                .as_u64()
                .context("package host PID")?,
        )?);
    }
    let paused = PausedOwner::new(&capacity)?;
    assert!(timeout(BOUND, pool.shutdown()).await?.is_err());
    let usage = governor.snapshot();
    assert_eq!(usage.running, 0);
    assert_eq!(usage.reserved_processes, 2);
    assert_eq!(usage.reserved_memory_bytes, baseline + 2 * RESIDENT_BYTES);
    assert_eq!(capacity.native_owner().snapshot().owner_deaths, 0);
    assert!(pids.iter().all(|pid| live(*pid)));
    drop(paused);
    timeout(BOUND, pool.shutdown()).await??;
    assert_cleaned(&governor, baseline, &pids).await?;
    Ok(())
}

#[tokio::test]
async fn owner_crash_during_import_and_reload_requires_explicit_recovery() -> Result<()> {
    for kind in LANGUAGES {
        for reload in [false, true] {
            let (_directory, pool, capacity) = coordinated_pool(RuntimeConfig::default(), 2)?;
            let governor = capacity.governor();
            let baseline = governor.snapshot().reserved_memory_bytes;
            let fixture = Fixture::new(kind, "crashed-import")?;
            if reload {
                load(&pool, &fixture).await?;
            }
            fixture.write_import_gate(false, None)?;
            let loading = start_import(&pool, &fixture, reload, CancellationToken::new());
            let pid = fixture.import_pid().await?;
            assert_import_reserved(&governor, baseline, 1);
            let before = capacity.snapshot();
            let owner = before.native.owner_pid.context("authenticated owner PID")?;
            let owner = rustix::process::Pid::from_raw(i32::try_from(owner)?)
                .context("positive native owner PID")?;
            rustix::process::kill_process(owner, rustix::process::Signal::KILL)?;

            let result = timeout(BOUND, loading).await;
            // Owner death is not descendant cleanup. Release the actual fixture
            // import so an interpreter surviving the broken link can exit.
            fixture.release_import()?;
            assert!(result??.is_err(), "owner loss reported a successful import");
            wait_reservations_released(&governor, baseline).await?;
            let lost = capacity.native_owner().snapshot();
            assert_eq!(lost.owner_deaths, 1);
            assert_eq!(lost.active_local_scopes, 0);
            assert_eq!(lost.last_dead_epoch, before.native.epoch);
            assert_eq!(lost.last_operation, before.native.last_operation);
            assert!(pool.provider(fixture.id(), "model").is_err());

            timeout(BOUND, async {
                loop {
                    let current = capacity.snapshot();
                    if current.operating.generation > before.operating.generation
                        && current.operating.contact_valid
                        && current.operating.target.max_jobs > 0
                    {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(2)).await;
                }
            })
            .await
            .context("fresh capacity observation did not restore admission")?;
            assert_eq!(
                capacity.native_owner().snapshot().last_operation,
                before.native.last_operation,
                "uncertain import was retried automatically"
            );
            timeout(BOUND, async {
                while live(pid) {
                    tokio::time::sleep(Duration::from_millis(2)).await;
                }
            })
            .await
            .context("released fixture import did not exit after owner loss")?;
            fixture.restore_entry()?;
            timeout(BOUND, pool.reload(fixture.id(), CancellationToken::new())).await??;
            let current = status(&pool, &fixture, governor.clone()).await?;
            assert_ne!(current["pid"].as_u64(), Some(u64::from(pid)));
            assert_ne!(
                capacity.native_owner().snapshot().epoch,
                before.native.epoch
            );
            timeout(BOUND, pool.shutdown()).await??;
            assert_cleaned(&governor, baseline, &[pid]).await?;
        }
    }
    Ok(())
}

#[derive(serde::Serialize, serde::Deserialize)]
struct CallerImportReceipt {
    caller: ProcessIdentity,
    interpreter: ProcessIdentity,
    owner: ProcessIdentity,
    native: ejm_harness_exec::native::NativeSnapshot,
    baseline: u64,
    usage: GovernorSnapshot,
}

impl CallerImportReceipt {
    fn validate(
        &self,
        caller: &ProcessIdentity,
        interpreter: &ProcessIdentity,
        owner: &ProcessIdentity,
    ) -> Result<()> {
        anyhow::ensure!(self.caller == *caller, "caller identity changed");
        anyhow::ensure!(
            self.interpreter == *interpreter,
            "interpreter identity changed"
        );
        anyhow::ensure!(self.owner == *owner, "native owner identity changed");
        anyhow::ensure!(self.usage.running == 1, "caller was not importing");
        anyhow::ensure!(
            self.usage.reserved_processes == 1,
            "missing import process charge"
        );
        anyhow::ensure!(
            self.usage.reserved_memory_bytes
                == self.baseline
                    + RESIDENT_BYTES
                    + IMPORT_BYTES
                    + u64::try_from(self.usage.reserved_output_bytes)?,
            "missing import memory charge"
        );
        Ok(())
    }
}

fn spawn_importing_caller(
    root: &Path,
    fixture: &Fixture,
    reload: bool,
) -> Result<tokio::process::Child> {
    std::fs::write(
        root.join("manifest.json"),
        serde_json::to_vec(&fixture.installed.manifest)?,
    )?;
    Ok(tokio::process::Command::new(std::env::current_exe()?)
        .args([
            "--exact",
            "lifecycle::importing_caller_fixture",
            "--ignored",
            "--nocapture",
        ])
        .env("EJM_PACKAGE_CALLER_FIXTURE_ROOT", root)
        .env("EJM_PACKAGE_CALLER_PACKAGE_ROOT", fixture.root.path())
        .env("EJM_PACKAGE_CALLER_RELOAD", reload.to_string())
        .stdout(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()?)
}

async fn fresh_caller_client_progress(
    registry: &Path,
    fixture: &Fixture,
    roots: &mut Roots,
    receipt: &CallerImportReceipt,
    phase: &mut &'static str,
) -> Result<()> {
    fixture.restore_entry()?;
    *phase = "prepare fresh participating client";
    let fresh = coordinated_capacity(registry, 2)?;
    let pool = RuntimePool::with_capacity(RuntimeConfig::default(), fresh.clone())?;
    let progress = async {
        *phase = "load package in fresh participating client";
        load(&pool, fixture).await?;
        *phase = "perform useful package call in fresh participating client";
        let current = status(&pool, fixture, fresh.governor()).await?;
        let current_pid = u32::try_from(current["pid"].as_u64().context("fresh interpreter PID")?)?;
        let current_identity = roots.capture(current_pid)?;
        roots.capture(
            fresh
                .native_owner()
                .snapshot()
                .owner_pid
                .context("fresh native owner PID")?,
        )?;
        anyhow::ensure!(
            current_identity != receipt.interpreter,
            "fresh work reused the crashed caller's interpreter"
        );
        anyhow::ensure!(
            current["value"] == "before",
            "fresh package returned no useful result"
        );
        anyhow::ensure!(
            fresh.native_owner().snapshot().epoch != receipt.native.epoch,
            "fresh work reused the lost helper generation"
        );
        Ok::<_, anyhow::Error>(())
    }
    .await;
    let shutdown = timeout(BOUND, pool.shutdown()).await;
    progress?;
    *phase = "finish fresh package pool shutdown";
    shutdown??;
    *phase = "wait for fresh interpreter and helper exit";
    roots.wait_all_ended().await?;
    *phase = "observe commitment retirement after fresh client shutdown";
    wait_helpers(&fresh, 0, phase).await?;
    Ok(())
}

#[tokio::test]
#[ignore = "Reexecuted only by the caller-crash integration test."]
async fn importing_caller_fixture() -> Result<()> {
    let Some(root) = std::env::var_os("EJM_PACKAGE_CALLER_FIXTURE_ROOT") else {
        return Ok(());
    };
    let root = std::path::PathBuf::from(root);
    let installed = InstalledPackage {
        manifest: Manifest::from_bytes(&std::fs::read(root.join("manifest.json"))?)?,
        root: std::path::PathBuf::from(
            std::env::var_os("EJM_PACKAGE_CALLER_PACKAGE_ROOT").context("package fixture root")?,
        ),
    };
    let reload = std::env::var("EJM_PACKAGE_CALLER_RELOAD")? == "true";
    let capacity = coordinated_capacity(&root.join("capacity"), 2)?;
    let governor = capacity.governor();
    let baseline = governor.snapshot().reserved_memory_bytes;
    let pool = RuntimePool::with_capacity(
        RuntimeConfig {
            call_timeout: Duration::from_secs(30),
            cleanup_timeout: Duration::from_millis(250),
            ..RuntimeConfig::default()
        },
        capacity.clone(),
    )?;
    let previous = if reload {
        pool.load(installed.clone(), grants(), CancellationToken::new())
            .await?;
        let value = pool
            .invoke_command(
                &installed.manifest.name,
                "status",
                json!({}),
                PackageContext {
                    session_id: "caller-fixture".into(),
                    cwd: installed.root.clone(),
                    cancel: CancellationToken::new(),
                    governor: governor.clone(),
                },
            )
            .await?;
        value["pid"].as_u64().context("initial package host PID")?
    } else {
        0
    };
    std::fs::write(root.join("initial-ready"), previous.to_string())?;
    wait_file(&root.join("begin-import")).await?;
    let importing = {
        let pool = pool.clone();
        let installed = installed.clone();
        tokio::spawn(async move {
            if reload {
                pool.reload(&installed.manifest.name, CancellationToken::new())
                    .await
            } else {
                pool.load(installed, grants(), CancellationToken::new())
                    .await
            }
        })
    };
    let pid: u32 = wait_file(&installed.root.join("import-started"))
        .await?
        .parse()?;
    assert_import_reserved(&governor, baseline, 1);
    let before = capacity.snapshot();
    let owner = before
        .native
        .owner_pid
        .context("fixture native owner PID")?;
    std::fs::write(
        root.join("importing.json"),
        serde_json::to_vec(&CallerImportReceipt {
            caller: process_identity(std::process::id()).context("caller identity")?,
            interpreter: process_identity(pid).context("importing interpreter identity")?,
            owner: process_identity(owner).context("native owner identity")?,
            native: before.native,
            baseline,
            usage: governor.snapshot(),
        })?,
    )?;
    let result = timeout(Duration::from_secs(30), importing).await;
    let cleanup = timeout(BOUND, pool.shutdown()).await;
    cleanup??;
    result???;
    anyhow::bail!("importing caller completed without the expected crash")
}

#[tokio::test]
async fn caller_crash_during_import_and_reload_cleans_roots_before_fresh_client_progress()
-> Result<()> {
    for kind in LANGUAGES {
        for reload in [false, true] {
            let directory = tempfile::tempdir()?;
            let root = directory.path().canonicalize()?;
            let registry = root.join("capacity");
            let observer = coordinated_capacity(&registry, 2)?;
            let key = registry_key(&registry)?;
            let fixture = Fixture::new(kind, "caller-crashed-import")?;
            let mut caller = spawn_importing_caller(&root, &fixture, reload)?;
            let mut roots = Roots::default();
            let mut phase = "capture caller identity";
            let result = async {
                let caller_identity = roots.capture(caller.id().context("caller fixture PID")?)?;
                phase = "wait for initial caller readiness";
                let previous: u32 = wait_file(&root.join("initial-ready")).await?.parse()?;
                let previous = (previous != 0)
                    .then(|| roots.capture(previous))
                    .transpose()?;
                fixture.write_import_gate(false, previous.as_ref().map(|identity| identity.pid))?;
                std::fs::write(root.join("begin-import"), "start")?;
                phase = "wait for gated import receipt";
                let receipt: CallerImportReceipt =
                    serde_json::from_str(&wait_file(&root.join("importing.json")).await?)?;
                let interpreter = roots.capture(fixture.import_pid().await?)?;
                let owner = roots.capture(receipt.owner.pid)?;
                receipt.validate(&caller_identity, &interpreter, &owner)?;
                if let Some(previous) = &previous {
                    phase = "wait for pre-reload interpreter exit";
                    Roots::wait_ended(previous).await?;
                }
                let witness = HelperWitness::capture(&registry)?;
                phase = "observe helper commitment before caller crash";
                let committed = wait_helpers(&observer, 1, phase).await?.native_helpers;
                anyhow::ensure!(
                    committed.memory_bytes == 32 * 1024 * 1024,
                    "missing coarse helper charge"
                );
                // The real caller dies here. Its governor can no longer be
                // inspected. Leave the helper runnable so lifetime-link EOF
                // can clean the root. Stopping its group before parent death
                // would invoke POSIX orphaned-group SIGHUP/SIGCONT behavior.
                phase = "kill and reap actual importing caller";
                caller.start_kill()?;
                let exit = timeout(BOUND, caller.wait()).await??;
                anyhow::ensure!(
                    exit.signal() == Some(9),
                    "caller fixture did not die from SIGKILL"
                );
                phase = "wait for importing root exit after lifetime-link EOF";
                Roots::wait_ended(&interpreter).await?;
                phase = "wait for helper idle exit after caller crash";
                Roots::wait_ended(&owner).await?;
                phase = "wait for original helper witness release";
                witness.wait_released().await?;
                phase = "observe commitment retirement after helper exit";
                wait_helpers(&observer, 0, phase).await?;
                anyhow::ensure!(
                    fixture.import_pid().await? == interpreter.pid,
                    "uncertain import was replayed"
                );
                anyhow::ensure!(
                    !fixture.root.path().join("import-release").exists(),
                    "test released the blocked import"
                );

                fresh_caller_client_progress(&registry, &fixture, &mut roots, &receipt, &mut phase)
                    .await?;
                anyhow::ensure!(
                    registry_key(&registry)? == key,
                    "capacity registry was removed or replaced"
                );
                witness.verify_identity()?;
                Ok::<_, anyhow::Error>(())
            }
            .await
            .with_context(|| {
                format!(
                    "caller crash case kind={kind:?}, reload={reload}, phase={phase}; observer={}",
                    snapshot_diagnostics(&observer.snapshot()),
                )
            });
            if caller.try_wait()?.is_none() {
                caller.start_kill()?;
            }
            let reaped = timeout(BOUND, caller.wait()).await;
            result?;
            reaped??;
        }
    }
    Ok(())
}

async fn load_idle_language_roots(
    pool: &RuntimePool,
    fixtures: &[Fixture],
    governor: &Arc<Governor>,
    baseline: u64,
    roots: &mut Roots,
    phase: &mut &'static str,
) -> Result<Vec<ProcessIdentity>> {
    let mut interpreters = Vec::new();
    for fixture in fixtures {
        load(pool, fixture).await?;
        let pid = u32::try_from(
            status(pool, fixture, governor.clone()).await?["pid"]
                .as_u64()
                .context("idle interpreter PID")?,
        )?;
        interpreters.push(roots.capture(pid)?);
    }
    *phase = "drain callbacks while both language hosts remain resident";
    timeout(BOUND, governor.wait_jobs_idle()).await?;
    let usage = governor.snapshot();
    anyhow::ensure!(
        usage.running == 0 && usage.queued == 0,
        "package callbacks did not drain"
    );
    anyhow::ensure!(
        usage.reserved_processes == 2
            && usage.reserved_memory_bytes == baseline + 2 * RESIDENT_BYTES,
        "idle language hosts lost their resident charges"
    );
    Ok(interpreters)
}

async fn reload_idle_packages(
    pool: &RuntimePool,
    fixtures: &[Fixture],
    capacity: &ejm_harness::CapacityContext,
    interpreters: &[ProcessIdentity],
    roots: &mut Roots,
) -> Result<()> {
    for (fixture, previous) in fixtures.iter().zip(interpreters) {
        timeout(BOUND, pool.reload(fixture.id(), CancellationToken::new())).await??;
        let current = status(pool, fixture, capacity.governor()).await?;
        let pid = u32::try_from(
            current["pid"]
                .as_u64()
                .context("reloaded interpreter PID")?,
        )?;
        anyhow::ensure!(
            roots.capture(pid)? != *previous && current["value"] == "before",
            "explicit reload did not perform fresh useful work"
        );
        roots.capture(
            capacity
                .native_owner()
                .snapshot()
                .owner_pid
                .context("reloaded native owner PID")?,
        )?;
    }
    Ok(())
}

#[tokio::test]
async fn idle_owner_crash_with_both_languages_requires_fresh_grant_and_explicit_reload()
-> Result<()> {
    let (directory, pool, capacity) = coordinated_pool(RuntimeConfig::default(), 2)?;
    let registry = directory.path().canonicalize()?.join("capacity");
    let key = registry_key(&registry)?;
    let governor = capacity.governor();
    let baseline = governor.snapshot().reserved_memory_bytes;
    let fixtures = [
        Fixture::new(RuntimeKind::Node, "idle-crashed-node")?,
        Fixture::new(RuntimeKind::Python, "idle-crashed-python")?,
    ];
    let mut roots = Roots::default();
    let mut phase = "load both language hosts";
    let result = async {
        let interpreters = load_idle_language_roots(
            &pool, &fixtures, &governor, baseline, &mut roots, &mut phase,
        )
        .await?;
        phase = "observe helper commitment before idle owner crash";
        let before = wait_helpers(&capacity, 1, phase).await?;
        let owner = roots.capture(before.native.owner_pid.context("idle native owner PID")?)?;
        let witness = HelperWitness::capture(&registry)?;
        phase = "kill idle owner and verify root exit";
        Roots::signal(&owner, rustix::process::Signal::KILL)?;
        Roots::wait_ended(&owner).await?;
        phase = "verify original idle owner witness release";
        witness.wait_released().await?;
        phase = "verify both original interpreter roots stopped";
        for interpreter in &interpreters {
            Roots::wait_ended(interpreter).await?;
        }
        phase = "verify original language-host reservations released";
        wait_reservations_released(&governor, baseline).await?;
        phase = "observe retired idle helper and fresh useful grant";
        let restored = wait_helpers(&capacity, 0, phase).await?;
        anyhow::ensure!(
            restored.native.owner_deaths == 1 && restored.native.active_local_scopes == 0,
            "idle owner death did not retire local ownership"
        );
        anyhow::ensure!(
            restored.operating.generation > before.operating.generation
                && restored.operating.target.max_jobs > 0,
            "no fresh useful grant after owner death"
        );
        phase = "reject package calls before explicit reload";
        for fixture in &fixtures {
            anyhow::ensure!(
                status(&pool, fixture, governor.clone()).await.is_err(),
                "idle helper loss recovered a package without explicit reload"
            );
        }
        anyhow::ensure!(
            capacity.native_owner().snapshot().last_operation == before.native.last_operation,
            "idle helper loss replayed native work"
        );
        phase = "explicitly reload both packages and perform useful calls";
        reload_idle_packages(&pool, &fixtures, &capacity, &interpreters, &mut roots).await?;
        anyhow::ensure!(
            capacity.native_owner().snapshot().epoch != before.native.epoch,
            "explicit reload reused the lost generation"
        );
        anyhow::ensure!(
            registry_key(&registry)? == key,
            "capacity registry was removed or replaced"
        );
        witness.verify_identity()?;
        Ok::<_, anyhow::Error>(())
    }
    .await
    .with_context(|| {
        format!(
            "idle owner crash phase={phase}; capacity={}",
            snapshot_diagnostics(&capacity.snapshot()),
        )
    });
    let shutdown = timeout(BOUND, pool.shutdown()).await;
    result?;
    shutdown??;
    wait_reservations_released(&governor, baseline).await?;
    roots.wait_all_ended().await?;
    wait_helpers(
        &capacity,
        0,
        "observe idle-recovery shutdown commitment retirement",
    )
    .await?;
    Ok(())
}

fn process_sample(system: &System, pid: Pid) -> Result<Value> {
    let process = system
        .process(pid)
        .context("measurement process disappeared")?;
    Ok(json!({
        "pid": pid.as_u32(),
        "rss_bytes": process.memory(),
        "cpu_percent_of_one_core": process.cpu_usage(),
    }))
}

async fn measure_host(kind: RuntimeKind, sample: usize, idle_interval: Duration) -> Result<Value> {
    let idle_interval_ms = u64::try_from(idle_interval.as_millis())?;
    let preparation = std::time::Instant::now();
    let (_directory, pool, capacity) = coordinated_pool(RuntimeConfig::default(), 2)?;
    let preparation_ms = preparation.elapsed().as_secs_f64() * 1000.0;
    let governor = capacity.governor();
    let baseline = governor.snapshot().reserved_memory_bytes;
    let fixture = Fixture::new(kind, "measured-host")?;
    let startup = std::time::Instant::now();
    load(&pool, &fixture).await?;
    let startup_ms = startup.elapsed().as_secs_f64() * 1000.0;
    let host = u32::try_from(
        status(&pool, &fixture, governor.clone()).await?["pid"]
            .as_u64()
            .context("package host PID")?,
    )?;
    let owner = capacity
        .native_owner()
        .snapshot()
        .owner_pid
        .context("authenticated owner PID")?;
    let pids = [
        Pid::from_u32(std::process::id()),
        Pid::from_u32(owner),
        Pid::from_u32(host),
    ];
    let refresh = ProcessRefreshKind::nothing().with_memory().with_cpu();
    let mut system = System::new();
    system.refresh_processes_specifics(ProcessesToUpdate::Some(&pids), true, refresh);
    tokio::time::sleep(idle_interval).await;
    system.refresh_processes_specifics(ProcessesToUpdate::Some(&pids), true, refresh);
    let usage = governor.snapshot();
    assert_eq!(usage.running, 0);
    assert_eq!(usage.reserved_processes, 1);
    assert_eq!(usage.reserved_memory_bytes, baseline + RESIDENT_BYTES);
    let result = json!({
        "kind": kind,
        "sample": sample,
        "preparation_ms": preparation_ms,
        "cold_owner_and_import_ms": startup_ms,
        "idle_interval_ms": idle_interval_ms,
        "caller": process_sample(&system, pids[0])?,
        "owner": process_sample(&system, pids[1])?,
        "interpreter": process_sample(&system, pids[2])?,
        "reserved_memory_bytes_above_baseline": usage.reserved_memory_bytes - baseline,
        "idle_active_jobs": usage.running,
    });
    let shutdown = std::time::Instant::now();
    timeout(BOUND, pool.shutdown()).await??;
    assert_cleaned(&governor, baseline, &[host]).await?;
    let shutdown_ms = shutdown.elapsed().as_secs_f64() * 1000.0;
    timeout(BOUND, async {
        while live(owner) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .context("idle native owner did not exit after package cleanup")?;
    let mut result = result;
    result["shutdown_ms"] = json!(shutdown_ms);
    Ok(result)
}

#[tokio::test]
#[ignore = "Offline startup and idle measurement; run separately from builds and other workloads."]
async fn measure_coordinated_language_host_overhead() -> Result<()> {
    let mut samples = Vec::new();
    for sample in 0..3 {
        for kind in LANGUAGES {
            samples.push(measure_host(kind, sample, Duration::from_secs(2)).await?);
        }
    }
    print_host_measurements(
        "PACKAGE_HOST_MEASUREMENTS",
        &samples,
        &[
            "one host, sequential samples, local interpreters and offline fixture imports",
            "idle RSS samples are not startup or native-tree peaks",
            "CPU percentage covers a two-second interval and is noisy near zero",
            "caller includes the libtest runtime and its capacity coordinator",
            "startup includes a fresh native owner, language bootstrap and import",
        ],
    )
}

#[tokio::test]
#[ignore = "Offline 60-second idle samples; run separately from builds and other workloads."]
async fn measure_coordinated_language_host_idle() -> Result<()> {
    let mut samples = Vec::new();
    for kind in LANGUAGES {
        samples.push(measure_host(kind, 0, Duration::from_secs(60)).await?);
    }
    print_host_measurements(
        "PACKAGE_HOST_IDLE_MEASUREMENTS",
        &samples,
        &[
            "one machine, one 60-second idle sample per language, run sequentially with offline fixture imports",
            "CPU measures the native owner and interpreter, not CLI whole-system idle",
            "one sample per language does not establish stable tails or behavior on another host",
            "idle RSS samples are not startup or native-tree peaks",
            "caller includes the libtest runtime and its capacity coordinator",
            "startup includes a fresh native owner, language bootstrap and import",
        ],
    )
}

fn print_host_measurements(marker: &str, samples: &[Value], limits: &[&str]) -> Result<()> {
    println!(
        "{marker} {}",
        serde_json::to_string(&json!({
            "target_os": std::env::consts::OS,
            "target_arch": std::env::consts::ARCH,
            "profile": if cfg!(debug_assertions) { "debug" } else { "release" },
            "samples": samples,
            "limits": limits,
        }))?
    );
    Ok(())
}
