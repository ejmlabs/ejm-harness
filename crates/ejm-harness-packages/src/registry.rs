//! Native registry client. Account tokens are never sent to package download hosts.

use anyhow::{Result, bail, ensure};
use ejm_harness_auth::SecretString;
use ejm_harness_package_spec::{
    MAX_ARCHIVE_BYTES, Manifest, archive_sha256, validate_archive, validate_name,
};
use reqwest::{Client, Response, Url};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

use crate::http::{bounded_body, client_builder};

mod credentials;
mod login;

pub use credentials::{
    delete_registry_token, load_registry_token, migrate_registry_token, save_registry_token,
};
pub use login::DeviceCode;

/// Public registry origin. Hosts must select it explicitly; there is no default or fallback.
pub const PUBLIC_REGISTRY_ORIGIN: &str = "https://registry.ejmlabs.com";
const MAX_JSON_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VersionRecord {
    pub manifest: Manifest,
    pub sha256: String,
    pub size: usize,
    pub download_url: String,
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
        let origin = validate_registry_origin(origin)?;
        let mut builder =
            client_builder().user_agent(concat!("ejm-harness/", env!("CARGO_PKG_VERSION")));
        // Plain HTTP is loopback only. Leave system proxies off so a bearer cannot
        // travel to an HTTP proxy in cleartext. HTTPS keeps the process proxy environment.
        if origin.scheme() != "https" {
            builder = builder.no_proxy();
        }
        let http = builder.build()?;
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

/// Normalize a registry origin and reject `meetgerry.ai`.
///
/// The returned URL uses a root path. Loopback HTTP is accepted for local development.
/// There is no fallback host.
///
/// # Errors
/// Rejects credentials, a query, a fragment, a non-root path, public HTTP, and a retired host.
pub fn validate_registry_origin(value: &str) -> Result<Url> {
    // A retired host is reported before generic path, userinfo, or scheme errors.
    let parsed = Url::parse(value)?;
    if parsed.host_str().is_some_and(retired_registry_host) {
        bail!("{}", retired_registry_message());
    }
    normalize_origin(value)
}

/// Whether a validated, explicitly selected origin is [`PUBLIC_REGISTRY_ORIGIN`].
///
/// # Errors
/// Returns the same errors as [`validate_registry_origin`].
pub fn registry_origin_is_public(value: &str) -> Result<bool> {
    Ok(validate_registry_origin(value)? == validate_registry_origin(PUBLIC_REGISTRY_ORIGIN)?)
}

pub(crate) fn retired_registry_message() -> String {
    format!(
        "the meetgerry.ai registry is retired; the public registry is {PUBLIC_REGISTRY_ORIGIN}. Select a registry origin explicitly and sign in again. A meetgerry.ai session is never sent or reused"
    )
}

pub(crate) fn retired_registry_host(host: &str) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    host == "meetgerry.ai" || host.ends_with(".meetgerry.ai")
}

/// Origin form stored in the vault. Retired hosts stay addressable so their records can be deleted.
pub(crate) fn normalize_origin(value: &str) -> Result<Url> {
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
    bounded_body(response, max, "registry response").await
}
