use std::{sync::Arc, time::Duration};

use anyhow::{Context, Result};
use async_trait::async_trait;
use ejm_harness::{
    EventKind, Harness, HarnessConfig, Hook, HookContext, ReplayProvider, RunHandle, RunStatus,
    RunSummary, SessionOptions,
};
use tokio::sync::{Notify, mpsc};

struct CompletionGate {
    started: mpsc::Sender<String>,
    start_release: Notify,
    ended: mpsc::Sender<String>,
    end_release: Notify,
}

#[async_trait]
impl Hook for CompletionGate {
    async fn run_start(&self, context: &HookContext, prompt: String) -> Result<String> {
        self.started.send(context.run_id.clone()).await?;
        self.start_release.notified().await;
        Ok(prompt)
    }

    async fn run_end(&self, _: &HookContext, summary: &RunSummary) -> Result<()> {
        self.ended.send(summary.run_id.clone()).await?;
        self.end_release.notified().await;
        Ok(())
    }
}

async fn assert_completion(handle: &mut RunHandle, queued_successor: bool) -> Result<RunSummary> {
    let completion = handle.wait_completion().await?;
    assert_eq!(completion.queued_successor, queued_successor);
    assert_eq!(
        serde_json::to_value(&completion.summary)?,
        serde_json::to_value(handle.wait().await?)?
    );
    Ok(completion.summary)
}

#[tokio::test]
async fn exact_subscriptions_follow_queued_successors_and_wait_for_observers() -> Result<()> {
    tokio::time::timeout(Duration::from_secs(5), async {
        let root = tempfile::tempdir()?;
        let (started, mut starts) = mpsc::channel(2);
        let (ended, mut ends) = mpsc::channel(2);
        let gate = Arc::new(CompletionGate {
            started,
            start_release: Notify::new(),
            ended,
            end_release: Notify::new(),
        });
        let harness = Harness::with_hooks(
            HarnessConfig {
                state_dir: root.path().join("state"),
                ..HarnessConfig::default()
            },
            Arc::new(ReplayProvider::echo()),
            vec![],
            vec![gate.clone()],
        )?;
        let session = harness.create_session(SessionOptions::default()).await?;
        let other = harness.create_session(SessionOptions::default()).await?;
        let mut first = harness.start(&session.id, "first".into()).await?;
        assert_eq!(
            starts.recv().await.context("missing first start")?,
            first.id
        );
        harness
            .enqueue_for_run(&session.id, &first.id, "successor".into())
            .await?;
        gate.start_release.notify_one();
        assert_eq!(ends.recv().await.context("missing first end")?, first.id);
        gate.end_release.notify_one();
        let successor = starts.recv().await.context("missing queued successor")?;
        assert!(
            harness
                .subscribe_run(&session.id, &first.id)
                .await
                .is_none()
        );
        assert!(harness.subscribe_run(&other.id, &successor).await.is_none());
        assert!(
            harness
                .subscribe_run(&session.id, "wrong-run")
                .await
                .is_none()
        );
        let mut observed = harness
            .subscribe_run(&session.id, &successor)
            .await
            .context("queued successor has no completion handle")?;
        let mut cloned = observed.clone();
        gate.start_release.notify_one();
        assert_eq!(
            ends.recv().await.context("missing successor end")?,
            successor
        );
        assert!(harness.events(&session.id, 0, 100).await?.iter().any(|event| {
            matches!(&event.kind, EventKind::RunFinished { summary } if summary.run_id == successor)
        }));
        assert!(futures_util::poll!(std::pin::pin!(observed.wait_completion())).is_pending());
        assert!(futures_util::poll!(std::pin::pin!(cloned.wait())).is_pending());
        drop(observed.clone());
        gate.end_release.notify_one();
        let (left, right) =
            tokio::try_join!(assert_completion(&mut observed, false), cloned.wait())?;
        for summary in [left, right] {
            assert_eq!(summary.run_id, successor);
            assert_eq!(summary.status, RunStatus::Completed);
        }
        // Observe the predecessor only after its successor has fully finished.
        let predecessor = assert_completion(&mut first, true).await?;
        assert_eq!(predecessor.run_id, first.id);
        assert_eq!(predecessor.status, RunStatus::Completed);
        assert!(
            harness
                .subscribe_run(&session.id, &successor)
                .await
                .is_none()
        );
        harness.shutdown().await
    })
    .await
    .context("completion subscription contract timed out")?
}
