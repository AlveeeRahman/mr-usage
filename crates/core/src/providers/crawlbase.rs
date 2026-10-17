//! Crawlbase monthly usage for the Crawling API.
//!
//! Verified 2026-10-17 against <https://crawlbase.com/docs/account-api>:
//! `GET https://api.crawlbase.com/account?token=<key>&product=crawling-api`.
//! The key travels as the `token` query parameter because the page offers
//! nothing else; `net::Http` keeps the URL out of errors and logs. Response
//! fields: `totalSuccess` and `totalFailed` (requests this month),
//! `totalDue` ("total amount due in USD for successful requests this
//! month"), `remainingCredits` ("only returned for subscription-based
//! products") and `domainStats`, an array of per-domain breakdowns. The
//! domain names are identifiers, so `domainStats` is never deserialized.
//!
//! The page states "Rate limit: 1 request per 5 minutes" and asks callers
//! to cache the response, hence the 300 s floor. It does not say the call
//! is billed; it returns usage and is not a crawl, so it is treated as
//! free. No error body is documented, so the status mapping is the shared
//! one and the tests use empty bodies.
//!
//! The page shows no example response. `crawlbase_200.json` is the
//! smallest body that matches the documented fields, with one made-up
//! domain so the identifier test has something to check.
//!
//! A subscription account shows a quota in credits. A pay-as-you-go account
//! has no `remainingCredits`; its card shows this month's `totalDue` as
//! spend in US cents instead, through `Snapshot::unit`.

use std::time::Duration;

use async_trait::async_trait;
use jiff::Timestamp;
use reqwest::StatusCode;
use secrecy::SecretString;
use serde::Deserialize;

use super::{cents, count, dollars, error_message, http_status_message, malformed};
use crate::net::{Auth, Http};
use crate::provider::{Category, Provider, ProviderError, Snapshot, Unit};

pub struct Crawlbase;

const ENDPOINT: &str = "https://api.crawlbase.com/account?product=crawling-api";
const HOST: &str = "api.crawlbase.com";

/// Numbers only; `domainStats` is left out on purpose.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Body {
    total_success: Option<f64>,
    total_failed: Option<f64>,
    total_due: Option<f64>,
    remaining_credits: Option<f64>,
}

#[async_trait]
impl Provider for Crawlbase {
    fn id(&self) -> &'static str {
        "crawlbase"
    }

    fn display_name(&self) -> &'static str {
        "Crawlbase"
    }

    fn docs_url(&self) -> &'static str {
        "https://crawlbase.com/docs/account-api"
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
        Duration::from_secs(300)
    }

    fn note(&self) -> Option<&'static str> {
        Some("Crawlbase allows one usage read every five minutes, so this card refreshes no faster. Shows the Crawling API product.")
    }

    async fn fetch(&self, key: &SecretString, _config: Option<&str>, http: &Http) -> Result<Snapshot, ProviderError> {
        let response = http.get(ENDPOINT, HOST, Auth::Query { name: "token", value: key }).await?;
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
    let success = count(parsed.total_success).ok_or_else(malformed)?;
    let mut extra = vec![("Successful this month".to_owned(), success.to_string())];
    if let Some(failed) = count(parsed.total_failed) {
        extra.push(("Failed this month".to_owned(), failed.to_string()));
    }
    let due = cents(parsed.total_due);
    let remaining = count(parsed.remaining_credits);
    let (spent, unit) = match remaining {
        Some(_) => {
            if let Some(d) = due {
                extra.push(("Due this month".to_owned(), dollars(d)));
            }
            (None, None)
        }
        // Pay as you go: no credit pool, so the month's bill is the headline.
        None => (due, due.map(|_| Unit::Usd)),
    };
    Ok(Snapshot {
        taken_at: now,
        remaining,
        limit: None,
        period_end: None,
        spent,
        spend_per_hour: None,
        unit,
        extra,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &str = include_str!("../../tests/fixtures/crawlbase_200.json");

    fn run(status: StatusCode, body: &str) -> Result<Snapshot, ProviderError> {
        parse(status, None, body.as_bytes(), Timestamp::UNIX_EPOCH)
    }

    #[test]
    fn subscription_body_parses_as_a_credit_quota() {
        let s = run(StatusCode::OK, OK).unwrap();
        assert_eq!(s.remaining, Some(8800));
        assert_eq!(s.limit, None);
        assert_eq!(s.spent, None);
        assert_eq!(s.unit, None);
        assert_eq!(
            s.extra,
            vec![
                ("Successful this month".to_owned(), "1200".to_owned()),
                ("Failed this month".to_owned(), "34".to_owned()),
                ("Due this month".to_owned(), "$12.50".to_owned()),
            ]
        );
    }

    #[test]
    fn pay_as_you_go_body_parses_as_spend_in_cents() {
        let body = OK.replace("\"remainingCredits\": 8800,", "");
        let s = run(StatusCode::OK, &body).unwrap();
        assert_eq!(s.remaining, None);
        assert_eq!(s.spent, Some(1250));
        assert_eq!(s.unit, Some(Unit::Usd));
        assert!(!s.extra.iter().any(|(k, _)| k == "Due this month"));
    }

    #[test]
    fn domain_names_do_not_survive() {
        let s = run(StatusCode::OK, OK).unwrap();
        let dump = format!("{s:?}");
        assert!(!dump.contains("example.com"), "{dump}");
        assert!(!dump.contains("domainStats"), "{dump}");
    }

    #[test]
    fn statuses_map() {
        assert_eq!(run(StatusCode::UNAUTHORIZED, ""), Err(ProviderError::Unauthorized));
        assert_eq!(run(StatusCode::FORBIDDEN, ""), Err(ProviderError::Unauthorized));
        let e = parse(
            StatusCode::TOO_MANY_REQUESTS,
            Some(Duration::from_secs(300)),
            b"",
            Timestamp::UNIX_EPOCH,
        )
        .unwrap_err();
        assert_eq!(
            e,
            ProviderError::RateLimited {
                retry_after: Some(Duration::from_secs(300)),
                message: None
            }
        );
        assert_eq!(
            run(StatusCode::BAD_REQUEST, r#"{"error":"Invalid product"}"#),
            Err(ProviderError::Vendor("Invalid product".into()))
        );
        assert_eq!(run(StatusCode::OK, "[]"), Err(malformed()));
    }

    #[test]
    fn floor_is_five_minutes() {
        use crate::provider::Provider;
        assert_eq!(Crawlbase.min_poll_interval(), Duration::from_secs(300));
    }
}
