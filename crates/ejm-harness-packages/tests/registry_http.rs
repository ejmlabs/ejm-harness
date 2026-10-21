use anyhow::Result;
use ejm_harness_packages::{RegistryClient, pack_directory};
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
        source.path().join("ejm.package.json"),
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
fn registry_origin_rejects_plain_http_credentials_and_paths() {
    for origin in [
        "http://registry.example",
        "https://user:secret@registry.example",
        "https://registry.example/publish",
        "https://registry.example/?key=secret",
    ] {
        assert!(RegistryClient::new(origin).is_err());
    }
}
