use std::{
    collections::{HashMap, HashSet},
    future::Future,
    panic::AssertUnwindSafe,
    time::Duration,
};

use anyhow::{Context, Result, ensure};
use futures_util::FutureExt;

use super::{Harness, validate_prompt};
use crate::{
    EventKind, HookContext, Message, Role, RunSummary, Session, ToolCall, ToolDecision, ToolOutput,
    types::{timestamp_ms, truncate_utf8},
};

const MAX_TOOL_OUTPUT: usize = 16 * 1024;

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
                        serde_json::to_vec(&arguments)?.len() <= 64 * 1024,
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

    pub(super) async fn record_hooked_output(
        &self,
        session: &Session,
        context: &HookContext,
        call: &ToolCall,
        mut output: ToolOutput,
    ) -> Result<()> {
        bound_output(&mut output)?;
        let sequence = self
            .inner
            .store
            .message(&session.id, Message::tool(&call.id, &output))
            .await;
        let sequence = self.require_journal(sequence)?;
        // The confirmed original pair survives cancellation or a failed prepared view write.
        let remembered = self
            .inner
            .store
            .remember_tool_output(sequence, output.clone())
            .await;
        self.require_journal(remembered)?;
        for (index, hook) in self.inner.hooks.iter().enumerate() {
            let transformed = guarded(
                "after_tool",
                index,
                hook.after_tool(context, call.clone(), output.clone()),
            )
            .await
            .and_then(|mut next| {
                ensure!(
                    next.is_error == output.is_error && next.artifact == output.artifact,
                    "after_tool hook {index} changed error status or artifact evidence"
                );
                bound_output(&mut next)?;
                Ok(next)
            });
            let failed = transformed.is_err();
            output = match transformed {
                Ok(next) => next,
                Err(error) => {
                    let mut detail =
                        format!("\n[Result hook failed; confirmed result retained: {error:#}]");
                    truncate_utf8(&mut detail, 1024);
                    truncate_utf8(&mut output.content, MAX_TOOL_OUTPUT - detail.len());
                    output.content.push_str(&detail);
                    output
                }
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
        context.deadline_ms = timestamp_ms().saturating_add(1000);
        context.cancel = tokio_util::sync::CancellationToken::new();
        let observers = async {
            for (index, hook) in self.inner.hooks.iter().enumerate() {
                let _ = guarded("run_end", index, hook.run_end(&context, summary)).await;
            }
        };
        let _ = tokio::time::timeout(Duration::from_secs(1), observers).await;
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

fn validate_context(original: &[Message], prepared: &[Message], max_bytes: usize) -> Result<()> {
    ensure!(
        serde_json::to_vec(prepared)?.len() <= max_bytes,
        "context hook exceeded the configured byte budget"
    );
    validate_intent(original, prepared)?;
    let mut pending = HashSet::new();
    let mut seen = HashSet::new();
    let original_protocols: HashSet<Vec<u8>> = original
        .iter()
        .filter(|message| message.role == Role::Assistant)
        .map(protocol_key)
        .collect::<Result<_>>()?;
    let results: HashMap<&str, &Message> = original
        .iter()
        .filter_map(|message| message.tool_call_id.as_deref().map(|id| (id, message)))
        .collect();
    let mut prepared_protocols = HashSet::new();
    for message in prepared {
        match message.role {
            Role::User => {
                ensure!(pending.is_empty(), "context hook split a tool exchange");
                ensure!(
                    message.tool_calls.is_empty()
                        && message.tool_call_id.is_none()
                        && message.continuation.is_none()
                        && !message.is_error
                        && message.artifact.is_none(),
                    "context hook added protocol state to a user message"
                );
            }
            Role::Assistant => {
                ensure!(pending.is_empty(), "context hook split a tool exchange");
                ensure!(
                    message.tool_call_id.is_none()
                        && !message.is_error
                        && message.artifact.is_none(),
                    "context hook added tool result state to an assistant message"
                );
                let protocol = protocol_key(message)?;
                if !message.tool_calls.is_empty() || message.continuation.is_some() {
                    ensure!(
                        original_protocols.contains(&protocol),
                        "context hook forged tool calls or changed provider continuation"
                    );
                }
                prepared_protocols.insert(protocol);
                for call in &message.tool_calls {
                    ensure!(seen.insert(&call.id), "context hook duplicated a tool call");
                    pending.insert(&call.id);
                }
            }
            Role::Tool => {
                let id = message
                    .tool_call_id
                    .as_ref()
                    .context("context hook removed a tool result identity")?;
                ensure!(
                    pending.remove(id),
                    "context hook added an orphan tool result"
                );
                ensure!(
                    message.tool_calls.is_empty() && message.continuation.is_none(),
                    "context hook added protocol state to a tool result"
                );
                let source = results
                    .get(id.as_str())
                    .context("context hook forged a tool result")?;
                ensure!(
                    message.is_error == source.is_error && message.artifact == source.artifact,
                    "context hook changed error status or artifact evidence"
                );
            }
        }
    }
    ensure!(pending.is_empty(), "context hook removed a tool result");
    if let Some(latest) = original
        .iter()
        .rfind(|message| message.role == Role::Assistant)
    {
        ensure!(
            prepared_protocols.contains(&protocol_key(latest)?),
            "context hook removed the latest assistant exchange"
        );
    }
    for source in original
        .iter()
        .filter(|message| message.continuation.is_some())
    {
        ensure!(
            prepared_protocols.contains(&protocol_key(source)?),
            "context hook removed provider continuation"
        );
    }
    Ok(())
}

fn protocol_key(message: &Message) -> Result<Vec<u8>> {
    serde_json::to_vec(&(&message.tool_calls, &message.continuation)).map_err(Into::into)
}

fn validate_intent(original: &[Message], prepared: &[Message]) -> Result<()> {
    let intent = original
        .iter()
        .rfind(|message| message.role == Role::User)
        .context("context has no user intent")?;
    let prepared_intent = prepared
        .iter()
        .rfind(|message| message.role == Role::User)
        .context("context hook removed the latest user intent")?;
    ensure!(
        serde_json::to_value(intent)? == serde_json::to_value(prepared_intent)?,
        "context hook changed the latest user intent"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ModelResponse, ProviderContinuation};
    use serde_json::json;

    #[test]
    fn context_hooks_preserve_intent_pairs_continuation_and_failure_evidence() -> Result<()> {
        let original = vec![
            Message::user("Keep this intent."),
            Message::assistant(&ModelResponse {
                tool_calls: vec![ToolCall {
                    id: "call".into(),
                    name: "read".into(),
                    arguments: json!({"path":"a"}),
                }],
                continuation: Some(ProviderContinuation::OpenAiResponses {
                    reasoning: vec![json!({"encrypted_content":"state"})],
                }),
                ..Default::default()
            }),
            Message::tool(
                "call",
                &ToolOutput {
                    artifact: Some("artifacts/error.txt".into()),
                    ..ToolOutput::error("Confirmed failure.")
                },
            ),
        ];
        validate_context(&original, &original, 4096)?;
        let mut prepared = original.clone();
        prepared.insert(0, Message::user("Extra host context."));
        prepared[3].content = "A prepared failure description.".into();
        validate_context(&original, &prepared, 4096)?;
        for mutation in 0..8 {
            let mut prepared = original.clone();
            match mutation {
                0 => {
                    prepared.remove(0);
                }
                1 => prepared[0].content = "Changed intent.".into(),
                2 => {
                    prepared.pop();
                }
                3 => prepared[1].tool_calls[0].arguments = json!({"path":"forged"}),
                4 => prepared[1].continuation = None,
                5 => prepared[2].artifact = None,
                6 => prepared[2].is_error = false,
                _ => prepared.push(prepared[2].clone()),
            }
            assert!(
                validate_context(&original, &prepared, 4096).is_err(),
                "mutation {mutation} was accepted"
            );
        }
        assert!(validate_context(&original, &original, 100).is_err());
        Ok(())
    }
}
