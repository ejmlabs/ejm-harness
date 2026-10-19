use super::{Harness, ModelWaiter, RunCompletion, ToolContext, hooks::bound_output};
use crate::{
    Effect, EventKind, HookContext, Message, ModelDelta, ModelRequest, ModelResponse, RunStatus,
    RunSummary, Session, StepDecision, ToolCall, ToolOutput, Usage, types::timestamp_ms,
};
use anyhow::{Context, Result, bail, ensure};
use futures_util::FutureExt;
use std::{
    collections::HashMap,
    sync::{Arc, atomic::Ordering},
    time::Duration,
};
use tokio::sync::{Mutex, mpsc, watch};
use tokio_util::sync::CancellationToken;

impl Harness {
    pub(super) async fn session_driver(
        &self,
        session: Session,
        mut run_id: String,
        mut prompt: String,
        mut cancel: CancellationToken,
        mut steering: mpsc::Receiver<String>,
        mut done: watch::Sender<Option<RunCompletion>>,
    ) {
        loop {
            let context = HookContext {
                session_id: session.id.clone(),
                run_id: run_id.clone(),
                cwd: session.cwd.clone(),
                step: 0,
                deadline_ms: timestamp_ms().saturating_add(self.inner.config.run_timeout_ms),
                context_max_bytes: self.inner.config.context_max_bytes,
                cancel: cancel.clone(),
                governor: self.inner.governor.clone(),
            };
            let mut summary = RunSummary {
                run_id: run_id.clone(),
                status: RunStatus::Completed,
                steps: 0,
                text: String::new(),
                usage: Usage::default(),
                error: None,
            };
            let deadline =
                tokio::time::sleep(Duration::from_millis(self.inner.config.run_timeout_ms));
            tokio::pin!(deadline);
            let result = tokio::select! {
                biased;
                () = cancel.cancelled() => { summary.status = RunStatus::Cancelled; Ok(()) }
                () = &mut deadline => { cancel.cancel(); summary.status = RunStatus::BudgetExhausted; Err(anyhow::anyhow!("run deadline exhausted")) }
                result = std::panic::AssertUnwindSafe(self.drive(&session, prompt, &context, &mut steering, &mut summary)).catch_unwind() => result.unwrap_or_else(|_| Err(anyhow::anyhow!("provider, policy, or tool panicked; turn stopped"))),
            };
            if let Err(error) = result {
                if summary.status == RunStatus::Completed {
                    summary.status = if cancel.is_cancelled() {
                        RunStatus::Cancelled
                    } else {
                        RunStatus::Failed
                    };
                }
                summary.error = Some(format!("{error:#}"));
            }
            // Cancellation can interrupt a multi-tool response. Close every outstanding pair explicitly.
            if let Err(error) = self.close_pending(&session.id).await {
                summary.status = RunStatus::Failed;
                summary.error = Some(format!("journal unavailable, effects stopped: {error:#}"));
                self.inner.stopped.cancel();
            }
            if let Err(error) = self
                .emit(
                    &session.id,
                    Some(&run_id),
                    EventKind::RunFinished {
                        summary: summary.clone(),
                    },
                )
                .await
            {
                summary.status = RunStatus::Failed;
                summary.error = Some(format!("could not persist completion: {error:#}"));
                self.inner.stopped.cancel();
            } else {
                self.observe_run_end(&context, &summary).await;
            }
            let mut active = self.inner.active.lock().await;
            let Some(entry) = active.get_mut(&session.id) else {
                let _ = done.send(Some(RunCompletion {
                    summary,
                    queued_successor: false,
                }));
                break;
            };
            while let Ok(text) = steering.try_recv() {
                entry.queued.push_back(text);
            }
            let may_continue = summary.status == RunStatus::Completed
                && !cancel.is_cancelled()
                && !self.inner.stopped.is_cancelled();
            let next = if may_continue {
                entry.queued.pop_front()
            } else {
                None
            };
            let _ = done.send(Some(RunCompletion {
                summary,
                queued_successor: next.is_some(),
            }));
            if let Some(next) = next {
                prompt = next;
                run_id = uuid::Uuid::new_v4().to_string();
                entry.id.clone_from(&run_id);
                cancel = self.inner.stopped.child_token();
                entry.cancel = cancel.clone();
                let (tx, result) = watch::channel(None);
                entry.done = result;
                done = tx;
                drop(active);
            } else {
                active.remove(&session.id);
                self.inner.changed.notify_waiters();
                break;
            }
        }
    }

    async fn drive(
        &self,
        session: &Session,
        prompt: String,
        context: &HookContext,
        steering: &mut mpsc::Receiver<String>,
        summary: &mut RunSummary,
    ) -> Result<()> {
        let run_id = &context.run_id;
        let cancel = &context.cancel;
        self.emit(
            &session.id,
            Some(run_id),
            EventKind::RunStarted {
                prompt: prompt.clone(),
            },
        )
        .await?;
        let prompt = self.prepare_prompt(context, prompt).await?;
        self.inner
            .store
            .message(&session.id, Message::user(prompt))
            .await?;
        let mut repeats = RepeatCounts::default();
        for step in 1..=self.inner.config.max_steps {
            let mut context = context.clone();
            context.step = step;
            ensure!(!cancel.is_cancelled(), "cancelled");
            while let Ok(text) = steering.try_recv() {
                self.inner
                    .store
                    .message(&session.id, Message::user(text))
                    .await?;
            }
            let messages = self.prepare_context(session, &context).await?;
            if summary.usage.input_tokens >= self.inner.config.max_input_tokens
                || summary.usage.output_tokens >= self.inner.config.max_output_tokens
            {
                summary.status = RunStatus::BudgetExhausted;
                bail!("token budget exhausted");
            }
            let request = ModelRequest {
                system: self.inner.config.system_prompt.clone(),
                messages,
                tools: self.inner.definitions.clone(),
                max_output_tokens: u32::try_from(
                    (self.inner.config.max_output_tokens - summary.usage.output_tokens).min(4096),
                )?,
            };
            let response = self
                .model_step(session, run_id, step, request, cancel)
                .await?;
            summary.steps = step;
            summary.text.clone_from(&response.content);
            summary.usage.add(&response.usage);
            self.emit(
                &session.id,
                Some(run_id),
                EventKind::Usage {
                    usage: response.usage.clone(),
                },
            )
            .await?;
            self.inner
                .store
                .message(&session.id, Message::assistant(&response))
                .await?;
            if self
                .apply_steering(session, run_id, &response.tool_calls, steering)
                .await?
            {
                continue;
            }
            let executed = self
                .execute_response_tools(session, &context, &response, &mut repeats, steering)
                .await;
            if repeats.exhausted {
                summary.status = RunStatus::BudgetExhausted;
            }
            if executed? {
                return Ok(());
            }
        }
        summary.status = RunStatus::BudgetExhausted;
        bail!("step budget exhausted")
    }

    async fn prepare_context(
        &self,
        session: &Session,
        context: &HookContext,
    ) -> Result<Vec<Message>> {
        let messages = self
            .inner
            .store
            .messages(&session.id, self.inner.config.context_max_bytes)
            .await?;
        let before = serde_json::to_vec(&messages)?.len();
        let messages = self
            .inner
            .context
            .prepare(messages, self.inner.config.context_max_bytes)?;
        let messages = self.prepare_hook_context(context, messages).await?;
        let after = serde_json::to_vec(&messages)?.len();
        ensure!(
            after <= self.inner.config.context_max_bytes,
            "context policy exceeded the configured byte budget"
        );
        if after < before {
            self.emit(
                &session.id,
                Some(&context.run_id),
                EventKind::ContextTrimmed {
                    before_bytes: before,
                    after_bytes: after,
                },
            )
            .await?;
        }
        Ok(messages)
    }

    async fn model_step(
        &self,
        session: &Session,
        run: &str,
        step: u32,
        request: ModelRequest,
        cancel: &CancellationToken,
    ) -> Result<ModelResponse> {
        self.inner.model_waiters.fetch_add(1, Ordering::Relaxed);
        let waiting = ModelWaiter(&self.inner.model_waiters);
        let slot = tokio::select! {
            biased;
            () = cancel.cancelled() => bail!("cancelled"),
            slot = self.inner.model_slots.clone().acquire_owned() => slot?,
        };
        drop(waiting);
        self.emit(&session.id, Some(run), EventKind::ModelStarted { step })
            .await?;
        let (deltas, mut receiver) = mpsc::channel(16);
        let selected = self
            .inner
            .provider
            .lock()
            .await
            .provider
            .clone()
            .context("model provider is not configured")?;
        let provider = selected.stream(request, cancel.child_token(), deltas);
        tokio::pin!(provider);
        let mut streamed = String::new();
        let mut response = loop {
            tokio::select! {
                biased;
                () = cancel.cancelled() => bail!("cancelled"),
                delta = receiver.recv(), if !receiver.is_closed() || !receiver.is_empty() => {
                    if let Some(ModelDelta::Text(text)) = delta {
                        ensure!(streamed.len().saturating_add(text.len()) <= 64 * 1024, "stream exceeds output byte budget");
                        streamed.push_str(&text);
                        self.emit_text(&session.id, run, &text).await?;
                    }
                }
                response = &mut provider => break response?,
            }
        };
        while let Ok(ModelDelta::Text(text)) = receiver.try_recv() {
            ensure!(
                streamed.len().saturating_add(text.len()) <= 64 * 1024,
                "stream exceeds output byte budget"
            );
            streamed.push_str(&text);
            self.emit_text(&session.id, run, &text).await?;
        }
        drop(slot);
        validate_response(&response)?;
        // Invocation identity belongs to the harness across concurrent provider sessions.
        for call in &mut response.tool_calls {
            call.id = format!("call_{}", uuid::Uuid::new_v4().simple());
        }
        if streamed.is_empty() && !response.content.is_empty() {
            self.emit_text(&session.id, run, &response.content).await?;
        } else {
            ensure!(
                streamed == response.content,
                "provider deltas disagree with its canonical response"
            );
        }
        Ok(response)
    }

    async fn apply_steering(
        &self,
        session: &Session,
        run: &str,
        calls: &[ToolCall],
        steering: &mut mpsc::Receiver<String>,
    ) -> Result<bool> {
        let mut incoming = vec![];
        while let Ok(text) = steering.try_recv() {
            incoming.push(text);
        }
        if incoming.is_empty() {
            return Ok(false);
        }
        for call in calls {
            self.record_output(
                session,
                run,
                call,
                ToolOutput::error("Skipped because the user steered this turn before execution."),
            )
            .await?;
        }
        for text in incoming {
            self.inner
                .store
                .message(&session.id, Message::user(text))
                .await?;
        }
        Ok(true)
    }

    async fn execute_response_tools(
        &self,
        session: &Session,
        context: &HookContext,
        response: &ModelResponse,
        repeats: &mut RepeatCounts,
        steering: &mut mpsc::Receiver<String>,
    ) -> Result<bool> {
        let run = &context.run_id;
        let cancel = &context.cancel;
        let (complete, calls) = match self.inner.strategy.decide(response)? {
            StepDecision::Complete => (true, vec![]),
            StepDecision::Execute(calls) => (false, calls),
        };
        ensure!(calls.len() <= 16, "strategy exceeded tool call budget");
        let mut selected = std::collections::HashSet::new();
        ensure!(
            calls.iter().all(|call| selected.insert(&call.id)
                && response
                    .tool_calls
                    .iter()
                    .any(|original| original.id == call.id
                        && original.name == call.name
                        && original.arguments == call.arguments)),
            "strategy invented or duplicated an unjournaled tool call"
        );
        for original in &response.tool_calls {
            if !calls.iter().any(|call| call.id == original.id) {
                self.record_output(
                    session,
                    run,
                    original,
                    ToolOutput::error("Skipped by host run strategy."),
                )
                .await?;
            }
        }
        for (index, original) in calls.iter().enumerate() {
            ensure!(!cancel.is_cancelled(), "cancelled");
            if self
                .apply_steering(session, run, &calls[index..], steering)
                .await?
            {
                return Ok(false);
            }
            let (call, skipped) = self
                .prepare_tool_call(context, original.clone())
                .await
                .unwrap_or_else(|error| {
                    (
                        original.clone(),
                        Some(ToolOutput::error(format!("{error:#}"))),
                    )
                });
            ensure!(!cancel.is_cancelled(), "cancelled");
            let signature = format!("{}:{}", call.name, call.arguments);
            let count = repeats.total.entry(signature).or_default();
            *count += 1;
            if *count > self.inner.config.max_repeated_calls {
                repeats.exhausted = true;
                bail!("repeated tool call stopped: {}", call.name);
            }
            self.emit(
                &session.id,
                Some(run),
                EventKind::ToolQueued { call: call.clone() },
            )
            .await?;
            let output = if let Some(output) = skipped {
                output
            } else {
                self.execute_tool(session, run, &call, cancel, steering)
                    .await
                    .unwrap_or_else(|error| ToolOutput::error(format!("{error:#}")))
            };
            if self.inner.hooks.is_empty() {
                self.record_output(session, run, &call, output).await?;
            } else {
                self.record_hooked_output(session, context, &call, output)
                    .await?;
            }
        }
        Ok(complete)
    }

    async fn emit_text(&self, session: &str, run: &str, text: &str) -> Result<()> {
        let mut rest = text;
        while !rest.is_empty() {
            let mut end = rest.len().min(8192);
            while !rest.is_char_boundary(end) {
                end -= 1;
            }
            self.emit(
                session,
                Some(run),
                EventKind::TextDelta {
                    text: rest[..end].into(),
                },
            )
            .await?;
            rest = &rest[end..];
        }
        Ok(())
    }

    async fn execute_tool(
        &self,
        session: &Session,
        run: &str,
        call: &ToolCall,
        cancel: &CancellationToken,
        steering: &mpsc::Receiver<String>,
    ) -> Result<ToolOutput> {
        let tool = self
            .inner
            .tools
            .get(&call.name)
            .context("tool is not granted by this host")?;
        let validator = self
            .inner
            .validators
            .get(&call.name)
            .context("tool schema missing")?;
        if let Err(error) = validator.validate(&call.arguments) {
            bail!("invalid arguments: {error}");
        }
        let lock = if tool.definition().effect == Effect::Read {
            None
        } else {
            let mut writes = self
                .inner
                .writes
                .lock()
                .map_err(|_| anyhow::anyhow!("workspace lock poisoned"))?;
            writes.retain(|_, value| value.strong_count() > 0);
            let lock = writes
                .get(&session.cwd)
                .and_then(std::sync::Weak::upgrade)
                .unwrap_or_else(|| Arc::new(Mutex::new(())));
            writes.insert(session.cwd.clone(), Arc::downgrade(&lock));
            Some(lock)
        };
        let effect_guard = match lock {
            Some(lock) => Some(
                tokio::select! { biased; () = cancel.cancelled() => bail!("cancelled"), guard = lock.lock_owned() => Arc::new(guard) },
            ),
            None => None,
        };
        let lease = if let Some(reservation) = tool.reservation() {
            Some(
                self.inner
                    .governor
                    .acquire(&session.id, reservation, cancel)
                    .await?,
            )
        } else {
            None
        };
        self.emit(
            &session.id,
            Some(run),
            EventKind::ToolStarted { call: call.clone() },
        )
        .await?;
        let context = ToolContext {
            session_id: session.id.clone(),
            run_id: run.into(),
            cwd: session.cwd.clone(),
            cancel: cancel.child_token(),
            governor: self.inner.governor.clone(),
            effect_guard,
            store: self.inner.store.clone(),
            events: self.inner.events.clone(),
        };
        ensure!(
            steering.is_empty(),
            "Skipped because the user steered before tool execution."
        );
        let output = tokio::select! { biased; () = cancel.cancelled() => bail!("cancelled; effect outcome may be uncertain"), output = tool.execute(call.clone(), context) => output };
        drop(lease);
        output
    }

    async fn record_output(
        &self,
        session: &Session,
        run: &str,
        call: &ToolCall,
        mut output: ToolOutput,
    ) -> Result<()> {
        bound_output(&mut output)?;
        self.inner
            .store
            .message(&session.id, Message::tool(call.id.clone(), &output))
            .await?;
        self.emit(
            &session.id,
            Some(run),
            EventKind::ToolFinished {
                call_id: call.id.clone(),
                output,
            },
        )
        .await
    }

    async fn close_pending(&self, session: &str) -> Result<()> {
        for call in self.inner.store.pending_calls(session).await? {
            self.inner.store.message(session, Message::tool(call.id, &ToolOutput::error("Interrupted before a confirmed result. The outcome may be unknown. Inspect before deciding on a new action; no automatic replay occurred."))).await?;
        }
        Ok(())
    }
}

fn validate_response(response: &ModelResponse) -> Result<()> {
    ensure!(
        response.content.len() <= 64 * 1024 && response.tool_calls.len() <= 16,
        "provider response exceeds limit"
    );
    ensure!(
        serde_json::to_vec(&response.continuation)?.len() <= 64 * 1024,
        "provider continuation exceeds limit"
    );
    let mut ids = std::collections::HashSet::new();
    for call in &response.tool_calls {
        ensure!(
            !call.id.is_empty()
                && call.id.len() <= 256
                && call.name.len() <= 64
                && serde_json::to_vec(&call.arguments)?.len() <= 64 * 1024,
            "invalid provider tool call"
        );
        ensure!(ids.insert(&call.id), "duplicate tool call identity");
    }
    Ok(())
}

#[derive(Default)]
struct RepeatCounts {
    total: HashMap<String, u32>,
    exhausted: bool,
}
