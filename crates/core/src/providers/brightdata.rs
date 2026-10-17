//! Bright Data account balance.
//!
//! Verified 2026-10-17 against
//! <https://docs.brightdata.com/api-reference/account-management-api/Get_total_balance_through_API>:
//! `GET https://api.brightdata.com/customer/balance`, `Authorization: Bearer
//! <key>`. 200 body `{balance, credit, prepayment, pending_costs}`, all
//! numbers in USD: `balance` is "current available funds", `credit` is
//! "credit on your account", `prepayment` "applies to custom plans with a
//! monthly prepayment commitment", `pending_costs` is "current usage costs
//! expected on next month's invoice". The page's example is
//! `{"balance": 220, "credit": 0, "prepayment": 0, "pending_costs": 0}`.
//!
//! The page documents no error body and only the 200 response, so the
//! status mapping below is the shared one and the tests use empty bodies.
//! It does not state a price for the call; it is an Account Management API
//! read, not a proxy or scraper request, so it is treated as free. The FAQ
//! on the page says no endpoint exposes free-tier credits, so a free-tier
//! account shows funds only.

use std::time::Duration;

use async_trait::async_trait;
use jiff::Timestamp;
use reqwest::StatusCode;
use secrecy::SecretString;
use serde::Deserialize;

use super::{cents, dollars, error_message, http_status_message, malformed};
use crate::net::{Auth, Http};
use crate::provider::{Category, Provider, ProviderError, Snapshot, Unit};

pub struct BrightData;

const ENDPOINT: &str = "https://api.brightdata.com/customer/balance";
const HOST: &str = "api.brightdata.com";

#[derive(Deserialize)]
struct Body {
    balance: Option<f64>,
    credit: Option<f64>,
    prepayment: Option<f64>,
    pending_costs: Option<f64>,
}

#[async_trait]
impl Provider for BrightData {
    fn id(&self) -> &'static str {
        "brightdata"
    }

    fn display_name(&self) -> &'static str {
        "Bright Data"
    }

    fn docs_url(&self) -> &'static str {
        "https://docs.brightdata.com/api-reference/account-management-api/Get_total_balance_through_API"
    }

    fn endpoint(&self) -> &'static str {
        ENDPOINT
    }

    fn allowed_host(&self) -> &'static str {
        HOST
    }

    fn unit(&self) -> Unit {
        Unit::Usd
    }

    fn category(&self) -> Category {
        Category::Search
    }

    fn min_poll_interval(&self) -> Duration {
        Duration::from_secs(60)
    }

    fn note(&self) -> Option<&'static str> {
        Some("Shows account funds. Bright Data's API does not expose free-tier credits.")
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
    let balance = cents(parsed.balance).ok_or_else(malformed)?;
    let mut extra = Vec::new();
    if let Some(c) = cents(parsed.credit).filter(|c| *c > 0) {
        extra.push(("Credit".to_owned(), dollars(c)));
    }
    if let Some(p) = cents(parsed.prepayment).filter(|p| *p > 0) {
        extra.push(("Prepayment".to_owned(), dollars(p)));
    }
    Ok(Snapshot {
        taken_at: now,
        remaining: Some(balance),
        limit: None,
        period_end: None,
        spent: cents(parsed.pending_costs),
        spend_per_hour: None,
        unit: None,
        extra,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &str = include_str!("../../tests/fixtures/brightdata_200.json");

    fn run(status: StatusCode, body: &str) -> Result<Snapshot, ProviderError> {
        parse(status, None, body.as_bytes(), Timestamp::UNIX_EPOCH)
    }

    #[test]
    fn documented_200_parses_in_cents() {
        let s = run(StatusCode::OK, OK).unwrap();
        assert_eq!(s.remaining, Some(22000));
        assert_eq!(s.limit, None);
        assert_eq!(s.spent, Some(0));
        assert!(s.extra.is_empty());
    }

    #[test]
    fn non_zero_credit_and_prepayment_become_extras() {
        let body = r#"{"balance": 12.5, "credit": 3.25, "prepayment": 100, "pending_costs": 7.125}"#;
        let s = run(StatusCode::OK, body).unwrap();
        assert_eq!(s.remaining, Some(1250));
        assert_eq!(s.spent, Some(713));
        assert_eq!(
            s.extra,
            vec![
                ("Credit".to_owned(), "$3.25".to_owned()),
                ("Prepayment".to_owned(), "$100.00".to_owned()),
            ]
        );
    }

    #[test]
    fn nothing_but_numbers_survive() {
        let s = run(StatusCode::OK, OK).unwrap();
        let dump = format!("{s:?}");
        assert!(!dump.contains("balance"), "{dump}");
        assert!(!dump.contains("Bearer"), "{dump}");
    }

    #[test]
    fn statuses_map() {
        assert_eq!(run(StatusCode::UNAUTHORIZED, ""), Err(ProviderError::Unauthorized));
        assert_eq!(run(StatusCode::FORBIDDEN, ""), Err(ProviderError::Unauthorized));
        assert_eq!(
            run(StatusCode::TOO_MANY_REQUESTS, r#"{"message":"Too many requests"}"#),
            Err(ProviderError::RateLimited {
                retry_after: None,
                message: Some("Too many requests".into())
            })
        );
        assert_eq!(
            run(StatusCode::INTERNAL_SERVER_ERROR, ""),
            Err(ProviderError::Vendor("The vendor returned HTTP 500.".into()))
        );
        assert_eq!(run(StatusCode::OK, r#"{"credit": 0}"#), Err(malformed()));
    }
}
