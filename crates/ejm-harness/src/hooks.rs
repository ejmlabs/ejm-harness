use std::{path::PathBuf, sync::Arc};

use anyhow::Result;
use async_trait::async_trait;
use ejm_harness_exec::Governor;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::{Message, RunSummary, ToolCall, ToolOutput};

/// Metadata shared by the ordered hooks for one run or model step.
#[derive(Clone, Serialize, JsonSchema)]
pub struct HookContext {
    pub session_id: String,
    pub run_id: String,
    pub cwd: PathBuf,
    /// Zero at run start, otherwise the current model step.
    pub step: u32,
    /// Absolute Unix deadline in milliseconds.
    pub deadline_ms: u64,
    pub context_max_bytes: usize,
    /// Cancellation is carried by the host transport, outside the wire metadata.
    #[serde(skip)]
    #[schemars(skip)]
    pub cancel: CancellationToken,
    /// The engine's shared governor. Wire metadata cannot create runtime ownership.
    #[serde(skip)]
    #[schemars(skip)]
    pub governor: Arc<Governor>,
}

impl std::fmt::Debug for HookContext {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HookContext")
            .field("session_id", &self.session_id)
            .field("run_id", &self.run_id)
            .field("cwd", &self.cwd)
            .field("step", &self.step)
            .field("deadline_ms", &self.deadline_ms)
            .field("context_max_bytes", &self.context_max_bytes)
            .finish_non_exhaustive()
    }
}

/// A hook can change arguments or stop a call, but cannot change its grant or identity.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ToolDecision {
    Execute {
        arguments: Value,
    },
    /// The engine marks skipped results as errors so they cannot imply an effect ran.
    Skip {
        output: ToolOutput,
    },
}

/// Ordered host extensions around the existing run loop. Hooks never call the model.
///
/// Preparation failures stop the turn. Tool gate failures become error results.
/// Result transform failures preserve the last durable result and add a failure note.
/// Run end observers have one shared second after durable completion; their failures
/// cannot change that completion. All other hooks share the run deadline and cancellation.
#[async_trait]
pub trait Hook: Send + Sync {
    /// Prepare the prompt before it becomes the durable user message.
    ///
    /// # Errors
    /// Return an error to stop the run before its first model request.
    async fn run_start(&self, _context: &HookContext, prompt: String) -> Result<String> {
        Ok(prompt)
    }

    /// Transform a bounded context copy while preserving intent, protocol, and evidence.
    ///
    /// # Errors
    /// Return an error to stop before model admission.
    async fn before_model(
        &self,
        _context: &HookContext,
        messages: Vec<Message>,
    ) -> Result<Vec<Message>> {
        Ok(messages)
    }

    /// Gate a granted tool or prepare its arguments before schema and effect admission.
    ///
    /// # Errors
    /// Return an error to reject this call without executing it.
    async fn before_tool(&self, _context: &HookContext, call: ToolCall) -> Result<ToolDecision> {
        Ok(ToolDecision::Execute {
            arguments: call.arguments,
        })
    }

    /// Prepare the model's result view after the original result is durable.
    /// Error status and artifact references must remain unchanged.
    ///
    /// # Errors
    /// Return an error to keep the last durable result and stop further result transforms.
    async fn after_tool(
        &self,
        _context: &HookContext,
        _call: ToolCall,
        output: ToolOutput,
    ) -> Result<ToolOutput> {
        Ok(output)
    }

    /// Observe a durable completion. The observer token has an independent deadline.
    ///
    /// # Errors
    /// Observer errors are isolated from the already durable run outcome.
    async fn run_end(&self, _context: &HookContext, _summary: &RunSummary) -> Result<()> {
        Ok(())
    }
}
