//! Own process so `HTTP_PROXY` and `ALL_PROXY` cannot reach the other test binaries.

use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::{Context, Result, bail, ensure};
use ejm_harness::{
    AnthropicConfig, AnthropicProvider, Message, ModelRequest, OpenAiConfig, OpenAiProvider,
    Provider, ResponsesConfig, ResponsesProvider,
};
use ejm_harness_auth::{SecretString, StaticCredential};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::TcpListener,
    sync::mpsc,
    time::timeout,
};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

const KEY: &str = "planted-proxy-key-7c2e";
const TEST_NAME: &str = "plain_http_loopback_does_not_send_the_key_to_a_proxy";

#[tokio::test]
async fn plain_http_loopback_does_not_send_the_key_to_a_proxy() -> Result<()> {
    match std::env::var("EJM_HARNESS_PROXY_ROLE") {
        Ok(role) if role == "http" => http_child().await,
        Ok(role) if role == "all_proxy" => all_proxy_child().await,
        Ok(role) if role == "https" => https_child().await,
        Ok(role) => bail!("unknown proxy role {role}"),
        Err(std::env::VarError::NotPresent) => parent().await,
        Err(error) => bail!("proxy role is not valid text: {error}"),
    }
}

async fn parent() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let proxy = listener.local_addr()?;
    let captured = Arc::new(Mutex::new(Vec::<u8>::new()));
    let store = Arc::clone(&captured);
    let accept = tokio::spawn(async move { accept_proxy(listener, store).await });
    let proxy_url = format!("http://{proxy}");

    let cleartext = run_child("http", &proxy_url).await?;
    let cleartext_capture = snapshot(&captured);
    if !cleartext.status.success() || !cleartext_capture.is_empty() {
        bail!(
            "plain http reached the proxy or the child failed\nstatus: {:?}\nbytes: {}\nstdout: {}\nstderr: {}",
            cleartext.status.code(),
            cleartext_capture.len(),
            clip(&cleartext.stdout),
            clip(&cleartext.stderr)
        );
    }

    let direct = run_child("all_proxy", &proxy_url).await?;
    let direct_capture = snapshot(&captured);
    if !direct.status.success() || !direct_capture.is_empty() {
        bail!(
            "all_proxy received a plain http localhost request or the child failed\nstatus: {:?}\nbytes: {}\nstdout: {}\nstderr: {}",
            direct.status.code(),
            direct_capture.len(),
            clip(&direct.stdout),
            clip(&direct.stderr)
        );
    }

    let tunnel = run_child("https", &proxy_url).await?;
    let tunnel_capture = snapshot(&captured);
    let tunnel_text = String::from_utf8_lossy(&tunnel_capture);
    if !tunnel.status.success()
        || !tunnel_text.to_ascii_uppercase().contains("CONNECT")
        || !tunnel_text.contains("api.example.invalid")
        || tunnel_text.contains(KEY)
    {
        bail!(
            "https did not keep the process proxy\nstatus: {:?}\nproxy: {}\nstderr: {}",
            tunnel.status.code(),
            clip(&tunnel_capture),
            clip(&tunnel.stderr)
        );
    }
    accept.abort();
    Ok(())
}

async fn run_child(role: &str, proxy_url: &str) -> Result<std::process::Output> {
    let mut command = tokio::process::Command::new(std::env::current_exe()?);
    command
        .arg("--exact")
        .arg(TEST_NAME)
        .arg("--test-threads=1")
        .env("EJM_HARNESS_PROXY_ROLE", role)
        .env_remove("NO_PROXY")
        .env_remove("no_proxy")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    match role {
        "http" => {
            command
                .env("HTTP_PROXY", proxy_url)
                .env("http_proxy", proxy_url)
                .env("ALL_PROXY", proxy_url)
                .env("all_proxy", proxy_url)
                .env_remove("HTTPS_PROXY")
                .env_remove("https_proxy");
        }
        "all_proxy" => {
            command
                .env("all_proxy", proxy_url)
                .env_remove("HTTP_PROXY")
                .env_remove("http_proxy")
                .env_remove("ALL_PROXY")
                .env_remove("HTTPS_PROXY")
                .env_remove("https_proxy");
        }
        "https" => {
            command
                .env("HTTPS_PROXY", proxy_url)
                .env("https_proxy", proxy_url)
                .env_remove("HTTP_PROXY")
                .env_remove("http_proxy")
                .env_remove("ALL_PROXY")
                .env_remove("all_proxy");
        }
        _ => bail!("unknown proxy role {role}"),
    }
    command
        .output()
        .await
        .context("proxy child failed to start")
}

async fn accept_proxy(listener: TcpListener, captured: Arc<Mutex<Vec<u8>>>) {
    loop {
        let Ok((mut socket, _)) = listener.accept().await else {
            return;
        };
        let mut buf = [0_u8; 4096];
        loop {
            match timeout(Duration::from_millis(300), socket.read(&mut buf)).await {
                Ok(Ok(0) | Err(_)) | Err(_) => break,
                Ok(Ok(count)) => {
                    let mut guard = captured
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    guard.extend_from_slice(&buf[..count]);
                }
            }
        }
    }
}

fn snapshot(captured: &Mutex<Vec<u8>>) -> Vec<u8> {
    captured
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}

fn clip(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).chars().take(2_000).collect()
}

async fn all_proxy_child() -> Result<()> {
    let (ipv4, ipv6, port) = localhost_pair().await?;
    let before_v4 = request_count(&ipv4).await?;
    let before_v6 = request_count(&ipv6).await?;
    stream_openai(&format!("http://localhost:{port}/v1")).await?;
    let added = request_count(&ipv4).await?.saturating_sub(before_v4)
        + request_count(&ipv6).await?.saturating_sub(before_v6);
    ensure!(added == 1, "all_proxy localhost sent {added} requests");
    assert_new_requests_carry_the_key(&ipv4, before_v4).await?;
    assert_new_requests_carry_the_key(&ipv6, before_v6).await?;
    Ok(())
}

async fn http_child() -> Result<()> {
    let ipv4 = start_server("127.0.0.1:0").await?;
    let ipv6 = start_server("[::1]:0").await?;
    let (local_v4, local_v6, port) = localhost_pair().await?;
    probe(
        &ipv4,
        &format!("http://127.0.0.1:{}/v1", ipv4.address().port()),
        true,
    )
    .await?;
    probe(
        &ipv6,
        &format!("http://[::1]:{}/v1", ipv6.address().port()),
        false,
    )
    .await?;
    probe_localhost(&local_v4, &local_v6, port).await?;
    Ok(())
}

async fn localhost_pair() -> Result<(MockServer, MockServer, u16)> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").context("bind localhost v4")?;
    let port = listener.local_addr()?.port();
    let v6_listener =
        std::net::TcpListener::bind(SocketAddr::from((std::net::Ipv6Addr::LOCALHOST, port)))
            .context("bind localhost v6")?;
    listener.set_nonblocking(true)?;
    v6_listener.set_nonblocking(true)?;
    let ipv4 = MockServer::builder().listener(listener).start().await;
    let ipv6 = MockServer::builder().listener(v6_listener).start().await;
    mount(&ipv4).await;
    mount(&ipv6).await;
    Ok((ipv4, ipv6, port))
}

async fn probe_localhost(ipv4: &MockServer, ipv6: &MockServer, port: u16) -> Result<()> {
    let before_v4 = request_count(ipv4).await?;
    let before_v6 = request_count(ipv6).await?;
    let base = format!("http://localhost:{port}/v1");
    stream_openai(&base).await?;
    stream_anthropic(&base).await?;
    stream_worker(&base).await?;
    let added = request_count(ipv4).await?.saturating_sub(before_v4)
        + request_count(ipv6).await?.saturating_sub(before_v6);
    ensure!(added == 3, "localhost sent {added} requests");
    assert_new_requests_carry_the_key(ipv4, before_v4).await?;
    assert_new_requests_carry_the_key(ipv6, before_v6).await?;
    Ok(())
}

async fn probe(server: &MockServer, base: &str, include_responses: bool) -> Result<()> {
    let before = request_count(server).await?;
    stream_openai(base).await?;
    stream_anthropic(base).await?;
    stream_worker(base).await?;
    if include_responses {
        stream_responses(base).await?;
    }
    let expected = if include_responses { 4 } else { 3 };
    let after = request_count(server).await?;
    ensure!(
        after == before + expected,
        "{base} sent {} new requests",
        after.saturating_sub(before)
    );
    assert_new_requests_carry_the_key(server, before).await
}

async fn assert_new_requests_carry_the_key(server: &MockServer, before: usize) -> Result<()> {
    let requests = server
        .received_requests()
        .await
        .context("request recording disabled")?;
    for request in requests.iter().skip(before) {
        let carries_key = request
            .headers
            .iter()
            .any(|(_, value)| value.to_str().is_ok_and(|text| text.contains(KEY)));
        ensure!(carries_key, "{} did not carry the key", request.url);
    }
    Ok(())
}

async fn request_count(server: &MockServer) -> Result<usize> {
    Ok(server
        .received_requests()
        .await
        .context("request recording disabled")?
        .len())
}

async fn https_child() -> Result<()> {
    let error = stream_openai("https://api.example.invalid/v1")
        .await
        .err()
        .context("https request succeeded without a tunnel")?;
    for rendered in [
        error.to_string(),
        format!("{error:#}"),
        format!("{error:?}"),
        format!("{error:#?}"),
    ] {
        assert!(!rendered.contains(KEY), "{rendered}");
    }
    Ok(())
}

async fn start_server(bind: impl std::net::ToSocketAddrs) -> Result<MockServer> {
    let listener = std::net::TcpListener::bind(bind).context("bind loopback target")?;
    listener.set_nonblocking(true)?;
    let server = MockServer::builder().listener(listener).start().await;
    mount(&server).await;
    Ok(server)
}

async fn mount(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(openai_sse(), "text/event-stream"))
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(anthropic_sse(), "text/event-stream"))
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(responses_sse(), "text/event-stream"))
        .mount(server)
        .await;
}

fn openai_sse() -> String {
    "data: {\"choices\":[{\"delta\":{\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}\r\n\r\ndata: [DONE]\r\n\r\n"
        .to_owned()
}

fn anthropic_sse() -> String {
    concat!(
        "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":1}}\r\n\r\n",
        "data: {\"type\":\"message_stop\"}\r\n\r\n"
    )
    .to_owned()
}

fn responses_sse() -> String {
    "data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"output\":[{\"type\":\"message\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"ok\"}]}],\"usage\":{\"input_tokens\":1,\"output_tokens\":1}}}\r\n\r\n"
        .to_owned()
}

fn prompt() -> ModelRequest {
    ModelRequest {
        system: "proxy probe".into(),
        messages: vec![Message::user("hi")],
        tools: vec![],
        max_output_tokens: 16,
    }
}

async fn stream_openai(base: &str) -> Result<()> {
    let provider = OpenAiProvider::new(OpenAiConfig {
        base_url: base.into(),
        api_key: KEY.into(),
        model: "fixture-model".into(),
    })?;
    stream(&provider).await
}

async fn stream_anthropic(base: &str) -> Result<()> {
    let provider = AnthropicProvider::new(AnthropicConfig {
        base_url: base.into(),
        api_key: KEY.into(),
        model: "fixture-model".into(),
    })?;
    stream(&provider).await
}

async fn stream_responses(base: &str) -> Result<()> {
    let provider = ResponsesProvider::for_endpoint(
        ResponsesConfig {
            model: "fixture-model".into(),
            reasoning_effort: None,
        },
        std::sync::Arc::new(StaticCredential::new(SecretString::new(KEY.to_owned()))),
        base,
    )?;
    stream(&provider).await
}

async fn stream(provider: &dyn Provider) -> Result<()> {
    let (send, mut receive) = mpsc::channel(8);
    let drain = tokio::spawn(async move { while receive.recv().await.is_some() {} });
    timeout(
        Duration::from_secs(5),
        provider.stream(prompt(), tokio_util::sync::CancellationToken::new(), send),
    )
    .await
    .context("provider stream timed out")??;
    drain.abort();
    Ok(())
}

async fn stream_worker(base: &str) -> Result<()> {
    let state = tempfile::tempdir()?;
    let (client_to_server, server_read) = tokio::io::duplex(64 * 1024);
    let (server_write, client_read) = tokio::io::duplex(64 * 1024);
    let served = tokio::spawn(async move {
        ejm_harness_worker::serve(BufReader::new(server_read), server_write).await
    });
    let (mut writer, mut reader) = (client_to_server, BufReader::new(client_read));
    rpc(
        &mut writer,
        &mut reader,
        json!({"id":"1","method":"hello","params":{"protocol":1}}),
    )
    .await?;
    rpc(
        &mut writer,
        &mut reader,
        json!({
            "id": "2",
            "method": "configure",
            "params": {
                "state_dir": state.path(),
                "provider": {
                    "kind": "openai",
                    "base_url": base,
                    "api_key": KEY,
                    "model": "fixture-model"
                }
            }
        }),
    )
    .await?;
    let session = rpc(
        &mut writer,
        &mut reader,
        json!({
            "id": "3",
            "method": "session.create",
            "params": {"cwd": state.path(), "title": "proxy"}
        }),
    )
    .await?;
    let session_id = session["id"].as_str().context("session id missing")?;
    rpc(
        &mut writer,
        &mut reader,
        json!({
            "id": "4",
            "method": "run.start",
            "params": {"session_id": session_id, "prompt": "hi"}
        }),
    )
    .await?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    let mut next_id = 5_u64;
    let mut finished = false;
    while tokio::time::Instant::now() < deadline {
        let events = rpc(
            &mut writer,
            &mut reader,
            json!({
                "id": next_id.to_string(),
                "method": "events.list",
                "params": {"session_id": session_id, "after": 0, "limit": 64}
            }),
        )
        .await?;
        next_id += 1;
        if events
            .as_array()
            .is_some_and(|events| events.iter().any(|event| event["type"] == "run_finished"))
        {
            finished = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    ensure!(finished, "worker run did not finish");
    let _ = rpc(
        &mut writer,
        &mut reader,
        json!({"id":"6","method":"shutdown","params":{}}),
    )
    .await;
    drop(writer);
    let _ = timeout(Duration::from_secs(2), served).await;
    Ok(())
}

async fn rpc<W, R>(writer: &mut W, reader: &mut R, message: Value) -> Result<Value>
where
    W: AsyncWriteExt + Unpin,
    R: AsyncBufReadExt + Unpin,
{
    let id = message["id"]
        .as_str()
        .context("request id missing")?
        .to_owned();
    let mut frame = serde_json::to_vec(&message)?;
    frame.push(b'\n');
    writer.write_all(&frame).await?;
    writer.flush().await?;
    let mut line = String::new();
    loop {
        line.clear();
        let read = timeout(Duration::from_secs(5), reader.read_line(&mut line)).await??;
        ensure!(read > 0, "worker closed the protocol stream");
        let parsed: Value = serde_json::from_str(&line).context("worker frame was not JSON")?;
        if parsed.get("id").and_then(Value::as_str) != Some(id.as_str()) {
            continue;
        }
        if let Some(error) = parsed.get("error") {
            bail!("worker returned {error}");
        }
        return Ok(parsed["result"].clone());
    }
}
