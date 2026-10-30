use std::sync::Arc;

use anyhow::{Context, Result, ensure};
use async_trait::async_trait;
use ejm_harness::{Effect, Tool, ToolCall, ToolContext, ToolDefinition, ToolOutput};
use ejm_harness_package_spec::{ToolEffect, ToolExport};
use serde_json::json;
use tokio::sync::mpsc;

use super::Binding;
use crate::{RuntimeEvent, RuntimePool, runtime::callback_reservation};

pub(crate) fn tool(
    pool: RuntimePool,
    package_id: String,
    revision: uuid::Uuid,
    export: ToolExport,
) -> Arc<dyn Tool> {
    Arc::new(PackageTool {
        binding: Binding {
            pool,
            package_id,
            revision,
        },
        export,
    })
}

struct PackageTool {
    binding: Binding,
    export: ToolExport,
}

#[async_trait]
impl Tool for PackageTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.export.name.clone(),
            description: self.export.description.clone(),
            input_schema: self.export.input_schema.clone(),
            effect: match self.export.effect {
                ToolEffect::Read => Effect::Read,
                ToolEffect::Write => Effect::Write,
                ToolEffect::External => Effect::External,
            },
            version: self.export.version.clone(),
        }
    }

    fn reservation(&self) -> Option<ejm_harness_exec::Reservation> {
        None
    }

    async fn execute(&self, call: ToolCall, context: ToolContext) -> Result<ToolOutput> {
        ensure!(
            call.name == self.export.name,
            "package tool identity mismatch"
        );
        let lease = context
            .governor
            .acquire(&context.session_id, callback_reservation(), &context.cancel)
            .await?;
        let (events, mut receiver) = mpsc::channel(32);
        let callback = self.binding.callback(
            self.export.handler.clone(),
            call.arguments,
            json!({
                "session_id": context.session_id,
                "run_id": context.run_id,
                "cwd": context.cwd,
                "call_id": call.id,
            }),
            context.cancel.clone(),
            Some(events),
        );
        let invoke = self
            .binding
            .pool
            .invoke(callback, (lease, context.retain_execution()));
        tokio::pin!(invoke);
        let output = loop {
            tokio::select! {
                result = &mut invoke => break result?,
                event = receiver.recv() => {
                    if let Some(event) = event { tool_event(&context, &call.id, event).await?; }
                }
            }
        };
        while let Some(event) = receiver.recv().await {
            tool_event(&context, &call.id, event).await?;
        }
        serde_json::from_value(output).context("package tool must return ToolOutput")
    }
}

async fn tool_event(context: &ToolContext, call_id: &str, event: RuntimeEvent) -> Result<()> {
    match event {
        RuntimeEvent::Progress(message) => context.progress(call_id, message).await,
        RuntimeEvent::Delta(_) => {
            anyhow::bail!("text deltas are only supported by provider exports")
        }
    }
}
