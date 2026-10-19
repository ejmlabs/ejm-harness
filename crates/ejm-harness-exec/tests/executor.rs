#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "Test fixtures and assertions must fail at their call site."
)]

use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use ejm_harness_exec::{
    CommandRequest, Executor, Governor, GovernorSnapshot, RequiredCapabilities, Reservation,
    ResourceLimits, RestrictionState,
};
use sysinfo::{Pid, ProcessRefreshKind, ProcessStatus, ProcessesToUpdate, System};
use tempfile::TempDir;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

fn executor(limits: ResourceLimits) -> (TempDir, Arc<Governor>, Executor) {
    let directory = tempfile::tempdir().unwrap();
    let governor = Arc::new(Governor::new(limits).unwrap());
    let executor = Executor::new(governor.clone(), directory.path().join("artifacts")).unwrap();
    (directory, governor, executor)
}

#[tokio::test]
async fn private_input_closes_at_eof_while_both_output_pipes_drain() {
    let (directory, governor, executor) = executor(ResourceLimits::default());
    let input = b"fixture-private-input".repeat(32 * 1024);
    let expected = input.len();
    let output = timeout(
        Duration::from_secs(5),
        executor.run_with_input(
            fixture_request("private-input", directory.path()),
            input,
            vec!["PATH".into()],
            CancellationToken::new(),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(output.exit_code, Some(0));
    assert!(output.stdout.contains(&format!("INPUT_LEN={expected}")));
    assert!(output.stdout.contains("PATH_REMOVED=true"));
    assert!(output.stderr.contains("STDERR_END"));
    let artifact = std::fs::read(output.artifact.unwrap()).unwrap();
    assert!(
        !artifact
            .windows(21)
            .any(|bytes| bytes == b"fixture-private-input")
    );
    assert_eq!(governor.snapshot(), GovernorSnapshot::default());
}

#[tokio::test]
async fn oversized_private_input_is_rejected_before_launch() {
    let (directory, governor, executor) = executor(ResourceLimits::default());
    let path = directory.path().join("started");
    let mut request = fixture_request("ready", directory.path());
    request.env.insert(
        "EJM_HARNESS_EXEC_PID_FILE".into(),
        path.to_string_lossy().into_owned(),
    );
    assert!(
        executor
            .run_with_input(
                request,
                vec![0; 8 * 1024 * 1024 + 1],
                Vec::new(),
                CancellationToken::new()
            )
            .await
            .is_err()
    );
    assert!(!path.exists());
    assert_eq!(governor.snapshot(), GovernorSnapshot::default());
}

#[tokio::test]
async fn cancellation_reaps_a_command_that_never_reads_private_input() {
    let (directory, governor, executor) = executor(ResourceLimits::default());
    let path = directory.path().join("pids");
    let mut request = fixture_request("blocked-input", directory.path());
    request.env.insert(
        "EJM_HARNESS_EXEC_PID_FILE".into(),
        path.to_string_lossy().into_owned(),
    );
    let cancel = CancellationToken::new();
    let child_cancel = cancel.clone();
    let task = tokio::spawn(async move {
        executor
            .run_with_input(request, vec![0; 8 * 1024 * 1024], Vec::new(), child_cancel)
            .await
    });
    let pids = parse_pids(&wait_for_file(&path).await);
    cancel.cancel();
    let output = timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(output.cancelled);
    wait_for_cleanup(&governor, &pids).await;
}

fn fixture_request(mode: &str, directory: &Path) -> CommandRequest {
    let mut request = CommandRequest {
        program: std::env::current_exe()
            .unwrap()
            .to_string_lossy()
            .into_owned(),
        args: vec![
            "--exact".into(),
            "fixture".into(),
            "--ignored".into(),
            "--nocapture".into(),
        ],
        cwd: directory.to_path_buf(),
        timeout_ms: 5_000,
        ..CommandRequest::default()
    };
    request
        .env
        .insert("EJM_HARNESS_EXEC_FIXTURE".into(), mode.into());
    request
}

fn live(pid: u32) -> bool {
    let mut system = System::new();
    let ids = [Pid::from_u32(pid)];
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&ids),
        true,
        ProcessRefreshKind::nothing(),
    );
    system
        .process(ids[0])
        .is_some_and(|process| process.status() != ProcessStatus::Zombie)
}

async fn wait_for_file(path: &Path) -> String {
    timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(text) = tokio::fs::read_to_string(path).await
                && !text.trim().is_empty()
            {
                return text;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("fixture became ready")
}

async fn wait_for_cleanup(governor: &Governor, pids: &[u32]) {
    timeout(Duration::from_secs(5), async {
        loop {
            if governor.snapshot() == GovernorSnapshot::default()
                && pids.iter().all(|pid| !live(*pid))
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("all command resources were cleaned up");
}

#[tokio::test]
async fn drains_both_noisy_pipes_and_bounds_retained_output() {
    let (directory, governor, executor) = executor(ResourceLimits::default());
    let mut request = fixture_request("noisy", directory.path());
    request.reservation.output_bytes = 1024;
    let output = executor
        .run(request, CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(output.exit_code, Some(0));
    assert!(!output.cancelled && !output.timed_out);
    assert!(output.truncated);
    assert!(output.stdout.len() <= 512);
    assert!(output.stderr.len() <= 512);
    assert!(output.stdout.contains("STDOUT_END"));
    assert!(output.stderr.contains("STDERR_END"));
    let artifact = std::fs::read(output.artifact.unwrap()).unwrap();
    assert!(artifact.len() > 1024 * 1024);
    assert_eq!(governor.snapshot(), GovernorSnapshot::default());
}

#[tokio::test]
async fn invalid_utf8_cannot_expand_public_output_past_reservation() {
    let (directory, _, executor) = executor(ResourceLimits::default());
    let mut request = fixture_request("binary", directory.path());
    request.reservation.output_bytes = 101;
    let output = executor
        .run(request, CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(output.exit_code, Some(0));
    assert!(output.truncated);
    assert!(output.stdout.len() + output.stderr.len() <= 101);
}

#[tokio::test]
async fn artifact_quota_is_shared_across_executors_and_jobs() {
    let (directory, governor, first) = executor(ResourceLimits {
        max_artifact_bytes: 12 * 1024,
        ..ResourceLimits::default()
    });
    let second = Executor::new(governor.clone(), directory.path().join("more-artifacts")).unwrap();
    let (left, right) = tokio::join!(
        first.run(
            fixture_request("noisy", directory.path()),
            CancellationToken::new()
        ),
        second.run(
            fixture_request("noisy", directory.path()),
            CancellationToken::new()
        ),
    );
    assert!(left.unwrap().truncated);
    assert!(right.unwrap().truncated);
    let mut accounted = 0;
    for root in ["artifacts", "more-artifacts"] {
        for entry in std::fs::read_dir(directory.path().join(root)).unwrap() {
            accounted += entry.unwrap().metadata().unwrap().len() + 4096;
        }
    }
    assert!(accounted <= 12 * 1024, "accounted bytes {accounted}");
    assert_eq!(governor.snapshot(), GovernorSnapshot::default());
}

#[tokio::test]
async fn existing_artifacts_are_counted_once_when_an_executor_is_reopened() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("existing.log"), vec![0_u8; 2048]).unwrap();
    let governor = Arc::new(
        Governor::new(ResourceLimits {
            max_artifact_bytes: 8192,
            ..ResourceLimits::default()
        })
        .unwrap(),
    );
    let _first = Executor::new(governor.clone(), directory.path()).unwrap();
    let _second = Executor::new(governor, directory.path()).unwrap();
    let too_small = Arc::new(
        Governor::new(ResourceLimits {
            max_artifact_bytes: 6143,
            ..ResourceLimits::default()
        })
        .unwrap(),
    );
    assert!(Executor::new(too_small, directory.path()).is_err());
}

#[tokio::test]
async fn required_isolation_or_hard_resource_limits_fail_before_launch() {
    let (directory, governor, executor) = executor(ResourceLimits::default());
    assert_eq!(
        executor.capabilities().filesystem_isolation,
        RestrictionState::Unsupported
    );
    assert_eq!(
        executor.capabilities().memory_limit,
        RestrictionState::Monitored
    );
    for required in [
        RequiredCapabilities {
            filesystem_isolation: true,
            ..RequiredCapabilities::default()
        },
        RequiredCapabilities {
            memory_limit: true,
            ..RequiredCapabilities::default()
        },
        RequiredCapabilities {
            process_tree_cancellation: true,
            ..RequiredCapabilities::default()
        },
    ] {
        let path = directory.path().join("must-not-exist");
        let mut request = fixture_request("ready", directory.path());
        request.env.insert(
            "EJM_HARNESS_EXEC_PID_FILE".into(),
            path.to_string_lossy().into_owned(),
        );
        request.required = required;
        let error = executor
            .run(request, CancellationToken::new())
            .await
            .unwrap_err();
        assert!(error.to_string().contains("required restriction"));
        assert!(!path.exists());
    }
    assert_eq!(governor.snapshot(), GovernorSnapshot::default());
}

#[tokio::test]
async fn spawn_errors_release_all_reservations() {
    let (directory, governor, executor) = executor(ResourceLimits::default());
    let mut request = fixture_request("ready", directory.path());
    request.program = directory
        .path()
        .join("nonexistent-program")
        .to_string_lossy()
        .into_owned();
    assert!(
        executor
            .run(request, CancellationToken::new())
            .await
            .is_err()
    );
    assert_eq!(governor.snapshot(), GovernorSnapshot::default());
}

#[tokio::test]
async fn cancelling_a_queued_command_never_launches_it() {
    let (directory, governor, executor) = executor(ResourceLimits {
        max_jobs: 1,
        ..ResourceLimits::default()
    });
    let held = governor
        .acquire("held", Reservation::default(), &CancellationToken::new())
        .await
        .unwrap();
    let path = directory.path().join("must-not-exist");
    let mut request = fixture_request("ready", directory.path());
    request.env.insert(
        "EJM_HARNESS_EXEC_PID_FILE".into(),
        path.to_string_lossy().into_owned(),
    );
    let cancel = CancellationToken::new();
    let worker_cancel = cancel.clone();
    let pending = tokio::spawn(async move { executor.run(request, worker_cancel).await });
    timeout(Duration::from_secs(5), async {
        while governor.snapshot().queued != 1 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    cancel.cancel();
    let result = pending.await.unwrap().unwrap();
    assert!(result.cancelled);
    assert!(!path.exists());
    drop(held);
    assert_eq!(governor.snapshot(), GovernorSnapshot::default());
}

#[tokio::test]
async fn queued_commands_obey_the_same_wall_deadline() {
    let (directory, governor, executor) = executor(ResourceLimits {
        max_jobs: 1,
        ..ResourceLimits::default()
    });
    let held = governor
        .acquire("held", Reservation::default(), &CancellationToken::new())
        .await
        .unwrap();
    let mut request = fixture_request("ready", directory.path());
    request.timeout_ms = 30;
    let result = executor
        .run(request, CancellationToken::new())
        .await
        .unwrap();
    assert!(result.timed_out);
    assert!(result.exit_code.is_none());
    assert_eq!(governor.snapshot().queued, 0);
    drop(held);
    assert_eq!(governor.snapshot(), GovernorSnapshot::default());
}

#[tokio::test]
async fn cancellation_reaps_command_and_ordinary_descendants() {
    let (directory, governor, executor) = executor(ResourceLimits::default());
    let path = directory.path().join("pids");
    let mut request = fixture_request("descendant", directory.path());
    request.env.insert(
        "EJM_HARNESS_EXEC_PID_FILE".into(),
        path.to_string_lossy().into_owned(),
    );
    let cancel = CancellationToken::new();
    let worker_cancel = cancel.clone();
    let running = tokio::spawn(async move { executor.run(request, worker_cancel).await });
    let pids = parse_pids(&wait_for_file(&path).await);
    assert_eq!(pids.len(), 2);
    assert!(pids.iter().all(|pid| live(*pid)));
    cancel.cancel();
    let result = timeout(Duration::from_secs(5), running)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(result.cancelled);
    wait_for_cleanup(&governor, &pids).await;
}

struct CleanupProbe {
    pids: Arc<std::sync::Mutex<Vec<u32>>>,
    dropped: Option<tokio::sync::oneshot::Sender<bool>>,
}

impl Drop for CleanupProbe {
    fn drop(&mut self) {
        let cleaned = self.pids.lock().unwrap().iter().all(|pid| !live(*pid));
        if let Some(sender) = self.dropped.take() {
            let _ = sender.send(cleaned);
        }
    }
}

#[tokio::test]
async fn dropping_the_run_future_retains_host_ownership_until_child_cleanup() {
    let (directory, governor, executor) = executor(ResourceLimits::default());
    let path = directory.path().join("pids");
    let mut request = fixture_request("descendant", directory.path());
    request.env.insert(
        "EJM_HARNESS_EXEC_PID_FILE".into(),
        path.to_string_lossy().into_owned(),
    );
    let tracked = Arc::new(std::sync::Mutex::new(Vec::new()));
    let (sender, mut dropped) = tokio::sync::oneshot::channel();
    let retained = CleanupProbe {
        pids: tracked.clone(),
        dropped: Some(sender),
    };
    let running = tokio::spawn(async move {
        executor
            .run_with_retained(request, CancellationToken::new(), retained)
            .await
    });
    let pids = parse_pids(&wait_for_file(&path).await);
    tracked.lock().unwrap().clone_from(&pids);
    assert!(matches!(
        dropped.try_recv(),
        Err(tokio::sync::oneshot::error::TryRecvError::Empty)
    ));
    running.abort();
    assert!(running.await.is_err());
    assert!(
        timeout(Duration::from_secs(5), dropped)
            .await
            .unwrap()
            .unwrap()
    );
    wait_for_cleanup(&governor, &pids).await;
}

#[tokio::test]
async fn normal_parent_exit_cleans_children_that_keep_pipes_open() {
    let (directory, governor, executor) = executor(ResourceLimits::default());
    let path = directory.path().join("pids");
    let mut request = fixture_request("orphan", directory.path());
    request.env.insert(
        "EJM_HARNESS_EXEC_PID_FILE".into(),
        path.to_string_lossy().into_owned(),
    );
    let result = timeout(
        Duration::from_secs(5),
        executor.run(request, CancellationToken::new()),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(result.exit_code, Some(0));
    assert!(!result.timed_out && !result.cancelled);
    let pids = parse_pids(&std::fs::read_to_string(path).unwrap());
    wait_for_cleanup(&governor, &pids).await;
}

#[tokio::test]
async fn deadline_terminates_a_command_tree() {
    let (directory, governor, executor) = executor(ResourceLimits::default());
    let path = directory.path().join("pids");
    let mut request = fixture_request("descendant", directory.path());
    request.timeout_ms = 200;
    request.env.insert(
        "EJM_HARNESS_EXEC_PID_FILE".into(),
        path.to_string_lossy().into_owned(),
    );
    let result = executor
        .run(request, CancellationToken::new())
        .await
        .unwrap();
    assert!(result.timed_out);
    let pids = parse_pids(&std::fs::read_to_string(path).unwrap());
    wait_for_cleanup(&governor, &pids).await;
}

#[tokio::test]
async fn observed_process_budget_abuse_is_stopped_and_released() {
    let (directory, governor, executor) = executor(ResourceLimits::default());
    let path = directory.path().join("pids");
    let mut request = fixture_request("many-descendants", directory.path());
    request.reservation.processes = 2;
    request.reservation.memory_bytes = 256 * 1024 * 1024;
    request.env.insert(
        "EJM_HARNESS_EXEC_PID_FILE".into(),
        path.to_string_lossy().into_owned(),
    );
    let error = executor
        .run(request, CancellationToken::new())
        .await
        .unwrap_err();
    assert!(error.to_string().contains("process count"), "{error:#}");
    let pids = parse_pids(&std::fs::read_to_string(path).unwrap());
    wait_for_cleanup(&governor, &pids).await;
}

fn parse_pids(text: &str) -> Vec<u32> {
    text.split_whitespace()
        .map(|part| part.parse().unwrap())
        .collect()
}

/// Run this ignored test only in a child process controlled by the executor.
#[test]
#[ignore = "subprocess fixture"]
fn fixture() {
    let mode = std::env::var("EJM_HARNESS_EXEC_FIXTURE").expect("fixture mode");
    let pid_file = std::env::var_os("EJM_HARNESS_EXEC_PID_FILE").map(PathBuf::from);
    match mode.as_str() {
        "private-input" => {
            let err = std::thread::spawn(|| {
                let mut stderr = std::io::stderr().lock();
                stderr.write_all(&vec![b'e'; 1024 * 1024]).unwrap();
                stderr.write_all(b"\nSTDERR_END\n").unwrap();
            });
            std::io::stdout()
                .write_all(&vec![b'o'; 1024 * 1024])
                .unwrap();
            let mut input = Vec::new();
            std::io::stdin().read_to_end(&mut input).unwrap();
            err.join().unwrap();
            println!(
                "INPUT_LEN={} PATH_REMOVED={}",
                input.len(),
                std::env::var_os("PATH").is_none()
            );
        }
        "blocked-input" => {
            if let Some(path) = pid_file {
                std::fs::write(path, std::process::id().to_string()).unwrap();
            }
            loop {
                std::thread::sleep(Duration::from_secs(1));
            }
        }
        "noisy" => {
            let err = std::thread::spawn(|| {
                let mut stderr = std::io::stderr().lock();
                stderr.write_all(&vec![b'e'; 1024 * 1024]).unwrap();
                stderr.write_all(b"\nSTDERR_END\n").unwrap();
            });
            std::io::stdout()
                .write_all(&vec![b'o'; 1024 * 1024])
                .unwrap();
            err.join().unwrap();
            std::io::stdout().write_all(b"\nSTDOUT_END\n").unwrap();
        }
        "binary" => std::io::stdout()
            .write_all(&vec![0xff_u8; 1024 * 1024])
            .unwrap(),
        "ready" => {
            if let Some(path) = pid_file {
                std::fs::write(path, std::process::id().to_string()).unwrap();
            }
        }
        "hold" => loop {
            std::thread::sleep(Duration::from_secs(1));
        },
        "descendant" | "orphan" | "many-descendants" => {
            let count = if mode == "many-descendants" { 5 } else { 1 };
            let mut children = Vec::new();
            let mut pids = vec![std::process::id()];
            for _ in 0..count {
                let child = std::process::Command::new(std::env::current_exe().unwrap())
                    .args(["--exact", "fixture", "--ignored", "--nocapture"])
                    .env("EJM_HARNESS_EXEC_FIXTURE", "hold")
                    .spawn()
                    .unwrap();
                pids.push(child.id());
                children.push(child);
            }
            if let Some(path) = pid_file {
                std::fs::write(
                    path,
                    pids.iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(" "),
                )
                .unwrap();
            }
            if mode != "orphan" {
                for mut child in children {
                    let _ = child.wait();
                }
            }
        }
        _ => std::process::exit(2),
    }
}
