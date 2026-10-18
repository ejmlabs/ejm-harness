use std::time::Duration;

use anyhow::{Context, Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use ejm_harness_package_spec::{MAX_ARCHIVE_BYTES, archive_sha256};
use futures_util::StreamExt;
use reqwest::{Client, Url, redirect::Policy};
use serde::Deserialize;
use sha2::{Digest, Sha512};
use tokio_util::sync::CancellationToken;

use super::archives;

const MAX_METADATA_BYTES: usize = 256 * 1024;

pub(super) struct Downloads {
    http: Client,
    npm: Url,
    pypi: Url,
    wheels: Url,
    github: Url,
}

impl Downloads {
    pub(super) fn new() -> Result<Self> {
        Ok(Self {
            http: Client::builder()
                .redirect(Policy::none())
                .connect_timeout(Duration::from_secs(10))
                .timeout(Duration::from_secs(30))
                .build()?,
            npm: Url::parse("https://registry.npmjs.org/")?,
            pypi: Url::parse("https://pypi.org/")?,
            wheels: Url::parse("https://files.pythonhosted.org/")?,
            github: Url::parse("https://codeload.github.com/")?,
        })
    }

    pub(super) async fn npm(&self, spec: &str, cancel: &CancellationToken) -> Result<Vec<u8>> {
        let (name, version) = spec
            .rsplit_once('@')
            .context("npm source requires name@exact-version")?;
        ensure!(
            semver::Version::parse(version)?.to_string() == version,
            "npm source requires exact semver"
        );
        let package = name.strip_prefix('@').unwrap_or(name);
        ensure!(
            !package.is_empty()
                && package.len() <= 214
                && package.bytes().all(|byte| byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'-' | b'_' | b'.' | b'/'))
                && if name.starts_with('@') {
                    package.split('/').count() == 2
                } else {
                    !package.contains('/')
                }
                && package
                    .split('/')
                    .all(|part| !part.is_empty() && !part.starts_with('.')),
            "invalid public npm package name"
        );
        let mut url = self.npm.clone();
        url.path_segments_mut()
            .map_err(|()| anyhow::anyhow!("invalid npm origin"))?
            .extend([name, version]);
        let metadata: NpmMetadata =
            serde_json::from_slice(&self.bytes(url, MAX_METADATA_BYTES, cancel).await?)?;
        ensure!(
            metadata.name == name && metadata.version == version,
            "npm returned a different package identity"
        );
        let url = checked_url(&metadata.dist.tarball, &self.npm)?;
        let bytes = self.bytes(url, MAX_ARCHIVE_BYTES, cancel).await?;
        ensure!(
            metadata.dist.integrity
                == format!("sha512-{}", STANDARD.encode(Sha512::digest(&bytes))),
            "npm requires matching SHA-512 archive integrity"
        );
        archives::tar(&bytes, Some("package"), false)
    }

    pub(super) async fn pypi(&self, spec: &str, cancel: &CancellationToken) -> Result<Vec<u8>> {
        let (name, version) = spec
            .split_once("==")
            .context("PyPI source requires name==exact-version")?;
        ensure!(
            valid_pypi_name(name)
                && !version.is_empty()
                && version.len() <= 48
                && version.bytes().all(|byte| byte.is_ascii_alphanumeric()
                    || matches!(byte, b'.' | b'+' | b'-' | b'!')),
            "invalid exact public PyPI source"
        );
        let mut url = self.pypi.clone();
        url.path_segments_mut()
            .map_err(|()| anyhow::anyhow!("invalid PyPI origin"))?
            .extend(["pypi", name, version, "json"]);
        let metadata: PypiMetadata =
            serde_json::from_slice(&self.bytes(url, MAX_METADATA_BYTES, cancel).await?)?;
        ensure!(
            normalized_name(&metadata.info.name) == normalized_name(name)
                && metadata.info.version == version,
            "PyPI returned a different package identity"
        );
        let mut wheels = metadata.urls.iter().filter(|file| {
            file.packagetype == "bdist_wheel"
                && !file.yanked
                && (file.filename.ends_with("-py3-none-any.whl")
                    || file.filename.ends_with("-py2.py3-none-any.whl"))
        });
        let wheel = wheels.next().context("PyPI package needs one portable py3-none-any wheel; bundle unsupported distributions locally")?;
        ensure!(wheels.next().is_none(), "ambiguous portable PyPI wheel");
        ensure!(wheel.size <= MAX_ARCHIVE_BYTES, "PyPI wheel exceeds 8 MiB");
        let url = checked_url(&wheel.url, &self.wheels)?;
        let bytes = self.bytes(url, MAX_ARCHIVE_BYTES, cancel).await?;
        ensure!(
            bytes.len() == wheel.size && archive_sha256(&bytes) == wheel.digests.sha256,
            "PyPI wheel integrity check failed"
        );
        archives::wheel(&bytes)
    }

    pub(super) async fn git(&self, spec: &str, cancel: &CancellationToken) -> Result<Vec<u8>> {
        let (url, commit) = spec
            .rsplit_once('#')
            .context("Git source requires https://github.com/OWNER/REPO#COMMIT")?;
        let parsed = checked_url(url, &Url::parse("https://github.com/")?)?;
        ensure!(
            commit.len() == 40 && commit.bytes().all(|byte| byte.is_ascii_hexdigit()),
            "GitHub source must pin a full 40-character commit"
        );
        let path = parsed.path().trim_start_matches('/');
        let path = path.strip_suffix(".git").unwrap_or(path);
        let parts: Vec<_> = path.split('/').collect();
        ensure!(
            parts.len() == 2
                && parts.iter().all(|part| !part.is_empty()
                    && !matches!(*part, "." | "..")
                    && part
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric()
                            || matches!(byte, b'-' | b'_' | b'.'))),
            "GitHub source requires one owner and repository"
        );
        let mut url = self.github.clone();
        url.path_segments_mut()
            .map_err(|()| anyhow::anyhow!("invalid GitHub archive origin"))?
            .extend([parts[0], parts[1], "tar.gz", commit]);
        let bytes = self.bytes(url, MAX_ARCHIVE_BYTES, cancel).await?;
        archives::tar(&bytes, None, true)
    }

    async fn bytes(&self, url: Url, max: usize, cancel: &CancellationToken) -> Result<Vec<u8>> {
        tokio::select! {
            biased;
            () = cancel.cancelled() => anyhow::bail!("package install cancelled"),
            result = async {
                let response = self.http.get(url).send().await?;
                ensure!(response.status().is_success(), "package source request failed (HTTP {})", response.status());
                ensure!(response.content_length().is_none_or(|size| size <= max as u64), "package source exceeds byte limit");
                let mut stream = response.bytes_stream();
                let mut bytes = Vec::new();
                while let Some(chunk) = stream.next().await {
                    let chunk = chunk?;
                    ensure!(bytes.len().saturating_add(chunk.len()) <= max, "package source exceeds byte limit");
                    bytes.extend_from_slice(&chunk);
                }
                Ok(bytes)
            } => result,
        }
    }
}

fn checked_url(value: &str, origin: &Url) -> Result<Url> {
    let url = Url::parse(value)?;
    ensure!(
        url.origin() == origin.origin()
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none(),
        "unsupported package download origin or credentials"
    );
    Ok(url)
}

fn valid_pypi_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 100
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        && name
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
        && name
            .as_bytes()
            .last()
            .is_some_and(u8::is_ascii_alphanumeric)
}

fn normalized_name(name: &str) -> String {
    name.split(['-', '_', '.'])
        .filter(|part| !part.is_empty())
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>()
        .join("-")
}

#[derive(Deserialize)]
struct NpmMetadata {
    name: String,
    version: String,
    dist: NpmDist,
}

#[derive(Deserialize)]
struct NpmDist {
    tarball: String,
    integrity: String,
}

#[derive(Deserialize)]
struct PypiMetadata {
    info: PypiInfo,
    urls: Vec<PypiFile>,
}

#[derive(Deserialize)]
struct PypiInfo {
    name: String,
    version: String,
}

#[derive(Deserialize)]
struct PypiFile {
    filename: String,
    packagetype: String,
    url: String,
    size: usize,
    digests: PypiDigests,
    #[serde(default)]
    yanked: bool,
}

#[derive(Deserialize)]
struct PypiDigests {
    sha256: String,
}

#[cfg(test)]
pub(super) fn fixture_downloads(origin: &str) -> Result<Downloads> {
    let mut downloads = Downloads::new()?;
    let origin = Url::parse(origin)?;
    downloads.npm = origin.clone();
    downloads.pypi = origin.clone();
    downloads.wheels = origin.clone();
    downloads.github = origin;
    Ok(downloads)
}
