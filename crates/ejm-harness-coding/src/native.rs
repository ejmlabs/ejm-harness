//! The file tools: one definition table and one blocking executor behind the shared governor.
use anyhow::{Context, Result, ensure};
use async_trait::async_trait;
use cap_std::{ambient_authority, fs::Dir};
use ejm_harness::{Effect, Tool, ToolCall, ToolContext, ToolDefinition, ToolOutput};
use ejm_harness_exec::Reservation;
use serde_json::{Value, json};

use super::{
    edit::{Edit, edit},
    list::list_files,
    read::read_file,
    search::search,
};

#[derive(Clone, Copy)]
pub(super) enum Kind {
    Read,
    List,
    Search,
    Write,
    Patch,
}

pub(super) struct NativeTool(pub(super) Kind);

fn tool_definition(
    name: &str,
    description: &str,
    properties: &Value,
    required: &[&str],
    effect: Effect,
) -> ToolDefinition {
    ToolDefinition {
        name: name.into(),
        description: description.into(),
        input_schema: json!({"type":"object","properties":properties,"required":required,"additionalProperties":false}),
        effect,
        version: "1".into(),
    }
}

#[async_trait]
impl Tool for NativeTool {
    fn definition(&self) -> ToolDefinition {
        match self.0 {
            Kind::Read => tool_definition(
                "read_file",
                "Read UTF-8 text with line numbers and a SHA-256 precondition for edits. Paths stay inside the session directory.",
                &json!({"path":{"type":"string"},"offset":{"type":"integer","minimum":1},"limit":{"type":"integer","minimum":1,"maximum":400}}),
                &["path"],
                Effect::Read,
            ),
            Kind::List => tool_definition(
                "list_files",
                "List entries in a directory inside this workspace, bounded to 200 entries and 14 KiB of entry text. Reports incomplete listings; narrow path to inspect a subdirectory.",
                &json!({"path":{"type":"string"}}),
                &[],
                Effect::Read,
            ),
            Kind::Search => tool_definition(
                "search",
                "Search a Rust regular expression in text files. Respects gitignore; bounded to 32 levels, 100 hits, 20000 entries, 8 MiB read and 14 KiB of match text. Skips non-UTF-8 files and files over 256 KiB. Reports incomplete results; narrow path or pattern.",
                &json!({"pattern":{"type":"string","maxLength":1024},"path":{"type":"string"},"limit":{"type":"integer","minimum":1,"maximum":100}}),
                &["pattern"],
                Effect::Read,
            ),
            Kind::Write => tool_definition(
                "write_file",
                "Create a file, or replace a file only when expected_hash from read_file matches. Parent directory must exist. Atomic replacement.",
                &json!({"path":{"type":"string"},"content":{"type":"string","maxLength":60000},"expected_hash":{"type":"string","pattern":"^[a-f0-9]{64}$"}}),
                &["path", "content"],
                Effect::Write,
            ),
            Kind::Patch => tool_definition(
                "patch_file",
                "Replace exactly one matching old string. expected_hash from read_file is required. Preserves a trailing newline when supplied by the replacement.",
                &json!({"path":{"type":"string"},"old":{"type":"string","minLength":1},"new":{"type":"string"},"expected_hash":{"type":"string","pattern":"^[a-f0-9]{64}$"}}),
                &["path", "old", "new", "expected_hash"],
                Effect::Write,
            ),
        }
    }

    fn reservation(&self) -> Option<Reservation> {
        None
    }

    async fn execute(&self, call: ToolCall, context: ToolContext) -> Result<ToolOutput> {
        // Keep the lease inside blocking work: dropping a future must not make an unfinished search free.
        let lease = context
            .governor
            .acquire(&context.session_id, Reservation::default(), &context.cancel)
            .await?;
        let kind = self.0;
        let retained = context.retain_execution();
        tokio::task::spawn_blocking(move || {
            let _retained = retained;
            let _lease = lease;
            ensure!(!context.cancel.is_cancelled(), "cancelled");
            let root = Dir::open_ambient_dir(&context.cwd, ambient_authority())
                .context("cannot open workspace")?;
            let arguments = &call.arguments;
            match kind {
                Kind::Read => read_file(&root, arguments),
                Kind::List => list_files(&root, arguments),
                Kind::Search => search(&root, &context.cwd, arguments, &context.cancel),
                Kind::Write => edit(&root, arguments, Edit::Write, &context.cancel),
                Kind::Patch => edit(&root, arguments, Edit::Patch, &context.cancel),
            }
        })
        .await
        .context("native tool worker stopped")?
    }
}
