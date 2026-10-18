use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use anyhow::{Context, Result, ensure};
use async_trait::async_trait;
use ejm_harness::{
    Effect, Harness, HarnessConfig, ModelDelta, ModelRequest, ModelResponse, Provider, Role,
    RunHandle, RunStatus, SessionOptions, Tool, ToolCall, ToolContext, ToolDefinition, ToolOutput,
};
use ejm_harness_exec::{CommandRequest, Executor, Reservation};
use serde_json::json;
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

struct Workload {
    ready: watch::Receiver<bool>,
    rounds: usize,
}

#[async_trait]
impl Provider for Workload {
    async fn stream(
        &self,
        request: ModelRequest,
        cancel: CancellationToken,
        deltas: mpsc::Sender<ModelDelta>,
    ) -> Result<ModelResponse> {
        let mut ready = self.ready.clone();
        while !*ready.borrow_and_update() {
            tokio::select! {
                () = cancel.cancelled() => anyhow::bail!("cancelled"),
                result = ready.changed() => result?,
            }
        }
        let round = request
            .messages
            .iter()
            .filter(|message| message.role == Role::Tool)
            .count();
        let content = if round < self.rounds {
            format!("Checking step {}.\n", round + 1)
        } else {
            "Verified all local work.\n".into()
        };
        for word in content.split_inclusive(' ') {
            tokio::select! {
                () = cancel.cancelled() => anyhow::bail!("cancelled"),
                result = deltas.send(ModelDelta::Text(word.into())) => result?,
            }
            tokio::task::yield_now().await;
        }
        let calls = if round < self.rounds {
            vec![ToolCall {
                id: uuid::Uuid::new_v4().to_string(),
                name: "local_check".into(),
                arguments: json!({"round":round}),
            }]
        } else {
            vec![]
        };
        Ok(ModelResponse {
            content,
            tool_calls: calls,
            ..Default::default()
        })
    }
}

struct LocalCheck {
    artifacts: std::path::PathBuf,
    completed: Arc<AtomicUsize>,
}

#[async_trait]
impl Tool for LocalCheck {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "local_check".into(),
            description: "Fixed read-only command fixture, without model-selected command text."
                .into(),
            input_schema: json!({"type":"object","properties":{"round":{"type":"integer","minimum":0}},"required":["round"],"additionalProperties":false}),
            effect: Effect::Read,
            version: "1".into(),
        }
    }

    fn reservation(&self) -> Option<Reservation> {
        None
    }

    async fn execute(&self, _call: ToolCall, context: ToolContext) -> Result<ToolOutput> {
        #[cfg(unix)]
        let (program, args) = (
            "/bin/bash".into(),
            vec![
                "--noprofile".into(),
                "--norc".into(),
                "-c".into(),
                "printf 'fixture verified\\n'; sleep 0.01".into(),
            ],
        );
        #[cfg(windows)]
        let (program, args) = (
            "cmd.exe".into(),
            vec!["/D".into(), "/C".into(), "echo fixture verified".into()],
        );
        let executor = Executor::new(context.governor, self.artifacts.clone())?;
        let output = executor
            .run(
                CommandRequest {
                    session_id: context.session_id,
                    cwd: context.cwd,
                    program,
                    args,
                    timeout_ms: 5000,
                    ..Default::default()
                },
                context.cancel,
            )
            .await?;
        ensure!(output.exit_code == Some(0), "fixture command failed");
        self.completed.fetch_add(1, Ordering::Relaxed);
        Ok(ToolOutput::text(output.stdout))
    }
}

fn argument(args: &[String], key: &str, default: usize) -> Result<usize> {
    args.windows(2)
        .find(|pair| pair[0] == key)
        .map_or(Ok(default), |pair| {
            pair[1].parse().with_context(|| format!("invalid {key}"))
        })
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|value| value == "--help") {
        println!(
            "ejm-harness-bench [--agents 100] [--rounds 3] [--park-ms 1000] [--parked]\nDeterministic hosted-model replay and real bounded command workload. No paid API calls."
        );
        return Ok(());
    }
    let agents = argument(&args, "--agents", 100)?;
    let rounds = argument(&args, "--rounds", 3)?;
    let park_ms = argument(&args, "--park-ms", 1000)?;
    ensure!(
        (1..=1000).contains(&agents) && rounds <= 8 && park_ms <= 60_000,
        "workload exceeds benchmark envelope"
    );
    let parked = args.iter().any(|value| value == "--parked");
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .max_blocking_threads(4)
        .enable_all()
        .build()?
        .block_on(workload(agents, rounds, park_ms, parked))
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

async fn workload(agents: usize, rounds: usize, park_ms: usize, parked: bool) -> Result<()> {
    let root = tempfile::tempdir()?;
    let (ready, gate) = watch::channel(false);
    let completed = Arc::new(AtomicUsize::new(0));
    let config = HarnessConfig {
        state_dir: root.path().join("state"),
        max_sessions: agents,
        ..Default::default()
    };
    let limits = config.resources.clone();
    let harness = Harness::new(
        config,
        Arc::new(Workload {
            ready: gate,
            rounds,
        }),
        vec![Arc::new(LocalCheck {
            artifacts: root.path().join("artifacts"),
            completed: completed.clone(),
        })],
    )?;
    let started = Instant::now();
    let (mut handles, ids) = start_agents(&harness, root.path(), agents).await?;
    let admitted_ms = started.elapsed().as_millis();
    let snapshot = harness.status().await;
    ensure!(
        snapshot.active_agents == agents,
        "not all agents were simultaneously managed"
    );
    eprintln!("PARKED {agents} agents");
    tokio::time::sleep(Duration::from_millis(u64::try_from(park_ms)?)).await;
    let sampling_stop = CancellationToken::new();
    let sampler = tokio::spawn(sample_jobs(harness.clone(), sampling_stop.clone()));
    let work_started = Instant::now();
    if parked {
        for handle in &handles {
            handle.cancel();
        }
    } else {
        ready.send(true)?;
    }
    tokio::time::timeout(Duration::from_secs(60), async {
        for handle in &mut handles {
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
    harness.shutdown().await?;
    let work_ms = work_started.elapsed().as_millis();
    sampling_stop.cancel();
    let peak_jobs = sampler.await?;
    ensure!(
        peak_jobs <= limits.max_jobs,
        "sampled executing jobs exceeded governor limit"
    );
    let expected = if parked { 0 } else { agents * rounds };
    ensure!(
        completed.load(Ordering::Relaxed) == expected,
        "fixture calls missing"
    );
    let durable_events = replay_events(&harness, &ids).await?;
    let output = json!({"agents":agents,"simultaneously_managed":snapshot.active_agents,"parked_model_requests":snapshot.model_requests,"parked_waiting_models":snapshot.waiting_model_requests,"rounds":rounds,"mode":if parked {"parked"} else {"mixed"},"admission_ms":admitted_ms,"work_ms":work_ms,"commands_completed":expected,"sampled_peak_executing_jobs":peak_jobs,"sampling_interval_ms":2,"durable_events":durable_events,"resources":limits,"final":harness.status().await,"remote_api_calls":0});
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
