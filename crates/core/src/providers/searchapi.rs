//! SearchApi.io account usage.
//!
//! Verified 2026-10-17 against <https://www.searchapi.io/docs/account-api>:
//! `GET https://www.searchapi.io/api/v1/me`. The page offers the key either
//! as `?api_key=` or as `Authorization: Bearer <key>`; this module uses the
//! header. 200 body `{account: {current_month_usage, monthly_allowance,
//! remaining_credits}, api_usage: {searches_this_hour, hourly_rate_limit},
//! subscription: {period_start, period_end}}`, where `subscription` is
//! present only "if an active subscription exists". All numbers are search
//! counts; the dates are RFC 3339.
//!
//! The page documents no error body and does not say whether the call is
//! billed. It reports "real-time data on your current monthly usage" and is
//! not a search engine call, so it is treated as free; this is unverified.
//! Error bodies go through the shared `error_message` extractor, and the
//! tests use empty bodies for the status mapping. The response in the docs
//! carries no identifier; `Body` reads only the numbers and `period_end`.

use std::time::Duration;

use async_trait::async_trait;
use jiff::Timestamp;
use reqwest::StatusCode;
use secrecy::SecretString;
use serde::Deserialize;

use super::{count, error_message, http_status_message, malformed};
use crate::net::{Auth, Http};
use crate::provider::{Category, Provider, ProviderError, Snapshot, Unit, parse_timestamp};

pub struct SearchApi;

const ENDPOINT: &str = "https://www.searchapi.io/api/v1/me";
const HOST: &str = "www.searchapi.io";

#[derive(Deserialize)]
struct Body {
    account: Account,
    api_usage: Option<ApiUsage>,
    subscription: Option<Subscription>,
}

#[derive(Deserialize)]
struct Account {
    current_month_usage: Option<f64>,
    monthly_allowance: Option<f64>,
    remaining_credits: Option<f64>,
}

#[derive(Deserialize)]
struct ApiUsage {
    searches_this_hour: Option<f64>,
    hourly_rate_limit: Option<f64>,
}

#[derive(Deserialize)]
struct Subscription {
    period_end: Option<String>,
}

#[async_trait]
impl Provider for SearchApi {
    fn id(&self) -> &'static str {
        "searchapi"
    }

    fn display_name(&self) -> &'static str {
        "SearchApi.io"
    }

    fn docs_url(&self) -> &'static str {
        "https://www.searchapi.io/docs/account-api"
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
        let response = http.get(ENDPOINT, HOST, Auth::Bearer(key)).await?;
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
    let mut extra = Vec::new();
    if let Some(used) = count(parsed.account.current_month_usage) {
        extra.push(("Used this month".to_owned(), used.to_string()));
    }
    if let Some(u) = parsed.api_usage
        && let (Some(used), Some(cap)) = (count(u.searches_this_hour), count(u.hourly_rate_limit))
    {
        extra.push(("This hour".to_owned(), format!("{used} of {cap}")));
    }
    Ok(Snapshot {
        taken_at: now,
        remaining: count(parsed.account.remaining_credits),
        limit: count(parsed.account.monthly_allowance).filter(|l| *l > 0),
        period_end: parsed.subscription.and_then(|s| s.period_end).as_deref().and_then(parse_timestamp),
        spent: None,
        spend_per_hour: None,
        unit: None,
        extra,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &str = include_str!("../../tests/fixtures/searchapi_200.json");

    fn run(status: StatusCode, body: &str) -> Result<Snapshot, ProviderError> {
        parse(status, None, body.as_bytes(), Timestamp::UNIX_EPOCH)
    }

    #[test]
    fn documented_200_parses() {
        let s = run(StatusCode::OK, OK).unwrap();
        assert_eq!(s.remaining, Some(5200));
        assert_eq!(s.limit, Some(10000));
        assert_eq!(s.spent, None);
        assert_eq!(s.period_end.unwrap().to_string(), "2025-01-04T00:34:59Z");
        assert_eq!(
            s.extra,
            vec![
                ("Used this month".to_owned(), "4800".to_owned()),
                ("This hour".to_owned(), "250 of 200000".to_owned()),
            ]
        );
    }

    #[test]
    fn no_subscription_means_no_period_end() {
        let body = OK.replace("\"subscription\"", "\"other\"");
        let s = run(StatusCode::OK, &body).unwrap();
        assert_eq!(s.period_end, None);
        assert_eq!(s.remaining, Some(5200));
    }

    #[test]
    fn nothing_but_numbers_and_the_date_survive() {
        let s = run(StatusCode::OK, OK).unwrap();
        let dump = format!("{s:?}");
        // The documented body has no identifier; the period start is the
        // only other string in it and must not be carried along.
        assert!(!dump.contains("2024-12-04"), "{dump}");
        assert!(!dump.contains("api_key"), "{dump}");
    }

    #[test]
    fn error_statuses_map() {
        assert_eq!(run(StatusCode::UNAUTHORIZED, ""), Err(ProviderError::Unauthorized));
        assert_eq!(run(StatusCode::FORBIDDEN, ""), Err(ProviderError::Unauthorized));
        assert_eq!(
            run(StatusCode::TOO_MANY_REQUESTS, r#"{"error":"Rate limit exceeded"}"#),
            Err(ProviderError::RateLimited {
                retry_after: None,
                message: Some("Rate limit exceeded".into())
            })
        );
        assert_eq!(
            run(StatusCode::INTERNAL_SERVER_ERROR, "<html>"),
            Err(ProviderError::Vendor("The vendor returned HTTP 500.".into()))
        );
        assert_eq!(run(StatusCode::OK, "not json"), Err(malformed()));
    }
}
