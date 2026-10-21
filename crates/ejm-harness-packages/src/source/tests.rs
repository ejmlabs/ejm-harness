use std::{io::Write, sync::Arc};

use anyhow::Result;
use base64::{Engine, engine::general_purpose::STANDARD};
use ejm_harness_exec::{Governor, ResourceLimits};
use ejm_harness_package_spec::{
    MANIFEST_FILE, MAX_ARCHIVE_BYTES, MAX_EXPANDED_BYTES, validate_archive,
};
use flate2::{Compression, write::GzEncoder};
use serde_json::json;
use sha2::{Digest, Sha512};
use tokio_util::sync::CancellationToken;
use wiremock::{Mock, MockServer, ResponseTemplate, matchers::method};

use super::{archives, download::fixture_downloads, install_source};
use crate::{PackageManager, RegistryClient};

fn manifest() -> Vec<u8> {
    serde_json::to_vec(&json!({
        "schema_version":1,"name":"@example/test","version":"1.0.0",
        "description":"Source fixture","exports":{}
    }))
    .unwrap_or_default()
}

fn tar_files(files: &[(&str, &[u8])]) -> Result<Vec<u8>> {
    let mut archive = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::fast()));
    for (path, bytes) in files {
        let mut header = tar::Header::new_gnu();
        header.set_size(u64::try_from(bytes.len())?);
        header.set_mode(0o644);
        header.set_cksum();
        archive.append_data(&mut header, path, *bytes)?;
    }
    Ok(archive.into_inner()?.finish()?)
}

fn wheel_files(files: &[(&str, &[u8])]) -> Result<Vec<u8>> {
    let mut archive = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (path, bytes) in files {
        archive.start_file(*path, zip::write::SimpleFileOptions::default())?;
        archive.write_all(bytes)?;
    }
    Ok(archive.finish()?.into_inner())
}

#[tokio::test]
async fn remote_references_cannot_be_shadowed_by_repository_files() -> Result<()> {
    const CHILD: &str = "EJM_HARNESS_SOURCE_SHADOW_FIXTURE";
    if std::env::var_os(CHILD).is_none() {
        let directory = tempfile::tempdir()?;
        let archive = tar_files(&[(MANIFEST_FILE, &manifest())])?;
        for name in [
            "npm:fixture@latest",
            "pypi:fixture",
            "git:fixture",
            "@example/test@latest",
        ] {
            let path = directory.path().join(name);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, &archive)?;
        }
        let output = std::process::Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "source::tests::remote_references_cannot_be_shadowed_by_repository_files",
                "--nocapture",
            ])
            .current_dir(directory.path())
            .env(CHILD, "1")
            .output()?;
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stderr),
            String::from_utf8_lossy(&output.stdout)
        );
        return Ok(());
    }
    let directory = tempfile::tempdir()?;
    let manager = PackageManager::new(directory.path().join("store"))?;
    let governor = Arc::new(Governor::new(ResourceLimits::default())?);
    for source in [
        "npm:fixture@latest",
        "pypi:fixture",
        "git:fixture",
        "@example/test@latest",
    ] {
        assert!(
            install_source(&manager, source, None, &governor, CancellationToken::new())
                .await
                .is_err()
        );
    }
    assert!(manager.list()?.is_empty());
    let record = install_source(
        &manager,
        "local:npm:fixture@latest",
        None,
        &governor,
        CancellationToken::new(),
    )
    .await?;
    assert_eq!(record.name, "@example/test");
    assert!(!record.enabled);
    assert_eq!(governor.snapshot().running, 0);
    Ok(())
}

#[tokio::test]
async fn registry_source_requires_configuration_before_installing() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let manager = PackageManager::new(directory.path().join("store"))?;
    let governor = Arc::new(Governor::new(ResourceLimits::default())?);
    let error = install_source(
        &manager,
        "@example/test@1.0.0",
        None,
        &governor,
        CancellationToken::new(),
    )
    .await;
    assert!(error.is_err_and(|error| error.to_string().contains("configured registry")));
    assert!(manager.list()?.is_empty());
    tokio::time::timeout(std::time::Duration::from_secs(2), governor.wait_idle()).await?;
    assert_eq!(governor.snapshot().running, 0);
    Ok(())
}

#[tokio::test]
async fn install_deadline_releases_admission_without_persisting() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_delay(std::time::Duration::from_secs(1)))
        .mount(&server)
        .await;
    let directory = tempfile::tempdir()?;
    let manager = PackageManager::new(directory.path().join("store"))?;
    let registry = RegistryClient::new(&server.uri())?;
    let governor = Arc::new(Governor::new(ResourceLimits {
        max_job_ms: 10,
        ..ResourceLimits::default()
    })?);
    let error = install_source(
        &manager,
        "@example/test@1.0.0",
        Some(&registry),
        &governor,
        CancellationToken::new(),
    )
    .await;
    assert!(error.is_err_and(|error| error.to_string().contains("deadline")));
    tokio::time::timeout(std::time::Duration::from_secs(5), governor.wait_idle()).await?;
    assert!(manager.list()?.is_empty());
    assert_eq!(governor.snapshot().running, 0);
    assert_eq!(governor.snapshot().queued, 0);
    Ok(())
}

#[tokio::test]
async fn npm_checks_metadata_integrity_and_download_origin() -> Result<()> {
    let server = MockServer::start().await;
    let downloads = fixture_downloads(&server.uri())?;
    let bytes = tar_files(&[("package/ejm.package.json", &manifest())])?;
    for (name, integrity, url, success) in [
        (
            "fixture",
            format!("sha512-{}", STANDARD.encode(Sha512::digest(&bytes))),
            format!("{}/archive", server.uri()),
            true,
        ),
        (
            "other",
            "sha512-wrong".into(),
            format!("{}/archive", server.uri()),
            false,
        ),
        (
            "fixture",
            "sha512-wrong".into(),
            format!("{}/archive", server.uri()),
            false,
        ),
        (
            "fixture",
            "sha512-wrong".into(),
            "http://127.0.0.1/private".into(),
            false,
        ),
    ] {
        Mock::given(method("GET"))
            .and(wiremock::matchers::path("/fixture/1.0.0"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "name":name,"version":"1.0.0","dist":{"tarball":url,"integrity":integrity}
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(wiremock::matchers::path("/archive"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(bytes.clone()))
            .mount(&server)
            .await;
        assert_eq!(
            downloads
                .npm("fixture@1.0.0", &CancellationToken::new())
                .await
                .is_ok(),
            success
        );
        server.reset().await;
    }
    Ok(())
}

#[tokio::test]
async fn source_http_rejects_redirects_oversized_bodies_and_cancellation() -> Result<()> {
    let server = MockServer::start().await;
    let downloads = fixture_downloads(&server.uri())?;
    for response in [
        ResponseTemplate::new(302).insert_header("Location", "https://attacker.invalid/archive"),
        ResponseTemplate::new(200).set_body_bytes(vec![b'x'; 256 * 1024 + 1]),
    ] {
        Mock::given(method("GET"))
            .respond_with(response)
            .mount(&server)
            .await;
        assert!(
            downloads
                .npm("fixture@1.0.0", &CancellationToken::new())
                .await
                .is_err()
        );
        server.reset().await;
    }
    let bytes = tar_files(&[("package/ejm.package.json", &manifest())])?;
    Mock::given(wiremock::matchers::path("/fixture/1.0.0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "name":"fixture","version":"1.0.0","dist":{"tarball":format!("{}/archive", server.uri()),"integrity":format!("sha512-{}",STANDARD.encode(Sha512::digest(&bytes)))}
        }))).mount(&server).await;
    Mock::given(wiremock::matchers::path("/archive"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![0; MAX_ARCHIVE_BYTES + 1]))
        .mount(&server)
        .await;
    assert!(
        downloads
            .npm("fixture@1.0.0", &CancellationToken::new())
            .await
            .is_err()
    );
    let cancel = CancellationToken::new();
    cancel.cancel();
    assert!(downloads.npm("fixture@1.0.0", &cancel).await.is_err());
    Ok(())
}

#[tokio::test]
async fn pypi_checks_portable_wheel_identity_size_and_hash() -> Result<()> {
    let server = MockServer::start().await;
    let downloads = fixture_downloads(&server.uri())?;
    let bytes = wheel_files(&[
        ("fixture/ejm.package.json", &manifest()),
        ("fixture/__init__.py", b""),
    ])?;
    for (name, version, filename, size, hash, success) in [
        (
            "Fixture_Name",
            "1.0.0",
            "fixture_name-1.0.0-py3-none-any.whl",
            bytes.len(),
            ejm_harness_package_spec::archive_sha256(&bytes),
            true,
        ),
        (
            "other",
            "1.0.0",
            "fixture_name-1.0.0-py3-none-any.whl",
            bytes.len(),
            ejm_harness_package_spec::archive_sha256(&bytes),
            false,
        ),
        (
            "Fixture_Name",
            "2.0.0",
            "fixture_name-1.0.0-py3-none-any.whl",
            bytes.len(),
            ejm_harness_package_spec::archive_sha256(&bytes),
            false,
        ),
        (
            "Fixture_Name",
            "1.0.0",
            "fixture_name-1.0.0-cp311-cp311-macos.whl",
            bytes.len(),
            ejm_harness_package_spec::archive_sha256(&bytes),
            false,
        ),
        (
            "Fixture_Name",
            "1.0.0",
            "fixture_name-1.0.0-py3-none-any.whl",
            bytes.len() + 1,
            ejm_harness_package_spec::archive_sha256(&bytes),
            false,
        ),
        (
            "Fixture_Name",
            "1.0.0",
            "fixture_name-1.0.0-py3-none-any.whl",
            bytes.len(),
            "a".repeat(64),
            false,
        ),
    ] {
        Mock::given(wiremock::matchers::path("/pypi/fixture-name/1.0.0/json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "info":{"name":name,"version":version},"urls":[{"filename":filename,"packagetype":"bdist_wheel","url":format!("{}/archive",server.uri()),"size":size,"digests":{"sha256":hash}}]
            }))).mount(&server).await;
        Mock::given(wiremock::matchers::path("/archive"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(bytes.clone()))
            .mount(&server)
            .await;
        assert_eq!(
            downloads
                .pypi("fixture-name==1.0.0", &CancellationToken::new())
                .await
                .is_ok(),
            success
        );
        server.reset().await;
    }
    Ok(())
}

#[test]
fn wheel_rejects_traversal_hooks_relocation_links_and_expansion() -> Result<()> {
    for path in [
        "../escape",
        "startup.pth",
        "fixture-1.0.0.data/purelib/escape",
    ] {
        let bytes = wheel_files(&[("fixture/ejm.package.json", &manifest()), (path, b"unsafe")])?;
        assert!(archives::wheel(&bytes).is_err());
    }
    let bytes = wheel_files(&[
        ("fixture/ejm.package.json", &manifest()),
        ("huge", &vec![0; MAX_EXPANDED_BYTES + 1]),
    ])?;
    assert!(archives::wheel(&bytes).is_err());
    let mut archive = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    archive.add_symlink(
        "fixture/linked",
        "outside",
        zip::write::SimpleFileOptions::default(),
    )?;
    assert!(archives::wheel(&archive.finish()?.into_inner()).is_err());
    Ok(())
}

#[test]
fn tar_rejects_wrong_roots_links_duplicates_and_expansion() -> Result<()> {
    let manifest = manifest();
    assert!(
        archives::tar(
            &tar_files(&[("other/ejm.package.json", &manifest)])?,
            Some("package"),
            false
        )
        .is_err()
    );
    assert!(
        archives::tar(
            &tar_files(&[
                ("package/ejm.package.json", &manifest),
                ("package/ejm.package.json", &manifest)
            ])?,
            Some("package"),
            false
        )
        .is_err()
    );
    let mut archive = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::fast()));
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(tar::EntryType::Symlink);
    header.set_size(0);
    header.set_link_name("outside")?;
    header.set_cksum();
    archive.append_data(&mut header, "package/linked", std::io::empty())?;
    assert!(archives::tar(&archive.into_inner()?.finish()?, Some("package"), false).is_err());
    let bomb = tar_files(&[("package/huge", &vec![0; MAX_EXPANDED_BYTES + 1])])?;
    assert!(archives::tar(&bomb, Some("package"), false).is_err());
    Ok(())
}

#[tokio::test]
async fn git_uses_only_exact_public_github_archives() -> Result<()> {
    let server = MockServer::start().await;
    let downloads = fixture_downloads(&server.uri())?;
    let bytes = tar_files(&[
        ("repo-commit/ejm.package.json", &manifest()),
        ("repo-commit/.github/ignored", b"ignored"),
    ])?;
    Mock::given(wiremock::matchers::path(format!(
        "/owner/repo/tar.gz/{}",
        "a".repeat(40)
    )))
    .respond_with(ResponseTemplate::new(200).set_body_bytes(bytes))
    .expect(1)
    .mount(&server)
    .await;
    let output = downloads
        .git(
            &format!("https://github.com/owner/repo.git#{}", "a".repeat(40)),
            &CancellationToken::new(),
        )
        .await?;
    assert_eq!(validate_archive(&output)?.name, "@example/test");
    for source in [
        "https://github.com/owner/repo#main".to_owned(),
        format!("https://attacker.invalid/owner/repo#{}", "a".repeat(40)),
        format!(
            "https://user:secret@github.com/owner/repo#{}",
            "a".repeat(40)
        ),
    ] {
        assert!(
            downloads
                .git(&source, &CancellationToken::new())
                .await
                .is_err()
        );
    }
    Ok(())
}
