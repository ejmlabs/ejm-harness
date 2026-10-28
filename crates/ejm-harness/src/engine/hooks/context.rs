//! Checks that a `before_model` hook kept the context a valid, honest copy of the original.
use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result, ensure};

use crate::{Message, Role};

/// A hook may add host messages and rewrite text. It may not change the latest intent,
/// split or forge tool exchanges, or touch error, artifact, or continuation evidence.
pub(super) fn validate_context(
    original: &[Message],
    prepared: &[Message],
    max_bytes: usize,
) -> Result<()> {
    ensure!(
        serde_json::to_vec(prepared)?.len() <= max_bytes,
        "context hook exceeded the configured byte budget"
    );
    validate_intent(original, prepared)?;
    let mut exchanges = Exchanges::new(original)?;
    for message in prepared {
        match message.role {
            Role::User => exchanges.check_user(message)?,
            Role::Assistant => exchanges.check_assistant(message)?,
            Role::Tool => exchanges.check_tool(message)?,
        }
    }
    exchanges.finish(original)
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

/// Walks the prepared messages in order and tracks which tool calls still await a result.
struct Exchanges<'a> {
    original_protocols: HashSet<Vec<u8>>,
    results: HashMap<&'a str, &'a Message>,
    prepared_protocols: HashSet<Vec<u8>>,
    seen: HashSet<&'a str>,
    pending: HashSet<&'a str>,
}

impl<'a> Exchanges<'a> {
    fn new(original: &'a [Message]) -> Result<Self> {
        Ok(Self {
            original_protocols: original
                .iter()
                .filter(|message| message.role == Role::Assistant)
                .map(protocol_key)
                .collect::<Result<_>>()?,
            results: original
                .iter()
                .filter_map(|message| message.tool_call_id.as_deref().map(|id| (id, message)))
                .collect(),
            prepared_protocols: HashSet::new(),
            seen: HashSet::new(),
            pending: HashSet::new(),
        })
    }

    fn check_user(&self, message: &Message) -> Result<()> {
        ensure!(
            self.pending.is_empty(),
            "context hook split a tool exchange"
        );
        ensure!(
            message.tool_calls.is_empty()
                && message.tool_call_id.is_none()
                && message.continuation.is_none()
                && !message.is_error
                && message.artifact.is_none(),
            "context hook added protocol state to a user message"
        );
        Ok(())
    }

    fn check_assistant(&mut self, message: &'a Message) -> Result<()> {
        ensure!(
            self.pending.is_empty(),
            "context hook split a tool exchange"
        );
        ensure!(
            message.tool_call_id.is_none() && !message.is_error && message.artifact.is_none(),
            "context hook added tool result state to an assistant message"
        );
        let protocol = protocol_key(message)?;
        if !message.tool_calls.is_empty() || message.continuation.is_some() {
            ensure!(
                self.original_protocols.contains(&protocol),
                "context hook forged tool calls or changed provider continuation"
            );
        }
        self.prepared_protocols.insert(protocol);
        for call in &message.tool_calls {
            ensure!(
                self.seen.insert(&call.id),
                "context hook duplicated a tool call"
            );
            self.pending.insert(&call.id);
        }
        Ok(())
    }

    fn check_tool(&mut self, message: &Message) -> Result<()> {
        let id = message
            .tool_call_id
            .as_deref()
            .context("context hook removed a tool result identity")?;
        ensure!(
            self.pending.remove(id),
            "context hook added an orphan tool result"
        );
        ensure!(
            message.tool_calls.is_empty() && message.continuation.is_none(),
            "context hook added protocol state to a tool result"
        );
        let source = self
            .results
            .get(id)
            .context("context hook forged a tool result")?;
        ensure!(
            message.is_error == source.is_error && message.artifact == source.artifact,
            "context hook changed error status or artifact evidence"
        );
        Ok(())
    }

    /// Every call got its result, and the latest assistant message and every provider
    /// continuation of the original survived unchanged.
    fn finish(&self, original: &[Message]) -> Result<()> {
        ensure!(
            self.pending.is_empty(),
            "context hook removed a tool result"
        );
        if let Some(latest) = original
            .iter()
            .rfind(|message| message.role == Role::Assistant)
        {
            ensure!(
                self.prepared_protocols.contains(&protocol_key(latest)?),
                "context hook removed the latest assistant exchange"
            );
        }
        for source in original
            .iter()
            .filter(|message| message.continuation.is_some())
        {
            ensure!(
                self.prepared_protocols.contains(&protocol_key(source)?),
                "context hook removed provider continuation"
            );
        }
        Ok(())
    }
}

fn protocol_key(message: &Message) -> Result<Vec<u8>> {
    serde_json::to_vec(&(&message.tool_calls, &message.continuation)).map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ModelResponse, ProviderContinuation, ToolCall, ToolOutput};
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
