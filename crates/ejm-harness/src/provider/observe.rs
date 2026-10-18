use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Optional host-owned capture. Callbacks must return promptly and bound retained data.
pub trait HttpObserver: Send + Sync {
    /// Receive credential-redacted content. Authorization headers are excluded.
    fn observe(&self, record: &HttpTraceRecord);
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HttpTracePhase {
    RequestBody,
    ResponseHead,
    SsePayload,
    HttpErrorBody,
    Finished,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct HttpTraceHeader {
    pub name: String,
    pub value: String,
}

/// Body content rather than a TCP capture. SSE framing is removed from payloads.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct HttpTraceRecord {
    pub request_id: String,
    pub phase: HttpTracePhase,
    pub payload: String,
    pub status: Option<u16>,
    pub redactions: usize,
    pub truncated: bool,
    /// Decoded payload bytes before redaction, excluding framing and transport overhead.
    pub payload_bytes: usize,
    pub method: Option<String>,
    pub url: Option<String>,
    /// Allowlisted diagnostic headers. Credentials and cookies are excluded.
    pub headers: Vec<HttpTraceHeader>,
}

pub(super) struct Trace<'a> {
    observer: Option<&'a Arc<dyn HttpObserver>>,
    id: String,
    token: &'a str,
}

impl<'a> Trace<'a> {
    pub(super) fn new(observer: Option<&'a Arc<dyn HttpObserver>>, token: &'a str) -> Self {
        Self {
            observer,
            id: observer.map_or_else(String::new, |_| uuid::Uuid::new_v4().to_string()),
            token,
        }
    }

    pub(super) fn emit(
        &self,
        phase: HttpTracePhase,
        payload: &str,
        status: Option<u16>,
        truncated: bool,
    ) {
        if self.observer.is_none() {
            return;
        }
        self.deliver(self.record(phase, payload, status, truncated));
    }

    pub(super) fn request(&self, request: &reqwest::Request) {
        if self.observer.is_none() {
            return;
        }
        let payload = request
            .body()
            .and_then(reqwest::Body::as_bytes)
            .unwrap_or_default();
        let payload = String::from_utf8_lossy(payload);
        let truncated = matches!(payload, std::borrow::Cow::Owned(_));
        let mut record = self.record(HttpTracePhase::RequestBody, &payload, None, truncated);
        record.method = Some(request.method().to_string());
        record.url = Some(request.url().to_string());
        capture_headers(&mut record, request.headers());
        self.deliver(record);
    }

    pub(super) fn response_head(&self, response: &reqwest::Response) {
        if self.observer.is_none() {
            return;
        }
        let mut record = self.record(
            HttpTracePhase::ResponseHead,
            "",
            Some(response.status().as_u16()),
            false,
        );
        capture_headers(&mut record, response.headers());
        self.deliver(record);
    }

    fn record(
        &self,
        phase: HttpTracePhase,
        payload: &str,
        status: Option<u16>,
        truncated: bool,
    ) -> HttpTraceRecord {
        HttpTraceRecord {
            request_id: self.id.clone(),
            phase,
            payload: payload.into(),
            status,
            redactions: 0,
            truncated,
            payload_bytes: payload.len(),
            method: None,
            url: None,
            headers: vec![],
        }
    }

    fn deliver(&self, mut record: HttpTraceRecord) {
        let Some(observer) = self.observer else {
            return;
        };
        if matches!(
            record.phase,
            HttpTracePhase::RequestBody
                | HttpTracePhase::SsePayload
                | HttpTracePhase::HttpErrorBody
        ) {
            if let Ok(mut value) = serde_json::from_str::<Value>(&record.payload) {
                record.redactions += redact_value(&mut value, self.token);
                if record.redactions > 0 {
                    record.payload = serde_json::to_string(&value).unwrap_or_else(|_| {
                        record.truncated = true;
                        "[diagnostic serialization failed]".into()
                    });
                }
            } else {
                // Broken JSON can hide escaped credentials from structural redaction.
                record.payload = "[undecodable body omitted]".into();
                record.truncated = true;
            }
        } else {
            redact_text(
                &mut record.payload,
                self.token,
                &mut record.redactions,
                &mut record.truncated,
            );
        }
        if let Some(url) = &mut record.url {
            redact_text(
                url,
                self.token,
                &mut record.redactions,
                &mut record.truncated,
            );
        }
        for header in &mut record.headers {
            redact_text(
                &mut header.name,
                self.token,
                &mut record.redactions,
                &mut record.truncated,
            );
            redact_text(
                &mut header.value,
                self.token,
                &mut record.redactions,
                &mut record.truncated,
            );
        }
        observer.observe(&record);
    }
}

fn capture_headers(record: &mut HttpTraceRecord, headers: &reqwest::header::HeaderMap) {
    let mut bytes = 0;
    for (name, value) in headers {
        let name = name.as_str();
        if !matches!(
            name,
            "content-type"
                | "content-length"
                | "accept"
                | "accept-encoding"
                | "user-agent"
                | "date"
                | "server"
                | "x-request-id"
                | "request-id"
                | "openai-version"
                | "openai-processing-ms"
                | "retry-after"
        ) && !name.starts_with("x-ratelimit-")
        {
            continue;
        }
        bytes += name.len() + value.len();
        if record.headers.len() >= 32 || bytes > 16 * 1024 {
            record.truncated = true;
            break;
        }
        let Ok(value) = value.to_str() else {
            record.truncated = true;
            continue;
        };
        record.headers.push(HttpTraceHeader {
            name: name.into(),
            value: value.into(),
        });
    }
}

fn redact_text(payload: &mut String, token: &str, redactions: &mut usize, truncated: &mut bool) {
    let literal = if token.is_empty() {
        0
    } else {
        payload.matches(token).count()
    };
    *redactions += literal;
    if literal > 0 {
        *payload = payload.replace(token, "[REDACTED]");
    }
    // Decode JSON escapes as well, so encoded copies of the credential stay private.
    if let Ok(mut value) = serde_json::from_str::<Value>(payload) {
        let encoded = redact_value(&mut value, token);
        if encoded > 0 {
            *redactions += encoded;
            *payload = serde_json::to_string(&value).unwrap_or_else(|_| {
                *truncated = true;
                "[diagnostic serialization failed]".into()
            });
        }
    }
}

fn redact_value(value: &mut Value, token: &str) -> usize {
    if token.is_empty() {
        return 0;
    }
    match value {
        Value::String(text) => {
            let count = text.matches(token).count();
            if count > 0 {
                *text = text.replace(token, "[REDACTED]");
            }
            count
        }
        Value::Array(values) => values
            .iter_mut()
            .map(|value| redact_value(value, token))
            .sum(),
        Value::Object(values) => {
            let mut count = 0;
            for (key, mut value) in std::mem::take(values) {
                count += redact_value(&mut value, token);
                count += key.matches(token).count();
                values.insert(key.replace(token, "[REDACTED]"), value);
            }
            count
        }
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Capture(std::sync::Mutex<Vec<HttpTraceRecord>>);
    impl HttpObserver for Capture {
        fn observe(&self, record: &HttpTraceRecord) {
            if let Ok(mut records) = self.0.lock() {
                records.push(record.clone());
            }
        }
    }

    #[test]
    fn redacts_literal_and_json_escaped_credentials_before_callback() -> anyhow::Result<()> {
        let capture = Arc::new(Capture::default());
        let observer: Arc<dyn HttpObserver> = capture.clone();
        let trace = Trace::new(Some(&observer), "secret");
        trace.emit(
            HttpTracePhase::SsePayload,
            r#"{"a":"secret","b":"\u0073ecret"}"#,
            None,
            false,
        );
        let records = capture
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("capture lock poisoned"))?;
        assert_eq!(records[0].redactions, 2);
        assert!(!records[0].payload.contains("secret"));
        assert_eq!(
            serde_json::from_str::<Value>(&records[0].payload)?,
            serde_json::json!({"a":"[REDACTED]","b":"[REDACTED]"})
        );
        Ok(())
    }

    #[test]
    fn malformed_json_is_omitted_and_request_metadata_excludes_credentials() -> anyhow::Result<()> {
        let capture = Arc::new(Capture::default());
        let observer: Arc<dyn HttpObserver> = capture.clone();
        let trace = Trace::new(Some(&observer), "secret");
        let malformed = r#"{"error":"\u0073ecret","padding":""#;
        trace.emit(HttpTracePhase::HttpErrorBody, malformed, Some(401), true);
        let request = reqwest::Client::new()
            .post("https://api.openai.com/v1/responses")
            .bearer_auth("secret")
            .header("cookie", "secret")
            .header("x-request-id", "secret")
            .header("content-type", "application/json")
            .body("{}")
            .build()?;
        trace.request(&request);
        let records = capture
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("capture lock poisoned"))?;
        assert!(records[0].truncated);
        assert_eq!(records[0].payload, "[undecodable body omitted]");
        assert_eq!(records[0].payload_bytes, malformed.len());
        assert_eq!(records[1].payload_bytes, 2);
        assert!(
            records[1]
                .headers
                .iter()
                .all(|header| header.name != "authorization"
                    && header.name != "cookie"
                    && !header.value.contains("secret"))
        );
        assert!(
            records[1]
                .headers
                .iter()
                .any(|header| header.name == "x-request-id" && header.value == "[REDACTED]")
        );
        Ok(())
    }
}
