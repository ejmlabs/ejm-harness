use std::sync::Arc;

use anyhow::{Context, Result, ensure};
use async_trait::async_trait;
use ejm_harness::{
    Effect, Hook, HookContext, Message, ModelDelta, ModelRequest, ModelResponse, Provider,
    RunSummary, Tool, ToolCall, ToolContext, ToolDecision, ToolDefinition, ToolOutput,
};
use ejm_harness_package_spec::{HookEvent, HookExport, ToolEffect, ToolExport};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::{RuntimeEvent, RuntimePool, runtime::Callback};

#[derive(Clone)]
struct Binding {
    pool: RuntimePool,
    package_id: String,
    revision: uuid::Uuid,
}

impl Binding {
    fn callback(
        &self,
        handler: String,
        args: Value,
        context: Value,
        cancel: CancellationToken,
    ) -> Callback {
        Callback {
            package_id: self.package_id.clone(),
            revision: self.revision,
            handler,
            args,
            context,
            cancel,
            events: None,
        }
    }
}

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
        self.binding.pool.require_governor(&context.governor)?;
        let lease = context
            .governor
            .acquire(
                &context.session_id,
                self.binding.pool.callback_reservation(),
                &context.cancel,
            )
            .await?;
        let mut callback = self.binding.callback(
            self.export.handler.clone(), call.arguments,
            json!({"session_id":context.session_id,"run_id":context.run_id,"cwd":context.cwd,"call_id":call.id}),
            context.cancel.clone(),
        );
        let (events, mut receiver) = mpsc::channel(32);
        callback.events = Some(events);
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
        let mut callback = self.binding.callback(
            self.handler.clone(),
            serde_json::to_value(request)?,
            json!({}),
            cancel.clone(),
        );
        let (events, mut receiver) = mpsc::channel(32);
        callback.events = Some(events);
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
            response.content.len() <= 64 * 1024,
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
            streamed.len().saturating_add(text.len()) <= 64 * 1024,
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
        self.binding.pool.require_governor(&context.governor)?;
        let lease = context
            .governor
            .acquire(
                &context.session_id,
                self.binding.pool.callback_reservation(),
                &context.cancel,
            )
            .await?;
        let callback = self.binding.callback(
            handler.into(),
            args,
            serde_json::to_value(context)?,
            context.cancel.clone(),
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
