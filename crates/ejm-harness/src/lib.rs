//! A reusable agent driver with shared resource admission, durable events, and host-owned tools.
mod capacity;
mod config;
mod engine;
mod hooks;
mod policy;
mod provider;
mod store;
mod types;

pub use capacity::{
    CapacityContext, CapacityMode, CapacityPolicy, CapacitySnapshot, LaneSnapshot, LearningState,
    LearningStatus, NativeHelperSnapshot, ProviderSnapshot, TimingSummary, reset_capacity_learning,
};
pub use config::HarnessConfig;
pub use ejm_harness_exec;
pub use engine::{
    EventPage, Harness, JournalSnapshot, RunCompletion, RunHandle, SchedulingSnapshot,
    SessionCursor, SessionEvent, SessionPage, SessionQuery, SessionRunTransition,
    SessionSubscription, SessionTailOptions, SessionView, Tool, ToolContext,
};
pub use hooks::{Hook, HookContext, ToolDecision};
pub use policy::{ContextPolicy, RecentContext, RunStrategy, StepDecision, ToolLoop};
#[cfg(feature = "test-support")]
pub use provider::ReplayProvider;
pub use provider::{
    AnthropicConfig, AnthropicProvider, AvailableModel, ClaudeCodeConfig, ClaudeCodeProvider,
    HttpObserver, HttpTraceHeader, HttpTracePhase, HttpTraceRecord, OpenAiConfig, OpenAiProvider,
    Provider, ProviderCapacity, ProviderCapacityObserver, ProviderObservation, ProviderRate,
    ProviderRateWindow, ProviderRetry, QuotaDomain, REASONING_EFFORTS, ResponsesConfig,
    ResponsesProvider,
};
pub use types::*;
