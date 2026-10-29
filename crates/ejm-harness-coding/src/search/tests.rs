use std::sync::Arc;

use async_trait::async_trait;
use cap_std::ambient_authority;
use ejm_harness::{
    EventKind, Harness, HarnessConfig, ModelDelta, ModelRequest, ModelResponse, Provider, Role,
    RunStatus, SessionOptions, ToolCall,
};

use super::*;
use crate::{CodingConfig, tools};

/// Searches a fixture directory through a fresh workspace capability.
fn search_in(directory: &Path, arguments: &Value) -> Result<ToolOutput> {
    let cwd = std::fs::canonicalize(directory)?;
    let root = Dir::open_ambient_dir(&cwd, ambient_authority())?;
    search(&root, &cwd, arguments, &CancellationToken::new())
}

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
        _cancel: CancellationToken,
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
    let output = search_in(temp.path(), &json!({"pattern":"needle"}))?;

    assert_eq!(output.content, "No matches.");
    assert_eq!(output.data["matches"], 0);
    assert_eq!(output.data["skipped"], 0);
    assert_eq!(output.data["truncated"], false);
    assert!(!output.is_error);
    Ok(())
}

#[test]
fn search_reports_depth_boundary_and_can_search_a_narrower_path() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let mut deep = temp.path().to_path_buf();
    for _ in 0..32 {
        deep.push("d");
    }
    std::fs::create_dir_all(&deep)?;
    std::fs::write(deep.join("sample"), "needle\n")?;
    let output = search_in(temp.path(), &json!({"pattern":"needle"}))?;
    assert_eq!(output.data["matches"], 0);
    assert_eq!(output.data["truncated"], true);
    assert!(output.content.contains("32-level depth limit"));

    let output = search_in(temp.path(), &json!({"pattern":"needle","path":"d"}))?;
    assert_eq!(output.data["matches"], 1);
    assert_eq!(output.data["truncated"], false);
    Ok(())
}

#[test]
fn search_reports_byte_limit_reached_in_the_final_file() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let bytes = vec![b'x'; 254 * 1024];
    for index in 0..33 {
        std::fs::write(temp.path().join(format!("sample-{index}")), &bytes)?;
    }
    let output = search_in(temp.path(), &json!({"pattern":"needle"}))?;
    assert_eq!(output.data["bytes_read"], json!(MAX_SEARCH_BYTES));
    assert_eq!(output.data["truncated"], true);
    assert!(output.content.contains("8 MiB scan limit"));
    Ok(())
}

#[test]
fn search_hit_limit_is_visible_with_bounded_matches() -> Result<()> {
    let temp = tempfile::tempdir()?;
    std::fs::write(
        temp.path().join("sample"),
        "needle one\nneedle two\nneedle three\n",
    )?;
    let output = search_in(temp.path(), &json!({"pattern":"needle","limit":2}))?;

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
    let output = search_in(temp.path(), &json!({"pattern":"needle"}))?;

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
    let output = search_in(temp.path(), &json!({"pattern":"needle"}))?;

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
