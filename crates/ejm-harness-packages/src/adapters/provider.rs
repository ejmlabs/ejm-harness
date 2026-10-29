use std::sync::Arc;

use anyhow::{Context, Result, ensure};
use async_trait::async_trait;
use ejm_harness::{ModelDelta, ModelRequest, ModelResponse, Provider};
use serde_json::json;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::Binding;
use crate::{RuntimeEvent, RuntimePool};

const MAX_RESPONSE_BYTES: usize = 64 * 1024;

pub(crate) fn provider(
    pool: RuntimePool,
    package_id: String,
    revision: uuid::Uuid,
    handler: String,
) -> Arc<dyn Provider> {
    Arc::new(PackageProvider {
        binding: Binding {
            pool,
            package_id,
            revision,
        },
        handler,
    })
}

struct PackageProvider {
    binding: Binding,
    handler: String,
}

#[async_trait]
impl Provider for PackageProvider {
    async fn stream(
        &self,
        request: ModelRequest,
        cancel: CancellationToken,
        deltas: mpsc::Sender<ModelDelta>,
    ) -> Result<ModelResponse> {
        let (events, mut receiver) = mpsc::channel(32);
        let callback = self.binding.callback(
            self.handler.clone(),
            serde_json::to_value(request)?,
            json!({}),
            cancel.clone(),
            Some(events),
        );
        let invoke = self.binding.pool.invoke(callback, ());
        tokio::pin!(invoke);
        let mut streamed = String::new();
        let output = loop {
            tokio::select! {
                result = &mut invoke => break result?,
                event = receiver.recv() => {
                    if let Some(event) = event { provider_event(event, &mut streamed, &deltas, &cancel).await?; }
                }
            }
        };
        while let Some(event) = receiver.recv().await {
            provider_event(event, &mut streamed, &deltas, &cancel).await?;
        }
        let response: ModelResponse =
            serde_json::from_value(output).context("package provider must return ModelResponse")?;
        ensure!(
            response.content.len() <= MAX_RESPONSE_BYTES,
            "package provider response exceeds 64 KiB"
        );
        if streamed.is_empty() {
            if !response.content.is_empty() {
                send_delta(response.content.clone(), &deltas, &cancel).await?;
            }
        } else {
            ensure!(
                response.content == streamed,
                "provider deltas differ from its canonical response"
            );
        }
        Ok(response)
    }
}

async fn provider_event(
    event: RuntimeEvent,
    streamed: &mut String,
    deltas: &mpsc::Sender<ModelDelta>,
    cancel: &CancellationToken,
) -> Result<()> {
    if let RuntimeEvent::Delta(text) = event {
        ensure!(
            streamed.len().saturating_add(text.len()) <= MAX_RESPONSE_BYTES,
            "package provider deltas exceed 64 KiB"
        );
        streamed.push_str(&text);
        send_delta(text, deltas, cancel).await?;
    }
    Ok(())
}

async fn send_delta(
    text: String,
    deltas: &mpsc::Sender<ModelDelta>,
    cancel: &CancellationToken,
) -> Result<()> {
    tokio::select! {
        () = cancel.cancelled() => anyhow::bail!("package provider cancelled"),
        result = deltas.send(ModelDelta::Text(text)) => result.context("provider delta receiver closed"),
    }
}
