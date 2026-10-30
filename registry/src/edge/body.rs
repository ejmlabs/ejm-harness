use futures_util::StreamExt;
use worker::{ByteStream, Request, Result};

/// Reads the request body, or None when its declared or actual length exceeds `maximum`.
pub(super) async fn read_body(req: &mut Request, maximum: usize) -> Result<Option<Vec<u8>>> {
    if req.headers().get("Content-Length")?.is_some_and(|length| {
        length
            .parse::<usize>()
            .map_or(true, |length| length > maximum)
    }) {
        return Ok(None);
    }
    let Ok(stream) = req.stream() else {
        return Ok(Some(Vec::new()));
    };
    read_stream(stream, maximum).await
}

/// Reads a stream, or None as soon as it exceeds `maximum` bytes.
pub(super) async fn read_stream(mut stream: ByteStream, maximum: usize) -> Result<Option<Vec<u8>>> {
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        if chunk.len() > maximum.saturating_sub(bytes.len()) {
            return Ok(None);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(Some(bytes))
}
