mod anthropic;
mod assemble;
mod claude;
mod http;
mod observe;
mod openai;
mod replay;
mod responses;
mod sse;

use std::future::Future;

use anyhow::{Result, bail};
use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::{Message, ModelDelta, ModelRequest, ModelResponse, Role};

pub use anthropic::{AnthropicConfig, AnthropicProvider};
pub use claude::{ClaudeCodeConfig, ClaudeCodeProvider};
pub use observe::{HttpObserver, HttpTraceHeader, HttpTracePhase, HttpTraceRecord};
pub use openai::{OpenAiConfig, OpenAiProvider};
pub use replay::ReplayProvider;
pub use responses::{AvailableModel, REASONING_EFFORTS, ResponsesConfig, ResponsesProvider};

const MAX_RESPONSE: usize = 64 * 1024;

#[async_trait]
pub trait Provider: Send + Sync {
    /// Return canonical content as well as deltas. Do not retry effects inside a provider.
    ///
    /// # Errors
    ///
    /// Returns an error on cancellation, a closed delta receiver, provider failure,
    /// malformed or incomplete output, or an exceeded response bound.
    async fn stream(
        &self,
        request: ModelRequest,
        cancel: CancellationToken,
        deltas: mpsc::Sender<ModelDelta>,
    ) -> Result<ModelResponse>;
}

/// Await `future` unless `cancel` fires first. Cancellation wins a tie.
async fn cancellable<T>(cancel: &CancellationToken, future: impl Future<Output = T>) -> Result<T> {
    tokio::select! {
        biased;
        () = cancel.cancelled() => bail!("cancelled"),
        value = future => Ok(value),
    }
}

/// Provider-visible text for a message, with tool error and artifact markers.
fn message_content(message: &Message) -> String {
    if message.role != Role::Tool {
        return message.content.clone();
    }
    let status = if message.is_error {
        "[ejm-harness tool error]\n"
    } else {
        ""
    };
    match &message.artifact {
        Some(path) => format!(
            "{status}{}\n[ejm-harness artifact: {}]",
            message.content,
            path.display()
        ),
        None => format!("{status}{}", message.content),
    }
}
