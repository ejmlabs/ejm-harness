use std::{sync::Arc, time::Duration};

use anyhow::{Context, Result};
use async_trait::async_trait;
use ejm_harness::{
    Harness, HarnessConfig, ModelDelta, ModelRequest, ModelResponse, Provider, RunStatus,
    SessionOptions, SessionStatus, SessionTailOptions,
};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

struct ControlledProvider {
    entered: mpsc::Sender<oneshot::Sender<()>>,
}

#[async_trait]
impl Provider for ControlledProvider {
    async fn stream(
        &self,
        _request: ModelRequest,
        cancel: CancellationToken,
        deltas: mpsc::Sender<ModelDelta>,
    ) -> Result<ModelResponse> {
        let (release, wait) = oneshot::channel();
        self.entered.send(release).await?;
        tokio::select! {
            () = cancel.cancelled() => anyhow::bail!("cancelled"),
            released = wait => released?,
        }
        deltas.send(ModelDelta::Text("done".into())).await?;
        Ok(ModelResponse {
            content: "done".into(),
            ..Default::default()
        })
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn returned_and_chained_run_handles_match_committed_authority() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (entered, mut calls) = mpsc::channel(2);
    let harness = Harness::new(
        HarnessConfig {
            state_dir: root.path().join("state"),
            max_model_requests: 1,
            ..Default::default()
        },
        Arc::new(ControlledProvider { entered }),
        vec![],
    )?;
    let session = harness
        .create_session(SessionOptions {
            cwd: root.path().into(),
            title: "authority lifecycle".into(),
        })
        .await?;
    let mut first = harness.start(&session.id, "first".into()).await?;
    let authority_only = SessionTailOptions {
        max_events: 0,
        max_bytes: 0,
    };
    let initial = harness.session_view(&session.id, authority_only).await?;
    assert_eq!(initial.run_id.as_deref(), Some(first.id.as_str()));
    assert_eq!(initial.session.status, SessionStatus::Running);
    let transition = initial
        .last_transition
        .context("start was not committed before its handle")?;
    assert_eq!(transition.run_id.as_deref(), Some(first.id.as_str()));
    assert!(transition.outcome.is_none());
    let release = tokio::time::timeout(Duration::from_secs(2), calls.recv())
        .await?
        .context("provider never started")?;
    harness
        .enqueue_for_run(&session.id, &first.id, "second".into())
        .await?;
    release
        .send(())
        .map_err(|()| anyhow::anyhow!("first provider stopped"))?;
    let _second_release = tokio::time::timeout(Duration::from_secs(2), calls.recv())
        .await?
        .context("queued provider never started")?;
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), first.wait())
            .await??
            .status,
        RunStatus::Completed
    );
    let current = harness.session_view(&session.id, authority_only).await?;
    let second_id = current
        .run_id
        .context("chained start has no authoritative identity")?;
    assert_ne!(second_id, first.id);
    assert_eq!(
        current
            .last_transition
            .as_ref()
            .and_then(|transition| transition.run_id.as_deref()),
        Some(second_id.as_str())
    );
    assert!(harness.cancel_run(&session.id, &first.id).await.is_err());
    harness.cancel_run(&session.id, &second_id).await?;
    tokio::time::timeout(Duration::from_secs(2), harness.shutdown()).await??;
    let final_view = harness.session_view(&session.id, authority_only).await?;
    assert!(final_view.run_id.is_none());
    let final_transition = final_view
        .last_transition
        .context("cancelled run has no transition")?;
    assert_eq!(final_transition.run_id.as_deref(), Some(second_id.as_str()));
    assert_eq!(final_transition.outcome, Some(RunStatus::Cancelled));
    Ok(())
}
