use std::{path::Path, sync::Arc, time::Duration};

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use ejm_harness::{
    Effect, Harness, HarnessConfig, Hook, HookContext, Message, ModelDelta, ModelRequest,
    ModelResponse, Provider, Role, RunStatus, SchedulingSnapshot, SessionOptions, Tool, ToolCall,
    ToolContext, ToolDefinition, ToolOutput,
};
use serde_json::json;
use tokio::sync::{Notify, mpsc};
use tokio_util::sync::CancellationToken;

const BOUND: Duration = Duration::from_secs(5);

async fn counts(
    harness: &Harness,
    expected: impl Fn(SchedulingSnapshot) -> bool,
) -> Result<SchedulingSnapshot> {
    tokio::time::timeout(BOUND, async {
        loop {
            let snapshot = harness.scheduling_snapshot();
            if expected(snapshot) {
                return snapshot;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .with_context(|| {
        format!(
            "scheduling phase did not settle: {:?}",
            harness.scheduling_snapshot()
        )
    })
}

struct PreparationGate(Arc<Notify>);

#[async_trait]
impl Hook for PreparationGate {
    async fn before_model(&self, _: &HookContext, messages: Vec<Message>) -> Result<Vec<Message>> {
        self.0.notified().await;
        Ok(messages)
    }
}

struct ModelGate {
    release: Arc<Notify>,
    fail: bool,
}

#[async_trait]
impl Provider for ModelGate {
    async fn stream(
        &self,
        _: ModelRequest,
        _: CancellationToken,
        _: mpsc::Sender<ModelDelta>,
    ) -> Result<ModelResponse> {
        self.release.notified().await;
        if self.fail {
            bail!("controlled provider failure");
        }
        Ok(ModelResponse {
            content: "finished".into(),
            ..ModelResponse::default()
        })
    }
}

fn model_harness(root: &Path, fail: bool) -> Result<(Harness, Arc<Notify>, Arc<Notify>)> {
    let preparation = Arc::new(Notify::new());
    let provider = Arc::new(Notify::new());
    let harness = Harness::with_optional_provider(
        HarnessConfig {
            state_dir: root.join("state"),
            max_model_requests: 1,
            ..HarnessConfig::default()
        },
        Some(Arc::new(ModelGate {
            release: provider.clone(),
            fail,
        })),
        Vec::new(),
        vec![Arc::new(PreparationGate(preparation.clone()))],
    )?;
    Ok((harness, preparation, provider))
}

async fn session(harness: &Harness, cwd: &Path) -> Result<ejm_harness::Session> {
    harness
        .create_session(SessionOptions {
            cwd: cwd.into(),
            ..SessionOptions::default()
        })
        .await
}

#[tokio::test]
async fn model_status_separates_admission_preparation_service_and_failure() -> Result<()> {
    for fail in [false, true] {
        let root = tempfile::tempdir()?;
        let (harness, preparation, provider) = model_harness(root.path(), fail)?;
        let first = session(&harness, root.path()).await?;
        let second = session(&harness, root.path()).await?;
        let mut running = harness.start(&first.id, "first".into()).await?;
        counts(&harness, |value| value.model_preparing == 1).await?;
        let mut waiting = harness.start(&second.id, "second".into()).await?;
        let snapshot = counts(&harness, |value| value.model_waiting == 1).await?;
        assert_eq!(snapshot.managed_runs, 2);
        assert_eq!(snapshot.runnable_runs, 0);
        assert_eq!(snapshot.model_preparing, 1);
        assert_eq!(snapshot.model_running, 0);
        let legacy = harness.status().await;
        assert_eq!(legacy.active_agents, 2);
        assert_eq!(legacy.waiting_model_requests, 1);
        waiting.cancel();
        assert_eq!(
            tokio::time::timeout(BOUND, waiting.wait()).await??.status,
            RunStatus::Cancelled
        );
        preparation.notify_one();
        let snapshot = counts(&harness, |value| value.model_running == 1).await?;
        assert_eq!(snapshot.model_preparing, 0);
        assert_eq!(snapshot.model_waiting, 0);
        assert_eq!(harness.status().await.waiting_model_requests, 0);
        provider.notify_one();
        let summary = tokio::time::timeout(BOUND, running.wait()).await??;
        assert_eq!(
            summary.status,
            if fail {
                RunStatus::Failed
            } else {
                RunStatus::Completed
            }
        );
        harness.shutdown().await?;
        assert_eq!(harness.scheduling_snapshot(), SchedulingSnapshot::default());
    }
    Ok(())
}

#[tokio::test]
async fn cancelling_preparation_or_provider_service_releases_live_counts() -> Result<()> {
    for during_preparation in [true, false] {
        let root = tempfile::tempdir()?;
        let (harness, preparation, _) = model_harness(root.path(), false)?;
        let selected = session(&harness, root.path()).await?;
        if !during_preparation {
            preparation.notify_one();
        }
        let mut run = harness
            .start(&selected.id, "cancel active phase".into())
            .await?;
        counts(&harness, |value| {
            if during_preparation {
                value.model_preparing == 1
            } else {
                value.model_running == 1
            }
        })
        .await?;
        run.cancel();
        assert_eq!(
            tokio::time::timeout(BOUND, run.wait()).await??.status,
            RunStatus::Cancelled
        );
        harness.shutdown().await?;
        assert_eq!(harness.scheduling_snapshot(), SchedulingSnapshot::default());
    }
    Ok(())
}

struct ToolProvider;

#[async_trait]
impl Provider for ToolProvider {
    async fn stream(
        &self,
        request: ModelRequest,
        _: CancellationToken,
        _: mpsc::Sender<ModelDelta>,
    ) -> Result<ModelResponse> {
        Ok(
            if request
                .messages
                .iter()
                .any(|message| message.role == Role::Tool)
            {
                ModelResponse {
                    content: "finished".into(),
                    ..ModelResponse::default()
                }
            } else {
                ModelResponse {
                    tool_calls: vec![ToolCall {
                        id: "proposal".into(),
                        name: "hold".into(),
                        arguments: json!({}),
                    }],
                    ..ModelResponse::default()
                }
            },
        )
    }
}

struct HeldWrite(mpsc::Sender<String>);

#[async_trait]
impl Tool for HeldWrite {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "hold".into(),
            description: "Hold a controlled workspace mutation".into(),
            input_schema: json!({"type":"object"}),
            effect: Effect::Write,
            version: "1".into(),
        }
    }

    async fn execute(&self, _: ToolCall, context: ToolContext) -> Result<ToolOutput> {
        self.0.send(context.session_id).await?;
        context.cancel.cancelled().await;
        bail!("controlled callback cancelled")
    }
}

#[tokio::test]
async fn tool_status_separates_dispatch_workspace_and_governor_waits() -> Result<()> {
    let root = tempfile::tempdir()?;
    let other = root.path().join("other");
    std::fs::create_dir(&other)?;
    let (entered, mut calls) = mpsc::channel(4);
    let harness = Harness::new(
        HarnessConfig {
            state_dir: root.path().join("state"),
            resources: ejm_harness_exec::ResourceLimits {
                max_jobs: 1,
                max_queue: 35,
                ..ejm_harness_exec::ResourceLimits::default()
            },
            ..HarnessConfig::default()
        },
        Arc::new(ToolProvider),
        vec![Arc::new(HeldWrite(entered))],
    )?;
    let first = session(&harness, root.path()).await?;
    let same_workspace = session(&harness, root.path()).await?;
    let other_workspace = session(&harness, &other).await?;
    let pending = session(&harness, &other).await?;
    let mut running = harness.start(&first.id, "running".into()).await?;
    assert_eq!(
        tokio::time::timeout(BOUND, calls.recv()).await?,
        Some(first.id)
    );
    let mut workspace = harness
        .start(&same_workspace.id, "workspace".into())
        .await?;
    counts(&harness, |value| value.workspace_waiting == 1).await?;
    let mut resources = harness
        .start(&other_workspace.id, "resources".into())
        .await?;
    counts(&harness, |value| value.tool_resource_waiting == 1).await?;
    let mut dispatch = harness.start(&pending.id, "dispatch".into()).await?;
    let snapshot = counts(&harness, |value| value.tool_dispatch_waiting == 1).await?;
    assert_eq!(snapshot.managed_runs, 4);
    assert_eq!(snapshot.runnable_runs, 0);
    assert_eq!(snapshot.workspace_waiting, 1);
    assert_eq!(snapshot.tool_resource_waiting, 1);
    assert_eq!(snapshot.tool_running, 1);
    assert!(
        calls.try_recv().is_err(),
        "a second workspace write started before release"
    );
    for run in [&mut dispatch, &mut resources, &mut workspace, &mut running] {
        run.cancel();
        assert_eq!(
            tokio::time::timeout(BOUND, run.wait()).await??.status,
            RunStatus::Cancelled
        );
    }
    harness.shutdown().await?;
    assert_eq!(harness.scheduling_snapshot(), SchedulingSnapshot::default());
    assert_eq!(harness.governor().snapshot().running, 0);
    Ok(())
}
