//! Elicit plan usage.
//!
//! Verified 2026-10-16 against <https://docs.elicit.com/> ("Usage" →
//! "Get usage status") and the OpenAPI document at
//! <https://docs.elicit.com/openapi.json> (API v2.0.0):
//! `GET https://elicit.com/api/v2/usage`, `Authorization: Bearer elk_live_...`.
//! 200 body (`GetUsageResponse`, all fields required): `hasUsageRemaining`
//! (bool), `percentUsed` (number), `periodStart`/`periodEnd` (ISO 8601
//! strings), `extraUsage` (`{limitUsdCents: int|null, spentUsdCents: int}` or
//! null). Errors: 401, 403 (plan has no API access), 409 (`not_credits_pricing`
//! or `usage_unavailable`), 429 (100 requests/min per IP, then a 5-minute
//! block), 500.
//!
//! Elicit reports a percentage, not a count, so the snapshot uses
//! `Unit::Percent` with a limit of 100. The docs do not say whether this call
//! is billable; it is a read of the account meter, like the dashboard.
//! TODO(verify): confirm with a live key.

use std::time::Duration;

use async_trait::async_trait;
use jiff::Timestamp;
use reqwest::StatusCode;
use secrecy::SecretString;
use serde::Deserialize;

use super::{error_code, error_message, http_status_message, malformed};
use crate::net::{Auth, Http};
use crate::provider::{Category, Provider, ProviderError, Snapshot, Unit, parse_timestamp};

pub struct Elicit;

const ENDPOINT: &str = "https://elicit.com/api/v2/usage";
const HOST: &str = "elicit.com";
/// Elicit blocks an IP for five minutes after a 429 and sends no Retry-After.
const RATE_LIMIT_BLOCK: Duration = Duration::from_secs(5 * 60);

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Body {
    has_usage_remaining: bool,
    percent_used: f64,
    period_end: Option<String>,
    extra_usage: Option<ExtraUsage>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExtraUsage {
    limit_usd_cents: Option<i64>,
    spent_usd_cents: i64,
}

fn dollars(cents: i64) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    let abs = cents.unsigned_abs();
    format!("{sign}${}.{:02}", abs / 100, abs % 100)
}

#[async_trait]
impl Provider for Elicit {
    fn id(&self) -> &'static str {
        "elicit"
    }

    fn display_name(&self) -> &'static str {
        "Elicit"
    }

    fn docs_url(&self) -> &'static str {
        "https://docs.elicit.com/"
    }

    fn endpoint(&self) -> &'static str {
        ENDPOINT
    }

    fn allowed_host(&self) -> &'static str {
        HOST
    }

    fn unit(&self) -> Unit {
        Unit::Percent
    }

    fn category(&self) -> Category {
        Category::Data
    }

    fn min_poll_interval(&self) -> Duration {
        Duration::from_secs(60)
    }

    fn note(&self) -> Option<&'static str> {
        Some("Elicit reports plan usage as a percentage, not a credit count.")
    }

    async fn fetch(&self, key: &SecretString, _config: Option<&str>, http: &Http) -> Result<Snapshot, ProviderError> {
        let response = http.get(ENDPOINT, HOST, Auth::Bearer(key)).await?;
        parse(response.status, response.retry_after, &response.body, Timestamp::now())
    }
}

pub(crate) fn parse(status: StatusCode, retry_after: Option<Duration>, body: &[u8], now: Timestamp) -> Result<Snapshot, ProviderError> {
    match status {
        StatusCode::UNAUTHORIZED => return Err(ProviderError::Unauthorized),
        // Elicit's 403 means the key is valid but the plan has no API access.
        StatusCode::FORBIDDEN => {
            return Err(ProviderError::Unsupported(
                error_message(body).unwrap_or_else(|| "API access is not available on this Elicit plan.".into()),
            ));
        }
        StatusCode::CONFLICT => {
            let text = error_message(body).unwrap_or_else(|| http_status_message(status));
            return Err(match error_code(body).as_deref() {
                Some("not_credits_pricing") => ProviderError::Unsupported(text),
                _ => ProviderError::Vendor(text),
            });
        }
        StatusCode::TOO_MANY_REQUESTS => {
            return Err(ProviderError::RateLimited {
                retry_after: Some(retry_after.unwrap_or(RATE_LIMIT_BLOCK)),
                message: error_message(body),
            });
        }
        s if !s.is_success() => {
            return Err(ProviderError::Vendor(error_message(body).unwrap_or_else(|| http_status_message(s))));
        }
        _ => {}
    }
    let parsed: Body = serde_json::from_slice(body).map_err(|_| malformed())?;
    if !parsed.percent_used.is_finite() {
        return Err(malformed());
    }
    let used = parsed.percent_used.clamp(0.0, 100.0);
    // Rounded to whole percent; the value is a meter reading, not money.
    let remaining = (100.0 - used).round() as u64;
    let mut extra = vec![(
        "Usage available".to_owned(),
        if parsed.has_usage_remaining { "Yes" } else { "No" }.to_owned(),
    )];
    if let Some(x) = parsed.extra_usage {
        let limit = x.limit_usd_cents.map(dollars).unwrap_or_else(|| "no cap".to_owned());
        extra.push(("Extra usage".to_owned(), format!("{} of {limit}", dollars(x.spent_usd_cents))));
    }
    Ok(Snapshot {
        taken_at: now,
        remaining: Some(remaining),
        limit: Some(100),
        period_end: parsed.period_end.as_deref().and_then(parse_timestamp),
        spent: None,
        spend_per_hour: None,
        unit: None,
        extra,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> &'static str {
        match name {
            "200" => include_str!("../../tests/fixtures/elicit_200.json"),
            "401" => include_str!("../../tests/fixtures/elicit_401.json"),
            "403" => include_str!("../../tests/fixtures/elicit_403.json"),
            "409a" => include_str!("../../tests/fixtures/elicit_409_not_available.json"),
            "409r" => include_str!("../../tests/fixtures/elicit_409_not_ready.json"),
            "429" => include_str!("../../tests/fixtures/elicit_429.json"),
            "500" => include_str!("../../tests/fixtures/elicit_500.json"),
            _ => unreachable!(),
        }
    }

    fn run(status: StatusCode, name: &str) -> Result<Snapshot, ProviderError> {
        parse(status, None, fixture(name).as_bytes(), Timestamp::UNIX_EPOCH)
    }

    #[test]
    fn documented_200_parses_despite_placeholder_dates() {
        let s = run(StatusCode::OK, "200").unwrap();
        assert_eq!(s.remaining, Some(99));
        assert_eq!(s.limit, Some(100));
        // The documented example carries the schema placeholder "string".
        assert_eq!(s.period_end, None);
        assert!(s.extra.contains(&("Extra usage".to_owned(), "$0.01 of no cap".to_owned())));
    }

    #[test]
    fn documented_errors_map() {
        assert_eq!(run(StatusCode::UNAUTHORIZED, "401"), Err(ProviderError::Unauthorized));
        assert!(matches!(run(StatusCode::FORBIDDEN, "403"), Err(ProviderError::Unsupported(m)) if m.contains("Pro or above")));
        assert!(matches!(run(StatusCode::CONFLICT, "409a"), Err(ProviderError::Unsupported(_))));
        assert_eq!(
            run(StatusCode::CONFLICT, "409r"),
            Err(ProviderError::Vendor("Usage data is not ready yet; retry shortly.".into()))
        );
        assert_eq!(
            run(StatusCode::TOO_MANY_REQUESTS, "429"),
            Err(ProviderError::RateLimited {
                retry_after: Some(RATE_LIMIT_BLOCK),
                message: Some("Rate limit exceeded. Too many requests. Please try again later.".into())
            })
        );
        assert_eq!(
            run(StatusCode::INTERNAL_SERVER_ERROR, "500"),
            Err(ProviderError::Vendor("Internal server error".into()))
        );
    }

    #[test]
    fn dollars_format() {
        assert_eq!(dollars(0), "$0.00");
        assert_eq!(dollars(12345), "$123.45");
        assert_eq!(dollars(-5), "-$0.05");
    }
}
