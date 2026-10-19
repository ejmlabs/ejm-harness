//! Native coding tools with explicit effect grants. Shell access is a separate host grant.
use std::fmt::Write as _;
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context, Result, ensure};
use async_trait::async_trait;
use cap_std::{ambient_authority, fs::Dir};
use ejm_harness::{Effect, Tool, ToolCall, ToolContext, ToolDefinition, ToolOutput};
use ejm_harness_exec::{CommandRequest, Executor, Reservation};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const MAX_FILE_BYTES: u64 = 1024 * 1024;
const MAX_SEARCH_BYTES: usize = 8 * 1024 * 1024;
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
        }));
    }
    Ok(result)
}

#[derive(Clone, Copy)]
enum Kind {
    Read,
    List,
    Search,
    Write,
    Patch,
}
struct NativeTool(Kind);

#[async_trait]
impl Tool for NativeTool {
    fn definition(&self) -> ToolDefinition {
        let (name, description, properties, required, effect) = match self.0 {
            Kind::Read => (
                "read_file",
                "Read UTF-8 text with line numbers and a SHA-256 precondition for edits. Paths stay inside the session directory.",
                json!({"path":{"type":"string"},"offset":{"type":"integer","minimum":1},"limit":{"type":"integer","minimum":1,"maximum":400}}),
                vec!["path"],
                Effect::Read,
            ),
            Kind::List => (
                "list_files",
                "List entries in a directory inside this workspace, bounded to 200 entries and 14 KiB of entry text. Reports incomplete listings; narrow path to inspect a subdirectory.",
                json!({"path":{"type":"string"}}),
                vec![],
                Effect::Read,
            ),
            Kind::Search => (
                "search",
                "Search a Rust regular expression in text files. Respects gitignore; bounded to 100 hits, 20000 entries, 8 MiB read and 14 KiB of match text. Skips non-UTF-8 files and files over 256 KiB. Reports incomplete results; narrow path or pattern.",
                json!({"pattern":{"type":"string","maxLength":1024},"path":{"type":"string"},"limit":{"type":"integer","minimum":1,"maximum":100}}),
                vec!["pattern"],
                Effect::Read,
            ),
            Kind::Write => (
                "write_file",
                "Create a file, or replace a file only when expected_hash from read_file matches. Parent directory must exist. Atomic replacement.",
                json!({"path":{"type":"string"},"content":{"type":"string","maxLength":60000},"expected_hash":{"type":"string","pattern":"^[a-f0-9]{64}$"}}),
                vec!["path", "content"],
                Effect::Write,
            ),
            Kind::Patch => (
                "patch_file",
                "Replace exactly one matching old string. expected_hash from read_file is required. Preserves a trailing newline when supplied by the replacement.",
                json!({"path":{"type":"string"},"old":{"type":"string","minLength":1},"new":{"type":"string"},"expected_hash":{"type":"string","pattern":"^[a-f0-9]{64}$"}}),
                vec!["path", "old", "new", "expected_hash"],
                Effect::Write,
            ),
        };
        ToolDefinition {
            name: name.into(),
            description: description.into(),
            input_schema: json!({"type":"object","properties":properties,"required":required,"additionalProperties":false}),
            effect,
            version: "1".into(),
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
            match kind {
                Kind::Read => read_file(&root, &call.arguments),
                Kind::List => list_files(&root, &call.arguments),
                Kind::Search => search(&root, &context.cwd, &call.arguments, &context.cancel),
                Kind::Write | Kind::Patch => edit(
                    &root,
                    &call.arguments,
                    matches!(kind, Kind::Patch),
                    &context.cancel,
                ),
            }
        })
        .await
        .context("native tool worker stopped")?
    }
}

fn relative_path(arguments: &Value, key: &str, default: Option<&str>) -> Result<PathBuf> {
    let text = arguments[key]
        .as_str()
        .or(default)
        .context("path is required")?;
    ensure!(text.len() <= 4096 && !text.contains('\0'), "invalid path");
    let path = PathBuf::from(text);
    ensure!(
        !path.is_absolute()
            && !path.components().any(|c| matches!(
                c,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )),
        "path must stay inside the workspace"
    );
    Ok(path)
}

fn load(root: &Dir, path: &Path) -> Result<Vec<u8>> {
    let file = root
        .open(path)
        .with_context(|| format!("cannot open {}", path.display()))?;
    ensure!(file.metadata()?.is_file(), "path is not a regular file");
    ensure!(
        file.metadata()?.len() <= MAX_FILE_BYTES,
        "file exceeds 1 MiB native-tool limit; use a scoped shell command if granted"
    );
    let mut bytes = vec![];
    file.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= MAX_FILE_BYTES,
        "file grew beyond the native-tool limit"
    );
    Ok(bytes)
}

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn read_file(root: &Dir, arguments: &Value) -> Result<ToolOutput> {
    let path = relative_path(arguments, "path", None)?;
    let bytes = load(root, &resolve_read_path(root, &path)?)?;
    let text = std::str::from_utf8(&bytes).context("file is not UTF-8 text")?;
    let offset = usize::try_from(arguments["offset"].as_u64().unwrap_or(1).max(1))
        .context("line offset exceeds platform range")?;
    let limit = arguments["limit"].as_u64().unwrap_or(200).min(400) as usize;
    let mut content = String::new();
    let mut emitted = 0;
    let total = text.lines().count();
    for (index, line) in text.lines().enumerate().skip(offset - 1).take(limit) {
        let rendered = format!("{}: {}\n", index + 1, line);
        if content.len() + rendered.len() > MAX_RETRIEVAL_BYTES {
            break;
        }
        content.push_str(&rendered);
        emitted += 1;
    }
    let digest = hash(&bytes);
    let truncated = offset.saturating_sub(1).saturating_add(emitted) < total;
    write!(
        content,
        "\nsha256: {digest} | {total} lines{}",
        if truncated {
            " | more lines available"
        } else {
            ""
        }
    )?;
    Ok(ToolOutput {
        content,
        data: json!({"path":path,"sha256":digest,"lines":total,"offset":offset,"returned":emitted,"truncated":truncated}),
        ..Default::default()
    })
}

fn list_files(root: &Dir, arguments: &Value) -> Result<ToolOutput> {
    let path = relative_path(arguments, "path", Some("."))?;
    let directory = root.open_dir(resolve_read_path(root, &path)?)?;
    let mut entries = vec![];
    let mut output_bytes = 0;
    let mut limit_reached = None;
    for entry in directory.entries()? {
        if entries.len() >= 200 {
            limit_reached = Some("200-entry limit");
            break;
        }
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if protected_path(Path::new(&name)) {
            continue;
        }
        let rendered = format!(
            "{}{}",
            name,
            if entry.file_type()?.is_dir() { "/" } else { "" }
        );
        let bytes = rendered.len() + usize::from(!entries.is_empty());
        if output_bytes + bytes > MAX_RETRIEVAL_BYTES {
            limit_reached = Some("14 KiB output limit");
            break;
        }
        output_bytes += bytes;
        entries.push(rendered);
    }
    entries.sort();
    let mut content = entries.join("\n");
    if let Some(reason) = limit_reached {
        write!(
            content,
            "\n\n[Listing incomplete: {reason} reached. Narrow path to inspect a subdirectory.]"
        )?;
    }
    Ok(ToolOutput {
        content,
        data: json!({"path":path,"truncated":limit_reached.is_some()}),
        ..Default::default()
    })
}

fn search(
    root: &Dir,
    cwd: &Path,
    arguments: &Value,
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<ToolOutput> {
    let path = relative_path(arguments, "path", Some("."))?;
    let path = resolve_read_path(root, &path)?;
    let pattern = arguments["pattern"]
        .as_str()
        .context("pattern is required")?;
    ensure!(pattern.len() <= 1024, "pattern exceeds limit");
    let regex = regex::RegexBuilder::new(pattern)
        .size_limit(1024 * 1024)
        .build()
        .context("invalid regular expression")?;
    let limit = arguments["limit"].as_u64().unwrap_or(50).clamp(1, 100) as usize;
    // The ambient walk only discovers names. Every file read is re-opened through the capability.
    let start = cwd.join(&path);
    let resolved = std::fs::canonicalize(&start)?;
    ensure!(resolved.starts_with(cwd), "search path escapes workspace");
    let walker = ignore::WalkBuilder::new(start)
        .follow_links(false)
        .max_depth(Some(32))
        .build();
    let mut bytes_read = 0;
    let mut visited = 0;
    let mut matches = vec![];
    let mut output_bytes = 0;
    let mut limit_reached = None;
    let mut skipped = 0;
    'scan: for entry in walker {
        ensure!(!cancel.is_cancelled(), "cancelled");
        visited += 1;
        if visited > 20_000 || bytes_read >= MAX_SEARCH_BYTES {
            limit_reached = Some(if visited > 20_000 {
                "20000-entry scan limit"
            } else {
                "8 MiB scan limit"
            });
            break;
        }
        let Ok(entry) = entry else {
            skipped += 1;
            continue;
        };
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let relative = entry.path().strip_prefix(cwd)?;
        if protected_path(relative) {
            continue;
        }
        let Ok(resolved) = resolve_read_path(root, relative) else {
            skipped += 1;
            continue;
        };
        let Ok(file) = root.open(resolved) else {
            skipped += 1;
            continue;
        };
        if file.metadata()?.len() > 256 * 1024 {
            skipped += 1;
            continue;
        }
        let mut bytes = vec![];
        file.take((256 * 1024 + 1).min(MAX_SEARCH_BYTES - bytes_read) as u64)
            .read_to_end(&mut bytes)?;
        bytes_read += bytes.len();
        let Ok(text) = std::str::from_utf8(&bytes) else {
            skipped += 1;
            continue;
        };
        for (index, line) in text.lines().enumerate() {
            if regex.is_match(line) {
                let line: String = line.chars().take(240).collect();
                let rendered = format!("{}:{}:{line}", relative.display(), index + 1);
                let bytes = rendered.len() + usize::from(!matches.is_empty());
                if output_bytes + bytes > MAX_RETRIEVAL_BYTES {
                    limit_reached = Some("14 KiB output limit");
                    break 'scan;
                }
                output_bytes += bytes;
                matches.push(rendered);
                if matches.len() == limit {
                    limit_reached = Some("match limit");
                    break 'scan;
                }
            }
        }
    }
    search_output(&matches, bytes_read, visited, skipped, limit_reached)
}

fn search_output(
    matches: &[String],
    bytes_read: usize,
    visited: usize,
    skipped: usize,
    limit_reached: Option<&str>,
) -> Result<ToolOutput> {
    let mut content = if matches.is_empty() {
        if limit_reached.is_some() || skipped > 0 {
            "No matches in the scanned text.".into()
        } else {
            "No matches.".into()
        }
    } else {
        matches.join("\n")
    };
    if let Some(reason) = limit_reached {
        write!(
            content,
            "\n\n[Search incomplete: {reason} reached. Narrow path or pattern.]"
        )?;
    }
    if skipped > 0 {
        write!(
            content,
            "\n\n[Skipped {skipped} entries: unreadable, over 256 KiB, or non-UTF-8. Results cover only scanned text; use read_file for a known text file.]"
        )?;
    }
    Ok(ToolOutput {
        content,
        data: json!({"matches":matches.len(),"bytes_read":bytes_read,"visited":visited,"skipped":skipped,"truncated":limit_reached.is_some()}),
        ..Default::default()
    })
}

fn protected_path(path: &Path) -> bool {
    path.components().any(|component| {
        let name = component.as_os_str().to_string_lossy();
        name.eq_ignore_ascii_case(".git") || name.eq_ignore_ascii_case(".ejm-harness")
    })
}

fn resolve_read_path(root: &Dir, path: &Path) -> Result<PathBuf> {
    ensure!(
        !protected_path(path),
        "tool cannot read repository metadata or harness state"
    );
    let resolved = root.canonicalize(path)?;
    ensure!(
        !protected_path(&resolved),
        "resolved path is protected metadata"
    );
    Ok(resolved)
}

fn validate_edit_path(root: &Dir, path: &Path) -> Result<()> {
    ensure!(
        !protected_path(path),
        "tool cannot edit repository metadata or harness state"
    );
    let parent = root.canonicalize(
        path.parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or(Path::new(".")),
    )?;
    ensure!(
        !protected_path(&parent),
        "resolved parent is protected metadata"
    );
    let mut prefix = PathBuf::new();
    for component in path.components() {
        prefix.push(component);
        match root.symlink_metadata(&prefix) {
            Ok(metadata) => ensure!(
                !metadata.file_type().is_symlink(),
                "write paths cannot contain symbolic links"
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && prefix == path => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn edit(
    root: &Dir,
    arguments: &Value,
    replace_match: bool,
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<ToolOutput> {
    let path = relative_path(arguments, "path", None)?;
    validate_edit_path(root, &path)?;
    let current = match load(root, &path) {
        Ok(bytes) => Some(bytes),
        Err(error)
            if error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) =>
        {
            None
        }
        Err(error) => return Err(error),
    };
    let expected = arguments["expected_hash"].as_str();
    if let Some(bytes) = &current {
        ensure!(
            expected == Some(hash(bytes).as_str()),
            "file changed or expected_hash is missing; read it before editing"
        );
    } else {
        ensure!(
            expected.is_none() && !replace_match,
            "expected file is missing; read workspace state again"
        );
    }
    let content = if replace_match {
        let original = std::str::from_utf8(current.as_deref().context("file does not exist")?)
            .context("file is not UTF-8")?;
        let old = arguments["old"].as_str().context("old is required")?;
        let new = arguments["new"].as_str().context("new is required")?;
        ensure!(
            !old.is_empty() && original.match_indices(old).count() == 1,
            "old text must match exactly once"
        );
        original.replacen(old, new, 1)
    } else {
        arguments["content"]
            .as_str()
            .context("content is required")?
            .to_owned()
    };
    ensure!(
        content.len() as u64 <= MAX_FILE_BYTES,
        "replacement exceeds file limit"
    );
    ensure!(!cancel.is_cancelled(), "cancelled before write");
    if current.is_none() {
        let mut options = cap_std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        let mut file = root
            .open_with(&path, &options)
            .context("file already exists or parent is unavailable")?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
    } else {
        let temp = path
            .parent()
            .unwrap_or(Path::new("."))
            .join(format!(".ejm-harness-edit-{}", uuid::Uuid::new_v4()));
        let result = (|| -> Result<()> {
            let mut options = cap_std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            let mut file = root.open_with(&temp, &options)?;
            file.set_permissions(root.metadata(&path)?.permissions())?;
            file.write_all(content.as_bytes())?;
            file.sync_all()?;
            ensure!(!cancel.is_cancelled(), "cancelled before replacement");
            ensure!(
                hash(&load(root, &path)?) == expected.context("missing precondition")?,
                "file changed during edit; replacement stopped"
            );
            root.rename(&temp, root, &path)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = root.remove_file(&temp);
        }
        result?;
    }
    Ok(ToolOutput {
        content: format!(
            "{} {} ({} bytes)",
            if replace_match { "Patched" } else { "Wrote" },
            path.display(),
            content.len()
        ),
        data: json!({"path":path,"sha256":hash(content.as_bytes()),"bytes":content.len()}),
        ..Default::default()
    })
}

struct ShellTool {
    artifact_dir: PathBuf,
}

#[async_trait]
impl Tool for ShellTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition { name: "shell".into(), description: "Run a bounded command with the host's granted shell access. Native execution is not a security sandbox. Prefer native tools for file work. Use heavy=true for builds or tests; commands preserve ordinary shell semantics.".into(), input_schema: json!({"type":"object","properties":{"command":{"type":"string","minLength":1,"maxLength":16000},"timeout_ms":{"type":"integer","minimum":1,"maximum":120_000},"heavy":{"type":"boolean"}},"required":["command"],"additionalProperties":false}), effect: Effect::External, version: "1".into() }
    }

    fn reservation(&self) -> Option<Reservation> {
        None
    }

    async fn execute(&self, call: ToolCall, context: ToolContext) -> Result<ToolOutput> {
        let command = call.arguments["command"]
            .as_str()
            .context("command is required")?;
        let heavy = call.arguments["heavy"].as_bool().unwrap_or(false);
        let reservation = if heavy {
            Reservation {
                cpu_millis: 1500,
                memory_bytes: 384 * 1024 * 1024,
                processes: 16,
                output_bytes: 128 * 1024,
                heavy: true,
            }
        } else {
            Reservation::default()
        };
        #[cfg(unix)]
        let (program, args) = (
            "/bin/bash".to_owned(),
            vec![
                "--noprofile".to_owned(),
                "--norc".to_owned(),
                "-c".to_owned(),
                command.into(),
            ],
        );
        #[cfg(windows)]
        let (program, args) = (
            "cmd.exe".to_owned(),
            vec![
                "/D".to_owned(),
                "/S".to_owned(),
                "/C".to_owned(),
                command.into(),
            ],
        );
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
                    reservation,
                    ..Default::default()
                },
                context.cancel,
                retained,
            )
            .await?;
        let display = format!(
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
        );
        Ok(ToolOutput {
            content: display,
            data: json!({"exit_code":output.exit_code,"duration_ms":output.duration_ms,"truncated":output.truncated,"peak_memory_bytes":output.peak_memory_bytes,"peak_processes":output.peak_processes}),
            is_error: output.exit_code != Some(0) || output.cancelled || output.timed_out,
            artifact: output.artifact,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ejm_harness::{
        EventKind, Harness, HarnessConfig, ModelDelta, ModelRequest, ModelResponse, Provider, Role,
        RunStatus, SessionOptions,
    };

    #[derive(Default)]
    struct SearchCaptureProvider {
        request: std::sync::Mutex<Option<ModelRequest>>,
        limit: usize,
    }

    #[async_trait]
    impl Provider for SearchCaptureProvider {
        async fn stream(
            &self,
            request: ModelRequest,
            _cancel: tokio_util::sync::CancellationToken,
            _deltas: tokio::sync::mpsc::Sender<ModelDelta>,
        ) -> Result<ModelResponse> {
            if request
                .messages
                .last()
                .is_some_and(|message| message.role == Role::Tool)
            {
                *self
                    .request
                    .lock()
                    .map_err(|_| anyhow::anyhow!("capture lock poisoned"))? = Some(request);
                Ok(ModelResponse {
                    content: "Search results inspected.".into(),
                    ..Default::default()
                })
            } else {
                Ok(ModelResponse {
                    tool_calls: vec![ToolCall {
                        id: "search-fixture".into(),
                        name: "search".into(),
                        arguments: json!({"pattern":"needle","limit":self.limit}),
                    }],
                    ..Default::default()
                })
            }
        }
    }

    #[tokio::test]
    async fn harness_passes_search_completeness_to_the_next_model_request() -> Result<()> {
        let unicode_line = format!("needle {}\n", "🦀".repeat(240));
        for (text, limit, reason) in [
            (
                "needle one\nneedle two\nneedle three\n".into(),
                2,
                "match limit",
            ),
            (unicode_line.repeat(100), 100, "14 KiB output limit"),
        ] {
            let temp = tempfile::tempdir()?;
            let workspace = temp.path().join("workspace");
            std::fs::create_dir(&workspace)?;
            std::fs::write(workspace.join("sample"), text)?;
            let provider = Arc::new(SearchCaptureProvider {
                limit,
                ..Default::default()
            });
            let harness = Harness::new(
                HarnessConfig {
                    state_dir: temp.path().join("state"),
                    max_steps: 2,
                    run_timeout_ms: 5000,
                    ..Default::default()
                },
                provider.clone(),
                tools(CodingConfig::default())?,
            )?;
            let session = harness
                .create_session(SessionOptions {
                    cwd: workspace,
                    title: "search completeness".into(),
                })
                .await?;
            let mut run = harness
                .start(&session.id, "Find needle in this workspace.".into())
                .await?;
            let result = run.wait().await?;
            harness.shutdown().await?;

            assert_eq!(result.status, RunStatus::Completed, "{:?}", result.error);
            let request = provider
                .request
                .lock()
                .map_err(|_| anyhow::anyhow!("capture lock poisoned"))?
                .take()
                .context("next model request was not captured")?;
            let observation = request
                .messages
                .iter()
                .find(|message| message.role == Role::Tool)
                .context("model request has no tool observation")?;
            assert!(observation.content.contains("needle"));
            assert!(
                observation
                    .content
                    .contains(&format!("Search incomplete: {reason}"))
            );
            assert!(observation.content.contains("Narrow path or pattern."));
            assert!(observation.content.len() <= 16 * 1024);
            assert!(!observation.is_error);
            let output = harness
                .events(&session.id, 0, 100)
                .await?
                .into_iter()
                .find_map(|event| match event.kind {
                    EventKind::ToolFinished { output, .. } => Some(output),
                    _ => None,
                })
                .context("search result was not journaled")?;
            let matches = observation
                .content
                .lines()
                .filter(|line| line.contains("needle"))
                .count();
            assert_eq!(output.data["matches"], json!(matches));
            assert_eq!(output.data["truncated"], true);
        }
        Ok(())
    }

    #[test]
    fn search_complete_no_match_has_no_incomplete_notice() -> Result<()> {
        let temp = tempfile::tempdir()?;
        std::fs::write(temp.path().join("sample"), "ordinary text\n")?;
        let cwd = std::fs::canonicalize(temp.path())?;
        let root = Dir::open_ambient_dir(&cwd, ambient_authority())?;
        let output = search(
            &root,
            &cwd,
            &json!({"pattern":"needle"}),
            &tokio_util::sync::CancellationToken::new(),
        )?;

        assert_eq!(output.content, "No matches.");
        assert_eq!(output.data["matches"], 0);
        assert_eq!(output.data["skipped"], 0);
        assert_eq!(output.data["truncated"], false);
        assert!(!output.is_error);
        Ok(())
    }

    #[test]
    fn search_hit_limit_is_visible_with_bounded_matches() -> Result<()> {
        let temp = tempfile::tempdir()?;
        std::fs::write(
            temp.path().join("sample"),
            "needle one\nneedle two\nneedle three\n",
        )?;
        let cwd = std::fs::canonicalize(temp.path())?;
        let root = Dir::open_ambient_dir(&cwd, ambient_authority())?;
        let output = search(
            &root,
            &cwd,
            &json!({"pattern":"needle","limit":2}),
            &tokio_util::sync::CancellationToken::new(),
        )?;

        assert_eq!(output.data["matches"], 2);
        assert_eq!(output.data["truncated"], true);
        assert_eq!(
            output
                .content
                .lines()
                .filter(|line| line.contains("needle"))
                .count(),
            2
        );
        assert!(output.content.contains("Search incomplete: match limit"));
        assert!(output.content.contains("Narrow path or pattern."));
        Ok(())
    }

    #[test]
    fn search_scan_limit_qualifies_an_empty_result() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let bytes = vec![b'x'; 256 * 1024];
        for index in 0..=MAX_SEARCH_BYTES / bytes.len() {
            std::fs::write(temp.path().join(format!("sample-{index}")), &bytes)?;
        }
        let cwd = std::fs::canonicalize(temp.path())?;
        let root = Dir::open_ambient_dir(&cwd, ambient_authority())?;
        let output = search(
            &root,
            &cwd,
            &json!({"pattern":"needle"}),
            &tokio_util::sync::CancellationToken::new(),
        )?;

        assert_eq!(output.data["matches"], 0);
        assert_eq!(output.data["bytes_read"], json!(MAX_SEARCH_BYTES));
        assert_eq!(output.data["truncated"], true);
        assert!(
            output
                .content
                .starts_with("No matches in the scanned text.")
        );
        assert!(
            output
                .content
                .contains("Search incomplete: 8 MiB scan limit")
        );
        assert!(output.content.contains("Narrow path or pattern."));
        assert!(output.content.len() < 256);
        Ok(())
    }

    #[test]
    fn search_skipped_entries_are_visible_without_a_scan_limit() -> Result<()> {
        let temp = tempfile::tempdir()?;
        std::fs::write(temp.path().join("large"), vec![b'x'; 256 * 1024 + 1])?;
        std::fs::write(temp.path().join("binary"), [0xff])?;
        let cwd = std::fs::canonicalize(temp.path())?;
        let root = Dir::open_ambient_dir(&cwd, ambient_authority())?;
        let output = search(
            &root,
            &cwd,
            &json!({"pattern":"needle"}),
            &tokio_util::sync::CancellationToken::new(),
        )?;

        assert_eq!(output.data["matches"], 0);
        assert_eq!(output.data["skipped"], 2);
        assert_eq!(output.data["truncated"], false);
        assert!(
            output
                .content
                .starts_with("No matches in the scanned text.")
        );
        assert!(output.content.contains("Skipped 2 entries"));
        assert!(output.content.contains("use read_file"));
        assert!(output.content.len() < 256);
        Ok(())
    }

    #[test]
    fn directory_listing_limit_is_visible_with_bounded_entries() -> Result<()> {
        let temp = tempfile::tempdir()?;
        for index in 0..201 {
            std::fs::write(temp.path().join(format!("item-{index:03}")), "")?;
        }
        let root = Dir::open_ambient_dir(temp.path(), ambient_authority())?;
        let output = list_files(&root, &json!({}))?;

        assert_eq!(output.data["truncated"], true);
        assert_eq!(
            output
                .content
                .lines()
                .filter(|line| line.starts_with("item-"))
                .count(),
            200
        );
        assert!(
            output
                .content
                .contains("Listing incomplete: 200-entry limit")
        );
        assert!(output.content.contains("Narrow path"));
        Ok(())
    }

    #[test]
    fn directory_listing_output_limit_preserves_the_notice_and_whole_names() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let suffix = "界".repeat(60);
        for index in 0..200 {
            std::fs::write(temp.path().join(format!("item-{index:03}-{suffix}")), "")?;
        }
        let root = Dir::open_ambient_dir(temp.path(), ambient_authority())?;
        let output = list_files(&root, &json!({}))?;
        let (listing, notice) = output
            .content
            .split_once("\n\n")
            .context("missing listing notice")?;

        assert_eq!(output.data["truncated"], true);
        assert!(listing.len() <= MAX_RETRIEVAL_BYTES);
        assert!((1..200).contains(&listing.lines().count()));
        assert!(
            listing
                .lines()
                .all(|line| line.starts_with("item-") && line.ends_with('界'))
        );
        assert!(notice.contains("Listing incomplete: 14 KiB output limit"));
        assert!(output.content.len() <= 16 * 1024);
        Ok(())
    }

    #[test]
    fn stale_edits_and_ambiguous_patch_fail() -> Result<()> {
        let temp = tempfile::tempdir()?;
        std::fs::write(temp.path().join("sample"), "one two one\n")?;
        let root = Dir::open_ambient_dir(temp.path(), ambient_authority())?;
        let digest = hash(b"one two one\n");
        let cancel = tokio_util::sync::CancellationToken::new();
        assert!(
            edit(
                &root,
                &json!({"path":"sample","content":"wrong","expected_hash":"0".repeat(64)}),
                false,
                &cancel
            )
            .is_err()
        );
        assert!(
            edit(
                &root,
                &json!({"path":"sample","old":"one","new":"three","expected_hash":digest}),
                true,
                &cancel
            )
            .is_err()
        );
        edit(
            &root,
            &json!({"path":"sample","old":"two","new":"three","expected_hash":digest}),
            true,
            &cancel,
        )?;
        assert_eq!(
            std::fs::read_to_string(temp.path().join("sample"))?,
            "one three one\n"
        );
        Ok(())
    }

    #[test]
    fn reads_cannot_traverse_parent_or_symlink_outside_workspace() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let outside = tempfile::tempdir()?;
        std::fs::write(outside.path().join("private"), "secret")?;
        let root = Dir::open_ambient_dir(temp.path(), ambient_authority())?;
        assert!(read_file(&root, &json!({"path":"../private"})).is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(outside.path(), temp.path().join("escape"))?;
            assert!(read_file(&root, &json!({"path":"escape/private"})).is_err());
            let cancel = tokio_util::sync::CancellationToken::new();
            assert!(
                edit(
                    &root,
                    &json!({"path":"escape/new","content":"bad"}),
                    false,
                    &cancel
                )
                .is_err()
            );
            assert!(!outside.path().join("new").exists());
        }
        Ok(())
    }

    #[test]
    fn reads_listings_and_search_reject_protected_names_and_case_variants() -> Result<()> {
        for name in [".git", ".GIT", ".ejm-harness", ".EjM-HaRnEsS"] {
            let temp = tempfile::tempdir()?;
            std::fs::create_dir(temp.path().join(name))?;
            std::fs::write(temp.path().join(name).join("state.txt"), "fixture needle\n")?;
            std::fs::write(temp.path().join("visible.txt"), "ordinary needle\n")?;
            let cwd = temp.path().canonicalize()?;
            let root = Dir::open_ambient_dir(&cwd, ambient_authority())?;
            let cancel = tokio_util::sync::CancellationToken::new();
            let file = format!("{name}/state.txt");
            assert!(
                read_file(&root, &json!({"path":file})).is_err(),
                "allowed {file}"
            );
            assert!(
                list_files(&root, &json!({"path":name})).is_err(),
                "listed {name}"
            );
            assert!(
                search(
                    &root,
                    &cwd,
                    &json!({"path":name,"pattern":"needle"}),
                    &cancel
                )
                .is_err(),
                "searched {name}"
            );
            let listing = list_files(&root, &json!({}))?;
            assert_eq!(listing.content, "visible.txt");
            assert!(
                read_file(&root, &json!({"path":"visible.txt"}))?
                    .content
                    .contains("ordinary")
            );
            let output = search(&root, &cwd, &json!({"pattern":"needle"}), &cancel)?;
            assert!(output.content.contains("ordinary needle"));
            assert!(!output.content.contains("fixture needle"));
        }
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn reads_listings_and_search_reject_protected_aliases_but_allow_workspace_aliases() -> Result<()>
    {
        let temp = tempfile::tempdir()?;
        let cwd = temp.path().canonicalize()?;
        for (directory, alias) in [(".git", "metadata"), (".ejm-harness", "state")] {
            std::fs::create_dir(cwd.join(directory))?;
            std::fs::write(cwd.join(directory).join("record.txt"), "private fixture\n")?;
            std::os::unix::fs::symlink(directory, cwd.join(alias))?;
            std::os::unix::fs::symlink(
                format!("{directory}/record.txt"),
                cwd.join(format!("{alias}-file")),
            )?;
        }
        std::fs::create_dir(cwd.join("ordinary"))?;
        std::fs::write(cwd.join("ordinary/visible.txt"), "ordinary needle\n")?;
        std::os::unix::fs::symlink("ordinary", cwd.join("allowed"))?;
        let root = Dir::open_ambient_dir(&cwd, ambient_authority())?;
        let cancel = tokio_util::sync::CancellationToken::new();
        for alias in ["metadata", "state"] {
            for path in [format!("{alias}/record.txt"), format!("{alias}-file")] {
                assert!(
                    read_file(&root, &json!({"path":path})).is_err(),
                    "read {path}"
                );
                assert!(
                    search(
                        &root,
                        &cwd,
                        &json!({"path":path,"pattern":"fixture"}),
                        &cancel
                    )
                    .is_err(),
                    "searched {path}"
                );
            }
            assert!(
                list_files(&root, &json!({"path":alias})).is_err(),
                "listed {alias}"
            );
            assert!(
                search(
                    &root,
                    &cwd,
                    &json!({"path":alias,"pattern":"fixture"}),
                    &cancel
                )
                .is_err(),
                "searched {alias}"
            );
        }
        assert!(
            read_file(&root, &json!({"path":"allowed/visible.txt"}))?
                .content
                .contains("ordinary needle")
        );
        assert_eq!(
            list_files(&root, &json!({"path":"allowed"}))?.content,
            "visible.txt"
        );
        let output = search(
            &root,
            &cwd,
            &json!({"path":"allowed","pattern":"needle"}),
            &cancel,
        )?;
        assert!(output.content.contains("ordinary needle"));
        Ok(())
    }

    #[test]
    fn effect_grants_are_explicit() -> Result<()> {
        let read = tools(CodingConfig::default())?;
        assert!(read.iter().all(|t| t.definition().effect == Effect::Read));
        assert_eq!(read.len(), 3);
        Ok(())
    }
    #[cfg(unix)]
    #[test]
    fn writes_reject_metadata_aliases_and_case_variants() -> Result<()> {
        let temp = tempfile::tempdir()?;
        std::fs::create_dir(temp.path().join(".git"))?;
        std::fs::write(temp.path().join(".git/config"), "original\n")?;
        std::os::unix::fs::symlink(".git", temp.path().join("meta"))?;
        std::os::unix::fs::symlink(".git/config", temp.path().join("alias"))?;
        let root = Dir::open_ambient_dir(temp.path(), ambient_authority())?;
        let cancel = tokio_util::sync::CancellationToken::new();
        for path in [
            "meta/config",
            "meta/new",
            "alias",
            ".GIT/config",
            ".EjM-HaRnEsS/state",
        ] {
            assert!(
                edit(
                    &root,
                    &json!({"path":path,"content":"changed\n","expected_hash":hash(b"original\n")}),
                    false,
                    &cancel
                )
                .is_err(),
                "allowed {path}"
            );
        }
        assert_eq!(
            std::fs::read_to_string(temp.path().join(".git/config"))?,
            "original\n"
        );
        assert!(!temp.path().join(".git/new").exists());
        Ok(())
    }
}
