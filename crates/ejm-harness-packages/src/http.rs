//! HTTP policy shared by the registry client and source downloads.

use anyhow::{Result, ensure};
use futures_util::StreamExt;
use reqwest::{Client, ClientBuilder, Response, redirect::Policy};
use std::time::Duration;

/// A client that never follows redirects and bounds connection and total request time.
pub(crate) fn client_builder() -> ClientBuilder {
    Client::builder()
        .redirect(Policy::none())
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
}

/// Read a response body, failing as soon as `what` would exceed `max` bytes.
pub(crate) async fn bounded_body(response: Response, max: usize, what: &str) -> Result<Vec<u8>> {
    ensure!(
        response
            .content_length()
            .is_none_or(|length| length <= u64::try_from(max).unwrap_or(u64::MAX)),
        "{what} exceeds byte limit"
    );
    let mut stream = response.bytes_stream();
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        ensure!(
            bytes.len().saturating_add(chunk.len()) <= max,
            "{what} exceeds byte limit"
        );
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}
