//! A reusable agent driver with shared resource admission, durable events, and host-owned tools.
mod config;
mod engine;
mod hooks;
mod policy;
mod provider;
mod store;
mod types;

pub use config::HarnessConfig;
pub use ejm_harness_exec;
pub use engine::{Harness, RunCompletion, RunHandle, Tool, ToolContext};
pub use hooks::{Hook, HookContext, ToolDecision};
pub use policy::{ContextPolicy, RecentContext, RunStrategy, StepDecision, ToolLoop};
pub use provider::{
    AnthropicConfig, AnthropicProvider, AvailableModel, ClaudeCodeConfig, ClaudeCodeProvider,
    HttpObserver, HttpTraceHeader, HttpTracePhase, HttpTraceRecord, OpenAiConfig, OpenAiProvider,
    Provider, REASONING_EFFORTS, ReplayProvider, ResponsesConfig, ResponsesProvider,
};
pub use types::*;
