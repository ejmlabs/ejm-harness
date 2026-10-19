#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "Offline subprocess fixtures fail at their assertion site."
)]

use std::{path::Path, time::Duration};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use ejm_harness_auth::ClaudeCodeClient;
use serde_json::json;
use sysinfo::{Pid, ProcessRefreshKind, ProcessStatus, ProcessesToUpdate, System};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

const CLIENT: &str = r"#!/usr/bin/env python3
import json
import os
from pathlib import Path
import subprocess
import sys
import time

root = Path(__file__).parent
config = json.loads((root / 'fixture.json').read_text())
operation = sys.argv[2]
if operation == 'logout' and config['mode'] == 'commit-logout':
    config['status'] = json.dumps({'loggedIn': False, 'authMethod': 'none'})
    config['code'] = 1
    (root / 'fixture.json').write_text(json.dumps(config))
    (root / 'ready').write_text(str(os.getpid()))
    time.sleep(30)
elif config['mode'] == 'hang':
    descendant = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(30)'])
    (root / 'ready').write_text(f'{os.getpid()} {descendant.pid}')
    time.sleep(30)
elif operation == 'login':
    (root / 'ready').write_text(str(os.getpid()))
    time.sleep(30)
else:
    sys.stderr.write('fixture-secret-that-must-stay-private')
    sys.stdout.write(config['status'])
    sys.exit(config['code'])
";

fn fixture(status: &str, code: i32, mode: &str) -> (TempDir, ClaudeCodeClient) {
    let directory = tempfile::tempdir().unwrap();
    #[cfg(not(windows))]
    let program = directory.path().join("claude");
    #[cfg(windows)]
    let program = directory.path().join("claude.cmd");
    #[cfg(not(windows))]
    std::fs::write(&program, CLIENT).unwrap();
    #[cfg(windows)]
    {
        std::fs::write(directory.path().join("fixture.py"), CLIENT).unwrap();
        std::fs::write(&program, "@echo off\r\npython \"%~dp0fixture.py\" %*\r\n").unwrap();
    }
    #[cfg(unix)]
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::write(
        directory.path().join("fixture.json"),
        json!({"status":status,"code":code,"mode":mode}).to_string(),
    )
    .unwrap();
    let client = ClaudeCodeClient::new(program)
        .with_timeout(Duration::from_secs(2))
        .unwrap();
    (directory, client)
}

async fn ready(path: &Path) -> Vec<u32> {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let Ok(text) = tokio::fs::read_to_string(path).await {
                let pids = text
                    .split_whitespace()
                    .map(str::parse)
                    .collect::<Result<Vec<u32>, _>>();
                if let Ok(pids) = pids
                    && !pids.is_empty()
                {
                    return pids;
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap()
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
async fn account_status_is_allowlisted_and_subscription_sources_are_explicit() {
    let (_directory, client) = fixture(
        r#"{"loggedIn":true,"authMethod":"claude.ai","email":"person@example.test","subscriptionType":"max","accessToken":"fixture-secret-that-must-stay-private"}"#,
        0,
        "normal",
    );
    let status = client.status(CancellationToken::new()).await.unwrap();
    status.require_subscription().unwrap();
    assert_eq!(status.email.as_deref(), Some("person@example.test"));
    assert!(
        !serde_json::to_string(&status)
            .unwrap()
            .contains("fixture-secret")
    );
    assert!(!format!("{status:?}").contains("fixture-secret"));

    for (logged_in, method, code) in [
        (false, "none", 1),
        (true, "api_key", 0),
        (true, "oauth_token", 0),
        (true, "third_party", 0),
    ] {
        let (_directory, client) = fixture(
            &json!({"loggedIn":logged_in,"authMethod":method}).to_string(),
            code,
            "normal",
        );
        let status = client.status(CancellationToken::new()).await.unwrap();
        assert_eq!(status.logged_in, logged_in);
        assert!(status.require_subscription().is_err());
    }
}

#[tokio::test]
async fn malformed_oversized_and_failed_status_never_exposes_output() {
    for (status, code) in [
        ("fixture-secret-invalid-json".into(), 0),
        ("fixture-secret".repeat(2048), 0),
        (r#"{"loggedIn":true,"authMethod":"claude.ai"}"#.into(), 1),
        ("fixture-secret".into(), 2),
    ] {
        let (_directory, client) = fixture(&status, code, "normal");
        let error = client.status(CancellationToken::new()).await.unwrap_err();
        assert!(!format!("{error:#}").contains("fixture-secret"));
    }
}

#[tokio::test]
async fn cancelled_and_dropped_auth_callers_reap_their_process() {
    for dropped in [false, true] {
        let (directory, client) = fixture("{}", 0, "hang");
        let cancel = CancellationToken::new();
        let child_cancel = cancel.clone();
        let task = tokio::spawn(async move { client.status(child_cancel).await });
        let pids = ready(&directory.path().join("ready")).await;
        assert_eq!(pids.len(), 2);
        if dropped {
            task.abort();
        } else {
            cancel.cancel();
        }
        let _ = task.await;
        tokio::time::timeout(Duration::from_secs(3), async {
            while pids.iter().any(|pid| live(*pid)) {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
    }
}

#[tokio::test]
async fn signout_committed_before_cancellation_remains_observable() {
    let (directory, client) = fixture(
        r#"{"loggedIn":true,"authMethod":"claude.ai"}"#,
        0,
        "commit-logout",
    );
    let cancel = CancellationToken::new();
    let child_cancel = cancel.clone();
    let task = tokio::spawn(async move { client.logout(child_cancel).await });
    let pids = ready(&directory.path().join("ready")).await;
    cancel.cancel();
    let status = tokio::time::timeout(Duration::from_secs(3), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(!status.logged_in);
    assert!(pids.iter().all(|pid| !live(*pid)));
}

#[tokio::test]
async fn login_deadline_reaps_the_inherited_stdio_child() {
    let (directory, client) = fixture("{}", 0, "normal");
    let client = client.with_timeout(Duration::from_secs(2)).unwrap();
    let task = tokio::spawn(async move { client.login(CancellationToken::new()).await });
    let pids = ready(&directory.path().join("ready")).await;
    let error = task.await.unwrap().unwrap_err();
    assert!(error.to_string().contains("timed out"));
    assert!(pids.iter().all(|pid| !live(*pid)));
}
