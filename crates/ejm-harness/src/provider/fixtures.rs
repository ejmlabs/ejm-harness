//! Deterministic providers for tests and benchmarks. Requires `test-support`.

use std::{collections::VecDeque, sync::Mutex};

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::Provider;
use crate::{ModelDelta, ModelRequest, ModelResponse, Role};

enum ReplayMode {
    Responses(VecDeque<ModelResponse>),
    Echo,
}

/// A development fixture, excluded from default builds and product configuration.
pub struct ReplayProvider {
    mode: Mutex<ReplayMode>,
}

impl ReplayProvider {
    #[must_use]
    pub fn new(responses: Vec<ModelResponse>) -> Self {
        Self {
            mode: Mutex::new(ReplayMode::Responses(responses.into())),
        }
    }

    #[must_use]
    pub fn echo() -> Self {
        Self {
            mode: Mutex::new(ReplayMode::Echo),
        }
    }
}

#[async_trait]
impl Provider for ReplayProvider {
    async fn stream(
        &self,
        request: ModelRequest,
        cancel: CancellationToken,
        deltas: mpsc::Sender<ModelDelta>,
    ) -> Result<ModelResponse> {
        let response = match &mut *self
            .mode
            .lock()
            .map_err(|_| anyhow::anyhow!("fixture lock poisoned"))?
        {
            ReplayMode::Responses(responses) => {
                responses.pop_front().context("fixture exhausted")?
            }
            ReplayMode::Echo => ModelResponse {
                content: request
                    .messages
                    .iter()
                    .rev()
                    .find(|message| message.role == Role::User)
                    .map(|message| format!("Demo: {}", message.content))
                    .unwrap_or_default(),
                ..Default::default()
            },
        };
        tokio::select! {
            biased;
            () = cancel.cancelled() => bail!("cancelled"),
            result = deltas.send(ModelDelta::Text(response.content.clone())) => {
                result.context("delta receiver closed")?;
            }
        }
        Ok(response)
    }
}
