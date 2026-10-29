use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use ejm_harness::{Hook, HookContext, Message, RunSummary, ToolCall, ToolDecision, ToolOutput};
use ejm_harness_package_spec::{HookEvent, HookExport};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use super::Binding;
use crate::{RuntimePool, runtime::callback_reservation};

pub(crate) fn hooks(
    pool: RuntimePool,
    package_id: String,
    revision: uuid::Uuid,
    exports: Vec<HookExport>,
) -> Vec<Arc<dyn Hook>> {
    if exports.is_empty() {
        return Vec::new();
    }
    vec![Arc::new(PackageHook {
        binding: Binding {
            pool,
            package_id,
            revision,
        },
        exports,
    })]
}

struct PackageHook {
    binding: Binding,
    exports: Vec<HookExport>,
}

impl PackageHook {
    fn handler(&self, event: HookEvent) -> Option<&str> {
        self.exports
            .iter()
            .find(|export| export.event == event)
            .map(|export| export.handler.as_str())
    }

    async fn invoke<T: DeserializeOwned>(
        &self,
        handler: &str,
        args: Value,
        context: &HookContext,
    ) -> Result<T> {
        let lease = context
            .governor
            .acquire(&context.session_id, callback_reservation(), &context.cancel)
            .await?;
        let callback = self.binding.callback(
            handler.into(),
            args,
            serde_json::to_value(context)?,
            context.cancel.clone(),
            None,
        );
        serde_json::from_value(self.binding.pool.invoke(callback, lease).await?)
            .context("invalid package hook result")
    }
}

#[async_trait]
impl Hook for PackageHook {
    async fn run_start(&self, context: &HookContext, prompt: String) -> Result<String> {
        let Some(handler) = self.handler(HookEvent::RunStart) else {
            return Ok(prompt);
        };
        self.invoke(handler, json!({"prompt":prompt}), context)
            .await
    }

    async fn before_model(
        &self,
        context: &HookContext,
        messages: Vec<Message>,
    ) -> Result<Vec<Message>> {
        let Some(handler) = self.handler(HookEvent::BeforeModel) else {
            return Ok(messages);
        };
        self.invoke(handler, json!({"messages":messages}), context)
            .await
    }

    async fn before_tool(&self, context: &HookContext, call: ToolCall) -> Result<ToolDecision> {
        let Some(handler) = self.handler(HookEvent::BeforeTool) else {
            return Ok(ToolDecision::Execute {
                arguments: call.arguments,
            });
        };
        self.invoke(handler, json!({"call":call}), context).await
    }

    async fn after_tool(
        &self,
        context: &HookContext,
        call: ToolCall,
        output: ToolOutput,
    ) -> Result<ToolOutput> {
        let Some(handler) = self.handler(HookEvent::AfterTool) else {
            return Ok(output);
        };
        self.invoke(handler, json!({"call":call,"output":output}), context)
            .await
    }

    async fn run_end(&self, context: &HookContext, summary: &RunSummary) -> Result<()> {
        if let Some(handler) = self.handler(HookEvent::RunEnd) {
            let _: Value = self
                .invoke(handler, json!({"summary":summary}), context)
                .await?;
        }
        Ok(())
    }
}
