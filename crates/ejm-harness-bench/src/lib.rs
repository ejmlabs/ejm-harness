//! Bounded deterministic workloads for native benchmarks and host acceptance.
//! This crate is unpublished development tooling and never calls paid models.

use anyhow::{Result, ensure};
use async_trait::async_trait;
use ejm_harness::{
    Effect, ModelDelta, ModelRequest, ModelResponse, Provider, Role, Tool, ToolCall, ToolContext,
    ToolDefinition, ToolOutput,
};
use ejm_harness_exec::{CommandRequest, Executor, Reservation};
use serde_json::json;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

/// Deterministic provider gated so managed sessions can queue first.
pub struct Workload {
    ready: watch::Receiver<bool>,
    rounds: usize,
    deltas: usize,
    markers: bool,
}

impl Workload {
    /// Construct the bounded deterministic workload.
    ///
    /// # Errors
    /// Rejects more than eight tool rounds or 128 text deltas per response.
    pub fn new(ready: watch::Receiver<bool>, rounds: usize, deltas: usize) -> Result<Self> {
        ensure!(
            rounds <= 8 && deltas <= 128,
            "workload exceeds benchmark envelope"
        );
        Ok(Self {
            ready,
            rounds,
            deltas,
            markers: false,
        })
    }

    /// Prefix each response with `response:{prompt}:round-{round}:`.
    /// Prompts are capped at 128 characters; unique prompts distinguish turns.
    #[must_use]
    pub const fn with_markers(mut self, enabled: bool) -> Self {
        self.markers = enabled;
        self
    }
}

impl LocalCheck {
    /// Construct a fixed read-only native command and a shared completion count.
    #[must_use]
    pub fn new(artifacts: PathBuf, completed: Arc<AtomicUsize>) -> Self {
        Self {
            artifacts,
            completed,
            native_owner: None,
        }
    }

    /// Route the fixed command through the caller's prepared native owner.
    #[must_use]
    pub fn with_native_owner(mut self, owner: ejm_harness_exec::native::NativeOwner) -> Self {
        self.native_owner = Some(owner);
        self
    }
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
        let turn_start = request
            .messages
            .iter()
            .rposition(|message| message.role == Role::User)
            .unwrap_or(0);
        if let Some(failed) = request.messages[turn_start..]
            .iter()
            .find(|message| message.role == Role::Tool && message.is_error)
        {
            anyhow::bail!("native fixture failed: {}", failed.content);
        }
        let round = request.messages[turn_start..]
            .iter()
            .filter(|message| message.role == Role::Tool)
            .count();
        let mut content = if self.deltas > 0 {
            "fixture ".repeat(self.deltas)
        } else if round < self.rounds {
            format!("Checking step {}.\n", round + 1)
        } else {
            "Verified all local work.\n".into()
        };
        if self.markers
            && let Some(prompt) = request.messages.get(turn_start)
            && prompt.role == Role::User
        {
            let marker: String = prompt.content.chars().take(128).collect();
            content = format!("response:{marker}:round-{round}:\n{content}");
        }
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

/// Fixed native command used by the benchmark and product acceptance fixtures.
pub struct LocalCheck {
    artifacts: std::path::PathBuf,
    completed: Arc<AtomicUsize>,
    native_owner: Option<ejm_harness_exec::native::NativeOwner>,
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
        let executor = if let Some(owner) = &self.native_owner {
            executor.with_native_owner(owner.clone())
        } else {
            executor
        };
        let output = executor
            .run(
                CommandRequest {
                    session_id: context.session_id,
                    cwd: context.cwd,
                    program,
                    args,
                    // Leave room for the bounded scheduling-stall fixture and
                    // work already in flight before suspension.
                    timeout_ms: 10_000,
                    ..Default::default()
                },
                context.cancel,
            )
            .await?;
        ensure!(
            output.exit_code == Some(0),
            "fixture command failed: exit={:?}, cancelled={}, timed_out={}, memory_peak={}, process_peak={}, stderr={}",
            output.exit_code,
            output.cancelled,
            output.timed_out,
            output.peak_memory_bytes,
            output.peak_processes,
            output.stderr
        );
        self.completed.fetch_add(1, Ordering::Relaxed);
        Ok(ToolOutput::text(output.stdout))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ejm_harness::Message;

    #[tokio::test]
    async fn reused_sessions_still_run_native_work_and_markers_stay_bounded() -> Result<()> {
        let (_ready, gate) = watch::channel(true);
        let provider = Workload::new(gate, 1, 0)?.with_markers(true);
        let (sender, _receiver) = mpsc::channel(16);
        let response = provider
            .stream(
                ModelRequest {
                    system: String::new(),
                    messages: vec![
                        Message::user("previous run"),
                        Message::tool("old-call", &ToolOutput::error("previous failure")),
                        Message::user("x".repeat(256)),
                    ],
                    tools: Vec::new(),
                    max_output_tokens: 128,
                },
                CancellationToken::new(),
                sender,
            )
            .await?;
        assert_eq!(response.tool_calls.len(), 1);
        assert_eq!(response.tool_calls[0].name, "local_check");
        assert_eq!(
            response.content.lines().next(),
            Some(format!("response:{}:round-0:", "x".repeat(128)).as_str())
        );
        let first_marker = response.content.lines().next().unwrap_or_default();
        let (sender, _receiver) = mpsc::channel(16);
        let finished = provider
            .stream(
                ModelRequest {
                    system: String::new(),
                    messages: vec![
                        Message::user("x".repeat(256)),
                        Message::tool(
                            response.tool_calls[0].id.as_str(),
                            &ToolOutput::text("fixture verified"),
                        ),
                    ],
                    tools: Vec::new(),
                    max_output_tokens: 128,
                },
                CancellationToken::new(),
                sender,
            )
            .await?;
        assert!(finished.tool_calls.is_empty());
        let second_marker = finished.content.lines().next().unwrap_or_default();
        assert_eq!(
            second_marker,
            format!("response:{}:round-1:", "x".repeat(128))
        );
        assert!(!first_marker.contains(second_marker));
        assert!(!second_marker.contains(first_marker));
        Ok(())
    }

    #[tokio::test]
    async fn failed_native_work_cannot_count_toward_a_successful_benchmark() -> Result<()> {
        let (_ready, gate) = watch::channel(true);
        let provider = Workload::new(gate, 1, 0)?;
        let (sender, _receiver) = mpsc::channel(16);
        let result = provider
            .stream(
                ModelRequest {
                    system: String::new(),
                    messages: vec![
                        Message::user("current run"),
                        Message::tool("failed-call", &ToolOutput::error("fixture regression")),
                    ],
                    tools: Vec::new(),
                    max_output_tokens: 128,
                },
                CancellationToken::new(),
                sender,
            )
            .await;
        let error = result
            .err()
            .ok_or_else(|| anyhow::anyhow!("failed fixture counted as useful work"))?;
        assert!(error.to_string().contains("fixture regression"));
        Ok(())
    }
}
