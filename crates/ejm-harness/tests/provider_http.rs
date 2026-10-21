use std::{
    sync::{Arc, Mutex},
    time::{Duration, SystemTime},
};

use anyhow::{Context, Result, anyhow, bail, ensure};
use ejm_harness::{
    AnthropicConfig, AnthropicProvider, Effect, Message, ModelDelta, ModelRequest, ModelResponse,
    OpenAiConfig, OpenAiProvider, Provider, ProviderCapacity, ProviderCapacityObserver,
    ProviderContinuation, ProviderObservation, ProviderRate, ProviderRetry, QuotaDomain,
    ResponsesConfig, ResponsesProvider, ToolCall, ToolDefinition, ToolOutput,
};
use ejm_harness_auth::{SecretString, StaticCredential};
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use wiremock::{Mock, MockServer, Request, ResponseTemplate, matchers::any};

struct Fixture {
    base_url: String,
    server: MockServer,
}

const FIRST_RESULT: &str = "[ejm-harness tool error]\nfirst result\n[ejm-harness artifact: /workspace/artifacts/inspect.log]";

impl Fixture {
    async fn start(body: String) -> Self {
        Self::with_status(body, 200).await
    }

    async fn with_status(body: String, status: u16) -> Self {
        Self::with_headers(body, status, &[]).await
    }

    async fn with_headers(body: String, status: u16, headers: &[(&str, &str)]) -> Self {
        let server = MockServer::start().await;
        let mut template = ResponseTemplate::new(status).set_body_raw(body, "text/event-stream");
        for (name, value) in headers {
            template = template.insert_header(*name, *value);
        }
        Mock::given(any())
            .respond_with(template)
            .expect(1)
            .mount(&server)
            .await;
        Self {
            base_url: format!("{}/v1", server.uri()),
            server,
        }
    }

    async fn request(&self) -> Result<Request> {
        let requests = self
            .server
            .received_requests()
            .await
            .context("request recording disabled")?;
        ensure!(requests.len() == 1, "expected exactly one provider request");
        requests
            .into_iter()
            .next()
            .context("missing provider request")
    }
}

#[derive(Default)]
struct CapacityFeedback(Mutex<Vec<ProviderObservation>>);

impl CapacityFeedback {
    fn entries(&self) -> Result<Vec<ProviderObservation>> {
        self.0
            .lock()
            .map(|events| events.clone())
            .map_err(|_| anyhow!("feedback fixture lock poisoned"))
    }
}

impl ProviderCapacityObserver for CapacityFeedback {
    fn observe(&self, observation: ProviderObservation) {
        if let Ok(mut events) = self.0.lock()
            && events.len() < 8
        {
            events.push(observation);
        }
    }
}

fn request() -> ModelRequest {
    let call = |id: &str| ToolCall {
        id: id.into(),
        name: "inspect".into(),
        arguments: json!({"path":"src"}),
    };
    ModelRequest {
        system: "Inspect the repository.".into(),
        messages: vec![
            Message::user("Read the files."),
            Message::assistant(&ModelResponse {
                tool_calls: vec![call("prior-a"), call("prior-b")],
                ..Default::default()
            }),
            Message::tool(
                "prior-a",
                &ToolOutput {
                    artifact: Some("/workspace/artifacts/inspect.log".into()),
                    ..ToolOutput::error("first result")
                },
            ),
            Message::tool("prior-b", &ToolOutput::text("second result")),
            Message::user("Continue."),
        ],
        tools: vec![ToolDefinition {
            name: "inspect".into(),
            description: "Read one path.".into(),
            input_schema: json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}),
            effect: Effect::Read,
            version: "1".into(),
        }],
        max_output_tokens: 123,
    }
}

fn openai(base_url: &str) -> Result<OpenAiProvider> {
    OpenAiProvider::new(OpenAiConfig {
        base_url: base_url.into(),
        api_key: "fixture-key".into(),
        model: "fixture-model".into(),
    })
}

fn anthropic(base_url: &str) -> Result<AnthropicProvider> {
    AnthropicProvider::new(AnthropicConfig {
        base_url: base_url.into(),
        api_key: "fixture-key".into(),
        model: "fixture-model".into(),
    })
}

fn responses(base_url: &str) -> Result<ResponsesProvider> {
    ResponsesProvider::for_endpoint(
        ResponsesConfig {
            model: "fixture-model".into(),
            reasoning_effort: Some("low".into()),
        },
        std::sync::Arc::new(StaticCredential::new(SecretString::new("fixture-key"))),
        base_url,
    )
}

fn reasoning() -> Value {
    json!({"type":"reasoning", "id":"rs_fixture", "summary":[], "encrypted_content":"fixture-encrypted-state"})
}

#[tokio::test]
async fn responses_contract_retains_reasoning_tools_and_disables_storage() -> Result<()> {
    let fixture = Fixture::start(sse(&[
        json!({"type":"response.output_text.delta", "delta":"雪 ready"}),
        json!({"type":"response.completed", "response":{"status":"completed", "output":[
            reasoning(),
            {"type":"message", "role":"assistant", "content":[{"type":"output_text", "text":"雪 ready"}]},
            {"type":"function_call", "id":"fc_item", "call_id":"next-call", "name":"inspect", "arguments":"{\"path\":\"src\"}"}
        ], "usage":{"input_tokens":12,"output_tokens":4,"input_tokens_details":{"cached_tokens":8},"output_tokens_details":{"reasoning_tokens":2}}}}),
    ])).await;
    let mut input = request();
    input.messages[1].continuation = Some(ProviderContinuation::OpenAiResponses {
        reasoning: vec![reasoning()],
    });
    let (send, mut receive) = mpsc::channel(8);
    let result = responses(&fixture.base_url)?
        .stream(input, CancellationToken::new(), send)
        .await?;
    assert_eq!(result.content, "雪 ready");
    assert!(matches!(receive.try_recv()?, ModelDelta::Text(text) if text == result.content));
    assert_eq!(result.tool_calls[0].id, "next-call");
    assert_eq!(result.tool_calls[0].arguments, json!({"path":"src"}));
    assert!(result.continuation.is_some());
    assert_eq!(result.usage.input_tokens, 12);
    assert_eq!(result.usage.cached_input_tokens, Some(8));
    assert_eq!(result.usage.reasoning_output_tokens, Some(2));
    let received = fixture.request().await?;
    let body: Value = received.body_json()?;
    assert_eq!(received.method, "POST");
    assert_eq!(received.url.path(), "/v1/responses");
    assert_eq!(
        received
            .headers
            .get("authorization")
            .and_then(|value| value.to_str().ok()),
        Some("Bearer fixture-key")
    );
    assert_eq!(body["store"], false);
    assert_eq!(body["stream"], true);
    assert_eq!(body["instructions"], request().system);
    assert_eq!(body["input"][1], reasoning());
    assert_eq!(body["input"][2]["call_id"], "prior-a");
    assert_eq!(body["input"][4]["type"], "function_call_output");
    assert_eq!(body["input"][4]["output"], FIRST_RESULT);
    assert_eq!(body["input"][5]["output"], "second result");
    assert_eq!(body["tools"][0]["strict"], false);
    assert_eq!(body["reasoning"]["effort"], "low");
    Ok(())
}

#[tokio::test]
async fn responses_requires_completion_and_reports_late_subscription_failure() -> Result<()> {
    for terminal in [
        Some(
            json!({"type":"response.failed", "response":{"error":{"code":"subscription_sharing_usage_limit_exceeded", "message":"untrusted-secret"}}}),
        ),
        None,
    ] {
        let mut events = vec![json!({"type":"response.output_text.delta", "delta":"partial"})];
        if let Some(terminal) = terminal {
            events.push(terminal);
        }
        let fixture = Fixture::start(sse(&events)).await;
        let (send, _receive) = mpsc::channel(8);
        let result = responses(&fixture.base_url)?
            .stream(request(), CancellationToken::new(), send)
            .await;
        let error = result
            .err()
            .context("incomplete response succeeded")?
            .to_string();
        assert!(error.contains("usage limit") || error.contains("without response.completed"));
        assert!(!error.contains("untrusted-secret"));
        fixture.request().await?;
    }
    Ok(())
}

#[tokio::test]
async fn responses_bounds_ignored_progress_events_before_completion() -> Result<()> {
    let mut body = sse(&[json!({"type":"response.in_progress"})]).repeat(100_001);
    body.push_str(&sse(&[json!({"type":"response.completed", "response":{
        "status":"completed", "output":[], "usage":{"input_tokens":1,"output_tokens":0}
    }})]));
    let fixture = Fixture::start(body).await;
    let (send, _receive) = mpsc::channel(8);
    let error = responses(&fixture.base_url)?
        .stream(request(), CancellationToken::new(), send)
        .await
        .err()
        .context("excess ignored progress events were accepted")?;
    assert!(format!("{error:#}").contains("100000 decoded events"));
    fixture.request().await?;
    Ok(())
}

#[tokio::test]
async fn responses_catalog_preserves_account_order_and_hides_nonlist_models() -> Result<()> {
    let fixture = Fixture::start(
        json!({"models":[
            {"slug":"second", "display_name":"Second", "visibility":"list"},
            {"slug":"hidden", "display_name":"Hidden", "visibility":"hidden"},
            {"slug":"first", "display_name":"First", "visibility":"list"}
        ]})
        .to_string(),
    )
    .await;
    let models = responses(&fixture.base_url)?
        .models(CancellationToken::new())
        .await?;
    assert_eq!(
        models
            .iter()
            .map(|model| model.slug.as_str())
            .collect::<Vec<_>>(),
        ["second", "first"]
    );
    let received = fixture.request().await?;
    assert_eq!(received.url.path(), "/v1/models");
    assert_eq!(received.method, "GET");
    assert!(responses("https://untrusted.example/v1").is_err());
    Ok(())
}

#[tokio::test]
async fn responses_catalog_accepts_public_api_model_ids_in_server_order() -> Result<()> {
    let fixture =
        Fixture::start(json!({"data":[{"id":"second"},{"id":"first"}]}).to_string()).await;
    let models = responses(&fixture.base_url)?
        .models(CancellationToken::new())
        .await?;
    assert_eq!(
        models
            .iter()
            .map(|model| model.slug.as_str())
            .collect::<Vec<_>>(),
        ["second", "first"]
    );
    assert_eq!(models[0].display_name, "second");
    fixture.request().await?;
    Ok(())
}

#[tokio::test]
async fn responses_observer_captures_request_and_http_failure_with_credentials_redacted()
-> Result<()> {
    use ejm_harness::{HttpObserver, HttpTracePhase, HttpTraceRecord};
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct Capture(Mutex<Vec<HttpTraceRecord>>);
    impl HttpObserver for Capture {
        fn observe(&self, record: &HttpTraceRecord) {
            if let Ok(mut records) = self.0.lock() {
                records.push(record.clone());
            }
        }
    }

    let fixture = Fixture::with_status(
        json!({"error":{"message":"fixture-key was rejected"}}).to_string(),
        401,
    )
    .await;
    let capture = Arc::new(Capture::default());
    let provider = responses(&fixture.base_url)?.with_observer(capture.clone());
    let mut input = request();
    input.messages.push(Message::user("fixture-key"));
    let (send, _receive) = mpsc::channel(8);
    let error = provider
        .stream(input, CancellationToken::new(), send)
        .await
        .err()
        .context("HTTP error succeeded")?;
    assert!(error.to_string().contains("HTTP 401"));
    fixture.request().await?;
    let records = capture
        .0
        .lock()
        .map_err(|_| anyhow!("capture lock poisoned"))?;
    assert_eq!(records.len(), 4);
    assert!(matches!(records[0].phase, HttpTracePhase::RequestBody));
    assert!(matches!(records[2].phase, HttpTracePhase::HttpErrorBody));
    assert_eq!(records[0].redactions, 1);
    assert_eq!(records[2].redactions, 1);
    assert_eq!(records[2].status, Some(401));
    assert_eq!(records[0].method.as_deref(), Some("POST"));
    assert_eq!(
        records[0].url.as_deref(),
        Some(format!("{}/responses", fixture.base_url).as_str())
    );
    assert!(
        records[0]
            .headers
            .iter()
            .all(|header| header.name != "authorization")
    );
    assert!(
        records[1]
            .headers
            .iter()
            .any(|header| header.name == "content-type" && header.value == "text/event-stream")
    );
    assert_eq!(
        records[2].payload_bytes,
        json!({"error":{"message":"fixture-key was rejected"}})
            .to_string()
            .len()
    );
    assert!(
        records
            .iter()
            .all(|record| !record.payload.contains("fixture-key") && !record.truncated)
    );
    assert!(
        records
            .iter()
            .all(|record| record.request_id == records[0].request_id)
    );
    Ok(())
}

fn capacity_provider(
    family: &str,
    base: &str,
    capacity: ProviderCapacity,
) -> Result<Box<dyn Provider>> {
    match family {
        "openai" => Ok(Box::new(openai(base)?.with_capacity(capacity)?)),
        "anthropic" => Ok(Box::new(anthropic(base)?.with_capacity(capacity)?)),
        "responses" => Ok(Box::new(responses(base)?.with_capacity(capacity)?)),
        _ => bail!("unknown fixture family"),
    }
}

#[tokio::test]
async fn typed_capacity_feedback_works_without_debug_capture() -> Result<()> {
    for family in ["openai", "anthropic", "responses"] {
        let body = match family {
            "openai" => format!(
                "{}data: [DONE]\r\n\r\n",
                sse(&[json!({"choices":[],"usage":{"prompt_tokens":12,"completion_tokens":4}}),])
            ),
            "anthropic" => sse(&[
                json!({"type":"message_start","message":{"usage":{"input_tokens":8,"cache_creation_input_tokens":2,"cache_read_input_tokens":2}}}),
                json!({"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":4}}),
                json!({"type":"message_stop"}),
            ]),
            _ => sse(&[
                json!({"type":"response.completed","response":{"status":"completed","output":[],"usage":{"input_tokens":12,"output_tokens":4}}}),
            ]),
        };
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_secs();
        let reset = (now + 60).to_string();
        let names = if family == "anthropic" {
            [
                "anthropic-ratelimit-requests-limit",
                "anthropic-ratelimit-requests-remaining",
                "anthropic-ratelimit-requests-reset",
            ]
        } else {
            [
                "x-ratelimit-limit-requests",
                "x-ratelimit-remaining-requests",
                "x-ratelimit-reset-requests",
            ]
        };
        let fixture = Fixture::with_headers(
            body,
            200,
            &[(names[0], "20"), (names[1], "7"), (names[2], &reset)],
        )
        .await;
        let domain = QuotaDomain::new(
            &fixture.base_url,
            "598b68e9-ea5c-427a-b8bc-44bbbfbe7278",
            "shared",
        )?;
        let provider = capacity_provider(
            family,
            &fixture.base_url,
            ProviderCapacity {
                domain: Some(domain.clone()),
                max_in_flight: Some(2),
                requests: Some(ProviderRate {
                    limit: 10,
                    window_ms: 60_000,
                }),
                ..Default::default()
            },
        )?;
        assert_eq!(provider.capacity().domain, Some(domain));
        assert_eq!(provider.capacity().max_in_flight, Some(2));
        let feedback = Arc::new(CapacityFeedback::default());
        let (send, _receive) = mpsc::channel(8);
        provider
            .stream_with_capacity(request(), CancellationToken::new(), send, feedback.clone())
            .await?;
        let entries = feedback.entries()?;
        assert!(matches!(&entries[..], [
            ProviderObservation::Limits { requests: Some(window), tokens: None },
            ProviderObservation::Usage(usage),
        ] if window.limit == 20 && window.remaining == 7 && window.reset_at_ms == (now + 60) * 1000 && usage.input_tokens == 12 && usage.output_tokens == 4));
        fixture.request().await?;
    }
    Ok(())
}

#[tokio::test]
async fn typed_throttle_feedback_keeps_long_and_unknown_hints_distinct_without_retry() -> Result<()>
{
    for family in ["openai", "anthropic", "responses"] {
        for (status, hint, kind) in [
            (429, "2", "delay"),
            (503, "901", "long"),
            (429, "invalid", "unknown"),
        ] {
            let fixture = Fixture::with_headers(
                "private error details".into(),
                status,
                &[("retry-after", hint)],
            )
            .await;
            let provider =
                capacity_provider(family, &fixture.base_url, ProviderCapacity::default())?;
            let feedback = Arc::new(CapacityFeedback::default());
            let (send, _receive) = mpsc::channel(8);
            let error = provider
                .stream_with_capacity(request(), CancellationToken::new(), send, feedback.clone())
                .await
                .err()
                .context("throttle response succeeded")?;
            assert!(!format!("{error:#}").contains("private error details"));
            let entries = feedback.entries()?;
            assert!(
                matches!(&entries[..], [ProviderObservation::Throttled { retry }]
                if matches!((kind, retry), ("delay", ProviderRetry::Delay { millis: 2000 })
                    | ("long", ProviderRetry::BeyondHorizon { retry_at_ms: Some(_) })
                    | ("unknown", ProviderRetry::Unknown)))
            );
            fixture.request().await?;
        }
    }
    Ok(())
}

#[tokio::test]
async fn incomplete_or_malformed_usage_cannot_refund_a_provider_charge() -> Result<()> {
    for (usage, completed) in [
        (json!({"prompt_tokens":12,"completion_tokens":4}), false),
        (
            json!({"prompt_tokens":12,"completion_tokens":4,"prompt_tokens_details":{"cached_tokens":13}}),
            true,
        ),
    ] {
        let mut body = sse(&[json!({"choices":[], "usage":usage})]);
        if completed {
            body.push_str("data: [DONE]\r\n\r\n");
        }
        let fixture = Fixture::start(body).await;
        let provider = openai(&fixture.base_url)?;
        let feedback = Arc::new(CapacityFeedback::default());
        let (send, _receive) = mpsc::channel(8);
        let result = provider
            .stream_with_capacity(request(), CancellationToken::new(), send, feedback.clone())
            .await;
        assert_eq!(result.is_ok(), completed);
        assert!(feedback.entries()?.is_empty());
        fixture.request().await?;
    }
    Ok(())
}

async fn collect(provider: &dyn Provider) -> Result<(ModelResponse, String)> {
    let (send, mut receive) = mpsc::channel(8);
    let response = tokio::time::timeout(
        Duration::from_secs(5),
        provider.stream(request(), CancellationToken::new(), send),
    )
    .await??;
    let mut text = String::new();
    while let Ok(ModelDelta::Text(delta)) = receive.try_recv() {
        text.push_str(&delta);
    }
    Ok((response, text))
}

fn sse(events: &[Value]) -> String {
    let mut body = String::new();
    for event in events {
        body.push_str("data: ");
        body.push_str(&event.to_string());
        body.push_str("\r\n\r\n");
    }
    body
}

#[tokio::test]
async fn openai_http_contract_preserves_history_streams_tools_and_usage() -> Result<()> {
    let mut events = sse(&[
        json!({"choices":[{"delta":{"content":"雪 ","tool_calls":[{"index":0,"id":"new-","function":{"name":"ins","arguments":"{\"path\":"}}]},"finish_reason":null}]}),
        json!({"choices":[{"delta":{"content":"ready","tool_calls":[{"index":0,"id":"id","function":{"name":"pect","arguments":"\"src\"}"}}]},"finish_reason":null}]}),
        json!({"choices":[{"delta":{},"finish_reason":"tool_calls"}]}),
        json!({"choices":[],"usage":{"prompt_tokens":42,"completion_tokens":7,"prompt_tokens_details":{"cached_tokens":10},"completion_tokens_details":{"reasoning_tokens":3}}}),
    ]);
    events.push_str("data: [DONE]\r\n\r\n");
    let fixture = Fixture::start(events).await;
    let (response, text) = collect(&openai(&fixture.base_url)?).await?;
    assert_eq!(text, "雪 ready");
    assert_eq!(response.content, text);
    assert_eq!(response.usage.input_tokens, 42);
    assert_eq!(response.usage.output_tokens, 7);
    assert_eq!(response.usage.cached_input_tokens, Some(10));
    assert_eq!(response.usage.reasoning_output_tokens, Some(3));
    assert_eq!(response.tool_calls.len(), 1);
    assert_eq!(response.tool_calls[0].id, "new-id");
    assert_eq!(response.tool_calls[0].name, "inspect");
    assert_eq!(response.tool_calls[0].arguments, json!({"path":"src"}));
    let received = fixture.request().await?;
    let body: Value = received.body_json()?;
    assert_eq!(received.method, "POST");
    assert_eq!(received.url.path(), "/v1/chat/completions");
    assert_eq!(
        received
            .headers
            .get("authorization")
            .and_then(|value| value.to_str().ok()),
        Some("Bearer fixture-key")
    );
    assert_eq!(body["model"], "fixture-model");
    assert_eq!(body["stream"], true);
    assert_eq!(body["stream_options"]["include_usage"], true);
    assert_eq!(body["max_tokens"], 123);
    assert_eq!(
        body["messages"][0],
        json!({"role":"system","content":"Inspect the repository."})
    );
    assert_eq!(
        body["messages"][2]["tool_calls"][0]["function"]["arguments"],
        "{\"path\":\"src\"}"
    );
    assert_eq!(body["messages"][3]["tool_call_id"], "prior-a");
    assert_eq!(body["messages"][3]["content"], FIRST_RESULT);
    assert_eq!(body["messages"][4]["content"], "second result");
    assert_eq!(
        body["tools"][0]["function"]["parameters"],
        request().tools[0].input_schema
    );
    Ok(())
}

#[tokio::test]
async fn anthropic_http_contract_groups_results_and_preserves_initial_tool_input() -> Result<()> {
    let fixture = Fixture::start(sse(&[
        json!({"type":"message_start","message":{"usage":{"input_tokens":43}}}),
        json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":"雪 "}}),
        json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"ready"}}),
        json!({"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"initial-input","name":"inspect","input":{"path":"initial"}}}),
        json!({"type":"content_block_start","index":2,"content_block":{"type":"tool_use","id":"streamed-input","name":"inspect","input":{}}}),
        json!({"type":"content_block_delta","index":2,"delta":{"type":"input_json_delta","partial_json":"{\"path\":"}}),
        json!({"type":"content_block_delta","index":2,"delta":{"type":"input_json_delta","partial_json":"\"streamed\"}"}}),
        json!({"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":8}}),
        json!({"type":"message_stop"}),
    ])).await;
    let (response, text) = collect(&anthropic(&fixture.base_url)?).await?;
    assert_eq!(text, "雪 ready");
    assert_eq!(response.content, text);
    assert_eq!(response.usage.input_tokens, 43);
    assert_eq!(response.usage.output_tokens, 8);
    assert_eq!(response.tool_calls.len(), 2);
    assert_eq!(response.tool_calls[0].arguments, json!({"path":"initial"}));
    assert_eq!(response.tool_calls[1].arguments, json!({"path":"streamed"}));
    let received = fixture.request().await?;
    let body: Value = received.body_json()?;
    assert_eq!(received.method, "POST");
    assert_eq!(received.url.path(), "/v1/messages");
    assert_eq!(
        received
            .headers
            .get("x-api-key")
            .and_then(|value| value.to_str().ok()),
        Some("fixture-key")
    );
    assert_eq!(
        received
            .headers
            .get("anthropic-version")
            .and_then(|value| value.to_str().ok()),
        Some("2023-06-01")
    );
    assert!(!received.headers.contains_key("authorization"));
    assert_eq!(body["system"], "Inspect the repository.");
    assert_eq!(body["max_tokens"], 123);
    assert_eq!(
        body["messages"]
            .as_array()
            .context("missing messages")?
            .len(),
        3
    );
    assert_eq!(
        body["messages"][2]["content"],
        json!([
            {"type":"tool_result","tool_use_id":"prior-a","content":FIRST_RESULT,"is_error":true},
            {"type":"tool_result","tool_use_id":"prior-b","content":"second result","is_error":false},
            {"type":"text","text":"Continue."},
        ])
    );
    assert_eq!(
        body["tools"][0]["input_schema"],
        request().tools[0].input_schema
    );
    Ok(())
}

#[tokio::test]
async fn openai_rejects_incomplete_error_and_invalid_tool_streams() -> Result<()> {
    for (body, expected) in [
        (
            sse(&[json!({"choices":[{"delta":{"content":"partial"},"finish_reason":null}]})]),
            "completion marker",
        ),
        (
            sse(&[json!({"choices":[{"delta":{},"finish_reason":"length"}]})]),
            "incomplete",
        ),
        (
            sse(&[json!({"error":{"message":"fixture failure"}})]),
            "stream error",
        ),
        (
            format!(
                "{}data: [DONE]\n\n",
                sse(&[
                    json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"a","function":{"name":"inspect","arguments":"{"}}]},"finish_reason":"tool_calls"}]})
                ])
            ),
            "invalid tool JSON",
        ),
        ("data: {\"choices\":[]}".into(), "completion marker"),
    ] {
        let fixture = Fixture::start(body).await;
        let error = collect(&openai(&fixture.base_url)?)
            .await
            .err()
            .context("invalid stream was accepted")?;
        ensure!(
            error.to_string().contains(expected),
            "expected {expected}, got {error:#}"
        );
        fixture.request().await?;
    }
    Ok(())
}

#[tokio::test]
async fn anthropic_rejects_incomplete_error_and_invalid_tool_streams() -> Result<()> {
    let start = json!({"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"a","name":"inspect","input":{}}});
    for (events, expected) in [
        (
            vec![
                json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":"partial"}}),
            ],
            "before message_stop",
        ),
        (
            vec![
                json!({"type":"message_delta","delta":{"stop_reason":"max_tokens"}}),
                json!({"type":"message_stop"}),
            ],
            "incomplete",
        ),
        (
            vec![json!({"type":"error","error":{"message":"fixture failure"}})],
            "stream error",
        ),
        (
            vec![
                start.clone(),
                json!({"type":"content_block_delta","index":0,"delta":{"partial_json":"{"}}),
                json!({"type":"message_stop"}),
            ],
            "invalid tool JSON",
        ),
        (vec![start.clone(), start], "duplicate tool block index"),
        (
            vec![json!({"type":"content_block_delta","index":0,"delta":{"partial_json":"{}"}})],
            "preceded its start",
        ),
    ] {
        let fixture = Fixture::start(sse(&events)).await;
        let error = collect(&anthropic(&fixture.base_url)?)
            .await
            .err()
            .context("invalid stream was accepted")?;
        ensure!(
            error.to_string().contains(expected),
            "expected {expected}, got {error:#}"
        );
        fixture.request().await?;
    }
    Ok(())
}

#[tokio::test]
async fn anthropic_rejects_oversized_tool_identity() -> Result<()> {
    let fixture = Fixture::start(sse(&[
        json!({"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"x".repeat(256),"name":"inspect","input":{}}}),
        json!({"type":"message_stop"}),
    ])).await;
    let result = collect(&anthropic(&fixture.base_url)?).await;
    let Err(error) = result else {
        bail!("oversized identity was accepted");
    };
    assert!(error.to_string().contains("exceeds limit"));
    fixture.request().await?;
    Ok(())
}
