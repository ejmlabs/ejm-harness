//! The account model catalog: a bounded `GET /models` and its two response shapes.

use anyhow::{Context, Result, ensure};
use ejm_harness_auth::CredentialSource;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::provider::cancellable;

const MAX_CATALOG: usize = 1024 * 1024;
const MAX_MODELS: usize = 1024;

/// A displayable model from the selected account, in server order.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AvailableModel {
    pub slug: String,
    pub display_name: String,
}

pub(super) async fn fetch(
    client: &reqwest::Client,
    credentials: &dyn CredentialSource,
    base_url: &str,
    cancel: &CancellationToken,
) -> Result<Vec<AvailableModel>> {
    let token = credentials.access_token(cancel.child_token()).await?;
    let response = cancellable(
        cancel,
        client
            .get(format!("{base_url}/models"))
            .bearer_auth(token.expose_secret())
            .send(),
    )
    .await?
    .context("model catalog connection failed")?;
    ensure!(
        response.status().is_success(),
        "model catalog returned HTTP {}",
        response.status().as_u16()
    );
    parse(&read_body(response, cancel).await?)
}

async fn read_body(response: reqwest::Response, cancel: &CancellationToken) -> Result<Vec<u8>> {
    let mut body = vec![];
    let mut stream = response.bytes_stream();
    while let Some(chunk) = cancellable(cancel, stream.next()).await? {
        let chunk = chunk.context("model catalog stream failed")?;
        ensure!(
            body.len().saturating_add(chunk.len()) <= MAX_CATALOG,
            "model catalog exceeds byte budget"
        );
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

/// Account catalogs list `models` with a visibility. The public API lists `data` ids.
fn parse(body: &[u8]) -> Result<Vec<AvailableModel>> {
    let value: Value = serde_json::from_slice(body).context("invalid model catalog JSON")?;
    let (models, subscription) = if let Some(models) = value.get("models") {
        (
            models.as_array().context("invalid account models array")?,
            true,
        )
    } else {
        (
            value["data"]
                .as_array()
                .context("model catalog has no models or data array")?,
            false,
        )
    };
    ensure!(
        models.len() <= MAX_MODELS,
        "model catalog exceeds entry budget"
    );
    models
        .iter()
        .filter(|model| !subscription || model["visibility"] == "list")
        .map(|model| entry(model, subscription))
        .collect()
}

fn entry(model: &Value, subscription: bool) -> Result<AvailableModel> {
    let model: AvailableModel = if subscription {
        serde_json::from_value(model.clone()).context("invalid model entry")?
    } else {
        let id = model["id"].as_str().context("model entry has no id")?;
        AvailableModel {
            slug: id.into(),
            display_name: id.into(),
        }
    };
    ensure!(
        !model.slug.is_empty()
            && model.slug.len() <= 256
            && !model.display_name.is_empty()
            && model.display_name.len() <= 256,
        "invalid model label"
    );
    Ok(model)
}
