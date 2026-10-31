//! The optional `shell` tool, run through the shared executor under the host's shell grant.
use std::{collections::BTreeMap, path::PathBuf};

use anyhow::{Context, Result};
use async_trait::async_trait;
use ejm_harness::{Effect, Tool, ToolCall, ToolContext, ToolDefinition, ToolOutput};
use ejm_harness_exec::{
    CommandOutput, CommandRequest, Executor, RequiredCapabilities, Reservation,
};
use serde_json::json;

pub(super) struct ShellTool {
    pub(super) artifact_dir: PathBuf,
    pub(super) require_process_tree_cancellation: bool,
}

#[async_trait]
impl Tool for ShellTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "shell".into(),
            description: "Run a bounded command with the host's granted shell access. Native execution is not a security sandbox. Prefer native tools for file work. Use heavy=true for builds or tests; commands preserve ordinary shell semantics.".into(),
            input_schema: json!({"type":"object","properties":{"command":{"type":"string","minLength":1,"maxLength":16000},"timeout_ms":{"type":"integer","minimum":1,"maximum":120_000},"heavy":{"type":"boolean"}},"required":["command"],"additionalProperties":false}),
            effect: Effect::External,
            version: "1".into(),
        }
    }

    fn reservation(&self) -> Option<Reservation> {
        None
    }

    async fn execute(&self, call: ToolCall, context: ToolContext) -> Result<ToolOutput> {
        let command = call.arguments["command"]
            .as_str()
            .context("command is required")?;
        let (program, args) = shell_invocation(command);
        let retained = context.retain_execution();
        let executor = Executor::new(context.governor, self.artifact_dir.clone())?;
        let output = executor
            .run_with_retained(
                CommandRequest {
                    session_id: context.session_id,
                    program,
                    args,
                    cwd: context.cwd,
                    env: BTreeMap::new(),
                    timeout_ms: call.arguments["timeout_ms"].as_u64().unwrap_or(30_000),
                    reservation: reservation(call.arguments["heavy"].as_bool().unwrap_or(false)),
                    required: RequiredCapabilities {
                        process_tree_cancellation: self.require_process_tree_cancellation,
                        ..Default::default()
                    },
                },
                context.cancel,
                retained,
            )
            .await?;
        Ok(ToolOutput {
            content: render(&output),
            data: json!({"exit_code":output.exit_code,"duration_ms":output.duration_ms,"truncated":output.truncated,"peak_memory_bytes":output.peak_memory_bytes,"peak_processes":output.peak_processes}),
            is_error: output.exit_code != Some(0) || output.cancelled || output.timed_out,
            artifact: output.artifact,
        })
    }
}

/// Heavy commands (builds, tests) reserve more of the shared governor's resources.
fn reservation(heavy: bool) -> Reservation {
    if heavy {
        Reservation {
            cpu_millis: 1500,
            memory_bytes: 384 * 1024 * 1024,
            processes: 16,
            output_bytes: 128 * 1024,
            heavy: true,
        }
    } else {
        Reservation::default()
    }
}

#[cfg(unix)]
fn shell_invocation(command: &str) -> (String, Vec<String>) {
    (
        "/bin/bash".to_owned(),
        vec![
            "--noprofile".to_owned(),
            "--norc".to_owned(),
            "-c".to_owned(),
            command.into(),
        ],
    )
}

#[cfg(windows)]
fn shell_invocation(command: &str) -> (String, Vec<String>) {
    (
        "cmd.exe".to_owned(),
        vec![
            "/D".to_owned(),
            "/S".to_owned(),
            "/C".to_owned(),
            command.into(),
        ],
    )
}

fn render(output: &CommandOutput) -> String {
    format!(
        "exit: {:?} | {} ms{}{}\n{}{}{}",
        output.exit_code,
        output.duration_ms,
        if output.cancelled { " | cancelled" } else { "" },
        if output.timed_out { " | timed out" } else { "" },
        output.stdout,
        if output.stderr.is_empty() {
            ""
        } else {
            "\n[stderr]\n"
        },
        output.stderr
    )
}
