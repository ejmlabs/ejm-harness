//! Optional typed feedback, independent of HTTP debug capture and error text.

use std::{
    sync::Arc,
    time::{Duration, SystemTime},
};

use anyhow::{Context, Result, ensure};
use reqwest::header::HeaderMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{Usage, types::timestamp_ms};
use serde_json::Value;

pub const MAX_PROVIDER_WAIT_MS: u64 = 15 * 60 * 1000;

/// A host-assigned account UUID is separate from credentials and model choice.
/// Reuse it across participating processes that share an account quota. Never
/// derive it from a credential, request text, or a session path.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct QuotaDomain {
    id: String,
}

impl QuotaDomain {
    /// Combine transport origin, opaque account UUID and declared quota family.
    /// Different model names do not establish separate quota families.
    ///
    /// # Errors
    /// Rejects a non-HTTP origin, non-UUID account identity, or invalid family.
    pub fn new(origin: &str, account_uuid: &str, family: &str) -> Result<Self> {
        let origin = reqwest::Url::parse(origin)?;
        ensure!(
            matches!(origin.scheme(), "https" | "http") && origin.host_str().is_some(),
            "quota transport must have an HTTP origin"
        );
        let account = uuid::Uuid::parse_str(account_uuid)?;
        ensure!(
            !family.is_empty()
                && family.len() <= 64
                && family
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"_-".contains(&byte)),
            "quota family must contain 1..64 ASCII letters, digits, underscores or hyphens"
        );
        let mut digest = Sha256::new();
        digest.update(b"ejm-provider-quota-v1");
        digest.update(origin.origin().ascii_serialization().as_bytes());
        digest.update(account.as_bytes());
        digest.update(family.as_bytes());
        Ok(Self {
            id: format!("{:x}", digest.finalize()),
        })
    }

    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    pub(crate) fn validate(&self) -> Result<()> {
        ensure!(
            self.id.len() == 64 && self.id.bytes().all(|byte| byte.is_ascii_hexdigit()),
            "invalid opaque quota identity"
        );
        Ok(())
    }
}

/// A locally configured sustained rate. Known rate charges are not concurrency
/// slots and survive local cancellation until their window expires.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ProviderRate {
    pub limit: u64,
    pub window_ms: u64,
}

impl ProviderRate {
    pub(crate) fn validate(self) -> Result<()> {
        ensure!(
            self.limit > 0 && (1_000..=86_400_000).contains(&self.window_ms),
            "provider rate requires a positive limit and 1 second..1 day window"
        );
        Ok(())
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct ProviderCapacity {
    pub domain: Option<QuotaDomain>,
    pub max_in_flight: Option<usize>,
    pub requests: Option<ProviderRate>,
    pub tokens: Option<ProviderRate>,
}

impl ProviderCapacity {
    pub(crate) fn validate(&self) -> Result<()> {
        if let Some(domain) = &self.domain {
            domain.validate()?;
        }
        if let Some(limit) = self.max_in_flight {
            ensure!(
                (1..=256).contains(&limit),
                "provider in-flight ceiling must be 1..256"
            );
        }
        if let Some(rate) = self.requests {
            rate.validate()?;
        }
        if let Some(rate) = self.tokens {
            rate.validate()?;
        }
        Ok(())
    }
}

/// Provider-reported remaining capacity at an absolute reset boundary. Missing
/// hints stay unknown and cannot be converted into an invented rate limit.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
pub struct ProviderRateWindow {
    pub limit: u64,
    pub remaining: u64,
    pub reset_at_ms: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProviderRetry {
    Delay { millis: u64 },
    BeyondHorizon { retry_at_ms: Option<u64> },
    Unknown,
}

#[derive(Debug, Clone)]
pub enum ProviderObservation {
    Limits {
        requests: Option<ProviderRateWindow>,
        tokens: Option<ProviderRateWindow>,
    },
    Throttled {
        retry: ProviderRetry,
    },
    Usage(Usage),
}

/// Receives only validated aggregates. Implementations must remain bounded and
/// nonblocking; they must not perform registry I/O or retain raw HTTP material.
pub trait ProviderCapacityObserver: Send + Sync {
    fn observe(&self, observation: ProviderObservation);
}

pub(super) type Feedback = Option<Arc<dyn ProviderCapacityObserver>>;

#[derive(Clone, Copy)]
pub(super) enum HeaderFamily {
    OpenAi,
    Anthropic,
}

pub(super) fn observe_response(
    feedback: &Feedback,
    response: &reqwest::Response,
    family: HeaderFamily,
) {
    let Some(feedback) = feedback else {
        return;
    };
    let now = timestamp_ms();
    let requests = window(response.headers(), family, "requests", now);
    let tokens = window(response.headers(), family, "tokens", now);
    if requests.is_some() || tokens.is_some() {
        feedback.observe(ProviderObservation::Limits { requests, tokens });
    }
    if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS
        || response.status() == reqwest::StatusCode::SERVICE_UNAVAILABLE
    {
        feedback.observe(ProviderObservation::Throttled {
            retry: retry_after(response.headers()),
        });
    }
}

pub(super) fn observe_usage(feedback: &Feedback, usage: &Usage) {
    if let Some(feedback) = feedback {
        feedback.observe(ProviderObservation::Usage(usage.clone()));
    }
}

fn retry_after(headers: &HeaderMap) -> ProviderRetry {
    let Some(value) = headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .filter(|value| value.len() <= 128)
    else {
        return ProviderRetry::Unknown;
    };
    let duration = value
        .parse::<u64>()
        .ok()
        .map(Duration::from_secs)
        .or_else(|| {
            httpdate::parse_http_date(value)
                .ok()
                .map(|time| time.duration_since(SystemTime::now()).unwrap_or_default())
        });
    match duration {
        Some(duration) if duration <= Duration::from_millis(MAX_PROVIDER_WAIT_MS) => {
            ProviderRetry::Delay {
                millis: u64::try_from(duration.as_millis()).unwrap_or(MAX_PROVIDER_WAIT_MS),
            }
        }
        Some(duration) => ProviderRetry::BeyondHorizon {
            retry_at_ms: SystemTime::now()
                .checked_add(duration)
                .and_then(|time| time.duration_since(SystemTime::UNIX_EPOCH).ok())
                .and_then(|duration| u64::try_from(duration.as_millis()).ok()),
        },
        None => ProviderRetry::Unknown,
    }
}

fn window(
    headers: &HeaderMap,
    family: HeaderFamily,
    resource: &str,
    now: u64,
) -> Option<ProviderRateWindow> {
    let read = |kind| {
        let key = match family {
            HeaderFamily::OpenAi => format!("x-ratelimit-{kind}-{resource}"),
            HeaderFamily::Anthropic => format!("anthropic-ratelimit-{resource}-{kind}"),
        };
        headers
            .get(key)
            .and_then(|value| value.to_str().ok())
            .filter(|value| value.len() <= 128)
    };
    let limit = read("limit")?.parse::<u64>().ok()?;
    let remaining = read("remaining")?.parse::<u64>().ok()?;
    // This adapter accepts absolute HTTP dates and epoch seconds. Vendor
    // duration grammars remain unsupported rather than guessed from text.
    let reset = read("reset")?;
    let reset_at_ms = reset
        .parse::<u64>()
        .ok()
        .and_then(|seconds| seconds.checked_mul(1000))
        .or_else(|| {
            httpdate::parse_http_date(reset)
                .ok()?
                .duration_since(SystemTime::UNIX_EPOCH)
                .ok()
                .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        })?;
    (limit > 0
        && remaining <= limit
        && reset_at_ms > now
        && reset_at_ms.saturating_sub(now) <= 86_400_000)
        .then_some(ProviderRateWindow {
            limit,
            remaining,
            reset_at_ms,
        })
}

pub(super) fn openai_usage(value: &Value) -> Option<Usage> {
    let input_tokens = value.get("prompt_tokens")?.as_u64()?;
    let output_tokens = value.get("completion_tokens")?.as_u64()?;
    input_tokens.checked_add(output_tokens)?;
    let cached_input_tokens = optional_usage(value, "/prompt_tokens_details/cached_tokens").ok()?;
    let reasoning_output_tokens =
        optional_usage(value, "/completion_tokens_details/reasoning_tokens").ok()?;
    if cached_input_tokens.is_some_and(|cached| cached > input_tokens)
        || reasoning_output_tokens.is_some_and(|reasoning| reasoning > output_tokens)
    {
        return None;
    }
    Some(Usage {
        input_tokens,
        output_tokens,
        cached_input_tokens,
        reasoning_output_tokens,
    })
}

fn optional_usage(value: &Value, path: &str) -> Result<Option<u64>> {
    match value.pointer(path) {
        None => Ok(None),
        Some(value) => value
            .as_u64()
            .map(Some)
            .context("provider usage field is not an unsigned integer"),
    }
}

#[derive(Default)]
pub(super) struct AnthropicUsage {
    input: Option<(u64, Option<u64>)>,
    output: Option<u64>,
}

impl AnthropicUsage {
    pub fn observe(&mut self, event: &Value) {
        match event["type"].as_str() {
            Some("message_start") => self.input = Self::input(&event["message"]["usage"]),
            Some("message_delta") => self.output = event["usage"]["output_tokens"].as_u64(),
            _ => {}
        }
    }

    fn input(usage: &Value) -> Option<(u64, Option<u64>)> {
        let uncached = usage["input_tokens"].as_u64()?;
        let created = optional_usage(usage, "/cache_creation_input_tokens")
            .ok()?
            .unwrap_or(0);
        let cached = optional_usage(usage, "/cache_read_input_tokens").ok()?;
        Some((
            uncached
                .checked_add(created)?
                .checked_add(cached.unwrap_or(0))?,
            cached,
        ))
    }

    pub fn finish(self) -> Option<Usage> {
        let (input_tokens, cached_input_tokens) = self.input?;
        let output_tokens = self.output?;
        input_tokens.checked_add(output_tokens)?;
        Some(Usage {
            input_tokens,
            output_tokens,
            cached_input_tokens,
            reasoning_output_tokens: None,
        })
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "Header fixtures validate standard retry semantics and redacted quota identities."
)]
mod tests {
    use super::*;

    #[test]
    fn retry_hint_horizon_and_malformed_values_remain_distinct() {
        let mut headers = HeaderMap::new();
        headers.insert("retry-after", "2".parse().unwrap());
        assert!(matches!(
            retry_after(&headers),
            ProviderRetry::Delay { millis: 2000 }
        ));
        headers.insert("retry-after", "99999".parse().unwrap());
        assert!(matches!(
            retry_after(&headers),
            ProviderRetry::BeyondHorizon { .. }
        ));
        headers.insert("retry-after", "not-a-date".parse().unwrap());
        assert!(matches!(retry_after(&headers), ProviderRetry::Unknown));
    }

    #[test]
    fn quota_family_shares_origin_and_opaque_account_identity() {
        let account = "598b68e9-ea5c-427a-b8bc-44bbbfbe7278";
        let one = QuotaDomain::new("https://example.test/v1/model-a", account, "shared").unwrap();
        let two = QuotaDomain::new("https://example.test/v2/model-b", account, "shared").unwrap();
        assert_eq!(one, two);
        assert!(!one.id().contains(account));
        assert_ne!(
            one,
            QuotaDomain::new("https://other.test", account, "shared").unwrap()
        );
    }
}
