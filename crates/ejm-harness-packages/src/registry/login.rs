use std::{fmt, time::Duration};

use anyhow::{Context, Result, bail, ensure};
use ejm_harness_auth::SecretString;
use reqwest::Url;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use super::{MAX_JSON_BYTES, RegistryClient, bounded_response, json_response};

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

impl RegistryClient {
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
            tokio::select! {
                () = cancel.cancelled() => bail!("registry login cancelled"),
                () = tokio::time::sleep(Duration::from_secs(interval)) => {}
            }
            ensure!(
                tokio::time::Instant::now() < deadline,
                "registry login expired"
            );
            let response = self
                .http
                .post(self.url("api/auth/device/token")?)
                .json(&json!({
                    "client_id": "ejm-cli",
                    "device_code": code.device_code,
                    "grant_type": "urn:ietf:params:oauth:grant-type:device_code",
                }))
                .send()
                .await?;
            let status = response.status();
            let value: Value =
                serde_json::from_slice(&bounded_response(response, MAX_JSON_BYTES).await?)?;
            if status.is_success() {
                return account_token(&value);
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
}

fn account_token(value: &Value) -> Result<SecretString> {
    let token = value["access_token"]
        .as_str()
        .context("registry returned no account token")?;
    ensure!(
        !token.is_empty() && token.len() <= 4096,
        "invalid registry account token"
    );
    Ok(SecretString::new(token))
}
