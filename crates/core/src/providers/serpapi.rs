//! SerpApi account usage.
//!
//! Verified 2026-10-16 against <https://serpapi.com/account-api>:
//! `GET https://serpapi.com/account.json?api_key=<key>`. The call is free and
//! does not count against the monthly quota. `plan_renewal_date` is `null` for
//! accounts without an active monthly plan. Error bodies are `{"error": text}`
//! with 401 for a bad key and 429 for throughput or exhausted searches
//! (<https://serpapi.com/api-status-and-error-codes>).
//!
//! The key travels in the query string because that is the only documented
//! way. `net::Http` never lets the URL reach an error or a log. The response
//! echoes `api_key` and `account_email`; `Body` deliberately has no such
//! fields, so serde skips them and the raw buffer is zeroized by the caller.

use std::time::Duration;

use async_trait::async_trait;
use jiff::Timestamp;
use reqwest::StatusCode;
use secrecy::SecretString;
use serde::Deserialize;

use super::{error_message, http_status_message, malformed};
use crate::net::{Auth, Http};
use crate::provider::{Category, Provider, ProviderError, Snapshot, Unit, parse_date};

pub struct SerpApi;

const ENDPOINT: &str = "https://serpapi.com/account.json";
const HOST: &str = "serpapi.com";

/// Only the numeric usage fields and the renewal date. No identifiers.
#[derive(Deserialize)]
struct Body {
    plan_renewal_date: Option<String>,
    searches_per_month: Option<i64>,
    plan_searches_left: Option<i64>,
    extra_credits: Option<i64>,
    total_searches_left: Option<i64>,
    this_hour_searches: Option<i64>,
    account_rate_limit_per_hour: Option<i64>,
}

fn non_negative(v: Option<i64>) -> Option<u64> {
    v.map(|n| n.max(0).unsigned_abs())
}

#[async_trait]
impl Provider for SerpApi {
    fn id(&self) -> &'static str {
        "serpapi"
    }

    fn display_name(&self) -> &'static str {
        "SerpApi"
    }

    fn docs_url(&self) -> &'static str {
        "https://serpapi.com/account-api"
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

    async fn fetch(&self, key: &SecretString, _config: Option<&str>, http: &Http) -> Result<Snapshot, ProviderError> {
        let response = http
            .get(
                ENDPOINT,
                HOST,
                Auth::Query {
                    name: "api_key",
                    value: key,
                },
            )
            .await?;
        parse(response.status, response.retry_after, &response.body, Timestamp::now())
    }
}

pub(crate) fn parse(status: StatusCode, retry_after: Option<Duration>, body: &[u8], now: Timestamp) -> Result<Snapshot, ProviderError> {
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => return Err(ProviderError::Unauthorized),
        StatusCode::TOO_MANY_REQUESTS => {
            return Err(ProviderError::RateLimited {
                retry_after,
                message: error_message(body),
            });
        }
        s if !s.is_success() => {
            return Err(ProviderError::Vendor(error_message(body).unwrap_or_else(|| http_status_message(s))));
        }
        _ => {}
    }
    let parsed: Body = serde_json::from_slice(body).map_err(|_| malformed())?;
    let monthly = non_negative(parsed.searches_per_month).filter(|m| *m > 0);
    let mut extra = Vec::new();
    let (remaining, limit) = match monthly {
        Some(m) => {
            if let Some(c) = non_negative(parsed.extra_credits) {
                extra.push(("Extra credits".to_owned(), c.to_string()));
            }
            if let Some(t) = non_negative(parsed.total_searches_left) {
                extra.push(("Total searches left".to_owned(), t.to_string()));
            }
            (non_negative(parsed.plan_searches_left), Some(m))
        }
        // No monthly plan: the credit pool is all there is.
        None => (non_negative(parsed.total_searches_left), None),
    };
    if let (Some(used), Some(cap)) = (
        non_negative(parsed.this_hour_searches),
        non_negative(parsed.account_rate_limit_per_hour),
    ) {
        extra.push(("This hour".to_owned(), format!("{used} of {cap}")));
    }
    Ok(Snapshot {
        taken_at: now,
        remaining,
        limit,
        period_end: parsed.plan_renewal_date.as_deref().and_then(parse_date),
        spent: None,
        spend_per_hour: None,
        extra,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &str = include_str!("../../tests/fixtures/serpapi_200.json");
    const BAD_KEY: &str = include_str!("../../tests/fixtures/serpapi_401.json");
    const OUT: &str = include_str!("../../tests/fixtures/serpapi_429.json");

    #[test]
    fn documented_200_parses() {
        let s = parse(StatusCode::OK, None, OK.as_bytes(), Timestamp::UNIX_EPOCH).unwrap();
        assert_eq!(s.remaining, Some(5958));
        assert_eq!(s.limit, Some(30000));
        assert_eq!(s.period_end.unwrap().to_string(), "2026-11-16T00:00:00Z");
        assert!(s.extra.contains(&("This hour".to_owned(), "87 of 6000".to_owned())));
    }

    #[test]
    fn echoed_key_and_email_do_not_survive() {
        let s = parse(StatusCode::OK, None, OK.as_bytes(), Timestamp::UNIX_EPOCH).unwrap();
        let dump = format!("{s:?}");
        assert!(!dump.contains("SECRET_API_KEY"), "{dump}");
        assert!(!dump.contains("demo@serpapi.com"), "{dump}");
        assert!(!dump.contains("5ac54d6adefb2f1dba1663f5"), "{dump}");
    }

    #[test]
    fn null_renewal_date_is_none() {
        let body = OK.replace("\"2026-11-16\"", "null");
        let s = parse(StatusCode::OK, None, body.as_bytes(), Timestamp::UNIX_EPOCH).unwrap();
        assert_eq!(s.period_end, None);
    }

    #[test]
    fn documented_401_is_unauthorized() {
        assert_eq!(
            parse(StatusCode::UNAUTHORIZED, None, BAD_KEY.as_bytes(), Timestamp::UNIX_EPOCH),
            Err(ProviderError::Unauthorized)
        );
    }

    #[test]
    fn documented_429_keeps_vendor_text() {
        let e = parse(StatusCode::TOO_MANY_REQUESTS, None, OUT.as_bytes(), Timestamp::UNIX_EPOCH).unwrap_err();
        assert_eq!(
            e,
            ProviderError::RateLimited {
                retry_after: None,
                message: Some("Your account has run out of searches.".into())
            }
        );
    }
}
