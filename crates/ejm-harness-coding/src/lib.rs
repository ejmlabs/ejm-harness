//! Native coding tools with explicit effect grants. Shell access is a separate host grant.
mod edit;
mod list;
mod native;
mod path;
mod read;
mod search;
mod shell;

use std::{path::PathBuf, sync::Arc};

use anyhow::Result;
use ejm_harness::Tool;

use native::{Kind, NativeTool};
use shell::ShellTool;

/// Visible output limit shared by file reads, listings, and search results.
const MAX_RETRIEVAL_BYTES: usize = 14 * 1024;

#[derive(Clone)]
pub struct CodingConfig {
    pub allow_write: bool,
    pub allow_shell: bool,
    pub artifact_dir: PathBuf,
}

impl Default for CodingConfig {
    fn default() -> Self {
        Self {
            allow_write: false,
            allow_shell: false,
            artifact_dir: PathBuf::from(".ejm-harness/artifacts"),
        }
    }
}

/// Registering these tools grants their effects. No repository file can add a grant.
///
/// # Errors
/// Reserved for invalid coding tool configuration.
pub fn tools(config: CodingConfig) -> Result<Vec<Arc<dyn Tool>>> {
    tools_with_shell_process_tree_cancellation(config, false)
}

/// Register coding tools with optional enforced shell process-tree cancellation.
///
/// # Errors
/// Reserved for invalid coding tool configuration.
pub fn tools_with_shell_process_tree_cancellation(
    config: CodingConfig,
    require_process_tree_cancellation: bool,
) -> Result<Vec<Arc<dyn Tool>>> {
    let mut result: Vec<Arc<dyn Tool>> = vec![
        Arc::new(NativeTool(Kind::Read)),
        Arc::new(NativeTool(Kind::List)),
        Arc::new(NativeTool(Kind::Search)),
    ];
    if config.allow_write {
        result.extend([
            Arc::new(NativeTool(Kind::Write)) as Arc<dyn Tool>,
            Arc::new(NativeTool(Kind::Patch)),
        ]);
    }
    if config.allow_shell {
        result.push(Arc::new(ShellTool {
            artifact_dir: config.artifact_dir,
            require_process_tree_cancellation,
        }));
    }
    Ok(result)
}

/// Output lines that stop growing before their newline-joined text passes [`MAX_RETRIEVAL_BYTES`].
#[derive(Default)]
struct BoundedLines {
    lines: Vec<String>,
    bytes: usize,
}

impl BoundedLines {
    /// Adds the line unless it would push the joined output past the limit.
    fn push(&mut self, line: String) -> bool {
        let bytes = line.len() + usize::from(!self.lines.is_empty());
        if self.bytes + bytes > MAX_RETRIEVAL_BYTES {
            return false;
        }
        self.bytes += bytes;
        self.lines.push(line);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Context;
    use async_trait::async_trait;
    use ejm_harness::{
        Effect, EventKind, Harness, HarnessConfig, ModelDelta, ModelRequest, ModelResponse,
        Provider, Role, RunStatus, SessionOptions, ToolCall,
    };
    use serde_json::json;

    struct ShellProvider;

    #[async_trait]
    impl Provider for ShellProvider {
        async fn stream(
            &self,
            request: ModelRequest,
            _cancel: tokio_util::sync::CancellationToken,
            _deltas: tokio::sync::mpsc::Sender<ModelDelta>,
        ) -> Result<ModelResponse> {
            if request
                .messages
                .last()
                .is_some_and(|m| m.role == Role::Tool)
            {
                Ok(ModelResponse {
                    content: "Done.".into(),
                    ..Default::default()
                })
            } else {
                Ok(ModelResponse {
                    tool_calls: vec![ToolCall {
                        id: "shell-fixture".into(),
                        name: "shell".into(),
                        arguments: json!({"command":"echo marker > marker"}),
                    }],
                    ..Default::default()
                })
            }
        }
    }

    #[tokio::test]
    async fn shell_requirements_fail_before_launch_and_default_remains_available() -> Result<()> {
        for (explicit, strict) in [(false, false), (true, false), (true, true)] {
            let temp = tempfile::tempdir()?;
            let workspace = temp.path().join("workspace");
            std::fs::create_dir(&workspace)?;
            let config = CodingConfig {
                allow_shell: true,
                artifact_dir: temp.path().join("artifacts"),
                ..Default::default()
            };
            std::fs::create_dir(&config.artifact_dir)?;
            let registered = if explicit {
                tools_with_shell_process_tree_cancellation(config, strict)?
            } else {
                tools(config)?
            };
            let harness = Harness::new(
                HarnessConfig {
                    state_dir: temp.path().join("state"),
                    max_steps: 2,
                    run_timeout_ms: 5000,
                    ..Default::default()
                },
                Arc::new(ShellProvider),
                registered,
            )?;
            let session = harness
                .create_session(SessionOptions {
                    cwd: workspace.clone(),
                    title: "shell requirement".into(),
                })
                .await?;
            let mut run = harness
                .start(&session.id, "Run the shell fixture".into())
                .await?;
            let result = run.wait().await?;
            harness.shutdown().await?;
            assert_eq!(result.status, RunStatus::Completed, "{:?}", result.error);
            let output = harness
                .events(&session.id, 0, 100)
                .await?
                .into_iter()
                .find_map(|event| match event.kind {
                    EventKind::ToolFinished { output, .. } => Some(output),
                    _ => None,
                })
                .context("shell result was not journaled")?;
            assert_eq!(workspace.join("marker").exists(), !strict);
            assert_eq!(output.is_error, strict);
            if strict {
                assert!(output.content.contains("process_tree_cancellation"));
            }
        }
        Ok(())
    }

    #[test]
    fn effect_grants_are_explicit() -> Result<()> {
        let read = tools(CodingConfig::default())?;
        assert!(read.iter().all(|t| t.definition().effect == Effect::Read));
        assert_eq!(read.len(), 3);
        Ok(())
    }
}
