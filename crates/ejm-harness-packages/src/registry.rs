//! Native registry client. Account tokens are never sent to package download hosts.

use std::{fmt, time::Duration};

use anyhow::{Context, Result, bail, ensure};
use ejm_harness_auth::SecretString;
use ejm_harness_package_spec::{
    MAX_ARCHIVE_BYTES, Manifest, archive_sha256, validate_archive, validate_name,
};
use futures_util::StreamExt;
use reqwest::{Client, Response, Url, redirect::Policy};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

mod credentials;
pub use credentials::{
    delete_registry_token, load_registry_token, migrate_registry_token, save_registry_token,
};

const MAX_JSON_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VersionRecord {
    pub manifest: Manifest,
    pub sha256: String,
    pub size: usize,
    pub download_url: String,
}

#[derive(Deserialize)]
pub struct DeviceCode {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    #[serde(default)]
    pub verification_uri_complete: Option<String>,
    pub expires_in: u64,
    #[serde(default = "default_poll_interval")]
    pub interval: u64,
}

impl fmt::Debug for DeviceCode {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        output
            .debug_struct("DeviceCode")
            .field("device_code", &"[REDACTED]")
            .field("expires_in", &self.expires_in)
            .finish_non_exhaustive()
    }
}

fn default_poll_interval() -> u64 {
    5
}

/// Fixed-origin client with no redirects and bounded responses.
#[derive(Clone, Debug)]
pub struct RegistryClient {
    origin: Url,
    http: Client,
}

impl RegistryClient {
    /// Connect to an HTTPS registry. Loopback HTTP is available for local development.
    ///
    /// # Errors
    /// Rejects unsafe origins, credentials in URLs, and HTTP client setup errors.
    pub fn new(origin: &str) -> Result<Self> {
        let origin = parse_origin(origin)?;
        let http = Client::builder()
            .redirect(Policy::none())
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .user_agent(concat!("ejm-harness/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self { origin, http })
    }

    #[must_use]
    pub fn origin(&self) -> &str {
        self.origin.as_str()
    }

    /// Search ready public packages. Results never imply maintainer approval.
    ///
    /// # Errors
    /// Returns bounded transport or registry failures.
    pub async fn search(&self, query: &str, cursor: Option<&str>) -> Result<Value> {
        ensure!(query.len() <= 128, "search query exceeds 128 bytes");
        let mut url = self.url("v1/packages")?;
        url.query_pairs_mut()
            .append_pair("q", query)
            .append_pair("limit", "20");
        if let Some(cursor) = cursor {
            ensure!(cursor.len() <= 128, "invalid cursor");
            url.query_pairs_mut().append_pair("cursor", cursor);
        }
        json_response(self.http.get(url).send().await?).await
    }

    /// Resolve an exact published version before downloading its archive.
    ///
    /// # Errors
    /// Rejects mismatched identities, digests, unsafe URLs, and unavailable versions.
    pub async fn version(&self, name: &str, version: &str) -> Result<VersionRecord> {
        validate_name(name)?;
        ensure!(
            semver::Version::parse(version)?.to_string() == version,
            "use an exact semver version"
        );
        let path = format!("v1/packages/{}/versions/{version}", &name[1..]);
        let record: VersionRecord =
            json_response(self.http.get(self.url(&path)?).send().await?).await?;
        record.manifest.validate()?;
        ensure!(
            record.manifest.name == name && record.manifest.version == version,
            "registry returned a different package identity"
        );
        ensure!(
            record.size <= MAX_ARCHIVE_BYTES
                && record.sha256.len() == 64
                && record
                    .sha256
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
            "invalid registry integrity metadata"
        );
        let url = self.origin.join(&record.download_url)?;
        ensure!(
            url.origin() == self.origin.origin()
                && url.username().is_empty()
                && url.password().is_none(),
            "registry download must use the same origin"
        );
        Ok(record)
    }

    /// Download a pinned archive without credentials, then validate both bytes and manifest.
    ///
    /// # Errors
    /// Rejects quarantine/unavailability, integrity failures, and unsafe archives.
    pub async fn download(&self, record: &VersionRecord) -> Result<Vec<u8>> {
        let url = self.origin.join(&record.download_url)?;
        ensure!(
            url.origin() == self.origin.origin(),
            "registry download origin changed"
        );
        let response = self.http.get(url).send().await?;
        ensure!(
            response.status().is_success(),
            "package download failed (HTTP {})",
            response.status()
        );
        let bytes = bounded_response(response, MAX_ARCHIVE_BYTES).await?;
        ensure!(
            bytes.len() == record.size && archive_sha256(&bytes) == record.sha256,
            "package archive integrity check failed"
        );
        ensure!(
            validate_archive(&bytes)? == record.manifest,
            "archive manifest differs from registry metadata"
        );
        Ok(bytes)
    }

    /// Reserve and upload one immutable version as its signed-in publisher.
    ///
    /// # Errors
    /// Returns validation, authorization, duplicate-version, or transport errors. Never retries effects.
    pub async fn publish(&self, bytes: Vec<u8>, token: &SecretString) -> Result<Value> {
        let manifest = validate_archive(&bytes)?;
        let path = format!("v1/packages/{}/versions", &manifest.name[1..]);
        let reservation =
            json!({"manifest":manifest,"sha256":archive_sha256(&bytes),"size":bytes.len()});
        let _: Value = json_response(
            self.http
                .post(self.url(&path)?)
                .bearer_auth(token.expose_secret())
                .json(&reservation)
                .send()
                .await?,
        )
        .await?;
        let path = format!("{path}/{}/archive", manifest.version);
        json_response(
            self.http
                .put(self.url(&path)?)
                .bearer_auth(token.expose_secret())
                .header("Content-Type", "application/gzip")
                .body(bytes)
                .send()
                .await?,
        )
        .await
    }

    /// Claim a publisher namespace through the registry's account ownership checks.
    ///
    /// # Errors
    /// Rejects invalid handles, existing ownership, unauthenticated accounts, or transport errors.
    pub async fn claim_publisher(
        &self,
        handle: &str,
        display_name: &str,
        token: &SecretString,
    ) -> Result<Value> {
        validate_name(&format!("@{handle}/package"))?;
        json_response(
            self.http
                .post(self.url("v1/publishers")?)
                .bearer_auth(token.expose_secret())
                .json(&json!({"handle":handle,"display_name":display_name}))
                .send()
                .await?,
        )
        .await
    }

    /// Start the maintained device flow. The host shows the code and registry verification URL.
    ///
    /// # Errors
    /// Rejects invalid or unsafe device-flow responses.
    pub async fn device_code(&self) -> Result<DeviceCode> {
        let code: DeviceCode = json_response(
            self.http
                .post(self.url("api/auth/device/code")?)
                .json(&json!({"client_id":"ejm-cli","scope":"registry"}))
                .send()
                .await?,
        )
        .await?;
        let url = Url::parse(&code.verification_uri)?;
        ensure!(
            url.origin() == self.origin.origin()
                && code.device_code.len() <= 512
                && code.user_code.len() <= 32
                && code.expires_in <= 3600,
            "invalid device authorization response"
        );
        Ok(code)
    }

    /// Poll for a revocable account session, respecting the server's device-flow backoff.
    ///
    /// # Errors
    /// Returns denial, expiry, cancellation, or bounded transport failures.
    pub async fn wait_for_login(
        &self,
        code: &DeviceCode,
        cancel: CancellationToken,
    ) -> Result<SecretString> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(code.expires_in);
        let mut interval = code.interval.clamp(5, 60);
        loop {
            tokio::select! { () = cancel.cancelled() => bail!("registry login cancelled"), () = tokio::time::sleep(Duration::from_secs(interval)) => {} }
            ensure!(
                tokio::time::Instant::now() < deadline,
                "registry login expired"
            );
            let response = self.http.post(self.url("api/auth/device/token")?).json(&json!({"client_id":"ejm-cli","device_code":code.device_code,"grant_type":"urn:ietf:params:oauth:grant-type:device_code"})).send().await?;
            let status = response.status();
            let value: Value =
                serde_json::from_slice(&bounded_response(response, MAX_JSON_BYTES).await?)?;
            if status.is_success() {
                let token = value["access_token"]
                    .as_str()
                    .context("registry returned no account token")?;
                ensure!(
                    !token.is_empty() && token.len() <= 4096,
                    "invalid registry account token"
                );
                return Ok(SecretString::new(token));
            }
            match value["error"].as_str() {
                Some("authorization_pending") => {}
                Some("slow_down") => interval = interval.saturating_add(5).min(60),
                Some("access_denied") => bail!("registry login denied"),
                Some("expired_token") => bail!("registry login expired"),
                _ => bail!("registry login failed (HTTP {status})"),
            }
        }
    }

    /// Revoke this registry session through the maintained authentication service.
    ///
    /// # Errors
    /// Returns authentication or transport errors.
    pub async fn logout(&self, token: &SecretString) -> Result<()> {
        let _: Value = json_response(
            self.http
                .post(self.url("api/auth/sign-out")?)
                .bearer_auth(token.expose_secret())
                .json(&json!({}))
                .send()
                .await?,
        )
        .await?;
        Ok(())
    }

    /// Inspect safe signed-in account metadata.
    ///
    /// # Errors
    /// Returns authentication or transport errors.
    pub async fn me(&self, token: &SecretString) -> Result<Value> {
        json_response(
            self.http
                .get(self.url("v1/me")?)
                .bearer_auth(token.expose_secret())
                .send()
                .await?,
        )
        .await
    }

    fn url(&self, path: &str) -> Result<Url> {
        Ok(self.origin.join(path)?)
    }
}

fn parse_origin(value: &str) -> Result<Url> {
    let mut origin = Url::parse(value)?;
    ensure!(
        origin.username().is_empty()
            && origin.password().is_none()
            && origin.query().is_none()
            && origin.fragment().is_none()
            && matches!(origin.path(), "" | "/"),
        "registry must be a plain origin without credentials"
    );
    let loopback = origin
        .host_str()
        .is_some_and(|host| matches!(host, "localhost" | "127.0.0.1" | "[::1]"));
    ensure!(
        origin.scheme() == "https" || (origin.scheme() == "http" && loopback),
        "registry requires HTTPS"
    );
    origin.set_path("/");
    Ok(origin)
}

async fn json_response<T: DeserializeOwned>(response: Response) -> Result<T> {
    let status = response.status();
    let bytes = bounded_response(response, MAX_JSON_BYTES).await?;
    ensure!(
        status.is_success(),
        "registry request failed (HTTP {status})"
    );
    Ok(serde_json::from_slice(&bytes)?)
}

async fn bounded_response(response: Response, max: usize) -> Result<Vec<u8>> {
    ensure!(
        response
            .content_length()
            .is_none_or(|length| length <= u64::try_from(max).unwrap_or(u64::MAX)),
        "registry response exceeds byte limit"
    );
    let mut stream = response.bytes_stream();
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        ensure!(
            bytes.len().saturating_add(chunk.len()) <= max,
            "registry response exceeds byte limit"
        );
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}
