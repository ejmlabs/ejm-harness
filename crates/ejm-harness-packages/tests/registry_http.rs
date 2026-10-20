use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use ejm_harness_packages::{
    DEFAULT_REGISTRY, DISCARD_RETIRED_REGISTRY_COMMAND, RegistryClient, pack_directory,
    registry_origin_is_default,
};
use serde_json::json;
use tokio_util::sync::CancellationToken;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_json, method, path},
};

#[tokio::test]
async fn device_code_and_polling_use_the_registered_cli_identity() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/auth/device/code"))
        .and(body_json(json!({"client_id":"ejm-cli","scope":"registry"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "device_code":"local-device-code",
            "user_code":"TEST-CODE",
            "verification_uri":format!("{}/device", server.uri()),
            "expires_in":600,
            "interval":5
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/auth/device/token"))
        .and(body_json(json!({
            "client_id":"ejm-cli",
            "device_code":"local-device-code",
            "grant_type":"urn:ietf:params:oauth:grant-type:device_code"
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token":"local-fixture-token"
        })))
        .expect(1)
        .mount(&server)
        .await;
    let client = RegistryClient::new(&server.uri())?;
    let code = client.device_code().await?;
    let token = client
        .wait_for_login(&code, CancellationToken::new())
        .await?;
    assert_eq!(token.expose_secret(), "local-fixture-token");
    Ok(())
}

#[tokio::test]
async fn pinned_download_checks_identity_hash_and_does_not_follow_redirects() -> Result<()> {
    let server = MockServer::start().await;
    let source = tempfile::tempdir()?;
    let manifest = json!({"schema_version":1,"name":"@example/theme","version":"1.0.0","description":"Fixture","exports":{}});
    std::fs::write(
        source.path().join("gerry.package.json"),
        serde_json::to_vec(&manifest)?,
    )?;
    let archive = pack_directory(source.path())?;
    let record = json!({"manifest":manifest,"sha256":ejm_harness_package_spec::archive_sha256(&archive),"size":archive.len(),"download_url":"/archive"});
    Mock::given(method("GET"))
        .and(path("/v1/packages/example/theme/versions/1.0.0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(record))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/archive"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(archive.clone()))
        .expect(1)
        .mount(&server)
        .await;
    let client = RegistryClient::new(&server.uri())?;
    let record = client.version("@example/theme", "1.0.0").await?;
    assert_eq!(client.download(&record).await?, archive);
    server.reset().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(302)
                .insert_header("Location", "https://attacker.invalid/archive"),
        )
        .mount(&server)
        .await;
    assert!(client.version("@example/theme", "1.0.0").await.is_err());
    Ok(())
}

#[tokio::test]
async fn cross_origin_download_and_identity_substitution_are_rejected() -> Result<()> {
    let server = MockServer::start().await;
    let client = RegistryClient::new(&server.uri())?;
    for (name, download_url) in [
        ("@attacker/theme", "/archive"),
        ("@example/theme", "https://attacker.invalid/archive"),
    ] {
        Mock::given(method("GET")).respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "manifest":{"schema_version":1,"name":name,"version":"1.0.0","description":"Fixture","exports":{}},
            "sha256":"a".repeat(64),"size":123,"download_url":download_url
        }))).mount(&server).await;
        assert!(client.version("@example/theme", "1.0.0").await.is_err());
        server.reset().await;
    }
    Ok(())
}

#[test]
fn registry_origin_rejects_plain_http_credentials_paths_and_the_retired_host() -> Result<()> {
    for origin in [
        "http://registry.example",
        "https://user:secret@registry.example",
        "https://registry.example/publish",
        "https://registry.example/?key=secret",
        "http://127.0.0.2",
        "http://127.0.0.2:8787",
    ] {
        let Err(error) = RegistryClient::new(origin) else {
            bail!("{origin} was accepted");
        };
        let message = format!("{error:#}");
        ensure!(!message.contains("retired"), "{origin}: {message}");
    }
    for origin in [
        "https://harness.meetgerry.ai",
        "https://other.meetgerry.ai",
        "https://meetgerry.ai",
        "http://harness.meetgerry.ai",
        "https://user:secret@harness.meetgerry.ai",
        "https://harness.meetgerry.ai/publish",
    ] {
        let Err(error) = RegistryClient::new(origin) else {
            bail!("{origin} was accepted");
        };
        let message = format!("{error:#}");
        ensure!(message.contains("retired"), "{origin}: {message}");
        ensure!(message.contains(DEFAULT_REGISTRY), "{origin}: {message}");
        ensure!(
            message.contains(DISCARD_RETIRED_REGISTRY_COMMAND),
            "{origin}: {message}"
        );
    }
    RegistryClient::new(DEFAULT_REGISTRY)?;
    RegistryClient::new("http://127.0.0.1:8787")?;
    RegistryClient::new("http://localhost:8787")?;
    RegistryClient::new("http://[::1]:8787")?;
    RegistryClient::new("https://notmeetgerry.ai")?;
    ensure!(registry_origin_is_default("https://registry.ejmlabs.com/")?);
    ensure!(!registry_origin_is_default("https://notmeetgerry.ai")?);
    Ok(())
}

#[tokio::test]
async fn loopback_http_registry_ignores_http_proxy() -> Result<()> {
    if std::env::var_os("EJM_REGISTRY_PROXY_CHILD").is_some() {
        let origin = std::env::var("EJM_REGISTRY_PROXY_ORIGIN")?;
        RegistryClient::new(&origin)?
            .search("proxy-check", None)
            .await?;
        return Ok(());
    }
    let registry = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/packages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"packages":[]})))
        .expect(1)
        .mount(&registry)
        .await;
    let proxy = MockServer::start().await;
    let output = tokio::time::timeout(
        Duration::from_secs(20),
        tokio::process::Command::new(std::env::current_exe()?)
            .env("EJM_REGISTRY_PROXY_CHILD", "1")
            .env("EJM_REGISTRY_PROXY_ORIGIN", registry.uri())
            .env("HTTP_PROXY", proxy.uri())
            .env("http_proxy", proxy.uri())
            .env("ALL_PROXY", proxy.uri())
            .env("all_proxy", proxy.uri())
            .env_remove("NO_PROXY")
            .env_remove("no_proxy")
            .env_remove("HTTPS_PROXY")
            .env_remove("https_proxy")
            .arg("--exact")
            .arg("loopback_http_registry_ignores_http_proxy")
            .arg("--test-threads=1")
            .output(),
    )
    .await
    .context("loopback proxy child timed out")??;
    ensure!(
        output.status.success(),
        "loopback proxy child failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let registry_hits = registry
        .received_requests()
        .await
        .context("registry hits")?;
    let proxy_hits = proxy.received_requests().await.context("proxy hits")?;
    ensure!(
        registry_hits.len() == 1,
        "registry hits: {}",
        registry_hits.len()
    );
    ensure!(proxy_hits.is_empty(), "proxy hits: {}", proxy_hits.len());
    Ok(())
}
