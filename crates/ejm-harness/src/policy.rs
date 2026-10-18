use anyhow::{Context, Result, ensure};

use crate::{Message, ModelResponse, Role, ToolCall};

/// Context preparation is host policy. It must preserve complete tool exchanges.
pub trait ContextPolicy: Send + Sync {
    /// Select bounded history while preserving complete tool exchanges.
    ///
    /// # Errors
    /// Rejects history that cannot fit without losing the latest intent or exchange.
    fn prepare(&self, messages: Vec<Message>, max_bytes: usize) -> Result<Vec<Message>>;
}

#[derive(Default)]
/// Keep recent exchanges and mask older successful observations under byte pressure.
/// Complete older exchanges may still be omitted when their retained evidence cannot fit.
pub struct RecentContext;

impl ContextPolicy for RecentContext {
    fn prepare(&self, messages: Vec<Message>, max_bytes: usize) -> Result<Vec<Message>> {
        let bytes =
            |messages: &[Message]| -> Result<usize> { Ok(serde_json::to_vec(messages)?.len()) };
        if bytes(&messages)? <= max_bytes {
            return Ok(messages);
        }
        // Prefer whole turns, then retain the latest intent and complete recent tool exchanges.
        let starts: Vec<usize> = messages
            .iter()
            .enumerate()
            .filter(|(_, m)| m.role == Role::User)
            .map(|(i, _)| i)
            .collect();
        for start in starts.iter().copied().skip(1) {
            if bytes(&messages[start..])? <= max_bytes {
                return Ok(messages[start..].to_vec());
            }
        }
        let start = starts.last().copied().unwrap_or(0);
        let mut turn = messages[start..].to_vec();
        let anchor = turn.first().context("context has no user intent")?.clone();
        ensure!(anchor.role == Role::User, "context has no user intent");
        let latest_exchange = turn
            .iter()
            .rposition(|message| message.role == Role::Assistant)
            .unwrap_or(0);
        let omitted = "[ejm-harness context note: older tool output omitted to fit the context budget. Call details and any artifact reference are retained.]";
        let omitted_bytes = serde_json::to_vec(omitted)?.len();
        let mut used = bytes(&turn)?;
        for message in turn.iter_mut().take(latest_exchange) {
            if message.role != Role::Tool || message.is_error {
                continue;
            }
            let content_bytes = serde_json::to_vec(&message.content)?.len();
            if content_bytes > omitted_bytes {
                message.content = omitted.into();
                used -= content_bytes - omitted_bytes;
                if used <= max_bytes {
                    break;
                }
            }
        }
        if used <= max_bytes {
            return Ok(turn);
        }
        let marker = Message::user(
            "[ejm-harness context note: older exchanges were omitted to fit the context budget. Complete events remain in the session journal. Verify current files before relying on omitted observations.]",
        );
        for index in 1..turn.len() {
            if turn[index].role != Role::Assistant {
                continue;
            }
            let mut recent = vec![anchor.clone(), marker.clone()];
            recent.extend_from_slice(&turn[index..]);
            if bytes(&recent)? <= max_bytes {
                return Ok(recent);
            }
        }
        anyhow::bail!("latest intent or tool exchange exceeds context budget ({max_bytes} bytes)")
    }
}

#[derive(Clone, Debug)]
pub enum StepDecision {
    Complete,
    Execute(Vec<ToolCall>),
}

/// Strategies choose the next step. The engine still owns grants, limits, and journaling.
pub trait RunStrategy: Send + Sync {
    /// Choose completion or a subset of the response tool calls.
    ///
    /// # Errors
    /// Rejects a response that cannot be handled under this strategy.
    fn decide(&self, response: &ModelResponse) -> Result<StepDecision>;
}

#[derive(Default)]
pub struct ToolLoop;

impl RunStrategy for ToolLoop {
    fn decide(&self, response: &ModelResponse) -> Result<StepDecision> {
        ensure!(
            response.tool_calls.len() <= 16,
            "provider returned more than 16 tool calls in one step"
        );
        if response.tool_calls.is_empty() {
            Ok(StepDecision::Complete)
        } else {
            Ok(StepDecision::Execute(response.tool_calls.clone()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ProviderContinuation, ToolOutput};
    use serde_json::json;

    fn exchange(id: &str, output: &ToolOutput) -> [Message; 2] {
        [
            Message::assistant(&ModelResponse {
                content: format!("Inspect {id}."),
                tool_calls: vec![ToolCall {
                    id: id.into(),
                    name: "read".into(),
                    arguments: json!({"path":id}),
                }],
                ..Default::default()
            }),
            Message::tool(id, output),
        ]
    }

    #[test]
    fn trims_whole_turns_and_preserves_tool_pairs() -> Result<()> {
        let mut messages = vec![Message::user("old".repeat(100)), Message::user("new")];
        messages.extend(exchange("c1", &ToolOutput::text("result")));
        let budget = serde_json::to_vec(&messages[1..])?.len();
        let result = RecentContext.prepare(messages, budget)?;
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].content, "new");
        assert!(
            RecentContext
                .prepare(vec![Message::user("big".repeat(100))], 100)
                .is_err()
        );
        Ok(())
    }

    #[test]
    fn masks_older_outputs_before_actions_and_preserves_artifacts_and_continuation() -> Result<()> {
        let output = ToolOutput {
            artifact: Some("artifacts/old.log".into()),
            ..ToolOutput::text("old observation ".repeat(400))
        };
        let mut older = exchange("old", &output);
        older[0].continuation = Some(ProviderContinuation::OpenAiResponses {
            reasoning: vec![json!({"id":"rs_old", "encrypted_content":"state"})],
        });
        let mut messages = vec![Message::user("Preserve this request.")];
        messages.extend(older);
        messages.extend(exchange("recent", &ToolOutput::text("current observation")));
        let original = serde_json::to_vec(&messages)?;
        assert_eq!(
            serde_json::to_vec(&RecentContext.prepare(messages.clone(), original.len())?)?,
            original
        );

        let result = RecentContext.prepare(messages.clone(), 1500)?;
        assert_eq!(result[0].content, "Preserve this request.");
        assert_eq!(result.len(), 5);
        for index in [1, 3] {
            assert_eq!(
                serde_json::to_value(&result[index])?,
                serde_json::to_value(&messages[index])?
            );
        }
        assert!(result[2].content.contains("older tool output omitted"));
        assert_eq!(result[2].tool_call_id.as_deref(), Some("old"));
        assert_eq!(result[2].artifact, output.artifact);
        assert_eq!(result[4].content, "current observation");
        assert_eq!(result[4].tool_call_id.as_deref(), Some("recent"));
        let prepared = serde_json::to_vec(&result)?;
        assert!(prepared.len() <= 1500);
        assert_eq!(
            serde_json::to_vec(&RecentContext.prepare(messages.clone(), 1500)?)?,
            prepared
        );
        assert_eq!(
            serde_json::to_vec(&RecentContext.prepare(result, 1500)?)?,
            prepared
        );
        assert_eq!(serde_json::to_vec(&messages)?, original);
        Ok(())
    }

    #[test]
    fn preserves_old_error_evidence_while_masking_successes() -> Result<()> {
        let error = ToolOutput {
            artifact: Some("artifacts/failure.log".into()),
            ..ToolOutput::error("Permission denied. Inspect the grant before retrying.")
        };
        let mut messages = vec![Message::user("Fix the failure.")];
        messages.extend(exchange("failed", &error));
        messages.extend(exchange("large", &ToolOutput::text("雪\n".repeat(2000))));
        messages.extend(exchange("recent", &ToolOutput::text("current observation")));
        let result = RecentContext.prepare(messages, 2000)?;
        assert_eq!(result.len(), 7);
        assert!(result[2].is_error);
        assert_eq!(result[2].content, error.content);
        assert_eq!(result[2].artifact, error.artifact);
        assert!(result[4].content.contains("older tool output omitted"));
        assert!(serde_json::to_vec(&result)?.len() <= 2000);
        Ok(())
    }

    #[test]
    fn keeps_the_latest_parallel_exchange_in_full_or_rejects_it() -> Result<()> {
        let latest = ModelResponse {
            tool_calls: vec![
                ToolCall {
                    id: "a".into(),
                    name: "read".into(),
                    arguments: json!({}),
                },
                ToolCall {
                    id: "b".into(),
                    name: "read".into(),
                    arguments: json!({}),
                },
            ],
            ..Default::default()
        };
        let mut messages = vec![Message::user("Preserve this request.")];
        messages.extend(exchange(
            "old",
            &ToolOutput::text("old observation ".repeat(400)),
        ));
        messages.push(Message::assistant(&latest));
        messages.push(Message::tool("a", &ToolOutput::text("雪".repeat(300))));
        messages.push(Message::tool("b", &ToolOutput::text("recent".repeat(150))));
        let result = RecentContext.prepare(messages.clone(), 3000)?;
        assert_eq!(result.len(), 6);
        for index in 3..6 {
            assert_eq!(
                serde_json::to_value(&result[index])?,
                serde_json::to_value(&messages[index])?
            );
        }
        assert!(RecentContext.prepare(messages, 1000).is_err());
        Ok(())
    }

    #[test]
    fn drops_complete_older_exchanges_when_retained_actions_still_exceed_budget() -> Result<()> {
        let mut old = exchange("old", &ToolOutput::text("small observation"));
        old[0].content = "older assistant text ".repeat(400);
        let mut messages = vec![Message::user("Preserve this request.")];
        messages.extend(old);
        messages.extend(exchange("recent", &ToolOutput::text("current observation")));
        let result = RecentContext.prepare(messages, 1000)?;
        assert_eq!(result.len(), 4);
        assert_eq!(result[0].content, "Preserve this request.");
        assert!(result[1].content.contains("older exchanges were omitted"));
        assert_eq!(result[2].tool_calls[0].id, "recent");
        assert_eq!(result[3].tool_call_id.as_deref(), Some("recent"));
        assert!(serde_json::to_vec(&result)?.len() <= 1000);
        Ok(())
    }
}
