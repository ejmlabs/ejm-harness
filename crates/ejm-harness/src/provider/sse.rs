use std::num::NonZeroUsize;

use anyhow::{Context, Result};
use futures_util::{Stream, StreamExt};
use sse_core::{SseDecoder, SseEvent, SseStream};
use tokio_util::sync::CancellationToken;

use super::cancellable;

const MAX_SSE_FIELD: usize = 512 * 1024;
const MAX_SSE_BODY: usize = 32 * 1024 * 1024;
const MAX_SSE_EVENTS: usize = 100_000;

// Count body chunks before decoding, including comments, ignored fields, and separators.
// Count emitted library events before provider filtering, including retry and empty messages.
pub(super) fn sse_stream<S, B, E>(
    inner: S,
) -> Result<impl Stream<Item = std::io::Result<SseEvent>> + Unpin>
where
    S: Stream<Item = std::result::Result<B, E>> + Unpin,
    B: AsRef<[u8]> + Unpin,
    E: std::error::Error + Send + Sync + 'static,
{
    let limit = NonZeroUsize::new(MAX_SSE_FIELD).context("SSE budget must be nonzero")?;
    let mut body_bytes = 0;
    let bounded = inner.map(move |chunk| {
        let chunk = chunk.map_err(std::io::Error::other)?;
        let bytes = chunk.as_ref().len();
        if bytes > MAX_SSE_BODY - body_bytes {
            return Err(std::io::Error::other(
                "provider SSE body exceeds 32 MiB budget",
            ));
        }
        body_bytes += bytes;
        Ok(std::io::Cursor::new(chunk))
    });
    let mut stream = SseStream::with_decoder(SseDecoder::with_limit(limit));
    stream.attach(bounded);
    Ok(stream
        .take(MAX_SSE_EVENTS + 1)
        .enumerate()
        .map(|(index, event)| {
            let event = event.map_err(std::io::Error::other)?;
            if index == MAX_SSE_EVENTS {
                return Err(std::io::Error::other(
                    "provider SSE stream exceeds 100000 decoded events",
                ));
            }
            Ok(event)
        }))
}

/// The data of the next message event, skipping retry events. `None` at end of stream.
pub(super) async fn next_sse<S, E>(
    stream: &mut S,
    cancel: &CancellationToken,
) -> Result<Option<String>>
where
    S: Stream<Item = std::result::Result<SseEvent, E>> + Unpin,
    E: std::error::Error + Send + Sync + 'static,
{
    loop {
        let event = cancellable(cancel, stream.next()).await?;
        match event.transpose().context("provider SSE framing failed")? {
            Some(SseEvent::Message(event)) => return Ok(Some(event.data)),
            Some(SseEvent::Retry(_)) => {}
            None => return Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[tokio::test]
    async fn sse_handles_split_utf8_crlf_comments_and_multiple_data_lines() -> Result<()> {
        let bytes =
            ": ping\r\ndata: {\"text\":\"雪\"}\r\n\r\ndata: first\ndata: second\n\n".as_bytes();
        let chunks = bytes.chunks(2).map(Ok::<_, std::io::Error>);
        let mut stream = sse_stream(futures_util::stream::iter(chunks))?;
        let cancel = CancellationToken::new();
        assert_eq!(
            next_sse(&mut stream, &cancel).await?,
            Some("{\"text\":\"雪\"}".into())
        );
        assert_eq!(
            next_sse(&mut stream, &cancel).await?,
            Some("first\nsecond".into())
        );
        assert!(next_sse(&mut stream, &cancel).await?.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn sse_bounds_empty_data_lines_before_the_frame_ends() -> Result<()> {
        let chunks = (0..MAX_SSE_FIELD + 2).map(|_| Ok::<_, std::io::Error>(b"data:\n".as_slice()));
        let mut stream = sse_stream(futures_util::stream::iter(chunks))?;
        let result = next_sse(&mut stream, &CancellationToken::new()).await;
        let error = result
            .err()
            .context("unbounded empty data lines were accepted")?;
        assert!(error.to_string().contains("SSE framing failed"));
        Ok(())
    }

    #[tokio::test]
    async fn sse_counts_comments_and_ignored_fields_before_decoding_an_overflowing_chunk()
    -> Result<()> {
        let mut comment = vec![b' '; 4096];
        comment[0] = b':';
        comment[4094..].copy_from_slice(b"\n\n");
        let mut ignored = comment.clone();
        ignored[0] = b'x';
        ignored[1] = b':';
        let chunks = (0..MAX_SSE_BODY / comment.len())
            .map(|index| {
                Ok::<_, std::io::Error>(if index % 2 == 0 {
                    comment.as_slice()
                } else {
                    ignored.as_slice()
                })
            })
            .chain(std::iter::once(Ok(b"data: [DONE]\n\n".as_slice())));
        let mut stream = sse_stream(futures_util::stream::iter(chunks))?;
        let error = next_sse(&mut stream, &CancellationToken::new())
            .await
            .err()
            .context("completion in an overflowing body chunk was decoded")?;
        assert!(format!("{error:#}").contains("body exceeds 32 MiB budget"));
        Ok(())
    }

    #[tokio::test]
    async fn sse_event_budget_counts_retry_and_empty_message_events() -> Result<()> {
        for frame in [b"retry: 1\n\n".as_slice(), b"data:\n\n".as_slice()] {
            let chunks = (0..=MAX_SSE_EVENTS).map(|_| Ok::<_, std::io::Error>(frame));
            let mut stream = sse_stream(futures_util::stream::iter(chunks))?;
            for _ in 0..MAX_SSE_EVENTS {
                stream.next().await.context("missing permitted event")??;
            }
            let error = stream
                .next()
                .await
                .context("missing event budget error")?
                .err()
                .context("excess decoded event was accepted")?;
            assert!(error.to_string().contains("100000 decoded events"));
            assert!(stream.next().await.is_none());
        }
        Ok(())
    }

    #[tokio::test]
    async fn sse_allows_completion_at_the_event_limit() -> Result<()> {
        let chunks = (0..MAX_SSE_EVENTS - 1)
            .map(|_| Ok::<_, std::io::Error>(b"retry: 1\n\n".as_slice()))
            .chain(std::iter::once(Ok(b"data: [DONE]\n\n".as_slice())));
        let mut stream = sse_stream(futures_util::stream::iter(chunks))?;
        let cancel = CancellationToken::new();
        assert_eq!(next_sse(&mut stream, &cancel).await?, Some("[DONE]".into()));
        assert!(next_sse(&mut stream, &cancel).await?.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn sse_cancellation_stops_a_pending_body_stream() -> Result<()> {
        let mut stream = sse_stream(futures_util::stream::pending::<
            std::io::Result<&'static [u8]>,
        >())?;
        let cancel = CancellationToken::new();
        let (result, ()) = tokio::time::timeout(Duration::from_millis(500), async {
            tokio::join!(next_sse(&mut stream, &cancel), async {
                tokio::task::yield_now().await;
                cancel.cancel();
            })
        })
        .await?;
        assert!(
            result
                .err()
                .context("cancelled SSE stream succeeded")?
                .to_string()
                .contains("cancelled")
        );
        Ok(())
    }
}
