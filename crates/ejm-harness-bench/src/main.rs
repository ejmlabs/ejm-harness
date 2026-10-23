use std::{
    future::Future,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context as TaskContext, Poll, Waker},
    time::{Duration, Instant},
};

use anyhow::{Context, Result, ensure};
use ejm_harness::{
    CapacityContext, CapacityMode, CapacityPolicy, Harness, HarnessConfig, Provider, RunHandle,
    RunStatus, SessionOptions, SessionQuery, SessionTailOptions,
};
use ejm_harness_bench::{LocalCheck, Workload};
use ejm_harness_exec::{AdmissionPolicy, Governor, Reservation, ResourceLimits};
use serde_json::json;
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

struct WorkloadOptions {
    agents: usize,
    rounds: usize,
    park_ms: usize,
    parked: bool,
    deltas: usize,
    model_requests: usize,
    jobs: usize,
    capacity: String,
    capacity_root: Option<PathBuf>,
    state_dir: Option<PathBuf>,
    admission_only: bool,
}

fn argument(args: &[String], key: &str, default: usize) -> Result<usize> {
    args.windows(2)
        .find(|pair| pair[0] == key)
        .map_or(Ok(default), |pair| {
            pair[1].parse().with_context(|| format!("invalid {key}"))
        })
}

fn string_argument(args: &[String], key: &str) -> Option<String> {
    args.windows(2)
        .find(|pair| pair[0] == key)
        .map(|pair| pair[1].clone())
}

fn main() -> Result<std::process::ExitCode> {
    if let Some(result) = ejm_harness_exec::native::dispatch_helper() {
        return result;
    }
    run_main()?;
    Ok(std::process::ExitCode::SUCCESS)
}

fn run_main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|value| value == "--help") {
        println!(
            "ejm-harness-bench [--agents 100] [--rounds 3] [--park-ms 1000] [--parked] [--deltas 0] [--model-requests 8] [--jobs 4] [--capacity legacy|fixed|auto] [--capacity-root /absolute/fixture/path] [--state-dir /absolute/new/fixture/state] [--admission-only]\nDeterministic model replay and bounded command workload. Envelope: 1..4096 managed runs. Unsupported admission is reported as status=rejected. Explicit capacity modes require an isolated shared fixture root. --state-dir retains completed sessions for a later host fixture and requires a new directory. No paid API calls."
        );
        return Ok(());
    }
    let agents = argument(&args, "--agents", 100)?;
    let rounds = argument(&args, "--rounds", 3)?;
    let park_ms = argument(&args, "--park-ms", 1000)?;
    let deltas = argument(&args, "--deltas", 0)?;
    let model_requests = argument(&args, "--model-requests", 8)?;
    let jobs = argument(&args, "--jobs", 4)?;
    ensure!(
        (1..=4096).contains(&agents)
            && rounds <= 8
            && park_ms <= 60_000
            && deltas <= 128
            && (1..=128).contains(&model_requests)
            && (1..=32).contains(&jobs),
        "workload exceeds benchmark envelope"
    );
    let parked = args.iter().any(|value| value == "--parked");
    let capacity = string_argument(&args, "--capacity").unwrap_or_else(|| "legacy".into());
    ensure!(
        ["legacy", "fixed", "auto"].contains(&capacity.as_str()),
        "unknown capacity mode"
    );
    let capacity_root = string_argument(&args, "--capacity-root").map(PathBuf::from);
    let state_dir = string_argument(&args, "--state-dir").map(PathBuf::from);
    if let Some(path) = &state_dir {
        ensure!(
            path.is_absolute() && !path.try_exists()?,
            "--state-dir requires an absolute new fixture directory"
        );
    }
    let admission_only = args.iter().any(|value| value == "--admission-only");
    if capacity != "legacy" && !admission_only {
        ensure!(
            capacity_root
                .as_ref()
                .is_some_and(|root| root.is_absolute()),
            "explicit capacity benchmarks need --capacity-root /absolute/fixture/path"
        );
    }
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .max_blocking_threads(4)
        .enable_all()
        .build()?
        .block_on(workload(WorkloadOptions {
            agents,
            rounds,
            park_ms,
            parked,
            deltas,
            model_requests,
            jobs,
            capacity,
            capacity_root,
            state_dir,
            admission_only,
        }))
}

async fn start_agents(
    harness: &Harness,
    cwd: &std::path::Path,
    agents: usize,
) -> Result<(Vec<RunHandle>, Vec<String>)> {
    let mut handles = Vec::with_capacity(agents);
    let mut ids = Vec::with_capacity(agents);
    for index in 0..agents {
        let session = harness
            .create_session(SessionOptions {
                cwd: cwd.into(),
                title: format!("agent-{index}"),
            })
            .await?;
        handles.push(
            harness
                .start(
                    &session.id,
                    format!("Complete fixture work for agent {index}."),
                )
                .await?,
        );
        ids.push(session.id);
    }
    Ok((handles, ids))
}

async fn replay_events(harness: &Harness, ids: &[String]) -> Result<usize> {
    let mut count = 0;
    for id in ids {
        let mut after = 0;
        loop {
            let page = harness.events(id, after, 1000).await?;
            if page.is_empty() {
                break;
            }
            after = page.last().context("missing page cursor")?.sequence;
            count += page.len();
        }
    }
    Ok(count)
}

async fn sample_jobs(harness: Harness, stop: CancellationToken) -> usize {
    let mut peak = 0;
    let mut interval = tokio::time::interval(Duration::from_millis(2));
    loop {
        tokio::select! {
            () = stop.cancelled() => return peak,
            _ = interval.tick() => peak = peak.max(harness.governor().snapshot().running),
        }
    }
}

fn build_harness(
    options: &WorkloadOptions,
    mut config: HarnessConfig,
    provider: Arc<dyn Provider>,
    local_check: LocalCheck,
) -> Result<Harness> {
    config.resources.max_jobs = options.jobs;
    config.resources.max_heavy_jobs = config.resources.max_heavy_jobs.min(options.jobs);
    if options.capacity == "legacy" {
        return Harness::new(config, provider, vec![Arc::new(local_check)]);
    }
    let resources = config.resources.clone();
    let capacity = CapacityContext::prepare_isolated(
        &mut config,
        CapacityPolicy {
            mode: if options.capacity == "auto" {
                CapacityMode::Auto
            } else {
                CapacityMode::Fixed
            },
            max_sessions: Some(options.agents),
            max_model_requests: Some(options.model_requests),
            max_jobs: Some(options.jobs),
            max_memory_bytes: Some(resources.memory_bytes),
            max_cpu_millis: Some(resources.cpu_millis),
            max_heavy_jobs: Some(resources.max_heavy_jobs),
            max_processes: Some(resources.max_processes),
            max_output_bytes: Some(resources.max_output_bytes),
        },
        options
            .capacity_root
            .as_deref()
            .context("missing isolated capacity root")?,
    )?;
    let tool = local_check.with_native_owner(capacity.native_owner());
    Harness::with_capacity(
        config,
        Some(provider),
        vec![Arc::new(tool)],
        vec![],
        capacity,
    )
}

fn timing_summary(samples: &mut [u128]) -> serde_json::Value {
    samples.sort_unstable();
    json!({"samples":samples.len(),"p50_ns":samples[(samples.len()*50).div_ceil(100)-1],"p95_ns":samples[(samples.len()*95).div_ceil(100)-1],"max_ns":samples.last(),"total_ns":samples.iter().sum::<u128>()})
}

fn process_totals() -> serde_json::Value {
    let Ok(pid) = sysinfo::get_current_pid() else {
        return json!({"available":false});
    };
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::nothing()
            .with_cpu()
            .with_memory()
            .with_disk_usage()
            .without_tasks(),
    );
    system.process(pid).map_or_else(|| json!({"available":false}), |process| {
        let io = process.disk_usage();
        json!({"available":true,"source":"sysinfo 0.39.6 own process","cpu_ms":process.accumulated_cpu_time(),"rss_bytes":process.memory(),"reported_read_bytes":io.total_read_bytes,"reported_written_bytes":io.total_written_bytes,"child_cpu_included":false,"cached_io_may_not_appear":true})
    })
}

async fn admission_workload(options: &WorkloadOptions) -> Result<()> {
    let governor = Arc::new(Governor::new_with_policy(
        ResourceLimits {
            max_jobs: 1,
            max_queue: options.agents,
            ..Default::default()
        },
        if options.capacity == "auto" {
            AdmissionPolicy::Automatic
        } else {
            AdmissionPolicy::Fixed
        },
    )?);
    let cancel = CancellationToken::new();
    let held = governor
        .acquire("held", Reservation::default(), &cancel)
        .await?;
    let identities: Vec<_> = (0..options.agents)
        .map(|index| format!("queued-{index}"))
        .collect();
    let mut waiters = Vec::with_capacity(options.agents);
    let mut enqueue_ns = Vec::with_capacity(options.agents);
    let mut remove_ns = Vec::with_capacity(options.agents);
    let mut context = TaskContext::from_waker(Waker::noop());
    for identity in &identities {
        let started = Instant::now();
        let mut future = Box::pin(governor.acquire(identity, Reservation::default(), &cancel));
        ensure!(
            matches!(future.as_mut().poll(&mut context), Poll::Pending),
            "held slot failed to queue reservation"
        );
        enqueue_ns.push(started.elapsed().as_nanos());
        waiters.push(future);
    }
    ensure!(
        governor.snapshot().queued == options.agents,
        "queue count differs from accepted waiters"
    );
    // Reverse removal exercises the furthest live ticket on each cancellation.
    while let Some(waiter) = waiters.pop() {
        let started = Instant::now();
        drop(waiter);
        remove_ns.push(started.elapsed().as_nanos());
    }
    ensure!(
        governor.snapshot().queued == 0,
        "dropped futures retained queue records"
    );
    drop(held);
    println!(
        "{}",
        serde_json::to_string(
            &json!({"status":"completed","mode":"admission","waiters":options.agents,"capacity_mode":options.capacity,"scope":"isolated local governor; no host grants","enqueue":timing_summary(&mut enqueue_ns),"reverse_drop_cancel":timing_summary(&mut remove_ns),"final":governor.snapshot(),"process_totals":process_totals(),"remote_api_calls":0})
        )?
    );
    Ok(())
}

async fn finish_agents(
    handles: &mut [RunHandle],
    parked: bool,
    ready: &watch::Sender<bool>,
) -> Result<()> {
    if parked {
        for handle in &*handles {
            handle.cancel();
        }
    } else {
        ready.send(true)?;
    }
    tokio::time::timeout(Duration::from_secs(180), async {
        for handle in handles {
            let result = handle.wait().await?;
            ensure!(
                result.status
                    == if parked {
                        RunStatus::Cancelled
                    } else {
                        RunStatus::Completed
                    },
                "agent failed: {:?}",
                result.error
            );
        }
        anyhow::Ok(())
    })
    .await??;
    Ok(())
}

async fn workload(options: WorkloadOptions) -> Result<()> {
    if options.admission_only {
        return admission_workload(&options).await;
    }
    let agents = options.agents;
    let rounds = options.rounds;
    let park_ms = options.park_ms;
    let parked = options.parked;
    let deltas = options.deltas;
    let root = tempfile::tempdir()?;
    let (ready, gate) = watch::channel(false);
    let completed = Arc::new(AtomicUsize::new(0));
    let config = HarnessConfig {
        state_dir: options
            .state_dir
            .clone()
            .unwrap_or_else(|| root.path().join("state")),
        max_sessions: agents,
        max_model_requests: options.model_requests,
        ..Default::default()
    };
    let constructed = build_harness(
        &options,
        config,
        Arc::new(Workload::new(gate, rounds, deltas)?),
        LocalCheck::new(root.path().join("artifacts"), completed.clone()),
    );
    let harness = match constructed {
        Ok(harness) => harness,
        Err(error) => {
            println!(
                "{}",
                serde_json::to_string(
                    &json!({"status":"rejected","requested_agents":agents,"participants":1,"capacity_mode":options.capacity,"error":format!("{error:#}"),"remote_api_calls":0})
                )?
            );
            return Ok(());
        }
    };
    let limits = harness.governor().limits().clone();
    let started = Instant::now();
    let workspace = options
        .state_dir
        .as_deref()
        .and_then(std::path::Path::parent)
        .unwrap_or_else(|| root.path());
    let (mut handles, ids) = start_agents(&harness, workspace, agents).await?;
    let admitted_ms = started.elapsed().as_millis();
    let snapshot = harness.status().await;
    let parked_journal = harness.journal_status();
    let process_at_park = process_totals();
    ensure!(
        snapshot.active_agents == agents,
        "not all agents were simultaneously managed"
    );
    eprintln!("PARKED {agents} agents");
    tokio::time::sleep(Duration::from_millis(u64::try_from(park_ms)?)).await;
    let sampling_stop = CancellationToken::new();
    let sampler = tokio::spawn(sample_jobs(harness.clone(), sampling_stop.clone()));
    let work_started = Instant::now();
    finish_agents(&mut handles, parked, &ready).await?;
    harness.shutdown().await?;
    let work_ms = work_started.elapsed().as_millis();
    sampling_stop.cancel();
    let peak_jobs = sampler.await?;
    let journal_after_work = harness.journal_status();
    ensure!(
        peak_jobs <= limits.max_jobs,
        "sampled executing jobs exceeded governor limit"
    );
    let expected = if parked { 0 } else { agents * rounds };
    let actual = completed.load(Ordering::Relaxed);
    ensure!(
        actual == expected,
        "fixture calls missing: {actual} successful commands, expected {expected}"
    );
    let durable_events = replay_events(&harness, &ids).await?;
    let view_started = Instant::now();
    let page = harness
        .sessions_page(SessionQuery {
            limit: 50,
            ..Default::default()
        })
        .await?;
    let view = harness
        .session_view(
            ids.first().context("no selected session")?,
            SessionTailOptions::default(),
        )
        .await?;
    ensure!(
        page.sessions.len() <= 50 && view.events.len() <= 200,
        "host view exceeded count budget"
    );
    let bounded_view_us = view_started.elapsed().as_micros();
    let output = json!({"status":"completed","agents":agents,"participants":1,"capacity_mode":options.capacity,"scope":if options.capacity == "legacy" {"legacy uncoordinated"} else {"isolated shared fixture registry"},"simultaneously_managed":snapshot.active_agents,"parked_model_requests":snapshot.model_requests,"parked_waiting_models":snapshot.waiting_model_requests,"rounds":rounds,"deltas_per_response":deltas,"mode":if parked {"parked"} else {"mixed"},"admission_ms":admitted_ms,"work_ms":work_ms,"commands_completed":expected,"sampled_peak_executing_jobs":peak_jobs,"sampling_interval_ms":2,"durable_events":durable_events,"resources":limits,"capacity":harness.capacity_snapshot(),"final":harness.status().await,"journal_at_park":parked_journal,"journal_after_work":journal_after_work,"process_at_park":process_at_park,"process_final":process_totals(),"bounded_page_and_tail_us":bounded_view_us,"selected_tail_events":view.events.len(),"selected_tail_omitted":view.omitted,"remote_api_calls":0,"retained_state_dir":options.state_dir,"first_session_id":ids.first()});
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
