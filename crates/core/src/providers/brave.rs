//! Brave Search monthly quota, read from rate-limit headers.
//!
//! Verified 2026-10-17 against
//! <https://api-dashboard.search.brave.com/documentation/guides/rate-limiting>
//! and <https://api-dashboard.search.brave.com/api-reference/web/search/get>.
//! Brave has no usage endpoint. Every response carries `X-RateLimit-Limit`,
//! `X-RateLimit-Policy`, `X-RateLimit-Remaining` and `X-RateLimit-Reset`, each
//! a comma-separated pair: the per-second value, then the monthly value. The
//! page's example is `X-RateLimit-Limit: 1, 15000`,
//! `X-RateLimit-Remaining: 0, 14523`, `X-RateLimit-Reset: 1, 1234567`
//! (seconds from now). A monthly limit of `0` means unlimited. The page says
//! twice that only successful requests are counted and billed.
//!
//! So the card sends `GET https://api.search.brave.com/res/v1/web/search`
//! with the key in the `X-Subscription-Token` header (the API reference's
//! only authentication) and no `q` parameter, which the reference marks
//! required. The API rejects that with a 422, the rejection is not billed,
//! and the quota headers on the rejection are what `parse` reads. Should
//! Brave ever accept the request, the same headers are read from the 200.
//! The body is never deserialized on that path, so no identifier from it can
//! reach the `Snapshot`.
//!
//! Error shapes, from the API reference: 404, 422 and 429 bodies are
//! `{type: "ErrorResponse", error: {id, status, detail, meta, code}, time}`
//! where `code` is one of `INTERNAL`, `QUOTA_LIMITED`, `RATE_LIMITED` and
//! others. A 401/403 is a bad key. A 429 carries the same headers: when the
//! monthly remaining is zero the month is used up and the card shows that
//! as a quota of zero; otherwise it is the per-second window and the card
//! reports a rate limit with `error.detail` as the message.
//!
//! Cost: nothing, per the rate-limiting page ("Failed requests don't count
//! against your quota, so you're only charged for successful API calls").
//! The floor is the global one minute; the probe is one request against a
//! per-second burst limit, so it cannot trip the per-second window on its
//! own.
//!
//! Fixtures: `brave_headers.json` holds the rate-limiting page's "Example
//! Response Headers" block as a name/value object with lower-cased names
//! (reqwest lower-cases header names), values verbatim. The API reference
//! shows the 422 schema but no example body, so `brave_422.json` is the
//! smallest body matching that schema, with a made-up `error.id`.

use std::time::Duration;

use async_trait::async_trait;
use jiff::Timestamp;
use reqwest::StatusCode;
use secrecy::SecretString;
use serde::Deserialize;

use super::{MAX_VENDOR_TEXT, error_message, http_status_message, malformed};
use crate::net::{Auth, Http};
use crate::provider::{Category, Provider, ProviderError, Snapshot, Unit};
use crate::redact;

pub struct Brave;

const ENDPOINT: &str = "https://api.search.brave.com/res/v1/web/search";
const HOST: &str = "api.search.brave.com";

#[async_trait]
impl Provider for Brave {
    fn id(&self) -> &'static str {
        "brave"
    }

    fn display_name(&self) -> &'static str {
        "Brave Search"
    }

    fn docs_url(&self) -> &'static str {
        "https://api-dashboard.search.brave.com/documentation/guides/rate-limiting"
    }

    fn endpoint(&self) -> &'static str {
        ENDPOINT
    }

    fn allowed_host(&self) -> &'static str {
        HOST
    }

    fn unit(&self) -> Unit {
        Unit::Searches
    }

    fn category(&self) -> Category {
        Category::Search
    }

    fn min_poll_interval(&self) -> Duration {
        Duration::from_secs(60)
    }

    fn note(&self) -> Option<&'static str> {
        Some(
            "Brave has no usage endpoint. The card sends a search request the API rejects on validation and reads the quota headers on the rejection; Brave bills only successful requests.",
        )
    }

    async fn fetch(&self, key: &SecretString, _config: Option<&str>, http: &Http) -> Result<Snapshot, ProviderError> {
        let response = http
            .get_with_headers(
                ENDPOINT,
                HOST,
                Auth::Header {
                    name: "X-Subscription-Token",
                    prefix: "",
                    value: key,
                },
                &[("Accept", "application/json")],
            )
            .await?;
        parse(
            response.status,
            response.retry_after,
            &response.rate_limit,
            &response.body,
            Timestamp::now(),
        )
    }
}

/// `error.detail`, else `error.code`, from Brave's `ErrorResponse`; falls
/// back to the shared extractor. The `id` is never read.
fn brave_message(body: &[u8]) -> Option<String> {
    #[derive(Deserialize)]
    struct Inner {
        detail: Option<String>,
        code: Option<String>,
    }
    #[derive(Deserialize)]
    struct Envelope {
        error: Option<Inner>,
    }
    let text = serde_json::from_slice::<Envelope>(body)
        .ok()
        .and_then(|e| e.error)
        .and_then(|e| e.detail.or(e.code))
        .map(|t| redact::vendor_text(&t, MAX_VENDOR_TEXT))
        .filter(|t| !t.is_empty());
    text.or_else(|| error_message(body))
}

/// One value of a `per-second, per-month` header: `index` 0 or 1.
fn header_value(headers: &[(String, String)], name: &str, index: usize) -> Option<u64> {
    let (_, value) = headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(name))?;
    value.split(',').nth(index)?.trim().parse().ok()
}

pub(crate) fn parse(
    status: StatusCode,
    retry_after: Option<Duration>,
    headers: &[(String, String)],
    body: &[u8],
    now: Timestamp,
) -> Result<Snapshot, ProviderError> {
    let limit = header_value(headers, "x-ratelimit-limit", 1);
    let remaining = header_value(headers, "x-ratelimit-remaining", 1);
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => return Err(ProviderError::Unauthorized),
        // A used-up month is a reading, not a failure. Anything else on a
        // 429 is the per-second window.
        StatusCode::TOO_MANY_REQUESTS if !(remaining == Some(0) && limit.is_some_and(|l| l > 0)) => {
            return Err(ProviderError::RateLimited {
                retry_after,
                message: brave_message(body),
            });
        }
        s if s.is_server_error() => {
            return Err(ProviderError::Vendor(brave_message(body).unwrap_or_else(|| http_status_message(s))));
        }
        _ => {}
    }
    let (Some(limit), Some(remaining)) = (limit, remaining) else {
        if status.is_success() {
            return Err(malformed());
        }
        return Err(ProviderError::Unsupported(
            "Brave answered without its X-RateLimit headers, so the quota cannot be read from this key.".into(),
        ));
    };
    let period_end = header_value(headers, "x-ratelimit-reset", 1).and_then(|secs| now.checked_add(Duration::from_secs(secs)).ok());
    let mut extra = Vec::new();
    let (remaining, limit) = if limit == 0 {
        extra.push(("Plan".to_owned(), "pay as you go".to_owned()));
        (None, None)
    } else {
        (Some(remaining), Some(limit))
    };
    if let Some(burst) = header_value(headers, "x-ratelimit-limit", 0).filter(|b| *b > 0) {
        extra.push(("Rate limit".to_owned(), format!("{burst} per second")));
    }
    Ok(Snapshot {
        taken_at: now,
        remaining,
        limit,
        period_end,
        spent: None,
        spend_per_hour: None,
        unit: None,
        extra,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEADERS: &str = include_str!("../../tests/fixtures/brave_headers.json");
    const REJECTION: &str = include_str!("../../tests/fixtures/brave_422.json");

    fn headers() -> Vec<(String, String)> {
        let map: serde_json::Map<String, serde_json::Value> = serde_json::from_str(HEADERS).unwrap();
        map.into_iter().map(|(k, v)| (k, v.as_str().unwrap().to_owned())).collect()
    }

    fn with_remaining(value: &str) -> Vec<(String, String)> {
        headers()
            .into_iter()
            .map(|(k, v)| {
                if k == "x-ratelimit-remaining" {
                    (k, value.to_owned())
                } else {
                    (k, v)
                }
            })
            .collect()
    }

    fn run(status: StatusCode, headers: &[(String, String)], body: &str) -> Result<Snapshot, ProviderError> {
        parse(status, None, headers, body.as_bytes(), Timestamp::UNIX_EPOCH)
    }

    #[test]
    fn documented_headers_parse_on_the_validation_rejection() {
        let s = run(StatusCode::UNPROCESSABLE_ENTITY, &headers(), REJECTION).unwrap();
        assert_eq!(s.remaining, Some(14523));
        assert_eq!(s.limit, Some(15000));
        assert_eq!(s.period_end.unwrap().to_string(), "1970-01-15T06:56:07Z");
        assert_eq!(s.extra, vec![("Rate limit".to_owned(), "1 per second".to_owned())]);
    }

    #[test]
    fn same_headers_parse_on_a_200() {
        let s = run(StatusCode::OK, &headers(), "{}").unwrap();
        assert_eq!((s.remaining, s.limit), (Some(14523), Some(15000)));
    }

    #[test]
    fn zero_monthly_limit_means_unlimited() {
        let h: Vec<_> = headers()
            .into_iter()
            .map(|(k, v)| if k == "x-ratelimit-limit" { (k, "1, 0".to_owned()) } else { (k, v) })
            .collect();
        let s = run(StatusCode::UNPROCESSABLE_ENTITY, &h, REJECTION).unwrap();
        assert_eq!((s.remaining, s.limit), (None, None));
        assert!(s.extra.contains(&("Plan".to_owned(), "pay as you go".to_owned())));
    }

    #[test]
    fn header_names_match_case_insensitively() {
        let h: Vec<_> = headers().into_iter().map(|(k, v)| (k.to_uppercase(), v)).collect();
        assert!(run(StatusCode::BAD_REQUEST, &h, "").is_ok());
    }

    #[test]
    fn missing_headers_on_a_rejection_are_unsupported() {
        let e = run(StatusCode::UNPROCESSABLE_ENTITY, &[], REJECTION).unwrap_err();
        assert!(matches!(e, ProviderError::Unsupported(m) if m.contains("X-RateLimit")));
    }

    #[test]
    fn missing_headers_on_a_200_are_malformed() {
        assert_eq!(run(StatusCode::OK, &[], "{}"), Err(malformed()));
    }

    #[test]
    fn used_up_month_is_a_zero_quota_not_an_error() {
        let quota = REJECTION.replace("VALIDATION", "QUOTA_LIMITED").replace("422", "429");
        let s = run(StatusCode::TOO_MANY_REQUESTS, &with_remaining("1, 0"), &quota).unwrap();
        assert_eq!((s.remaining, s.limit), (Some(0), Some(15000)));
    }

    #[test]
    fn per_second_429_is_rate_limited_with_the_detail() {
        let burst = REJECTION
            .replace("VALIDATION", "RATE_LIMITED")
            .replace("422", "429")
            .replace("Unable to validate request parameter(s)", "Request rate limit exceeded for plan.");
        let e = parse(
            StatusCode::TOO_MANY_REQUESTS,
            Some(Duration::from_secs(1)),
            &with_remaining("0, 14523"),
            burst.as_bytes(),
            Timestamp::UNIX_EPOCH,
        )
        .unwrap_err();
        assert_eq!(
            e,
            ProviderError::RateLimited {
                retry_after: Some(Duration::from_secs(1)),
                message: Some("Request rate limit exceeded for plan.".into())
            }
        );
        // No headers at all on a 429 is still a rate limit, not Unsupported.
        assert!(matches!(
            run(StatusCode::TOO_MANY_REQUESTS, &[], ""),
            Err(ProviderError::RateLimited { message: None, .. })
        ));
    }

    #[test]
    fn auth_and_server_errors_map() {
        assert_eq!(run(StatusCode::UNAUTHORIZED, &headers(), ""), Err(ProviderError::Unauthorized));
        assert_eq!(run(StatusCode::FORBIDDEN, &[], ""), Err(ProviderError::Unauthorized));
        let internal = REJECTION
            .replace("VALIDATION", "INTERNAL")
            .replace("422", "500")
            .replace("Unable to validate request parameter(s)", "Something went wrong.");
        assert_eq!(
            run(StatusCode::INTERNAL_SERVER_ERROR, &headers(), &internal),
            Err(ProviderError::Vendor("Something went wrong.".into()))
        );
        assert_eq!(
            run(StatusCode::BAD_GATEWAY, &[], "<html>"),
            Err(ProviderError::Vendor("The vendor returned HTTP 502.".into()))
        );
    }

    #[test]
    fn body_identifiers_do_not_survive() {
        let s = run(StatusCode::UNPROCESSABLE_ENTITY, &headers(), REJECTION).unwrap();
        let dump = format!("{s:?}");
        assert!(!dump.contains("7a9c1e4b"), "{dump}");
        assert!(!dump.contains("ErrorResponse"), "{dump}");
        assert!(!dump.contains("1700000000"), "{dump}");
        let e = run(StatusCode::INTERNAL_SERVER_ERROR, &[], &REJECTION.replace("422", "500")).unwrap_err();
        assert!(!format!("{e:?}").contains("7a9c1e4b"), "{e:?}");
    }
}
