//! The provider's token endpoint response and the bounded credentials it becomes.

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Deserializer};

use super::timestamp;
use crate::{SecretString, StoredCredentials, types::plan_enabled};

#[derive(Deserialize)]
pub(super) struct TokenResponse {
    #[serde(default, deserialize_with = "optional_secret")]
    access_token: Option<SecretString>,
    #[serde(default, deserialize_with = "optional_secret")]
    pub(super) refresh_token: Option<SecretString>,
    #[serde(default, deserialize_with = "optional_secret")]
    pub(super) id_token: Option<SecretString>,
    token_type: Option<String>,
    expires_in: Option<u64>,
    scope: Option<String>,
    earliest_refresh_at: Option<u64>,
}

impl TokenResponse {
    pub(super) fn credentials(
        self,
        id_token: SecretString,
        previous_scopes: Option<&[String]>,
    ) -> Result<StoredCredentials> {
        self.ensure_bounded_tokens()?;
        let scopes = granted_scopes(self.scope, previous_scopes)?;
        let expires_at = if self.access_token.is_some() {
            let expires = self.expires_in.context("access token expiry missing")?;
            ensure!(
                (1..=86_400).contains(&expires),
                "invalid access token expiry"
            );
            Some(timestamp().saturating_add(expires))
        } else {
            None
        };
        ensure!(
            !plan_enabled(&scopes) || self.access_token.is_some(),
            "plan permission has no access token"
        );
        Ok(StoredCredentials {
            access_token: self.access_token,
            refresh_token: self.refresh_token,
            id_token,
            scopes,
            expires_at,
            earliest_refresh_at: self.earliest_refresh_at,
        })
    }

    fn ensure_bounded_tokens(&self) -> Result<()> {
        for token in [self.access_token.as_ref(), self.refresh_token.as_ref()]
            .into_iter()
            .flatten()
        {
            ensure!(
                !token.expose_secret().is_empty() && token.expose_secret().len() <= 64 * 1024,
                "token field is missing or exceeds limit"
            );
        }
        if self.access_token.is_some() {
            ensure!(
                self.token_type
                    .as_deref()
                    .is_some_and(|kind| kind.eq_ignore_ascii_case("Bearer")),
                "unsupported access token type"
            );
        }
        Ok(())
    }
}

/// The granted scopes, or the previous ones when a renewal omits them.
fn granted_scopes(scope: Option<String>, previous: Option<&[String]>) -> Result<Vec<String>> {
    let Some(scope) = scope else {
        return Ok(previous
            .context("authorization response omitted granted scopes")?
            .to_vec());
    };
    ensure!(scope.len() <= 8192, "granted scopes exceed limit");
    let scopes = scope
        .split_ascii_whitespace()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    ensure!(scopes.len() <= 64, "too many granted scopes");
    Ok(scopes)
}

fn optional_secret<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Option<SecretString>, D::Error> {
    Option::<String>::deserialize(deserializer).map(|value| value.map(SecretString::new))
}
