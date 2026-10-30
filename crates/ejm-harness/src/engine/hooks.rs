//! Runs the host hooks in order around the run loop. Each hook has one failure rule:
//! preparation stops the turn, tool gates become error results, result views keep the
//! confirmed result, and run end observers cannot change a durable completion.
mod context;

use std::{future::Future, panic::AssertUnwindSafe, time::Duration};

use anyhow::{Context, Result, ensure};
use futures_util::FutureExt;
use tokio_util::sync::CancellationToken;

use super::{Harness, validate_prompt};
use crate::{
    EventKind, HookContext, Message, RunSummary, Session, ToolCall, ToolDecision, ToolOutput,
    types::{timestamp_ms, truncate_utf8},
};
use context::validate_context;

const MAX_TOOL_OUTPUT: usize = 16 * 1024;
const MAX_HOOK_ARGUMENTS: usize = 64 * 1024;
const MAX_FAILURE_NOTE: usize = 1024;
const RUN_END_BUDGET_MS: u64 = 1000;

impl Harness {
    pub(super) async fn prepare_prompt(
        &self,
        context: &HookContext,
        mut prompt: String,
    ) -> Result<String> {
        for (index, hook) in self.inner.hooks.iter().enumerate() {
            prompt = guarded("run_start", index, hook.run_start(context, prompt)).await?;
            validate_prompt(&prompt)?;
        }
        Ok(prompt)
    }

    pub(super) async fn prepare_hook_context(
        &self,
        context: &HookContext,
        mut messages: Vec<Message>,
    ) -> Result<Vec<Message>> {
        for (index, hook) in self.inner.hooks.iter().enumerate() {
            let original = messages.clone();
            messages = guarded("before_model", index, hook.before_model(context, messages)).await?;
            validate_context(&original, &messages, context.context_max_bytes)
                .with_context(|| format!("before_model hook {index} returned invalid context"))?;
        }
        Ok(messages)
    }

    pub(super) async fn prepare_tool_call(
        &self,
        context: &HookContext,
        mut call: ToolCall,
    ) -> Result<(ToolCall, Option<ToolOutput>)> {
        ensure!(
            self.inner.tools.contains_key(&call.name),
            "tool is not granted by this host"
        );
        for (index, hook) in self.inner.hooks.iter().enumerate() {
            match guarded(
                "before_tool",
                index,
                hook.before_tool(context, call.clone()),
            )
            .await?
            {
                ToolDecision::Execute { arguments } => {
                    ensure!(
                        serde_json::to_vec(&arguments)?.len() <= MAX_HOOK_ARGUMENTS,
                        "before_tool hook {index} exceeded the argument byte budget"
                    );
                    call.arguments = arguments;
                }
                ToolDecision::Skip { mut output } => {
                    output.is_error = true;
                    return Ok((call, Some(output)));
                }
            }
        }
        Ok((call, None))
    }

    /// Journal the confirmed result, let the hooks prepare the model's view of it, and
    /// announce the final view.
    pub(super) async fn record_hooked_output(
        &self,
        session: &Session,
        context: &HookContext,
        call: &ToolCall,
        mut output: ToolOutput,
    ) -> Result<()> {
        bound_output(&mut output)?;
        let sequence = self.journal_original(&session.id, call, &output).await?;
        let output = self
            .apply_result_hooks(context, call, sequence, output)
            .await?;
        self.emit(
            &session.id,
            Some(&context.run_id),
            EventKind::ToolFinished {
                call_id: call.id.clone(),
                output,
            },
        )
        .await
    }

    /// The confirmed original pair survives cancellation or a failed prepared view write.
    async fn journal_original(
        &self,
        session_id: &str,
        call: &ToolCall,
        output: &ToolOutput,
    ) -> Result<i64> {
        let sequence = self
            .inner
            .store
            .message(session_id, Message::tool(&call.id, output))
            .await;
        let sequence = self.require_journal(sequence)?;
        let remembered = self
            .inner
            .store
            .remember_tool_output(sequence, output.clone())
            .await;
        self.require_journal(remembered)?;
        Ok(sequence)
    }

    /// Chain `after_tool` hooks, storing each prepared view. The first failure keeps the
    /// last good result with a failure note and stops the chain.
    async fn apply_result_hooks(
        &self,
        context: &HookContext,
        call: &ToolCall,
        sequence: i64,
        mut output: ToolOutput,
    ) -> Result<ToolOutput> {
        for (index, hook) in self.inner.hooks.iter().enumerate() {
            let transformed = guarded(
                "after_tool",
                index,
                hook.after_tool(context, call.clone(), output.clone()),
            )
            .await
            .and_then(|next| checked_transform(index, &output, next));
            let failed = transformed.is_err();
            output = match transformed {
                Ok(next) => next,
                Err(error) => with_failure_note(output, &error),
            };
            let prepared = self
                .inner
                .store
                .prepare_tool_result(sequence, Message::tool(&call.id, &output))
                .await;
            self.require_journal(prepared)?;
            if failed {
                break;
            }
        }
        Ok(output)
    }

    fn require_journal<T>(&self, result: Result<T>) -> Result<T> {
        result.map_err(|error| {
            self.inner.stopped.cancel();
            error.context("journal unavailable, effects stopped")
        })
    }

    pub(super) async fn observe_run_end(&self, context: &HookContext, summary: &RunSummary) {
        if self.inner.hooks.is_empty() {
            return;
        }
        let mut context = context.clone();
        context.step = summary.steps;
        context.deadline_ms = timestamp_ms().saturating_add(RUN_END_BUDGET_MS);
        context.cancel = CancellationToken::new();
        let observers = async {
            for (index, hook) in self.inner.hooks.iter().enumerate() {
                let _ = guarded("run_end", index, hook.run_end(&context, summary)).await;
            }
        };
        let _ = tokio::time::timeout(Duration::from_millis(RUN_END_BUDGET_MS), observers).await;
        context.cancel.cancel();
    }
}

async fn guarded<T>(
    phase: &str,
    index: usize,
    future: impl Future<Output = Result<T>>,
) -> Result<T> {
    AssertUnwindSafe(future)
        .catch_unwind()
        .await
        .map_err(|_| anyhow::anyhow!("{phase} hook {index} panicked"))?
        .with_context(|| format!("{phase} hook {index} failed"))
}

/// A hook result view must keep the original's error status and artifact evidence.
fn checked_transform(
    index: usize,
    original: &ToolOutput,
    mut next: ToolOutput,
) -> Result<ToolOutput> {
    ensure!(
        next.is_error == original.is_error && next.artifact == original.artifact,
        "after_tool hook {index} changed error status or artifact evidence"
    );
    bound_output(&mut next)?;
    Ok(next)
}

fn with_failure_note(mut output: ToolOutput, error: &anyhow::Error) -> ToolOutput {
    let mut note = format!("\n[Result hook failed; confirmed result retained: {error:#}]");
    truncate_utf8(&mut note, MAX_FAILURE_NOTE);
    truncate_utf8(&mut output.content, MAX_TOOL_OUTPUT - note.len());
    output.content.push_str(&note);
    output
}

pub(super) fn bound_output(output: &mut ToolOutput) -> Result<()> {
    if truncate_utf8(&mut output.content, MAX_TOOL_OUTPUT) {
        output
            .content
            .push_str("\n[tool content truncated by harness]");
    }
    if serde_json::to_vec(&output.data)?.len() > MAX_TOOL_OUTPUT {
        output.data = serde_json::json!({"truncated":true});
    }
    Ok(())
}
