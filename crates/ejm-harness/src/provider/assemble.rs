use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::{MAX_RESPONSE, cancellable};
use crate::{ModelDelta, ModelResponse, ToolCall};

/// Append streamed text to the canonical response and forward it as a delta.
pub(super) async fn push_text(
    result: &mut ModelResponse,
    text: &str,
    deltas: &mpsc::Sender<ModelDelta>,
    cancel: &CancellationToken,
) -> Result<()> {
    ensure!(
        result.content.len().saturating_add(text.len()) <= MAX_RESPONSE,
        "provider response exceeds byte budget"
    );
    result.content.push_str(text);
    cancellable(cancel, deltas.send(ModelDelta::Text(text.into())))
        .await?
        .context("delta receiver closed")
}

/// The array position a streamed tool block reports for itself.
pub(super) fn block_index(value: &Value) -> Result<usize> {
    let index = value["index"].as_u64().context("tool block has no index")?;
    usize::try_from(index).context("tool block index exceeds platform limit")
}

/// A tool call whose identity and JSON arguments arrive in fragments.
#[derive(Default)]
pub(super) struct PendingCall {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) arguments: String,
}

impl PendingCall {
    pub(super) fn validate_size(&self) -> Result<()> {
        ensure!(
            self.id.len() < 256 && self.name.len() <= 128 && self.arguments.len() <= MAX_RESPONSE,
            "provider tool call exceeds limit"
        );
        Ok(())
    }

    pub(super) fn finish(self) -> Result<ToolCall> {
        ensure!(
            !self.id.is_empty() && !self.name.is_empty(),
            "provider tool identity is missing"
        );
        let arguments = if self.arguments.is_empty() {
            json!({})
        } else {
            serde_json::from_str(&self.arguments).context("provider returned invalid tool JSON")?
        };
        Ok(ToolCall {
            id: self.id,
            name: self.name,
            arguments,
        })
    }
}
