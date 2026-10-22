//! Real helper/client crash and transport fixtures. Run with `cargo run -p
//! ejm-harness-exec --example native_owner_smoke --offline --locked`.

use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    process::ExitCode,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

#[cfg(unix)]
use std::sync::atomic::AtomicU32;

use anyhow::{Context, Result, ensure};
use ejm_harness_exec::{
    CommandRequest, Executor, Governor, OperatingTarget, Reservation, ResourceLimits,
    native::{self, NativeCommand, NativeOwner},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn main() -> Result<ExitCode> {
    #[cfg(unix)]
    pause_before_helper_dispatch()?;
    if let Some(result) = native::dispatch_helper() {
        #[cfg(unix)]
        if let Err(error) = &result
            && let Some(marker) = std::env::var_os("EJM_FIXTURE_NATIVE_PRE_DISPATCH")
        {
            std::fs::write(
                PathBuf::from(marker).with_extension("error"),
                format!("{error:#}"),
            )?;
        }
        return result;
    }
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.first().is_some_and(|arg| arg == "scope-exit") {
        return Ok(ExitCode::SUCCESS);
    }
    if args.first().is_some_and(|arg| arg == "scope-echo") {
        let mut input = Vec::new();
        ensure!(
            std::env::var_os("EJM_FIXTURE_REMOVED").is_none(),
            "removed environment survived native transport"
        );
        std::io::stdin().read_to_end(&mut input)?;
        println!("{}", std::env::var("EJM_FIXTURE_VALUE")?);
        println!("{}", std::env::current_dir()?.display());
        std::io::stdout().write_all(&input)?;
        return Ok(ExitCode::SUCCESS);
    }
    if args.first().is_some_and(|arg| arg == "scope-detach-parent") {
        let _child = std::process::Command::new(std::env::current_exe()?)
            .arg("scope-detached-sleep")
            .arg(args.get(1).context("missing detached PID path")?)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()?;
        std::thread::sleep(Duration::from_secs(30));
        return Ok(ExitCode::SUCCESS);
    }
    if args
        .first()
        .is_some_and(|arg| arg == "scope-sleep" || arg == "scope-detached-sleep")
    {
        #[cfg(unix)]
        if args
            .first()
            .is_some_and(|arg| arg == "scope-detached-sleep")
        {
            rustix::process::setsid()?;
        }
        std::fs::write(
            args.get(1).context("missing PID path")?,
            std::process::id().to_string(),
        )?;
        std::thread::sleep(Duration::from_secs(30));
        return Ok(ExitCode::SUCCESS);
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    runtime.block_on(run(&args))?;
    Ok(ExitCode::SUCCESS)
}

async fn run(args: &[std::ffi::OsString]) -> Result<()> {
    #[cfg(unix)]
    if args
        .first()
        .is_some_and(|arg| arg == "witness-client" || arg == "admission-pause-client")
    {
        let root = PathBuf::from(args.get(1).context("missing witness state path")?);
        let marker = PathBuf::from(args.get(2).context("missing witness marker")?);
        let owner = NativeOwner::new(root, "fixture")?;
        let owner = if let Some(executable) = args.get(3) {
            owner.with_helper_executable(PathBuf::from(executable))
        } else {
            owner
        };
        let owner = if args
            .first()
            .is_some_and(|arg| arg == "admission-pause-client")
        {
            owner.with_admission(Arc::new(PausedAdmission(marker.clone())))
        } else {
            owner
        };
        let ready = owner.ready().await?;
        if args.first().is_some_and(|arg| arg == "witness-client") {
            std::fs::write(
                marker,
                serde_json::to_vec(&serde_json::json!({
                    "pid": owner.snapshot().owner_pid.context("ready owner PID missing")?,
                    "generation": ready.generation(),
                }))?,
            )?;
        }
        std::future::pending::<()>().await;
        return Ok(());
    }
    if args
        .first()
        .is_some_and(|arg| arg == "client" || arg == "idle-client")
    {
        let root = PathBuf::from(args.get(1).context("missing state path")?);
        let marker = PathBuf::from(args.get(2).context("missing marker path")?);
        let owner = NativeOwner::new(root, "fixture")?;
        let _child = if args.first().is_some_and(|arg| arg == "idle-client") {
            let mut child = owner
                .spawn(
                    NativeCommand::new(std::env::current_exe()?)?
                        .args(["scope-exit"])
                        .piped_stdin(false),
                )
                .await?;
            ensure!(child.wait().await?.success(), "idle client command failed");
            child.kill().await?;
            std::fs::write(
                &marker,
                owner
                    .snapshot()
                    .owner_pid
                    .context("idle client owner PID missing")?
                    .to_string(),
            )?;
            child
        } else {
            owner.spawn(sleep_command(&marker)?).await?
        };
        std::future::pending::<()>().await;
        return Ok(());
    }
    tokio::time::timeout(Duration::from_secs(45), smoke())
        .await
        .context("native owner smoke timed out")?
}

#[cfg(unix)]
fn pause_before_helper_dispatch() -> Result<()> {
    if std::env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new("--ejm-native-owner-v2"))
        && let Some(marker) = std::env::var_os("EJM_FIXTURE_NATIVE_PRE_DISPATCH")
    {
        let marker = PathBuf::from(marker);
        std::fs::write(&marker, std::process::id().to_string())?;
        let resume = marker.with_extension("resume");
        let deadline = Instant::now() + Duration::from_secs(10);
        while !resume.exists() {
            ensure!(
                Instant::now() < deadline,
                "pre-dispatch fixture was not released"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    Ok(())
}

#[cfg(unix)]
struct PausedAdmission(PathBuf);

#[cfg(unix)]
impl native::NativeOwnerAdmission for PausedAdmission {
    fn admit(&self, request: &native::NativeOwnerRequest) -> Result<native::NativeOwnerPermit> {
        std::fs::write(&self.0, std::process::id().to_string())?;
        rustix::process::kill_process(rustix::process::getpid(), rustix::process::Signal::STOP)?;
        Ok(request.permit())
    }
}

fn sleep_command(marker: &Path) -> Result<NativeCommand> {
    Ok(NativeCommand::new(std::env::current_exe()?)?
        .args([std::ffi::OsStr::new("scope-sleep"), marker.as_os_str()])
        .lifetime(Duration::from_secs(30))
        .reservation(Reservation {
            memory_bytes: 128 * 1024 * 1024,
            processes: 4,
            ..Reservation::default()
        }))
}

async fn echo(owner: Option<&NativeOwner>, directory: &Path) -> Result<()> {
    let mut child = native::spawn(
        NativeCommand::new(std::env::current_exe()?)?
            .args(["scope-echo"])
            .current_dir(directory)
            .env_clear()
            .env("EJM_FIXTURE_VALUE", "private-client-value")
            .env("EJM_FIXTURE_REMOVED", "removed-value")
            .env_remove("EJM_FIXTURE_REMOVED")
            .lifetime(Duration::from_secs(10)),
        owner,
    )
    .await?;
    let mut stdin = child.take_stdin().context("stdin missing")?;
    let stdout = child.take_stdout().context("stdout missing")?;
    let stderr = child.take_stderr().context("stderr missing")?;
    let input = vec![b'x'; 2 * 1024 * 1024];
    let expected = input.len();
    let write = tokio::spawn(async move {
        stdin.write_all(&input).await?;
        stdin.shutdown().await
    });
    let read = tokio::spawn(async move {
        let mut stdout = stdout;
        let mut value = Vec::new();
        stdout.read_to_end(&mut value).await?;
        Ok::<_, std::io::Error>(value)
    });
    let errors = tokio::spawn(async move {
        let mut stderr = stderr;
        tokio::io::copy(&mut stderr, &mut tokio::io::sink()).await
    });
    ensure!(child.wait().await?.success(), "context echo failed");
    write.await??;
    errors.await??;
    let output = read.await??;
    let prefix = format!("private-client-value\n{}\n", directory.display());
    ensure!(
        output.starts_with(prefix.as_bytes()) && output.len() == prefix.len() + expected,
        "caller environment, cwd, or stream bytes changed"
    );
    Ok(())
}

async fn smoke() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let directory = temp.path().canonicalize()?;
    let root = directory.join("state");
    #[cfg(unix)]
    startup_witness_crashes(&directory)
        .await
        .context("startup ownership crash fixtures")?;
    let owner = NativeOwner::new(&root, "fixture")?;
    ensure!(
        owner.snapshot().owner_pid.is_none() && !root.exists(),
        "lazy handle started native work"
    );
    let cost = measure_owner(&owner, &directory).await?;
    #[cfg(unix)]
    pause_owner(&owner).await.context("paused owner")?;
    let eof_us = client_loss(&owner, &root, &directory)
        .await
        .context("busy client loss")?;
    idle_client_loss(&owner, &root, &directory)
        .await
        .context("idle client loss")?;
    #[cfg(unix)]
    idle_owner_loss(&owner, &directory)
        .await
        .context("idle owner loss")?;
    #[cfg(unix)]
    let detached_survived = owner_loss(&owner, &directory)
        .await
        .context("busy owner loss")?;
    #[cfg(not(unix))]
    let detached_survived: Option<bool> = None;
    let idle = Instant::now();
    wait_for_exit(
        owner
            .snapshot()
            .owner_pid
            .context("final owner PID missing")?,
    )
    .await?;
    let idle_us = idle.elapsed().as_micros();
    #[cfg(unix)]
    executor_owner_loss(&directory)
        .await
        .context("executor owner loss")?;
    #[cfg(unix)]
    executor_caller_cleanup(&directory)
        .await
        .context("executor caller cleanup")?;
    println!(
        "{}",
        serde_json::json!({
            "platform":format!("{}-{}",std::env::consts::OS,std::env::consts::ARCH),
            "profile":if cfg!(debug_assertions) { "debug" } else { "release" }, "warm_iterations":4,
            "cold_ready_us":cost.ready_us,"first_ready_echo_2mib_us":cost.first_echo_us,
            "warm_echo_2mib_us":cost.warm_us,"local_echo_2mib_us":cost.baseline_us,
            "warm_exit_us":cost.native_exit,"local_exit_us":cost.local_exit,
            "owner_memory_scope":"helper_process_only",
            "owner_ready_rss_bytes":cost.ready_rss,"owner_ready_descriptors":cost.ready_descriptors,
            "owner_ready_physical_footprint":cost.ready_footprint,
            "owner_retained_physical_footprint":cost.retained_footprint,
            "owner_retained_native_operations":21,"owner_retained_capacity_saturated":false,
            "owner_idle_rss_bytes":cost.rss,"owner_idle_descriptors":cost.descriptors,
            "client_eof_cleanup_us":eof_us,"idle_exit_us":idle_us,
            "busy_client_crash_progress":true,"idle_client_crash_progress":true,
            "idle_owner_crash_progress":cfg!(unix),
            "paused_owner_replaced":false,"stale_epoch_replayed":false,"detached_survived_owner_death":detached_survived,
            "executor_owner_death_parked_before_guard_release":cfg!(unix),
            "executor_progress_after_fresh_grant":cfg!(unix),
            "executor_cancel_retained_through_cleanup":cfg!(unix),
            "executor_dropped_caller_retained_through_cleanup":cfg!(unix),
            "executor_spawn_failure_released_after_rejection":cfg!(unix),
            "pre_spawn_launcher_death_released_witness":cfg!(unix),
            "pre_dispatch_helper_retained_witness_after_launcher_death":cfg!(unix),
            "helper_exit_released_inherited_witness":cfg!(unix),
            "user_children_did_not_retain_helper_witness":cfg!(unix),
        })
    );
    Ok(())
}

struct OwnerMeasurements {
    ready_us: u128,
    first_echo_us: u128,
    warm_us: u128,
    baseline_us: u128,
    native_exit: u128,
    local_exit: u128,
    ready_rss: u64,
    ready_descriptors: Option<usize>,
    ready_footprint: Option<OwnerFootprint>,
    retained_footprint: Option<OwnerFootprint>,
    rss: u64,
    descriptors: Option<usize>,
}

async fn measure_owner(owner: &NativeOwner, directory: &Path) -> Result<OwnerMeasurements> {
    let cold = Instant::now();
    let ready = owner.ready().await.context("cold readiness")?;
    let ready_us = cold.elapsed().as_micros();
    let pid = ready
        .owner()
        .snapshot()
        .owner_pid
        .context("ready owner PID missing")?;
    let ready_footprint = owner_footprint(pid, None);
    let (ready_rss, ready_descriptors) = owner_cost(pid)?;
    let first_echo = Instant::now();
    echo(Some(ready.owner()), directory)
        .await
        .context("first ready echo")?;
    let first_echo_us = first_echo.elapsed().as_micros();
    let operation = owner
        .snapshot()
        .last_operation
        .context("operation missing")?;
    let warm = Instant::now();
    for _ in 0..4 {
        echo(Some(owner), directory).await.context("warm echo")?;
    }
    let warm_us = warm.elapsed().as_micros() / 4;
    let baseline = Instant::now();
    for _ in 0..4 {
        echo(None, directory).await.context("local echo")?;
    }
    let baseline_us = baseline.elapsed().as_micros() / 4;
    let native_exit = exit_cost(Some(owner)).await.context("native exit cost")?;
    let local_exit = exit_cost(None).await.context("local exit cost")?;
    let retained = owner.snapshot();
    ensure!(
        retained.owner_pid == Some(pid) && retained.epoch == Some(ready.generation()),
        "owner generation changed during cost measurements"
    );
    let retained_footprint = owner_footprint(
        pid,
        ready_footprint
            .as_ref()
            .map(|sample| sample.process_start_abstime),
    );
    let (rss, descriptors) = owner_cost(pid)?;
    drop(ready);
    ensure!(
        owner.reconcile(operation).await? == native::NativeOperationState::Finished,
        "same-generation reconciliation lost completion"
    );
    Ok(OwnerMeasurements {
        ready_us,
        first_echo_us,
        warm_us,
        baseline_us,
        native_exit,
        local_exit,
        ready_rss,
        ready_descriptors,
        ready_footprint,
        retained_footprint,
        rss,
        descriptors,
    })
}

#[cfg(unix)]
async fn startup_witness_crashes(directory: &Path) -> Result<()> {
    use std::fs::TryLockError;

    use rustix::process::Pid;

    for after_creation in [false, true] {
        let name = if after_creation {
            "witness-after"
        } else {
            "witness-before"
        };
        let root = directory.join(name);
        let marker = directory.join(format!("{name}.pid"));
        let mut command = std::process::Command::new(std::env::current_exe()?);
        command
            .arg(if after_creation {
                "witness-client"
            } else {
                "admission-pause-client"
            })
            .arg(&root)
            .arg(&marker)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        if after_creation {
            command.env("EJM_FIXTURE_NATIVE_PRE_DISPATCH", &marker);
        }
        let mut launcher = command.spawn()?;
        wait_for_file(&marker).await?;
        let paused_pid = std::fs::read_to_string(&marker)?.parse::<u32>()?;
        let mut paused = Some(OwnerPause::new(
            Pid::from_raw(i32::try_from(paused_pid)?).context("invalid paused fixture PID")?,
        )?);
        let witness = std::fs::File::open(root.join("native/fixture/owner.lock"))?;
        ensure!(
            matches!(witness.try_lock(), Err(TryLockError::WouldBlock)),
            "paused startup lost its lifetime witness"
        );
        // Resume before orphaning the helper's process group. Unix job control
        // may otherwise end a stopped orphaned group with SIGHUP. The helper
        // remains behind its release barrier until after launcher death.
        if after_creation {
            drop(paused.take());
        }
        launcher.kill()?;
        launcher.wait()?;
        drop(paused);
        if after_creation {
            ensure!(
                matches!(witness.try_lock(), Err(TryLockError::WouldBlock)),
                "paused pre-dispatch helper lost its inherited witness after launcher death"
            );
            std::fs::write(marker.with_extension("resume"), [])?;
            wait_for_file(&root.join("native/fixture/endpoint.json"))
                .await
                .with_context(|| {
                    format!(
                        "resumed helper did not adopt its inherited witness: {}",
                        std::fs::read_to_string(marker.with_extension("error"))
                            .unwrap_or_else(|_| "no helper error report".into())
                    )
                })?;
            let ready = NativeOwner::new(&root, "fixture")?.ready().await?;
            ensure!(
                ready.owner().snapshot().owner_pid == Some(paused_pid),
                "resumed helper was replaced before readiness"
            );
            drop(ready);
            wait_for_witness(&witness)
                .await
                .context("post-dispatch helper witness remained held")?;
            wait_for_exit(paused_pid).await?;
        } else {
            wait_for_witness(&witness)
                .await
                .context("pre-spawn launcher witness remained held")?;
            ensure!(
                !root.join("native/fixture/endpoint.json").exists(),
                "interrupted pre-spawn admission created a helper"
            );
        }
    }
    Ok(())
}

#[cfg(unix)]
async fn wait_for_witness(witness: &std::fs::File) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match witness.try_lock() {
                Ok(()) => return Ok(()),
                Err(std::fs::TryLockError::WouldBlock) => {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                Err(std::fs::TryLockError::Error(error)) => return Err(error),
            }
        }
    })
    .await
    .context("lifetime witness did not release")??;
    Ok(())
}

struct AdmissionProbe {
    governor: Arc<Governor>,
    released_while_parked: Arc<AtomicBool>,
}

impl Drop for AdmissionProbe {
    fn drop(&mut self) {
        self.released_while_parked.store(
            !self.governor.operating_snapshot().contact_valid,
            Ordering::Relaxed,
        );
    }
}

#[cfg(unix)]
async fn executor_owner_loss(directory: &Path) -> Result<()> {
    use rustix::process::{Pid, Signal, kill_process, kill_process_group};
    let owner = NativeOwner::new(directory.join("executor-state"), "fixture")?;
    let governor = Arc::new(Governor::new(ResourceLimits::default())?);
    let executor = Executor::new(governor.clone(), directory.join("artifacts"))?
        .with_native_owner(owner.clone());
    let marker = directory.join("executor-child.pid");
    let mut request = CommandRequest {
        program: std::env::current_exe()?.to_string_lossy().into_owned(),
        args: vec!["scope-sleep".into(), marker.to_string_lossy().into_owned()],
        cwd: directory.to_owned(),
        timeout_ms: 30_000,
        ..CommandRequest::default()
    };
    let released_while_parked = Arc::new(AtomicBool::new(false));
    let retained = AdmissionProbe {
        governor: governor.clone(),
        released_while_parked: released_while_parked.clone(),
    };
    let running_executor = executor.clone();
    let running_request = request.clone();
    let running = tokio::spawn(async move {
        running_executor
            .run_with_retained(
                running_request,
                tokio_util::sync::CancellationToken::new(),
                retained,
            )
            .await
    });
    wait_for_file(&marker).await?;
    let child_pid = std::fs::read_to_string(&marker)?.parse::<u32>()?;
    let owner_pid = owner
        .snapshot()
        .owner_pid
        .context("executor owner PID missing")?;
    kill_process(
        Pid::from_raw(i32::try_from(owner_pid)?).context("invalid owner PID")?,
        Signal::KILL,
    )?;
    let outcome = tokio::time::timeout(Duration::from_secs(5), running).await;
    // Unix descendants may survive owner death. Clean the fixture explicitly.
    if let Some(pid) = Pid::from_raw(i32::try_from(child_pid)?) {
        let _ = kill_process_group(pid, Signal::KILL);
    }
    wait_for_exit(child_pid).await?;
    ensure!(outcome??.is_err(), "lost owner returned command success");
    ensure!(
        released_while_parked.load(Ordering::Relaxed),
        "guard released before admission parked"
    );
    ensure!(
        governor.snapshot().running == 0,
        "dead scope retained active job slot"
    );
    ensure!(
        owner.snapshot().owner_deaths == 1,
        "owner death was not cached"
    );
    let target = OperatingTarget::from_limits(governor.limits());
    governor.install_target(governor.operating_snapshot().generation + 1, target, None)?;
    request.args = vec!["scope-exit".into()];
    let output = executor
        .run(request, tokio_util::sync::CancellationToken::new())
        .await?;
    ensure!(
        output.exit_code == Some(0),
        "fresh grant made no useful progress"
    );
    wait_for_exit(
        owner
            .snapshot()
            .owner_pid
            .context("replacement owner PID missing")?,
    )
    .await?;
    Ok(())
}

async fn exit_cost(owner: Option<&NativeOwner>) -> Result<u128> {
    let start = Instant::now();
    for _ in 0..16 {
        let mut child = native::spawn(
            NativeCommand::new(std::env::current_exe()?)?
                .args(["scope-exit"])
                .piped_stdin(false),
            owner,
        )
        .await?;
        ensure!(child.wait().await?.success(), "exit fixture failed");
        child.kill().await?;
    }
    Ok(start.elapsed().as_micros() / 16)
}

fn owner_cost(pid: u32) -> Result<(u64, Option<usize>)> {
    let mut system = sysinfo::System::new();
    let native = sysinfo::Pid::from_u32(pid);
    system.refresh_processes_specifics(
        sysinfo::ProcessesToUpdate::Some(&[native]),
        true,
        sysinfo::ProcessRefreshKind::nothing().with_memory(),
    );
    let process = system
        .process(native)
        .context("owner process absent during cost sample")?;
    Ok((process.memory(), process.open_files()))
}

#[derive(serde::Serialize)]
struct OwnerFootprint {
    current_bytes: u64,
    lifetime_peak_bytes: u64,
    process_start_abstime: u64,
}

#[cfg(target_os = "macos")]
fn owner_footprint(pid: u32, expected_start: Option<u64>) -> Option<OwnerFootprint> {
    use libproc::pid_rusage::{RUsageInfoV4, pidrusage};

    let usage = pidrusage::<RUsageInfoV4>(i32::try_from(pid).ok()?).ok()?;
    if usage.ri_proc_start_abstime == 0
        || usage.ri_proc_exit_abstime != 0
        || usage.ri_phys_footprint == 0
        || usage.ri_lifetime_max_phys_footprint < usage.ri_phys_footprint
        || expected_start.is_some_and(|start| start != usage.ri_proc_start_abstime)
    {
        return None;
    }
    Some(OwnerFootprint {
        current_bytes: usage.ri_phys_footprint,
        lifetime_peak_bytes: usage.ri_lifetime_max_phys_footprint,
        process_start_abstime: usage.ri_proc_start_abstime,
    })
}

#[cfg(not(target_os = "macos"))]
fn owner_footprint(_pid: u32, _expected_start: Option<u64>) -> Option<OwnerFootprint> {
    None
}

#[cfg(unix)]
async fn pause_owner(owner: &NativeOwner) -> Result<()> {
    use rustix::process::Pid;
    let pid = owner.snapshot().owner_pid.context("owner PID missing")?;
    let native = Pid::from_raw(i32::try_from(pid)?).context("invalid PID")?;
    let paused_owner = OwnerPause::new(native)?;
    let paused = owner
        .spawn(NativeCommand::new(std::env::current_exe()?)?.args(["scope-exit"]))
        .await;
    drop(paused_owner);
    ensure!(paused.is_err(), "paused owner accepted work");
    ensure!(
        owner.snapshot().owner_pid == Some(pid),
        "paused owner was replaced"
    );
    Ok(())
}

async fn client_loss(owner: &NativeOwner, root: &Path, directory: &Path) -> Result<u128> {
    let marker = directory.join("client-child.pid");
    let mut client = tokio::process::Command::new(std::env::current_exe()?)
        .arg("client")
        .arg(root)
        .arg(&marker)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    wait_for_file(&marker).await?;
    let pid = std::fs::read_to_string(&marker)?.parse::<u32>()?;
    let start = Instant::now();
    client.kill().await?;
    wait_for_exit(pid).await?;
    let cleanup_us = start.elapsed().as_micros();
    echo(Some(owner), directory).await?;
    Ok(cleanup_us)
}

async fn idle_client_loss(owner: &NativeOwner, root: &Path, directory: &Path) -> Result<()> {
    let marker = directory.join("idle-client-owner.pid");
    let mut client = tokio::process::Command::new(std::env::current_exe()?)
        .arg("idle-client")
        .arg(root)
        .arg(&marker)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    wait_for_file(&marker).await?;
    let owner_pid = std::fs::read_to_string(&marker)?.parse::<u32>()?;
    client.kill().await?;
    echo(Some(owner), directory).await?;
    ensure!(
        owner.snapshot().owner_pid == Some(owner_pid),
        "idle client death replaced a live owner"
    );
    Ok(())
}

#[cfg(unix)]
async fn idle_owner_loss(owner: &NativeOwner, directory: &Path) -> Result<()> {
    use rustix::process::{Pid, Signal, kill_process};
    let previous = owner.snapshot();
    let pid = previous.owner_pid.context("idle owner PID missing")?;
    kill_process(
        Pid::from_raw(i32::try_from(pid)?).context("invalid owner PID")?,
        Signal::KILL,
    )?;
    wait_for_exit(pid).await?;
    echo(Some(owner), directory).await?;
    ensure!(
        owner.snapshot().epoch != previous.epoch,
        "idle owner crash retained its old epoch"
    );
    Ok(())
}

#[cfg(unix)]
struct OwnerPause(rustix::process::Pid);

#[cfg(unix)]
impl OwnerPause {
    fn new(pid: rustix::process::Pid) -> Result<Self> {
        rustix::process::kill_process(pid, rustix::process::Signal::STOP)?;
        Ok(Self(pid))
    }
}

#[cfg(unix)]
impl Drop for OwnerPause {
    fn drop(&mut self) {
        let _ = rustix::process::kill_process(self.0, rustix::process::Signal::CONT);
    }
}

#[cfg(unix)]
struct CallerProbe {
    released: Arc<AtomicBool>,
    child_pid: Arc<AtomicU32>,
    released_while_live: Arc<AtomicBool>,
}

#[cfg(unix)]
impl Drop for CallerProbe {
    fn drop(&mut self) {
        let pid = self.child_pid.load(Ordering::Relaxed);
        self.released_while_live.store(
            pid != 0 && ejm_harness_exec::process_identity(pid).is_some(),
            Ordering::Relaxed,
        );
        self.released.store(true, Ordering::Release);
    }
}

#[cfg(unix)]
async fn executor_caller_cleanup(directory: &Path) -> Result<()> {
    use tokio_util::sync::CancellationToken;

    let owner = NativeOwner::new(directory.join("caller-state"), "fixture")?;
    let governor = Arc::new(Governor::new(ResourceLimits::default())?);
    let executor = Executor::new(governor.clone(), directory.join("caller-artifacts"))?
        .with_native_owner(owner.clone());
    for dropped in [false, true] {
        executor_cancelled_caller(&executor, &owner, &governor, directory, dropped).await?;
    }

    executor_spawn_failure(&executor, &governor, directory).await?;
    let output = executor
        .run(
            CommandRequest {
                program: std::env::current_exe()?.to_string_lossy().into_owned(),
                args: vec!["scope-exit".into()],
                cwd: directory.to_owned(),
                ..CommandRequest::default()
            },
            CancellationToken::new(),
        )
        .await?;
    ensure!(
        output.exit_code == Some(0),
        "caller cleanup blocked later work"
    );
    wait_for_exit(
        owner
            .snapshot()
            .owner_pid
            .context("final caller owner missing")?,
    )
    .await?;
    Ok(())
}

#[cfg(unix)]
async fn executor_cancelled_caller(
    executor: &Executor,
    owner: &NativeOwner,
    governor: &Governor,
    directory: &Path,
    dropped: bool,
) -> Result<()> {
    use rustix::process::Pid;
    use tokio_util::sync::CancellationToken;

    let marker = directory.join(format!("caller-{dropped}.pid"));
    let request = CommandRequest {
        program: std::env::current_exe()?.to_string_lossy().into_owned(),
        args: vec!["scope-sleep".into(), marker.to_string_lossy().into_owned()],
        cwd: directory.to_owned(),
        timeout_ms: 30_000,
        ..CommandRequest::default()
    };
    let released = Arc::new(AtomicBool::new(false));
    let child_pid = Arc::new(AtomicU32::new(0));
    let released_while_live = Arc::new(AtomicBool::new(false));
    let retained = CallerProbe {
        released: released.clone(),
        child_pid: child_pid.clone(),
        released_while_live: released_while_live.clone(),
    };
    let cancel = CancellationToken::new();
    let running_executor = executor.clone();
    let running_cancel = cancel.clone();
    let running = tokio::spawn(async move {
        running_executor
            .run_with_retained(request, running_cancel, retained)
            .await
    });
    wait_for_file(&marker).await?;
    let pid = std::fs::read_to_string(&marker)?.parse::<u32>()?;
    ensure!(
        ejm_harness_exec::process_identity(pid).is_some(),
        "live child identity was unavailable to the retention probe"
    );
    child_pid.store(pid, Ordering::Relaxed);
    let owner_pid = Pid::from_raw(i32::try_from(
        owner
            .snapshot()
            .owner_pid
            .context("caller owner PID missing")?,
    )?)
    .context("invalid owner PID")?;
    let paused_owner = OwnerPause::new(owner_pid)?;
    if dropped {
        running.abort();
    } else {
        cancel.cancel();
    }
    tokio::time::sleep(Duration::from_millis(30)).await;
    let retained_during_pause =
        !released.load(Ordering::Acquire) && governor.snapshot().running == 1;
    drop(paused_owner);
    ensure!(
        retained_during_pause,
        "caller exit released live native ownership"
    );
    let result = tokio::time::timeout(Duration::from_secs(5), running).await?;
    if dropped {
        ensure!(
            result.is_err_and(|error| error.is_cancelled()),
            "caller was not dropped"
        );
    } else {
        ensure!(result??.cancelled, "command cancellation was not reported");
    }
    wait_for_release(&released).await?;
    ensure!(
        !released_while_live.load(Ordering::Relaxed) && governor.snapshot().running == 0,
        "cleanup released the guard or lease before reaping its child"
    );
    wait_for_exit(pid).await?;
    Ok(())
}

#[cfg(unix)]
async fn wait_for_release(released: &AtomicBool) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(5), async {
        while !released.load(Ordering::Acquire) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .context("caller guard was not released after cleanup")
}

#[cfg(unix)]
async fn executor_spawn_failure(
    executor: &Executor,
    governor: &Governor,
    directory: &Path,
) -> Result<()> {
    let released = Arc::new(AtomicBool::new(false));
    let rejected = executor
        .run_with_retained(
            CommandRequest {
                program: directory
                    .join("missing-program")
                    .to_string_lossy()
                    .into_owned(),
                cwd: directory.to_owned(),
                ..CommandRequest::default()
            },
            tokio_util::sync::CancellationToken::new(),
            CallerProbe {
                released: released.clone(),
                child_pid: Arc::new(AtomicU32::new(0)),
                released_while_live: Arc::new(AtomicBool::new(false)),
            },
        )
        .await;
    ensure!(
        rejected.is_err() && released.load(Ordering::Acquire) && governor.snapshot().running == 0,
        "spawn rejection retained a guard or job lease"
    );
    Ok(())
}

#[cfg(unix)]
async fn owner_loss(owner: &NativeOwner, directory: &Path) -> Result<bool> {
    use rustix::process::{Pid, Signal, kill_process, kill_process_group};
    let marker = directory.join("detached.pid");
    let mut busy = owner
        .spawn(
            NativeCommand::new(std::env::current_exe()?)?
                .args([
                    std::ffi::OsStr::new("scope-detach-parent"),
                    marker.as_os_str(),
                ])
                .lifetime(Duration::from_secs(30))
                .reservation(Reservation {
                    memory_bytes: 128 * 1024 * 1024,
                    processes: 4,
                    ..Reservation::default()
                }),
        )
        .await?;
    wait_for_file(&marker).await?;
    let detached = std::fs::read_to_string(&marker)?.parse::<u32>()?;
    let busy_pid = busy.id().context("busy PID missing")?;
    let old = owner
        .snapshot()
        .last_operation
        .context("busy operation missing")?;
    let native = Pid::from_raw(i32::try_from(
        owner.snapshot().owner_pid.context("owner PID missing")?,
    )?)
    .context("invalid owner PID")?;
    kill_process(native, Signal::KILL)?;
    ensure!(
        busy.wait().await.is_err(),
        "owner death claimed confirmed cleanup"
    );
    let survived = ejm_harness_exec::process_identity(detached).is_some();
    // Clean fixture descendants explicitly. Owner death cannot promise this on Unix.
    for pid in [busy_pid, detached] {
        if let Some(native) = Pid::from_raw(i32::try_from(pid)?) {
            let _ = kill_process_group(native, Signal::KILL);
        }
        wait_for_exit(pid).await?;
    }
    echo(Some(owner), directory).await?;
    ensure!(
        owner.snapshot().epoch != Some(old.epoch),
        "owner generation did not change"
    );
    ensure!(
        owner.reconcile(old).await.is_err(),
        "stale epoch was accepted"
    );
    Ok(survived)
}

async fn wait_for_file(path: &Path) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(5), async {
        while !path.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .context("fixture child did not start")
}

async fn wait_for_exit(pid: u32) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut system = sysinfo::System::new();
        loop {
            let native = sysinfo::Pid::from_u32(pid);
            system.refresh_processes_specifics(
                sysinfo::ProcessesToUpdate::Some(&[native]),
                true,
                sysinfo::ProcessRefreshKind::nothing(),
            );
            if system
                .process(native)
                .is_none_or(|process| process.status() == sysinfo::ProcessStatus::Zombie)
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .context("fixture process remained live")
}
