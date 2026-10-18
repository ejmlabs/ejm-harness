use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use anyhow::Result;
use async_trait::async_trait;
use ejm_harness::{
    EventKind, Harness, HarnessConfig, ModelDelta, ModelRequest, ModelResponse, Provider, Role,
    RunStatus, SessionOptions, ToolCall,
};
use ejm_harness_exec::ResourceLimits;
use ejm_harness_package_spec::Manifest;
use ejm_harness_packages::{
    HookEvent, InstalledPackage, PackageGrants, RuntimeConfig, RuntimePool,
};
use serde_json::json;
use tokio::{sync::mpsc, time::timeout};
use tokio_util::sync::CancellationToken;

const NODE: &str = r"
import {writeFileSync} from 'node:fs';
export async function inspect(args, ctx) {
  await ctx.progress('inspecting');
  return {content: 'value:' + args.value};
}
export function run_start(args, ctx) { return args.prompt + '|prepared'; }
export function before_model(args, ctx) {
  if ('governor' in ctx || 'cancel' in ctx) throw new Error('runtime capability leaked');
  return args.messages;
}
export function before_tool(args, ctx) { return {action:'execute', arguments:{value:'changed'}}; }
export function after_tool(args, ctx) { return {...args.output, content:args.output.content+'|observed'}; }
export function run_end(args, ctx) { writeFileSync(ctx.cwd + '/ended', args.summary.status); }
";

const PYTHON: &str = r"
from pathlib import Path
async def inspect(args, ctx):
    await ctx.progress('inspecting')
    return {'content': 'value:' + args['value']}
def run_start(args, ctx):
    return args['prompt'] + '|prepared'
def before_model(args, ctx):
    assert 'governor' not in ctx and 'cancel' not in ctx
    return args['messages']
def before_tool(args, ctx):
    return {'action':'execute', 'arguments':{'value':'changed'}}
def after_tool(args, ctx):
    return {**args['output'], 'content':args['output']['content']+'|observed'}
def run_end(args, ctx):
    Path(ctx['cwd'], 'ended').write_text(args['summary']['status'])
";

struct CheckingProvider(AtomicUsize);

#[async_trait]
impl Provider for CheckingProvider {
    async fn stream(
        &self,
        request: ModelRequest,
        _cancel: CancellationToken,
        _deltas: mpsc::Sender<ModelDelta>,
    ) -> Result<ModelResponse> {
        assert!(
            request
                .messages
                .iter()
                .any(|message| message.role == Role::User && message.content == "inspect|prepared")
        );
        if self.0.fetch_add(1, Ordering::SeqCst) == 0 {
            Ok(ModelResponse {
                tool_calls: vec![ToolCall {
                    id: "call".into(),
                    name: "inspect".into(),
                    arguments: json!({"value":"original"}),
                }],
                ..Default::default()
            })
        } else {
            assert!(
                request
                    .messages
                    .iter()
                    .any(|message| message.role == Role::Tool
                        && message.content == "value:changed|observed")
            );
            Ok(ModelResponse {
                content: "done".into(),
                ..Default::default()
            })
        }
    }
}

async fn exercise(kind: &str, entry: &str, source: &str) -> Result<()> {
    let root = tempfile::tempdir()?;
    std::fs::write(root.path().join(entry), source)?;
    let events = [
        HookEvent::RunStart,
        HookEvent::BeforeModel,
        HookEvent::BeforeTool,
        HookEvent::AfterTool,
        HookEvent::RunEnd,
    ];
    let hook_exports = [
        "run_start",
        "before_model",
        "before_tool",
        "after_tool",
        "run_end",
    ]
    .iter()
    .map(|event| json!({"event":event,"handler":event}))
    .collect::<Vec<_>>();
    let manifest: Manifest = serde_json::from_value(json!({
        "schema_version":1,"name":"@tests/adapters","version":"1.0.0","description":"Native port fixture",
        "runtime":{"kind":kind,"entry":entry},
        "exports":{
            "tools":[{"name":"inspect","description":"Inspect a fixture","input_schema":{"type":"object","properties":{"value":{"type":"string"}},"required":["value"],"additionalProperties":false},"effect":"write","version":"1.0.0","handler":"inspect"}],
            "hooks":hook_exports
        }
    }))?;
    let pool = RuntimePool::new(RuntimeConfig {
        node: std::env::var_os("EJM_HARNESS_TEST_NODE")
            .unwrap_or_else(|| "node".into())
            .into(),
        call_timeout: Duration::from_secs(5),
        ..Default::default()
    })?;
    pool.load(
        InstalledPackage {
            manifest,
            root: root.path().into(),
        },
        PackageGrants {
            tools: vec!["inspect".into()],
            hooks: events.to_vec(),
            ..Default::default()
        },
        CancellationToken::new(),
    )
    .await?;
    let harness = Harness::with_hooks(
        HarnessConfig {
            state_dir: root.path().join("state"),
            resources: ResourceLimits {
                max_jobs: 1,
                ..Default::default()
            },
            ..Default::default()
        },
        Arc::new(CheckingProvider(AtomicUsize::new(0))),
        pool.tools("@tests/adapters")?,
        pool.hooks("@tests/adapters")?,
    )?;
    let session = harness
        .create_session(SessionOptions {
            cwd: root.path().into(),
            ..Default::default()
        })
        .await?;
    let summary = timeout(Duration::from_secs(10), async {
        harness
            .start(&session.id, "inspect".into())
            .await?
            .wait()
            .await
    })
    .await??;
    assert_eq!(summary.status, RunStatus::Completed, "{:?}", summary.error);
    let journal = harness.events(&session.id, 0, 100).await?;
    assert!(journal.iter().any(|event| matches!(&event.kind, EventKind::Progress { message, .. } if message == "inspecting")), "{journal:?}");
    assert!(journal.iter().any(|event| matches!(&event.kind, EventKind::ToolFinished { output, .. } if output.content == "value:changed|observed" && !output.is_error)));
    harness.shutdown().await?;
    assert_eq!(
        std::fs::read_to_string(root.path().join("ended"))?,
        "completed"
    );
    pool.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn native_tools_and_all_hook_events_use_existing_harness_ports() -> Result<()> {
    exercise("node", "entry.mjs", NODE).await?;
    exercise("python", "entry.py", PYTHON).await
}
