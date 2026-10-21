#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "Test fixtures and assertions must fail at their call site."
)]

use std::{path::Path, sync::Arc, time::Duration};

use anyhow::{Context, Result};
use ejm_harness::{ModelDelta, ModelRequest};
use ejm_harness_exec::{Governor, GovernorSnapshot, ResourceLimits};
use ejm_harness_package_spec::Manifest;
use ejm_harness_packages::{
    InstalledPackage, PackageContext, PackageGrants, RuntimeConfig, RuntimeKind, RuntimePool,
};
use serde_json::{Value, json};
use sysinfo::{Pid, ProcessRefreshKind, ProcessStatus, ProcessesToUpdate, System};
use tempfile::TempDir;
use tokio::{sync::mpsc, task::JoinSet, time::timeout};
use tokio_util::sync::CancellationToken;

const BOUND: Duration = Duration::from_secs(10);
const LANGUAGES: [RuntimeKind; 2] = [RuntimeKind::Node, RuntimeKind::Python];

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[path = "runtime/lifecycle.rs"]
mod lifecycle;

#[cfg(unix)]
#[tokio::test]
async fn python_bootstrap_excludes_untrusted_repository_imports() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let fixture = Fixture::new(RuntimeKind::Python, "isolated")?;
    let python = std::process::Command::new("python3")
        .args(["-I", "-c", "import sys; print(sys.executable)"])
        .output()?;
    anyhow::ensure!(
        python.status.success(),
        "resolve fixture Python interpreter"
    );
    let executable = String::from_utf8(python.stdout)?.trim().to_owned();
    let marker = fixture.root.path().join("repository-imported");
    std::fs::write(
        fixture.root.path().join("asyncio.py"),
        format!(
            "from pathlib import Path\nPath({:?}).write_text('executed')\nraise RuntimeError('repository module imported')\n",
            marker.to_string_lossy()
        ),
    )?;
    let wrapper = fixture.root.path().join("python-wrapper");
    std::fs::write(
        &wrapper,
        format!(
            "#!{executable}\nimport os, sys\nos.chdir({:?})\nos.execv(sys.executable, [sys.executable, *sys.argv[1:]])\n",
            fixture.root.path().to_string_lossy()
        ),
    )?;
    std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o700))?;
    let pool = RuntimePool::new(RuntimeConfig {
        python: wrapper,
        call_timeout: BOUND,
        cleanup_timeout: Duration::from_secs(1),
        ..Default::default()
    })?;
    load(&pool, &fixture).await?;
    let result = status(&pool, &fixture, governor()?).await?;
    assert!(result["pid"].as_u64().is_some());
    assert!(
        !marker.exists(),
        "repository code ran while bootstrapping Python"
    );
    timeout(BOUND, pool.shutdown()).await??;
    Ok(())
}

const NODE_SOURCE: &str = r"
import { spawn } from 'node:child_process';
import { existsSync, writeFileSync } from 'node:fs';
import { value } from './helper.mjs';
let active = 0;
let peak = 0;
export async function command(args, ctx) {
  console.log('fixture ran');
  active++;
  peak = Math.max(peak, active);
  try {
    if (args.delay_ms) await new Promise(resolve => setTimeout(resolve, args.delay_ms));
    return {pid: process.pid, session_id: ctx.session_id, cwd: ctx.cwd, args, value, active, peak};
  } finally { active--; }
}
export async function hold(args, ctx) {
  writeFileSync(args.started, String(process.pid));
  while (!existsSync(args.release)) {
    ctx.signal.throwIfAborted();
    await new Promise(resolve => setTimeout(resolve, 5));
  }
  return {pid: process.pid};
}
export async function stubborn(args, ctx) {
  const descendant = spawn(process.execPath, ['-e', 'setInterval(() => {}, 1000)'], {stdio: 'inherit'});
  ctx.signal.addEventListener('abort', () => writeFileSync(args.cancelled, 'seen'), {once: true});
  writeFileSync(args.started, `${process.pid} ${descendant.pid}`);
  await new Promise(() => {});
}
export async function forbidden(args, ctx) { writeFileSync(args.marker, 'called'); return null; }
export async function provider(args, ctx) {
  await ctx.delta('hello ');
  await ctx.delta('world');
  return {content: 'hello world', usage: {input_tokens: 2, output_tokens: args.max_output_tokens}};
}
";

const PYTHON_SOURCE: &str = r"
import asyncio
import os
from pathlib import Path
import subprocess
import sys
from .helper import VALUE

active = 0
peak = 0

async def command(args, ctx):
    global active, peak
    print('fixture ran')
    active += 1
    peak = max(peak, active)
    try:
        await asyncio.sleep(args.get('delay_ms', 0) / 1000)
        return {'pid': os.getpid(), 'session_id': ctx['session_id'], 'cwd': ctx['cwd'],
                'args': args, 'value': VALUE, 'active': active, 'peak': peak}
    finally:
        active -= 1

async def hold(args, ctx):
    Path(args['started']).write_text(str(os.getpid()))
    while not Path(args['release']).exists():
        ctx.raise_if_cancelled()
        await asyncio.sleep(0.005)
    return {'pid': os.getpid()}

async def stubborn(args, ctx):
    descendant = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(30)'])
    Path(args['started']).write_text(f'{os.getpid()} {descendant.pid}')
    while not ctx.is_cancelled():
        await asyncio.sleep(0.005)
    Path(args['cancelled']).write_text('seen')
    await asyncio.Event().wait()

async def forbidden(args, ctx):
    Path(args['marker']).write_text('called')

async def provider(args, ctx):
    await ctx.delta('hello ')
    await ctx.delta('world')
    return {'content': 'hello world',
            'usage': {'input_tokens': 2, 'output_tokens': args['max_output_tokens']}}
";

struct Fixture {
    root: TempDir,
    installed: InstalledPackage,
    kind: RuntimeKind,
}

impl Fixture {
    fn new(kind: RuntimeKind, name: &str) -> Result<Self> {
        let root = tempfile::tempdir()?;
        let (entry, source, runtime) = match kind {
            RuntimeKind::Node => ("entry.mjs", NODE_SOURCE, "node"),
            RuntimeKind::Python => ("entry.py", PYTHON_SOURCE, "python"),
        };
        std::fs::write(root.path().join(entry), source)?;
        let manifest = Manifest::from_bytes(&serde_json::to_vec(&json!({
            "schema_version": 1,
            "name": format!("@tests/{name}"),
            "version": "1.0.0",
            "description": "Offline extension runtime fixture",
            "runtime": {"kind": runtime, "entry": entry},
            "exports": {
                "commands": [
                    {"name": "status", "description": "Return runtime identity", "handler": "command"},
                    {"name": "hold", "description": "Wait for a fixture gate", "handler": "hold"},
                    {"name": "stubborn", "description": "Require host cleanup", "handler": "stubborn"},
                    {"name": "denied", "description": "Never granted", "handler": "forbidden"}
                ],
                "providers": [{"name": "model", "handler": "provider"}]
            }
        }))?)?;
        let installed = InstalledPackage {
            manifest,
            root: root.path().to_path_buf(),
        };
        let fixture = Self {
            root,
            installed,
            kind,
        };
        fixture.write_helper("before")?;
        Ok(fixture)
    }

    fn id(&self) -> &str {
        &self.installed.manifest.name
    }

    fn write_helper(&self, value: &str) -> Result<()> {
        let (name, source) = match self.kind {
            RuntimeKind::Node => ("helper.mjs", format!("export const value = {value:?};\n")),
            RuntimeKind::Python => ("helper.py", format!("VALUE = {value:?}\n")),
        };
        std::fs::write(self.root.path().join(name), source)?;
        Ok(())
    }

    fn context(&self, governor: Arc<Governor>, session: &str) -> PackageContext {
        PackageContext {
            session_id: session.into(),
            cwd: self.root.path().to_path_buf(),
            cancel: CancellationToken::new(),
            governor,
        }
    }
}

fn pool() -> Result<RuntimePool> {
    RuntimePool::new(RuntimeConfig {
        node: std::env::var_os("EJM_HARNESS_TEST_NODE")
            .unwrap_or_else(|| "node".into())
            .into(),
        python: std::env::var_os("EJM_HARNESS_TEST_PYTHON")
            .unwrap_or_else(|| RuntimeConfig::default().python.into_os_string())
            .into(),
        call_timeout: BOUND,
        cleanup_timeout: Duration::from_secs(1),
        ..RuntimeConfig::default()
    })
}

fn governor() -> Result<Arc<Governor>> {
    Ok(Arc::new(Governor::new(ResourceLimits::default())?))
}

fn grants() -> PackageGrants {
    PackageGrants {
        commands: vec!["status".into(), "hold".into(), "stubborn".into()],
        providers: vec!["model".into()],
        ..PackageGrants::default()
    }
}

async fn load(pool: &RuntimePool, fixture: &Fixture) -> Result<()> {
    timeout(
        BOUND,
        pool.load(
            fixture.installed.clone(),
            grants(),
            CancellationToken::new(),
        ),
    )
    .await??;
    Ok(())
}

async fn status(pool: &RuntimePool, fixture: &Fixture, governor: Arc<Governor>) -> Result<Value> {
    timeout(
        BOUND,
        pool.invoke_command(
            fixture.id(),
            "status",
            json!({}),
            fixture.context(governor, "fixture"),
        ),
    )
    .await?
}

async fn wait_file(path: &Path) -> Result<String> {
    timeout(BOUND, async {
        loop {
            if let Ok(value) = tokio::fs::read_to_string(path).await
                && !value.is_empty()
            {
                return value;
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .context("fixture callback did not reach its gate")
}

fn live(pid: u32) -> bool {
    let mut system = System::new();
    let pids = [Pid::from_u32(pid)];
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&pids),
        true,
        ProcessRefreshKind::nothing(),
    );
    system
        .process(pids[0])
        .is_some_and(|process| process.status() != ProcessStatus::Zombie)
}

#[tokio::test]
async fn languages_execute_callbacks_and_share_one_host_across_packages() -> Result<()> {
    for kind in LANGUAGES {
        let pool = pool()?;
        let first = Fixture::new(kind, "first")?;
        let second = Fixture::new(kind, "second")?;
        let governor = governor()?;
        load(&pool, &first).await?;
        load(&pool, &second).await?;
        let left = status(&pool, &first, governor.clone()).await?;
        let right = status(&pool, &second, governor.clone()).await?;
        assert!(left["pid"].as_u64().is_some_and(|pid| pid > 0));
        assert_eq!(left["pid"], right["pid"]);
        assert_eq!(left["session_id"], "fixture");
        assert_eq!(left["cwd"], json!(first.root.path()));
        assert_eq!(governor.snapshot(), GovernorSnapshot::default());
        timeout(BOUND, pool.shutdown()).await??;
    }
    Ok(())
}

#[tokio::test]
async fn exports_require_explicit_declared_grants() -> Result<()> {
    for kind in LANGUAGES {
        let pool = pool()?;
        let fixture = Fixture::new(kind, "grants")?;
        let governor = governor()?;
        let marker = fixture.root.path().join("forbidden");
        let invalid = PackageGrants {
            commands: vec!["undeclared".into()],
            ..PackageGrants::default()
        };
        assert!(
            timeout(
                BOUND,
                pool.load(fixture.installed.clone(), invalid, CancellationToken::new())
            )
            .await?
            .is_err()
        );
        timeout(
            BOUND,
            pool.load(
                fixture.installed.clone(),
                PackageGrants::default(),
                CancellationToken::new(),
            ),
        )
        .await??;
        let error = timeout(
            BOUND,
            pool.invoke_command(
                fixture.id(),
                "denied",
                json!({"marker":marker}),
                fixture.context(governor.clone(), "denied"),
            ),
        )
        .await?
        .expect_err("ungranted command executed");
        assert!(error.to_string().contains("not granted"));
        assert!(pool.provider(fixture.id(), "model").is_err());
        assert!(!marker.exists());
        assert_eq!(governor.snapshot(), GovernorSnapshot::default());
        timeout(BOUND, pool.shutdown()).await??;
    }
    Ok(())
}

#[tokio::test]
async fn hundred_sessions_share_four_callback_slots_without_process_reservations() -> Result<()> {
    for kind in LANGUAGES {
        let pool = pool()?;
        let fixture = Fixture::new(kind, "admission")?;
        let governor = governor()?;
        load(&pool, &fixture).await?;
        let mut tasks = JoinSet::new();
        for index in 0..100 {
            let pool = pool.clone();
            let package = fixture.id().to_owned();
            let context = fixture.context(governor.clone(), &format!("agent-{index}"));
            tasks.spawn(async move {
                pool.invoke_command(
                    &package,
                    "status",
                    json!({"delay_ms":25,"index":index}),
                    context,
                )
                .await
            });
        }
        let mut peak_running = 0;
        let mut peak_queued = 0;
        let mut peak_callback = 0;
        let mut host_pid = None;
        let mut tick = tokio::time::interval(Duration::from_millis(2));
        timeout(BOUND, async {
            while !tasks.is_empty() {
                tokio::select! {
                    result = tasks.join_next() => {
                        let value = result.context("missing callback task")???;
                        let pid = value["pid"].as_u64().context("callback omitted host pid")?;
                        assert_eq!(*host_pid.get_or_insert(pid), pid);
                        peak_callback = peak_callback.max(value["peak"].as_u64().context("callback omitted peak")?);
                    }
                    _ = tick.tick() => {
                        let snapshot = governor.snapshot();
                        assert!(snapshot.running <= 4);
                        assert_eq!(snapshot.reserved_processes, 0);
                        peak_running = peak_running.max(snapshot.running);
                        peak_queued = peak_queued.max(snapshot.queued);
                    }
                }
            }
            anyhow::Ok(())
        }).await??;
        assert_eq!(peak_running, 4);
        assert_eq!(peak_callback, 4);
        assert!(peak_queued > 0);
        assert_eq!(governor.snapshot(), GovernorSnapshot::default());
        timeout(BOUND, pool.shutdown()).await??;
    }
    Ok(())
}

#[tokio::test]
async fn stubborn_cancellation_retains_admission_until_host_exit() -> Result<()> {
    for kind in LANGUAGES {
        let pool = pool()?;
        let fixture = Fixture::new(kind, "cancel")?;
        let governor = governor()?;
        load(&pool, &fixture).await?;
        let started = fixture.root.path().join("started");
        let cancelled = fixture.root.path().join("cancelled");
        let context = fixture.context(governor.clone(), "stubborn");
        let cancel = context.cancel.clone();
        let worker = pool.clone();
        let package = fixture.id().to_owned();
        let started_arg = started.clone();
        let cancelled_arg = cancelled.clone();
        let task = tokio::spawn(async move {
            worker
                .invoke_command(
                    &package,
                    "stubborn",
                    json!({"started":started_arg,"cancelled":cancelled_arg}),
                    context,
                )
                .await
        });
        let pids: Vec<u32> = wait_file(&started)
            .await?
            .split_whitespace()
            .map(str::parse)
            .collect::<std::result::Result<_, _>>()?;
        assert_eq!(pids.len(), 2);
        cancel.cancel();
        wait_file(&cancelled).await?;
        assert!(!task.is_finished());
        assert_eq!(governor.snapshot().running, 1);
        assert_eq!(governor.snapshot().reserved_processes, 0);
        assert!(pids.iter().all(|pid| live(*pid)));
        assert!(timeout(BOUND, task).await??.is_err());
        timeout(BOUND, governor.wait_idle()).await?;
        assert_eq!(governor.snapshot(), GovernorSnapshot::default());
        assert!(pids.iter().all(|pid| !live(*pid)));
        assert!(status(&pool, &fixture, governor).await.is_err());
        timeout(BOUND, pool.shutdown()).await??;
    }
    Ok(())
}

#[tokio::test]
async fn active_callbacks_block_unload_and_reload_without_interrupting_work() -> Result<()> {
    for kind in LANGUAGES {
        let pool = pool()?;
        let fixture = Fixture::new(kind, "busy")?;
        let governor = governor()?;
        load(&pool, &fixture).await?;
        let started = fixture.root.path().join("started");
        let release = fixture.root.path().join("release");
        let context = fixture.context(governor.clone(), "hold");
        let worker = pool.clone();
        let package = fixture.id().to_owned();
        let args = json!({"started":started,"release":release});
        let task =
            tokio::spawn(
                async move { worker.invoke_command(&package, "hold", args, context).await },
            );
        wait_file(&started).await?;
        assert!(
            timeout(BOUND, pool.unload(fixture.id(), CancellationToken::new()))
                .await?
                .is_err()
        );
        assert!(
            timeout(BOUND, pool.reload(fixture.id(), CancellationToken::new()))
                .await?
                .is_err()
        );
        assert!(!task.is_finished());
        std::fs::write(&release, "continue")?;
        let before = timeout(BOUND, task).await???;
        let after = status(&pool, &fixture, governor).await?;
        assert_eq!(before["pid"], after["pid"]);
        timeout(BOUND, pool.shutdown()).await??;
    }
    Ok(())
}

#[tokio::test]
async fn idle_reload_refreshes_transitive_imports_and_preserves_other_packages() -> Result<()> {
    for kind in LANGUAGES {
        let pool = pool()?;
        let first = Fixture::new(kind, "reload-first")?;
        let second = Fixture::new(kind, "reload-second")?;
        let governor = governor()?;
        load(&pool, &first).await?;
        load(&pool, &second).await?;
        let before = status(&pool, &first, governor.clone()).await?;
        assert_eq!(before["value"], "before");
        first.write_helper("after a transitive reload")?;
        timeout(BOUND, pool.reload(first.id(), CancellationToken::new())).await??;
        let after = status(&pool, &first, governor.clone()).await?;
        let retained = status(&pool, &second, governor.clone()).await?;
        assert_eq!(after["value"], "after a transitive reload");
        assert_ne!(before["pid"], after["pid"]);
        assert_eq!(after["pid"], retained["pid"]);
        assert_eq!(retained["value"], "before");
        timeout(BOUND, pool.unload(first.id(), CancellationToken::new())).await??;
        assert!(status(&pool, &first, governor.clone()).await.is_err());
        assert_eq!(status(&pool, &second, governor).await?["value"], "before");
        timeout(BOUND, pool.shutdown()).await??;
    }
    Ok(())
}

#[tokio::test]
async fn provider_adapter_streams_deltas_and_preserves_usage() -> Result<()> {
    for kind in LANGUAGES {
        let pool = pool()?;
        let fixture = Fixture::new(kind, "provider")?;
        load(&pool, &fixture).await?;
        let provider = pool.provider(fixture.id(), "model")?;
        let (sender, mut receiver) = mpsc::channel(8);
        let request = ModelRequest {
            system: "fixture".into(),
            messages: vec![],
            tools: vec![],
            max_output_tokens: 3,
        };
        let result = timeout(
            BOUND,
            provider.stream(request, CancellationToken::new(), sender),
        )
        .await??;
        assert_eq!(result.content, "hello world");
        assert_eq!(result.usage.input_tokens, 2);
        assert_eq!(result.usage.output_tokens, 3);
        let deltas = timeout(BOUND, async {
            let mut deltas = vec![];
            while let Some(ModelDelta::Text(text)) = receiver.recv().await {
                deltas.push(text);
            }
            deltas
        })
        .await?;
        assert_eq!(deltas, ["hello ", "world"]);
        timeout(BOUND, pool.shutdown()).await??;
    }
    Ok(())
}

#[tokio::test]
async fn dropping_invocation_retains_admission_and_blocks_reload_until_host_exit() -> Result<()> {
    for kind in LANGUAGES {
        let pool = pool()?;
        let fixture = Fixture::new(kind, "dropped-call")?;
        let governor = governor()?;
        load(&pool, &fixture).await?;
        let started = fixture.root.path().join("started");
        let cancelled = fixture.root.path().join("cancelled");
        let context = fixture.context(governor.clone(), "dropped");
        let worker = pool.clone();
        let package = fixture.id().to_owned();
        let args = json!({"started":started,"cancelled":cancelled});
        let task = tokio::spawn(async move {
            worker
                .invoke_command(&package, "stubborn", args, context)
                .await
        });
        let pids: Vec<u32> = wait_file(&started)
            .await?
            .split_whitespace()
            .map(str::parse)
            .collect::<std::result::Result<_, _>>()?;
        assert_eq!(pids.len(), 2);
        task.abort();
        let dropped = timeout(BOUND, task)
            .await?
            .expect_err("aborted invocation returned a result");
        assert!(dropped.is_cancelled());
        wait_file(&cancelled).await?;
        assert_eq!(governor.snapshot().running, 1);
        let error = timeout(BOUND, pool.reload(fixture.id(), CancellationToken::new()))
            .await?
            .expect_err("reload discarded a callback whose caller was dropped");
        assert!(error.to_string().contains("active callbacks"));
        assert!(pids.iter().all(|pid| live(*pid)));
        timeout(BOUND, governor.wait_idle()).await?;
        assert_eq!(governor.snapshot(), GovernorSnapshot::default());
        assert!(pids.iter().all(|pid| !live(*pid)));
        timeout(BOUND, pool.shutdown()).await??;
    }
    Ok(())
}

#[tokio::test]
async fn provider_adapter_cannot_cross_installation_revisions() -> Result<()> {
    for kind in LANGUAGES {
        let pool = pool()?;
        let fixture = Fixture::new(kind, "provider-revision")?;
        load(&pool, &fixture).await?;
        let old = pool.provider(fixture.id(), "model")?;
        timeout(BOUND, pool.unload(fixture.id(), CancellationToken::new())).await??;
        load(&pool, &fixture).await?;
        let request = ModelRequest {
            system: "fixture".into(),
            messages: vec![],
            tools: vec![],
            max_output_tokens: 3,
        };
        let (sender, mut receiver) = mpsc::channel(8);
        let error = timeout(
            BOUND,
            old.stream(request.clone(), CancellationToken::new(), sender),
        )
        .await?
        .expect_err("an old provider adapter invoked a new installation");
        assert!(error.to_string().contains("stale"));
        assert!(receiver.try_recv().is_err());
        let current = pool.provider(fixture.id(), "model")?;
        let (sender, mut receiver) = mpsc::channel(8);
        let response = timeout(
            BOUND,
            current.stream(request, CancellationToken::new(), sender),
        )
        .await??;
        assert_eq!(response.content, "hello world");
        let deltas = timeout(BOUND, async {
            let mut deltas = vec![];
            while let Some(ModelDelta::Text(text)) = receiver.recv().await {
                deltas.push(text);
            }
            deltas
        })
        .await?;
        assert_eq!(deltas, ["hello ", "world"]);
        timeout(BOUND, pool.shutdown()).await??;
    }
    Ok(())
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn coordinated_pool(
    config: RuntimeConfig,
    max_jobs: usize,
) -> Result<(TempDir, RuntimePool, ejm_harness::CapacityContext)> {
    let directory = tempfile::tempdir()?;
    let context =
        coordinated_capacity(&directory.path().canonicalize()?.join("capacity"), max_jobs)?;
    let pool = RuntimePool::with_capacity(config, context.clone())?;
    Ok((directory, pool, context))
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn coordinated_capacity(root: &Path, max_jobs: usize) -> Result<ejm_harness::CapacityContext> {
    let mut harness = ejm_harness::HarnessConfig::default();
    let executable = std::env::current_exe()?;
    let helper = executable
        .parent()
        .and_then(Path::parent)
        .context("resolve native fixture target directory")?
        .join("examples")
        .join(format!(
            "native_owner_helper{}",
            std::env::consts::EXE_SUFFIX
        ));
    anyhow::ensure!(
        helper.is_file(),
        "build native fixture first: cargo build -p ejm-harness-exec --example native_owner_helper"
    );
    ejm_harness::CapacityContext::prepare_isolated_with_native_helper(
        &mut harness,
        ejm_harness::CapacityPolicy {
            mode: ejm_harness::CapacityMode::Fixed,
            max_jobs: Some(max_jobs),
            ..ejm_harness::CapacityPolicy::default()
        },
        root,
        &helper,
    )
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[tokio::test]
async fn coordinated_hosts_keep_resident_memory_without_holding_execution_slots() -> Result<()> {
    let (_directory, pool, capacity) = coordinated_pool(RuntimeConfig::default(), 2)?;
    let governor = capacity.governor();
    let baseline = governor.snapshot().reserved_memory_bytes;
    assert_eq!(governor.operating_snapshot().memory_leases, 1);
    let first = Fixture::new(RuntimeKind::Node, "coordinated-node")?;
    let second = Fixture::new(RuntimeKind::Python, "coordinated-python")?;
    load(&pool, &first).await?;
    timeout(BOUND, governor.wait_jobs_idle()).await?;
    let resident = governor.snapshot().reserved_memory_bytes;
    assert_eq!(resident, baseline + 64 * 1024 * 1024);
    assert_eq!(governor.snapshot().running, 0);
    assert_eq!(governor.snapshot().reserved_processes, 1);
    assert_eq!(governor.operating_snapshot().memory_leases, 2);
    status(&pool, &first, governor.clone()).await?;
    timeout(BOUND, governor.wait_jobs_idle()).await?;
    assert_eq!(governor.snapshot().reserved_memory_bytes, resident);
    load(&pool, &second).await?;
    timeout(BOUND, governor.wait_jobs_idle()).await?;
    assert_eq!(
        governor.snapshot().reserved_memory_bytes,
        resident + 64 * 1024 * 1024
    );
    assert_eq!(governor.snapshot().reserved_processes, 2);
    let wrong = self::governor()?;
    assert!(status(&pool, &first, wrong).await.is_err());
    timeout(BOUND, pool.shutdown()).await??;
    assert_eq!(governor.snapshot().reserved_memory_bytes, baseline);
    assert_eq!(governor.snapshot().running, 0);
    assert_eq!(governor.snapshot().reserved_processes, 0);
    Ok(())
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[tokio::test]
async fn coordinated_launch_waits_before_starting_an_interpreter_and_cancels_cleanly() -> Result<()>
{
    let (_directory, pool, capacity) = coordinated_pool(
        RuntimeConfig {
            node: "missing-capacity-fixture-interpreter".into(),
            python: "missing-capacity-fixture-interpreter".into(),
            ..RuntimeConfig::default()
        },
        1,
    )?;
    let governor = capacity.governor();
    let baseline = governor.snapshot().reserved_memory_bytes;
    let busy = governor
        .acquire(
            "occupied",
            ejm_harness_exec::Reservation {
                memory_bytes: 16 * 1024 * 1024,
                processes: 0,
                ..ejm_harness_exec::Reservation::default()
            },
            &CancellationToken::new(),
        )
        .await?;
    let fixture = Fixture::new(RuntimeKind::Python, "waiting-launch")?;
    let cancel = CancellationToken::new();
    let loading = {
        let pool = pool.clone();
        let cancel = cancel.clone();
        tokio::spawn(async move { pool.load(fixture.installed, grants(), cancel).await })
    };
    timeout(BOUND, async {
        while governor.snapshot().queued == 0 && !loading.is_finished() {
            tokio::task::yield_now().await;
        }
    })
    .await?;
    assert!(
        !loading.is_finished(),
        "interpreter launch bypassed capacity admission"
    );
    cancel.cancel();
    assert!(timeout(BOUND, loading).await??.is_err());
    drop(busy);
    timeout(BOUND, governor.wait_jobs_idle()).await?;
    timeout(BOUND, pool.shutdown()).await??;
    assert_eq!(governor.snapshot().reserved_memory_bytes, baseline);
    Ok(())
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[tokio::test]
async fn coordinated_owner_death_fails_busy_callback_then_explicit_reload_progresses() -> Result<()>
{
    use rustix::process::{Signal, kill_process};

    let (_directory, pool, capacity) = coordinated_pool(RuntimeConfig::default(), 2)?;
    let governor = capacity.governor();
    let baseline = governor.snapshot().reserved_memory_bytes;
    let fixture = Fixture::new(RuntimeKind::Python, "owner-death")?;
    load(&pool, &fixture).await?;
    let started = fixture.root.path().join("started");
    let release = fixture.root.path().join("release");
    let task = {
        let pool = pool.clone();
        let package = fixture.id().to_owned();
        let args = json!({"started":started,"release":release});
        let context = fixture.context(governor.clone(), "owner-death");
        tokio::spawn(async move { pool.invoke_command(&package, "hold", args, context).await })
    };
    let original_host: u32 = wait_file(&started).await?.parse()?;
    let before = capacity.snapshot();
    let owner = before.native.owner_pid.context("authenticated owner PID")?;
    let owner = rustix::process::Pid::from_raw(i32::try_from(owner)?)
        .context("positive native owner PID")?;
    assert_eq!(before.operating.target.max_jobs, 2);
    assert_eq!(governor.snapshot().running, 1);
    kill_process(owner, Signal::KILL)?;

    let callback = timeout(BOUND, task).await;
    // A surviving fixture host may finish its already-started callback. Opening
    // its gate avoids leaving test work behind without claiming owner cleanup.
    std::fs::write(&release, "finish the original fixture callback")?;
    assert!(callback??.is_err(), "owner loss reported callback success");
    timeout(BOUND, async {
        loop {
            let native = capacity.native_owner().snapshot();
            let usage = governor.snapshot();
            if native.owner_deaths == 1
                && native.active_local_scopes == 0
                && usage.running == 0
                && usage.reserved_processes == 0
                && usage.reserved_memory_bytes == baseline
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .context("dead owner retained package capacity")?;
    let lost = capacity.native_owner().snapshot();
    assert_eq!(lost.last_dead_epoch, before.native.epoch);
    assert_eq!(lost.last_operation, before.native.last_operation);
    assert!(status(&pool, &fixture, governor.clone()).await.is_err());

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
    .context("fresh host sample did not restore admission")?;
    timeout(BOUND, pool.reload(fixture.id(), CancellationToken::new())).await??;
    let current = status(&pool, &fixture, governor.clone()).await?;
    assert_ne!(current["pid"].as_u64(), Some(u64::from(original_host)));
    assert_ne!(
        capacity.native_owner().snapshot().epoch,
        before.native.epoch
    );
    timeout(BOUND, pool.shutdown()).await??;
    assert_eq!(governor.snapshot().reserved_memory_bytes, baseline);
    Ok(())
}
