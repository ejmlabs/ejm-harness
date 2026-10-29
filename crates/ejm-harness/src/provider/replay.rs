use std::{collections::VecDeque, sync::Mutex};

use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::{Provider, cancellable};
use crate::{ModelDelta, ModelRequest, ModelResponse, Role};

enum ReplayMode {
    Responses(VecDeque<ModelResponse>),
    Echo,
}

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
            .map_err(|_| anyhow!("replay lock poisoned"))?
        {
            ReplayMode::Responses(responses) => {
                responses.pop_front().context("replay exhausted")?
            }
            ReplayMode::Echo => echo(&request),
        };
        cancellable(
            &cancel,
            deltas.send(ModelDelta::Text(response.content.clone())),
        )
        .await?
        .context("delta receiver closed")?;
        Ok(response)
    }
}

fn echo(request: &ModelRequest) -> ModelResponse {
    ModelResponse {
        content: request
            .messages
            .iter()
            .rev()
            .find(|m| m.role == Role::User)
            .map(|m| format!("Demo: {}", m.content))
            .unwrap_or_default(),
        ..Default::default()
    }
}
