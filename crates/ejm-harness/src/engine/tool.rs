use crate::{
    Event, EventKind, ToolCall, ToolDefinition, ToolOutput,
    store::Store,
    types::{timestamp_ms, truncate_utf8},
};
use anyhow::{Result, ensure};
use async_trait::async_trait;
use ejm_harness_exec::{Governor, Reservation};
use std::{path::PathBuf, sync::Arc};
use tokio::sync::{OwnedMutexGuard, broadcast};
use tokio_util::sync::CancellationToken;

#[async_trait]
pub trait Tool: Send + Sync {
    fn definition(&self) -> ToolDefinition;

    /// Return None only when the implementation acquires the same governor itself.
    fn reservation(&self) -> Option<Reservation> {
        Some(Reservation::default())
    }

    /// Execute a granted, schema validated call. Honour cancellation and resource ownership.
    ///
    /// # Errors
    /// Return failures with enough context for the host to report the outcome.
    async fn execute(&self, call: ToolCall, context: ToolContext) -> Result<ToolOutput>;
}

#[derive(Clone)]
pub struct ToolContext {
    pub session_id: String,
    pub run_id: String,
    pub cwd: PathBuf,
    pub cancel: CancellationToken,
    pub governor: Arc<Governor>,
    pub(super) native_owner: Option<ejm_harness_exec::native::NativeOwner>,
    pub(super) effect_guard: Option<Arc<OwnedMutexGuard<()>>>,
    pub(super) store: Store,
    pub(super) events: broadcast::Sender<Event>,
}

impl ToolContext {
    /// Share the prepared native owner with an executor using this governor.
    ///
    /// Legacy harnesses return `None`. Reading the handle starts no helper.
    #[must_use]
    pub fn native_owner(&self) -> Option<ejm_harness_exec::native::NativeOwner> {
        self.native_owner.clone()
    }

    /// Retain effect serialization until work has actually stopped.
    ///
    /// Move this token into blocking work or an executor supervisor that can outlive
    /// the tool future. It holds no resource admission lease by itself.
    #[must_use]
    pub fn retain_execution(&self) -> impl Send + Sync + 'static {
        self.effect_guard.clone()
    }

    /// Persist a bounded progress message for this tool call.
    ///
    /// # Errors
    /// Rejects oversized call identities or journal failures.
    pub async fn progress(&self, call_id: &str, mut message: String) -> Result<()> {
        ensure!(call_id.len() <= 256, "call id exceeds limit");
        truncate_utf8(&mut message, 2048);
        let event = self
            .store
            .append(Event {
                sequence: 0,
                session_id: self.session_id.clone(),
                run_id: Some(self.run_id.clone()),
                timestamp_ms: timestamp_ms(),
                kind: EventKind::Progress {
                    call_id: call_id.into(),
                    message,
                },
            })
            .await?;
        let _ = self.events.send(event);
        Ok(())
    }
}
