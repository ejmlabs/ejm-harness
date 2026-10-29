use std::time::Duration;

use anyhow::{Context, Result, ensure};
use futures_util::Stream;
use sse_core::SseEvent;
use tokio_util::sync::CancellationToken;

use super::{cancellable, sse::sse_stream};

pub(super) fn http_client(base_url: &str) -> Result<reqwest::Client> {
    let mut builder = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(600))
        .pool_max_idle_per_host(4)
        .redirect(reqwest::redirect::Policy::none());
    // Skip every proxy unless the scheme is `https`. Plain `http` is the only
    // non-https scheme the endpoint check accepts, and it is always loopback.
    // A proxy would send the API key in cleartext. `https` keeps the process
    // proxy so a CONNECT tunnel can still be used.
    if !reqwest::Url::parse(base_url).is_ok_and(|url| url.scheme() == "https") {
        builder = builder.no_proxy();
    }
    builder.build().context("HTTP client initialization failed")
}

pub(super) fn endpoint(base: &str, suffix: &str) -> Result<String> {
    let url = reqwest::Url::parse(base).context("invalid provider base URL")?;
    ensure!(
        url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none(),
        "provider URL must not contain credentials, query, or fragment"
    );
    ensure!(
        url.scheme() == "https" || (url.scheme() == "http" && http_host_is_loopback(&url)),
        "provider URL must use HTTPS, or HTTP on a loopback host"
    );
    Ok(if base.trim_end_matches('/').ends_with(suffix) {
        base.trim_end_matches('/').into()
    } else {
        format!("{}{suffix}", base.trim_end_matches('/'))
    })
}

fn http_host_is_loopback(url: &reqwest::Url) -> bool {
    match url.host() {
        Some(url::Host::Domain(domain)) => domain.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(address)) => address.is_loopback(),
        Some(url::Host::Ipv6(address)) => address.is_loopback(),
        None => false,
    }
}

/// Send a streaming request and return the bounded SSE events of a successful response.
pub(super) async fn open_stream(
    request: reqwest::RequestBuilder,
    cancel: &CancellationToken,
) -> Result<impl Stream<Item = std::io::Result<SseEvent>> + Unpin> {
    let response = cancellable(cancel, request.send())
        .await?
        .context("provider connection failed")?;
    ensure!(
        response.status().is_success(),
        "provider returned HTTP {}",
        response.status().as_u16()
    );
    sse_stream(response.bytes_stream())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{AnthropicConfig, AnthropicProvider, OpenAiConfig, OpenAiProvider};

    #[test]
    fn endpoints_never_accept_embedded_credentials() -> Result<()> {
        let error = endpoint("https://key:secret@host/v1", "/messages")
            .err()
            .context("embedded credentials were accepted")?;
        assert_eq!(
            error.to_string(),
            "provider URL must not contain credentials, query, or fragment"
        );
        assert_eq!(
            endpoint("https://api.deepseek.com", "/chat/completions")?,
            "https://api.deepseek.com/chat/completions"
        );
        assert_eq!(
            endpoint("https://api.deepseek.com/v1", "/chat/completions")?,
            "https://api.deepseek.com/v1/chat/completions"
        );
        Ok(())
    }

    #[test]
    fn provider_endpoints_accept_https_and_loopback_http() -> Result<()> {
        let accepted = [
            (
                "https://api.deepseek.com/v1",
                "/chat/completions",
                "https://api.deepseek.com/v1/chat/completions",
            ),
            (
                "https://api.example.test:8443/v1",
                "/messages",
                "https://api.example.test:8443/v1/messages",
            ),
            (
                "HTTPS://api.example.test/v1",
                "/messages",
                "HTTPS://api.example.test/v1/messages",
            ),
            (
                "https://127.0.0.1/v1",
                "/messages",
                "https://127.0.0.1/v1/messages",
            ),
            (
                "https://localhost/v1",
                "/messages",
                "https://localhost/v1/messages",
            ),
            (
                "http://localhost:11434/v1",
                "/chat/completions",
                "http://localhost:11434/v1/chat/completions",
            ),
            (
                "http://LOCALHOST:8080",
                "/messages",
                "http://LOCALHOST:8080/messages",
            ),
            (
                "http://127.0.0.1:1234/v1",
                "/messages",
                "http://127.0.0.1:1234/v1/messages",
            ),
            (
                "http://127.0.0.2:1234",
                "/messages",
                "http://127.0.0.2:1234/messages",
            ),
            (
                "http://[::1]:1234/v1",
                "/messages",
                "http://[::1]:1234/v1/messages",
            ),
            (
                "HTTP://127.0.0.1/v1",
                "/chat/completions",
                "HTTP://127.0.0.1/v1/chat/completions",
            ),
            (
                "http://2130706433/v1",
                "/messages",
                "http://2130706433/v1/messages",
            ),
        ];
        for (base, suffix, expected) in accepted {
            assert_eq!(endpoint(base, suffix)?, expected, "{base}");
        }
        Ok(())
    }

    #[test]
    fn provider_endpoints_reject_remote_private_and_other_schemes() -> Result<()> {
        const HTTPS: &str = "provider URL must use HTTPS, or HTTP on a loopback host";
        let remote_http = [
            "http://api.example.invalid/v1",
            "http://api.example.test:443/v1",
            "http://localhost.evil.invalid/v1",
            "http://127.0.0.1.nip.io/v1",
            "http://localhost./v1",
            "http://evil-localhost/v1",
            "http://foo.localhost/v1",
            "http://0.0.0.0:8080",
            "http://[::ffff:127.0.0.1]:8080",
            "http://10.1.2.3/v1",
            "http://172.16.5.5/v1",
            "http://192.168.1.10:11434",
            "http://[fd00::1]/v1",
            "ws://api.example.invalid/v1",
            "wss://api.example.invalid/v1",
            "ftp://api.example.invalid/v1",
            "file:///tmp/a",
        ];
        for base in remote_http {
            let error = endpoint(base, "/messages")
                .err()
                .with_context(|| format!("{base} was accepted"))?;
            assert_eq!(error.to_string(), HTTPS, "{base}");
        }
        let unparsed = endpoint("api.example.invalid/v1", "/messages")
            .err()
            .context("schemeless URL was accepted")?;
        assert_eq!(unparsed.to_string(), "invalid provider base URL");
        assert!(!format!("{unparsed:#}").contains("api.example.invalid"));
        Ok(())
    }

    #[test]
    fn provider_endpoints_keep_rejecting_credentials_query_and_fragment() -> Result<()> {
        const HTTPS: &str = "provider URL must use HTTPS, or HTTP on a loopback host";
        const CREDENTIALS: &str = "provider URL must not contain credentials, query, or fragment";
        const PLANTED: &str = "planted-provider-secret";
        let credential_urls = [
            "https://key:secret@api.example.invalid/v1",
            &format!("http://user:{PLANTED}@127.0.0.1/v1"),
            "https://api.example.invalid/v1?token=1",
            "https://api.example.invalid/v1#fragment",
        ];
        for base in credential_urls {
            let error = endpoint(base, "/messages")
                .err()
                .with_context(|| format!("{base} was accepted"))?;
            assert_eq!(error.to_string(), CREDENTIALS, "{base}");
            assert!(!error.to_string().contains(HTTPS), "{base}");
            assert_secret_absent(&error, PLANTED);
        }
        let userinfo = endpoint("http://localhost@evil.invalid/v1", "/messages")
            .err()
            .context("localhost@evil.invalid was accepted")?;
        assert_eq!(userinfo.to_string(), CREDENTIALS);
        assert_secret_absent(&userinfo, "evil.invalid");
        Ok(())
    }

    #[test]
    fn provider_endpoints_cover_expanded_loopback_and_hostile_spellings() -> Result<()> {
        const HTTPS: &str = "provider URL must use HTTPS, or HTTP on a loopback host";
        let accepted = [
            (
                "http://[0:0:0:0:0:0:0:1]/v1",
                "http://[0:0:0:0:0:0:0:1]/v1/messages",
            ),
            ("http://[::0001]/v1", "http://[::0001]/v1/messages"),
            ("http://127.1/v1", "http://127.1/v1/messages"),
            ("http://0x7f.0.0.1/v1", "http://0x7f.0.0.1/v1/messages"),
            ("http://0177.0.0.1/v1", "http://0177.0.0.1/v1/messages"),
            (
                "http://localhost\\@evil.invalid/v1",
                "http://localhost\\@evil.invalid/v1/messages",
            ),
        ];
        for (base, expected) in accepted {
            assert_eq!(endpoint(base, "/messages")?, expected, "{base}");
        }
        for base in [
            "http://[::]/v1",
            "http://[fe80::1]/v1",
            "http://[fc00::1]/v1",
            "http://169.254.169.254/v1",
            "http://evil.invalid\\@localhost/v1",
        ] {
            let error = endpoint(base, "/messages")
                .err()
                .with_context(|| format!("{base} was accepted"))?;
            assert_eq!(error.to_string(), HTTPS, "{base}");
            for marker in ["[::]", "fe80", "fc00", "169.254", "evil.invalid"] {
                if base.contains(marker) {
                    assert_secret_absent(&error, marker);
                }
            }
        }
        Ok(())
    }

    #[test]
    fn cleartext_remote_constructors_send_no_credential() -> Result<()> {
        const PLANTED: &str = "planted-provider-secret";
        for result in [
            OpenAiProvider::new(OpenAiConfig {
                base_url: "http://api.example.invalid/v1".into(),
                api_key: PLANTED.into(),
                model: "fixture-model".into(),
            })
            .map(|_| ()),
            AnthropicProvider::new(AnthropicConfig {
                base_url: "http://api.example.invalid/v1".into(),
                api_key: PLANTED.into(),
                model: "fixture-model".into(),
            })
            .map(|_| ()),
        ] {
            let error = result
                .err()
                .context("cleartext remote provider was created")?;
            assert_eq!(
                error.to_string(),
                "provider URL must use HTTPS, or HTTP on a loopback host"
            );
            assert_secret_absent(&error, PLANTED);
        }
        Ok(())
    }

    fn assert_secret_absent(error: &anyhow::Error, secret: &str) {
        for rendered in [
            format!("{error:#}"),
            format!("{error:?}"),
            format!("{error:#?}"),
        ] {
            assert!(!rendered.contains(secret), "{rendered}");
        }
    }
}
