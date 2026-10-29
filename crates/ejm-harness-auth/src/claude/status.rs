use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

use super::MAX_STATUS_BYTES;

const MAX_METADATA_BYTES: usize = 512;

/// Allowlisted metadata from `claude auth status --json`.
/// Subscription tokens and unknown client fields are never retained here.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeCodeStatus {
    pub logged_in: bool,
    #[serde(default)]
    pub auth_method: String,
    pub email: Option<String>,
    pub subscription_type: Option<String>,
}

impl ClaudeCodeStatus {
    /// Decode bounded, nonsecret status captured by a host-owned supervisor.
    ///
    /// # Errors
    /// Returns an error for a failed client, oversized or malformed JSON,
    /// inconsistent sign-in state, or invalid account metadata. Raw output is
    /// never included in errors. The caller must first reject cancelled,
    /// timed-out and truncated process results.
    pub fn from_client_output(exit_code: Option<i32>, stdout: &[u8]) -> Result<Self> {
        ensure!(
            matches!(exit_code, Some(0 | 1)),
            "Claude Code account status failed"
        );
        ensure!(
            stdout.len() <= MAX_STATUS_BYTES,
            "Claude Code auth output exceeds limit"
        );
        let mut status: Self = serde_json::from_slice(stdout)
            .map_err(|_| anyhow::anyhow!("invalid Claude Code account status"))?;
        ensure!(
            status.logged_in == (exit_code == Some(0)),
            "inconsistent Claude Code account status"
        );
        status.auth_method = match status.auth_method.as_str() {
            "none" | "claude.ai" | "oauth_token" | "api_key" | "api_key_helper" | "third_party" => {
                status.auth_method
            }
            _ => "unknown".into(),
        };
        for field in [&status.email, &status.subscription_type]
            .into_iter()
            .flatten()
        {
            ensure!(
                field.len() <= MAX_METADATA_BYTES && !field.chars().any(char::is_control),
                "invalid Claude Code account metadata"
            );
        }
        Ok(status)
    }

    /// Require the official client's own Claude subscription sign-in.
    ///
    /// # Errors
    /// Returns an error for signed-out, API-key, token-override and cloud sources.
    pub fn require_subscription(&self) -> Result<()> {
        ensure!(
            self.logged_in,
            "Claude Code is signed out; sign in through the official client"
        );
        ensure!(
            self.auth_method == "claude.ai",
            "Claude subscription access requires the official client's Claude account sign-in; API-key and third-party sources use a separate provider"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{ClaudeCodeStatus, MAX_STATUS_BYTES};

    #[test]
    fn supervised_status_decoder_enforces_its_byte_boundary() -> anyhow::Result<()> {
        let mut output = br#"{"loggedIn":false,"authMethod":"none"}"#.to_vec();
        output.resize(MAX_STATUS_BYTES, b' ');
        assert!(!ClaudeCodeStatus::from_client_output(Some(1), &output)?.logged_in);
        output.push(b' ');
        assert!(ClaudeCodeStatus::from_client_output(Some(1), &output).is_err());
        Ok(())
    }
}
