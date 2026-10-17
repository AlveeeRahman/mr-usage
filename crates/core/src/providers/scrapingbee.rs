//! ScrapingBee credit usage.
//!
//! Verified 2026-10-17 against <https://www.scrapingbee.com/documentation/>
//! ("Usage endpoint"): `GET https://app.scrapingbee.com/api/v1/usage`,
//! `Authorization: Bearer <key>`. 200 body `{max_api_credit,
//! used_api_credit, max_concurrency, current_concurrency,
//! renewal_subscription_date}`. The example date is
//! `"2022-04-18T10:05:58.134716"`, a civil date-time with no offset, so it
//! is read as UTC; a value with an offset is accepted too.
//!
//! The page says calls to this endpoint "will not increase concurrency, but
//! you can only call it 6 times per minute", so the global one-minute floor
//! holds. It does not say in so many words that the call is free of
//! credits; it is a usage read, not a scrape, and the "Status Code" table
//! bills only successful scrapes. Treated as free.
//!
//! Error shapes come from that table, which is written for the scraping
//! endpoint: 401 "Out of credits, or an invalid API key", 403 "The account
//! is banned", 429 "Too many concurrent requests", 400 and 500 with "the
//! message in the response body". No example error body is published, so
//! messages go through the shared `error_message` extractor and the tests
//! use empty bodies for the status mapping.

use std::time::Duration;

use async_trait::async_trait;
use jiff::Timestamp;
use reqwest::StatusCode;
use secrecy::SecretString;
use serde::Deserialize;

use super::{count, error_message, http_status_message, malformed};
use crate::net::{Auth, Http};
use crate::provider::{Category, Provider, ProviderError, Snapshot, Unit, parse_timestamp};

pub struct ScrapingBee;

const ENDPOINT: &str = "https://app.scrapingbee.com/api/v1/usage";
const HOST: &str = "app.scrapingbee.com";

/// Numbers and the renewal date only.
#[derive(Deserialize)]
struct Body {
    max_api_credit: Option<f64>,
    used_api_credit: Option<f64>,
    max_concurrency: Option<f64>,
    current_concurrency: Option<f64>,
    renewal_subscription_date: Option<String>,
}

#[async_trait]
impl Provider for ScrapingBee {
    fn id(&self) -> &'static str {
        "scrapingbee"
    }

    fn display_name(&self) -> &'static str {
        "ScrapingBee"
    }

    fn docs_url(&self) -> &'static str {
        "https://www.scrapingbee.com/documentation/#usage-endpoint"
    }

    fn endpoint(&self) -> &'static str {
        ENDPOINT
    }

    fn allowed_host(&self) -> &'static str {
        HOST
    }

    fn unit(&self) -> Unit {
        Unit::Credits
    }

    fn category(&self) -> Category {
        Category::Search
    }

    fn min_poll_interval(&self) -> Duration {
        Duration::from_secs(60)
    }

    fn note(&self) -> Option<&'static str> {
        Some("ScrapingBee allows six usage reads a minute; the card polls at most once a minute.")
    }

    async fn fetch(&self, key: &SecretString, _config: Option<&str>, http: &Http) -> Result<Snapshot, ProviderError> {
        let response = http.get(ENDPOINT, HOST, Auth::Bearer(key)).await?;
        parse(response.status, response.retry_after, &response.body, Timestamp::now())
    }
}

/// RFC 3339 first, then a civil date-time with no offset read as UTC.
fn parse_renewal(raw: &str) -> Option<Timestamp> {
    parse_timestamp(raw).or_else(|| {
        let civil: jiff::civil::DateTime = raw.parse().ok()?;
        civil.to_zoned(jiff::tz::TimeZone::UTC).ok().map(|z| z.timestamp())
    })
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
    let limit = count(parsed.max_api_credit).ok_or_else(malformed)?;
    let used = count(parsed.used_api_credit).ok_or_else(malformed)?;
    let mut extra = Vec::new();
    if let (Some(now_running), Some(cap)) = (count(parsed.current_concurrency), count(parsed.max_concurrency)) {
        extra.push(("Concurrency".to_owned(), format!("{now_running} of {cap}")));
    }
    Ok(Snapshot {
        taken_at: now,
        remaining: Some(limit.saturating_sub(used)),
        limit: Some(limit),
        period_end: parsed.renewal_subscription_date.as_deref().and_then(parse_renewal),
        spent: None,
        spend_per_hour: None,
        unit: None,
        extra,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &str = include_str!("../../tests/fixtures/scrapingbee_200.json");

    fn run(status: StatusCode, body: &str) -> Result<Snapshot, ProviderError> {
        parse(status, None, body.as_bytes(), Timestamp::UNIX_EPOCH)
    }

    #[test]
    fn documented_200_parses() {
        let s = run(StatusCode::OK, OK).unwrap();
        assert_eq!(s.remaining, Some(20_000_000 - 3_704_332));
        assert_eq!(s.limit, Some(20_000_000));
        assert_eq!(s.period_end.unwrap().to_string(), "2022-04-18T10:05:58.134716Z");
        assert_eq!(s.extra, vec![("Concurrency".to_owned(), "1 of 200".to_owned())]);
    }

    #[test]
    fn renewal_with_an_offset_is_accepted_too() {
        let body = OK.replace("2022-04-18T10:05:58.134716", "2022-04-18T12:05:58+02:00");
        let s = run(StatusCode::OK, &body).unwrap();
        assert_eq!(s.period_end.unwrap().to_string(), "2022-04-18T10:05:58Z");
    }

    #[test]
    fn overuse_does_not_underflow() {
        let body = OK.replace("3704332", "25000000");
        let s = run(StatusCode::OK, &body).unwrap();
        assert_eq!(s.remaining, Some(0));
    }

    #[test]
    fn only_numbers_and_the_renewal_survive() {
        let s = run(StatusCode::OK, OK).unwrap();
        let dump = format!("{s:?}");
        assert!(!dump.contains("YOUR-API-KEY"), "{dump}");
        assert!(!dump.contains("api_credit"), "{dump}");
    }

    #[test]
    fn documented_statuses_map() {
        assert_eq!(run(StatusCode::UNAUTHORIZED, ""), Err(ProviderError::Unauthorized));
        assert_eq!(run(StatusCode::FORBIDDEN, ""), Err(ProviderError::Unauthorized));
        assert_eq!(
            run(StatusCode::TOO_MANY_REQUESTS, ""),
            Err(ProviderError::RateLimited {
                retry_after: None,
                message: None
            })
        );
        assert_eq!(
            run(StatusCode::BAD_REQUEST, r#"{"message":"Incorrect parameters"}"#),
            Err(ProviderError::Vendor("Incorrect parameters".into()))
        );
        assert_eq!(
            run(StatusCode::INTERNAL_SERVER_ERROR, ""),
            Err(ProviderError::Vendor("The vendor returned HTTP 500.".into()))
        );
        assert_eq!(run(StatusCode::OK, "{}"), Err(malformed()));
    }
}
