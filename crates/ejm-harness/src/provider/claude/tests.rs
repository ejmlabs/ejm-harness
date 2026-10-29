use std::path::PathBuf;

use ejm_harness_exec::CommandOutput;
use serde_json::{Value, json};

use super::{
    ClaudeCodeConfig,
    protocol::{MAX_CALLS, MAX_INPUT, decode_response, request_input, response_schema},
};
use crate::{
    Effect, Message, ModelRequest, ModelResponse, ProviderContinuation, ToolCall, ToolDefinition,
    ToolOutput, provider::MAX_RESPONSE,
};

fn definition() -> ToolDefinition {
    ToolDefinition {
        name: "inspect".into(),
        description: "Read evidence".into(),
        input_schema: json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"],"additionalProperties":false}),
        effect: Effect::Read,
        version: "1".into(),
    }
}

fn request() -> ModelRequest {
    ModelRequest {
        system: "PRIVATE_SYSTEM".into(),
        messages: vec![Message::user("PRIVATE_PROMPT")],
        tools: vec![definition()],
        max_output_tokens: 1024,
    }
}

fn envelope(structured: &Value) -> Value {
    json!({
        "type":"result", "subtype":"success", "is_error":false,
        "result":"PRIVATE_OUTPUT", "structured_output":structured,
        "usage":{"input_tokens":10,"output_tokens":7,"cache_creation_input_tokens":3,"cache_read_input_tokens":4}
    })
}

fn output(value: &Value) -> CommandOutput {
    CommandOutput {
        exit_code: Some(0),
        stdout: value.to_string(),
        ..CommandOutput::default()
    }
}

#[test]
fn relative_executable_directories_cannot_change_between_auth_and_inference() {
    let mut config = ClaudeCodeConfig::default();
    for program in ["./claude", "../claude", "bin/claude"] {
        config.program = program.into();
        assert!(config.validate().is_err());
    }
    config.program = "claude".into();
    config.validate().unwrap();
}

#[test]
fn canonical_reply_uses_real_usage_and_locally_unique_tool_ids() {
    let schema = response_schema(&[definition()]).unwrap();
    let reply = envelope(&json!({
        "content":"Inspecting café.txt",
        "tool_calls":[{"name":"inspect","arguments_json":"{\"path\":\"café.txt\"}"}]
    }));
    let first = decode_response(&output(&reply), &schema).unwrap();
    let second = decode_response(&output(&reply), &schema).unwrap();
    assert_eq!(first.content, "Inspecting café.txt");
    assert_eq!(first.tool_calls[0].arguments, json!({"path":"café.txt"}));
    assert_ne!(first.tool_calls[0].id, second.tool_calls[0].id);
    assert_eq!(first.usage.input_tokens, 17);
    assert_eq!(first.usage.output_tokens, 7);
    assert_eq!(first.usage.cached_input_tokens, Some(4));
    assert!(first.continuation.is_none());
}

#[test]
fn rejects_untrusted_or_incomplete_results_without_exposing_payloads() {
    let schema = response_schema(&[definition()]).unwrap();
    let base = envelope(&json!({"content":"Ready","tool_calls":[]}));
    let mut invalid = vec![];
    for (path, replacement) in [
        ("/type", json!("assistant")),
        ("/subtype", json!("error_max_structured_output_retries")),
        ("/is_error", json!(true)),
        ("/structured_output", Value::Null),
        ("/usage", Value::Null),
        ("/usage/output_tokens", json!("PRIVATE_OUTPUT")),
        (
            "/structured_output/content",
            json!("é".repeat(MAX_RESPONSE / 2 + 1)),
        ),
        (
            "/structured_output/tool_calls",
            json!([{"name":"shell","arguments_json":"{}"}]),
        ),
        (
            "/structured_output/tool_calls",
            json!([{"name":"inspect","arguments_json":"PRIVATE_OUTPUT"}]),
        ),
        (
            "/structured_output/tool_calls",
            json!(vec![
                json!({"name":"inspect","arguments_json":"{}"});
                MAX_CALLS + 1
            ]),
        ),
    ] {
        let mut value = base.clone();
        *value.pointer_mut(path).unwrap() = replacement;
        invalid.push(value);
    }
    let mut extra = base.clone();
    extra["structured_output"]["usage"] = json!({"input_tokens":0});
    invalid.push(extra);
    let mut overflow = base;
    overflow["usage"]["input_tokens"] = json!(u64::MAX);
    invalid.push(overflow);
    for value in invalid {
        let error = decode_response(&output(&value), &schema).unwrap_err();
        assert!(!format!("{error:#}").contains("PRIVATE_OUTPUT"));
    }
    let no_tools = response_schema(&[]).unwrap();
    let proposal =
        envelope(&json!({"content":"","tool_calls":[{"name":"inspect","arguments_json":"{}"}]}));
    assert!(decode_response(&output(&proposal), &no_tools).is_err());
}

#[test]
fn rejects_failed_or_truncated_process_output() {
    let schema = response_schema(&[]).unwrap();
    let reply = envelope(&json!({"content":"Ready","tool_calls":[]}));
    let mut cases = vec![CommandOutput {
        stdout: "PRIVATE_OUTPUT".into(),
        exit_code: Some(0),
        ..CommandOutput::default()
    }];
    for mode in 0..4 {
        let mut result = output(&reply);
        match mode {
            0 => result.truncated = true,
            1 => result.cancelled = true,
            2 => result.timed_out = true,
            _ => result.exit_code = Some(1),
        }
        cases.push(result);
    }
    for result in cases {
        let error = decode_response(&result, &schema).unwrap_err();
        assert!(!format!("{error:#}").contains("PRIVATE_OUTPUT"));
    }
}

#[test]
fn input_replays_readable_tool_evidence_without_foreign_continuation() {
    let mut request = request();
    request.messages.push(Message::assistant(&ModelResponse {
        tool_calls: vec![ToolCall {
            id: "prior-call".into(),
            name: "inspect".into(),
            arguments: json!({"path":"café.txt"}),
        }],
        continuation: Some(ProviderContinuation::OpenAiResponses {
            reasoning: vec![json!({"encrypted_content":"PRIVATE_CONTINUATION"})],
        }),
        ..ModelResponse::default()
    }));
    request.messages.push(Message::tool(
        "prior-call",
        &ToolOutput {
            content: "Permission denied".into(),
            is_error: true,
            artifact: Some(PathBuf::from("evidence.txt")),
            ..ToolOutput::default()
        },
    ));
    let bytes = request_input(&request).unwrap();
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["messages"][2]["tool_call_id"], "prior-call");
    assert_eq!(value["messages"][2]["is_error"], true);
    assert_eq!(value["messages"][2]["artifact"], "evidence.txt");
    assert!(
        !String::from_utf8(bytes)
            .unwrap()
            .contains("PRIVATE_CONTINUATION")
    );
    request.system = "x".repeat(MAX_INPUT);
    assert!(request_input(&request).is_err());
}

#[cfg(unix)]
mod process {
    use std::{os::unix::fs::PermissionsExt, sync::Arc, time::Duration};

    use ejm_harness_exec::{Executor, Governor, GovernorSnapshot, Reservation, ResourceLimits};
    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;

    use super::{super::ClaudeCodeProvider, *};
    use crate::{ModelDelta, Provider};

    fn fixture(script: &str) -> (tempfile::TempDir, Arc<Governor>, ClaudeCodeProvider) {
        let root = tempfile::tempdir().unwrap();
        let program = root.path().join("claude-fixture");
        std::fs::write(&program, format!("#!/bin/sh\nif [ \"$1\" = auth ]; then\n: > \"$(dirname \"$0\")/auth-started\"\ncat \"$(dirname \"$0\")/auth.json\"\nexit\nfi\n{script}")).unwrap();
        std::fs::write(
            root.path().join("auth.json"),
            r#"{"loggedIn":true,"authMethod":"claude.ai"}"#,
        )
        .unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
        let governor = Arc::new(Governor::new(ResourceLimits::default()).unwrap());
        let executor = Executor::new(governor.clone(), root.path().join("artifacts")).unwrap();
        let provider = ClaudeCodeProvider::new(
            ClaudeCodeConfig {
                program,
                cwd: root.path().into(),
                ..ClaudeCodeConfig::default()
            },
            executor,
        )
        .unwrap();
        (root, governor, provider)
    }

    #[tokio::test]
    async fn client_gets_private_stdin_and_only_canonical_text_is_emitted() {
        let (root, governor, provider) = fixture(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > args.txt\ncat > input.json\ncat reply.json\n",
        );
        std::fs::write(root.path().join("reply.json"), envelope(&json!({
            "content":"Canonical answer", "tool_calls":[{"name":"inspect","arguments_json":"{\"path\":\"café.txt\"}"}]
        })).to_string()).unwrap();
        let (send, mut receive) = mpsc::channel(4);
        let response = provider
            .stream(request(), CancellationToken::new(), send)
            .await
            .unwrap();
        assert_eq!(response.content, "Canonical answer");
        assert!(
            matches!(receive.recv().await, Some(ModelDelta::Text(text)) if text == response.content)
        );
        assert!(receive.recv().await.is_none());
        let args = std::fs::read_to_string(root.path().join("args.txt")).unwrap();
        assert!(!args.contains("PRIVATE_SYSTEM") && !args.contains("PRIVATE_PROMPT"));
        assert!(args.contains("--tools\n\n") && args.contains("--strict-mcp-config\n"));
        let input: Value =
            serde_json::from_slice(&std::fs::read(root.path().join("input.json")).unwrap())
                .unwrap();
        assert_eq!(input["system"], "PRIVATE_SYSTEM");
        assert_eq!(input["messages"][0]["content"], "PRIVATE_PROMPT");
        assert_eq!(governor.snapshot(), GovernorSnapshot::default());
    }

    #[tokio::test]
    async fn changed_authentication_source_is_rejected_before_model_launch() {
        let (root, governor, provider) = fixture("cat > input.json\ncat reply.json\n");
        for source in ["api_key", "api_key_helper", "third_party", "oauth_token"] {
            std::fs::write(
                root.path().join("auth.json"),
                json!({"loggedIn":true,"authMethod":source}).to_string(),
            )
            .unwrap();
            let (send, _receive) = mpsc::channel(4);
            let result = provider
                .stream(request(), CancellationToken::new(), send)
                .await;
            assert!(result.is_err());
            assert!(!root.path().join("input.json").exists());
            assert_eq!(governor.snapshot(), GovernorSnapshot::default());
        }
    }

    #[tokio::test]
    async fn account_check_waits_for_shared_admission_and_cancels_without_launching() {
        let (root, governor, provider) = fixture(": > model-started\n");
        let held = governor
            .acquire(
                "occupied",
                Reservation {
                    memory_bytes: governor.limits().memory_bytes,
                    ..Reservation::default()
                },
                &CancellationToken::new(),
            )
            .await
            .unwrap();
        let cancel = CancellationToken::new();
        let request_cancel = cancel.clone();
        let (send, _receive) = mpsc::channel(4);
        let task =
            tokio::spawn(async move { provider.stream(request(), request_cancel, send).await });
        tokio::time::timeout(Duration::from_secs(5), async {
            while governor.snapshot().queued != 1 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert!(!root.path().join("auth-started").exists());
        assert!(!root.path().join("model-started").exists());
        cancel.cancel();
        let result = tokio::time::timeout(Duration::from_secs(5), task)
            .await
            .unwrap()
            .unwrap();
        assert!(result.is_err());
        assert_eq!(governor.snapshot().queued, 0);
        drop(held);
        tokio::time::timeout(Duration::from_secs(5), governor.wait_idle())
            .await
            .unwrap();
        assert_eq!(governor.snapshot(), GovernorSnapshot::default());
        assert!(!root.path().join("auth-started").exists());
        assert!(!root.path().join("model-started").exists());
    }

    #[tokio::test]
    async fn cancellation_reaps_the_official_client_before_releasing_its_job() {
        let (root, governor, provider) =
            fixture("#!/bin/sh\ncat > input.json\n: > ready\nexec sleep 30\n");
        let cancel = CancellationToken::new();
        let request_cancel = cancel.clone();
        let (send, _receive) = mpsc::channel(4);
        let task =
            tokio::spawn(async move { provider.stream(request(), request_cancel, send).await });
        tokio::time::timeout(Duration::from_secs(5), async {
            while !root.path().join("ready").exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        cancel.cancel();
        let result = tokio::time::timeout(Duration::from_secs(5), task)
            .await
            .unwrap()
            .unwrap();
        assert!(result.is_err());
        assert_eq!(governor.snapshot(), GovernorSnapshot::default());
    }
}
