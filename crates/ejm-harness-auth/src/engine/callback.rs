//! Loopback redirect and authorization callback validation.

use std::collections::HashMap;

use anyhow::{Context, Result, ensure};
use reqwest::Url;

use super::{accounts::valid_client, attempt::AttemptSecrets, equal};
use crate::SecretString;

pub(super) struct Callback {
    pub(super) client_id: String,
    pub(super) code: SecretString,
}

pub(super) fn callback_redirect(value: &str) -> Result<Url> {
    ensure!(value.len() <= 2048, "callback URI exceeds limit");
    let url = Url::parse(value).map_err(|_| anyhow::anyhow!("invalid callback URI"))?;
    ensure!(
        url.scheme() == "http"
            && url.host_str() == Some("127.0.0.1")
            && url.port().is_some_and(|port| port > 0)
            && url.path() == "/auth/callback"
            && url.query().is_none()
            && url.fragment().is_none()
            && url.username().is_empty()
            && url.password().is_none(),
        "callback must be http://127.0.0.1:<port>/auth/callback"
    );
    Ok(url)
}

pub(super) fn parse_callback(pending: &AttemptSecrets, callback: &str) -> Result<Callback> {
    ensure!(callback.len() <= 16 * 1024, "callback exceeds limit");
    let url =
        Url::parse(callback).map_err(|_| anyhow::anyhow!("invalid authorization callback"))?;
    ensure!(
        url.fragment().is_none() && url.username().is_empty() && url.password().is_none(),
        "unexpected callback parameters"
    );
    let mut base = url.clone();
    base.set_query(None);
    ensure!(
        base == pending.redirect_uri,
        "callback URI does not match authorization attempt"
    );
    let mut values = query_values(&url)?;
    let state = values.get("state").context("callback state missing")?;
    ensure!(
        equal(state, pending.state.expose_secret()),
        "callback state mismatch"
    );
    ensure!(
        !values.contains_key("error"),
        "authorization was declined or failed"
    );
    let client_id = callback_client_id(pending, &values)?;
    let code = values
        .remove("code")
        .context("authorization code missing")?;
    ensure!(
        !code.is_empty() && code.len() <= 4096,
        "invalid authorization code"
    );
    Ok(Callback {
        client_id,
        code: SecretString::new(code),
    })
}

fn query_values(url: &Url) -> Result<HashMap<String, String>> {
    let mut values = HashMap::new();
    for (key, value) in url.query_pairs() {
        ensure!(
            values.len() < 16
                && values
                    .insert(key.into_owned(), value.into_owned())
                    .is_none(),
            "duplicate or excessive callback parameters"
        );
    }
    Ok(values)
}

/// A new registration takes the issued client ID. A saved one must not change it.
fn callback_client_id(
    pending: &AttemptSecrets,
    values: &HashMap<String, String>,
) -> Result<String> {
    if pending.new_registration {
        let issued = values
            .get("client_id")
            .context("registration callback omitted issued client ID")?;
        ensure!(
            valid_client(issued),
            "registration callback has invalid issued client ID"
        );
        return Ok(issued.clone());
    }
    ensure!(
        values
            .get("client_id")
            .is_none_or(|issued| issued == &pending.client_id),
        "callback client ID differs from selected account"
    );
    Ok(pending.client_id.clone())
}
