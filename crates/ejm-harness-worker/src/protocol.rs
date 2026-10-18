//! Versioned worker messages. The SDK wire types are generated from this schema.
use std::{fmt, path::PathBuf};

use ejm_harness::{
    Event, HarnessSnapshot, ModelResponse, RunSummary, Session, SessionOptions, ToolDefinition,
    ToolOutput,
};
use ejm_harness_exec::{GovernorSnapshot, ResourceLimits};
use ejm_harness_package_spec::Manifest;
use ejm_harness_packages::PackageGrants;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const PROTOCOL_VERSION: u32 = 1;
pub const MAX_FRAME_BYTES: usize = 1024 * 1024;

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct Request {
    pub id: String,
    #[serde(flatten)]
    pub call: ClientCall,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "method", content = "params")]
pub enum ClientCall {
    #[serde(rename = "hello")]
    Hello(HelloParams),
    #[serde(rename = "configure")]
    Configure(Box<ConfigureParams>),
    #[serde(rename = "session.create")]
    SessionCreate(SessionOptions),
    #[serde(rename = "session.list")]
    SessionList(EmptyParams),
    #[serde(rename = "run.start")]
    RunStart(StartParams),
    #[serde(rename = "run.cancel")]
    RunCancel(SessionParams),
    #[serde(rename = "run.steer")]
    RunSteer(TextParams),
    #[serde(rename = "run.queue")]
    RunQueue(TextParams),
    #[serde(rename = "events.list")]
    EventsList(EventsParams),
    #[serde(rename = "governor.status")]
    GovernorStatus(EmptyParams),
    #[serde(rename = "harness.status")]
    HarnessStatus(EmptyParams),
    #[serde(rename = "package.list")]
    PackageList(PackageListParams),
    #[serde(rename = "package.reload")]
    PackageReload(PackageParams),
    #[serde(rename = "package.command")]
    PackageCommand(PackageInvokeParams),
    #[serde(rename = "package.widget")]
    PackageWidget(PackageInvokeParams),
    #[serde(rename = "shutdown")]
    Shutdown(EmptyParams),
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, JsonSchema)]
pub struct EmptyParams {}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct HelloParams {
    pub protocol: u32,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, JsonSchema)]
pub struct ConfigureParams {
    pub state_dir: Option<PathBuf>,
    pub provider: Option<ProviderConfig>,
    #[serde(default)]
    pub tools: Vec<ToolDefinition>,
    #[serde(default)]
    pub packages: Vec<PackageConfig>,
    #[serde(default)]
    pub allow_write: bool,
    #[serde(default)]
    pub allow_shell: bool,
    pub resources: Option<ResourceLimits>,
    pub max_sessions: Option<usize>,
    pub max_model_requests: Option<usize>,
    pub max_steps: Option<u32>,
    pub context_max_bytes: Option<usize>,
    pub run_timeout_ms: Option<u64>,
    pub max_repeated_calls: Option<u32>,
    pub max_input_tokens: Option<u64>,
    pub max_output_tokens: Option<u64>,
    pub max_store_bytes: Option<u64>,
    pub system_prompt: Option<String>,
}

#[derive(Clone, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProviderConfig {
    Echo,
    Replay {
        responses: Vec<ModelResponse>,
    },
    Openai {
        base_url: String,
        api_key: String,
        model: String,
    },
    Responses {
        api_key: String,
        model: String,
        reasoning_effort: Option<String>,
    },
    Anthropic {
        api_key: String,
        model: String,
    },
    Package {
        package: String,
        name: String,
    },
}

impl fmt::Debug for ProviderConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Echo => formatter.write_str("Echo"),
            Self::Replay { responses } => formatter
                .debug_struct("Replay")
                .field("responses", responses)
                .finish(),
            Self::Openai { model, .. } => formatter
                .debug_struct("Openai")
                .field("model", model)
                .field("api_key", &"[REDACTED]")
                .finish_non_exhaustive(),
            Self::Responses {
                model,
                reasoning_effort,
                ..
            } => formatter
                .debug_struct("Responses")
                .field("model", model)
                .field("reasoning_effort", reasoning_effort)
                .field("api_key", &"[REDACTED]")
                .finish(),
            Self::Anthropic { model, .. } => formatter
                .debug_struct("Anthropic")
                .field("model", model)
                .field("api_key", &"[REDACTED]")
                .finish(),
            Self::Package { package, name } => formatter
                .debug_struct("Package")
                .field("package", package)
                .field("name", name)
                .finish(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub struct PackageConfig {
    pub root: PathBuf,
    pub grants: PackageGrants,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub struct PackageInfo {
    pub root: PathBuf,
    pub manifest: Manifest,
    pub grants: PackageGrants,
}

#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
pub struct PackageListParams {
    #[serde(default)]
    pub after: usize,
    #[serde(default = "default_package_limit")]
    pub limit: usize,
}

fn default_package_limit() -> usize {
    16
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct PackagePage {
    pub packages: Vec<PackageInfo>,
    pub next: Option<usize>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct PackageParams {
    pub package: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct PackageInvokeParams {
    pub package: String,
    pub name: String,
    pub session_id: String,
    #[serde(default)]
    pub arguments: Value,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct StartParams {
    pub session_id: String,
    pub prompt: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SessionParams {
    pub session_id: String,
    pub run_id: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct TextParams {
    pub session_id: String,
    pub run_id: Option<String>,
    pub text: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct EventsParams {
    pub session_id: String,
    #[serde(default)]
    pub after: u64,
    #[serde(default = "default_page_limit")]
    pub limit: usize,
}

fn default_page_limit() -> usize {
    64
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ProtocolError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum ResponsePayload {
    Success { result: Value },
    Failure { error: ProtocolError },
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct Response {
    pub id: String,
    #[serde(flatten)]
    pub payload: ResponsePayload,
}

impl Response {
    /// Construct a response with any serializable result.
    ///
    /// # Errors
    /// Returns an error when the result cannot be serialized as JSON.
    pub fn ok(id: String, result: impl Serialize) -> anyhow::Result<Self> {
        Ok(Self {
            id,
            payload: ResponsePayload::Success {
                result: serde_json::to_value(result)?,
            },
        })
    }

    pub fn error(id: String, code: &str, message: impl Into<String>) -> Self {
        Self {
            id,
            payload: ResponsePayload::Failure {
                error: ProtocolError {
                    code: code.into(),
                    message: message.into(),
                },
            },
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
pub struct ToolInvocation {
    pub session_id: String,
    pub run_id: String,
    pub call_id: String,
    pub name: String,
    pub arguments: Value,
    pub deadline_ms: u64,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ToolProgress {
    pub call_id: String,
    pub message: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ToolCancellation {
    pub call_id: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "method", content = "params")]
pub enum Notification {
    #[serde(rename = "event")]
    Event(Event),
    #[serde(rename = "tool.cancel")]
    ToolCancel(ToolCancellation),
    #[serde(rename = "tool.progress")]
    ToolProgress(ToolProgress),
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ToolRequest {
    pub id: String,
    pub method: ToolMethod,
    pub params: ToolInvocation,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub enum ToolMethod {
    #[serde(rename = "tool.invoke")]
    Invoke,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct RunStarted {
    pub run_id: String,
    pub session_id: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct HelloResult {
    pub protocol: u32,
    pub version: String,
    pub capabilities: Vec<String>,
}

/// Collect every public wire type in one authoritative JSON Schema.
#[derive(JsonSchema)]
#[expect(
    dead_code,
    reason = "schema-only container makes all public wire types reachable"
)]
pub struct ProtocolSchema {
    request: Request,
    response: Response,
    notification: Notification,
    tool_request: ToolRequest,
    tool_output: ToolOutput,
    session: Session,
    run_started: RunStarted,
    run_summary: RunSummary,
    governor: GovernorSnapshot,
    harness: HarnessSnapshot,
    hello: HelloResult,
    package_page: PackagePage,
}

#[must_use]
pub fn schema() -> schemars::Schema {
    schemars::schema_for!(ProtocolSchema)
}
