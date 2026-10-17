//! Linkup credit balance.
//!
//! Verified 2026-10-17 against
//! <https://docs.linkup.so/pages/documentation/endpoints/account/balance>:
//! `GET https://api.linkup.so/v1/credits/balance`, `Authorization: Bearer
//! <key>`. 200 body `{balance}`, "the number of credits remaining in your
//! account", example `{"balance": 123.456}`. Credits are US dollars:
//! <https://docs.linkup.so/pages/documentation/platform/pricing> lists the
//! API-key payment method as "USD, prepaid credit balance", credits new
//! accounts with $20 and prices every call in dollars. The balance is shown
//! in whole cents.
//!
//! Errors, per <https://docs.linkup.so/pages/documentation/platform/errors>:
//! every error body is `{statusCode, error: {code, message, details: [...]}}`;
//! 401 for a missing or invalid key, 403 for a key without permission, 429
//! when "you have run out of credit or you are sending too many concurrent
//! requests", 500 and 504 for server trouble. The pricing page says an
//! amount is subtracted "each time you make a successful request" to the
//! search, fetch, research, tasks and extract endpoints; the balance
//! endpoint is in none of its tables, so the call is treated as free. Not
//! stated in so many words.

use std::time::Duration;

use async_trait::async_trait;
use jiff::Timestamp;
use reqwest::StatusCode;
use secrecy::SecretString;
use serde::Deserialize;

use super::{cents, error_message, http_status_message, malformed};
use crate::net::{Auth, Http};
use crate::provider::{Category, Provider, ProviderError, Snapshot, Unit};

pub struct Linkup;

const ENDPOINT: &str = "https://api.linkup.so/v1/credits/balance";
const HOST: &str = "api.linkup.so";

#[derive(Deserialize)]
struct Body {
    balance: Option<f64>,
}

#[async_trait]
impl Provider for Linkup {
    fn id(&self) -> &'static str {
        "linkup"
    }

    fn display_name(&self) -> &'static str {
        "Linkup"
    }

    fn docs_url(&self) -> &'static str {
        "https://docs.linkup.so/pages/documentation/endpoints/account/balance"
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
        Some("Shows the prepaid balance in dollars. Linkup's API has no spend history.")
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
    Ok(Snapshot {
        taken_at: now,
        remaining: Some(balance),
        limit: None,
        period_end: None,
        spent: None,
        spend_per_hour: None,
        unit: None,
        extra: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &str = include_str!("../../tests/fixtures/linkup_200.json");
    const BAD_REQUEST: &str = include_str!("../../tests/fixtures/linkup_400.json");

    fn run(status: StatusCode, body: &str) -> Result<Snapshot, ProviderError> {
        parse(status, None, body.as_bytes(), Timestamp::UNIX_EPOCH)
    }

    #[test]
    fn documented_200_parses_in_cents() {
        let s = run(StatusCode::OK, OK).unwrap();
        assert_eq!(s.remaining, Some(12346));
        assert_eq!(s.limit, None);
        assert_eq!(s.spent, None);
        assert!(s.extra.is_empty());
    }

    #[test]
    fn nothing_but_the_number_survives() {
        let s = run(StatusCode::OK, OK).unwrap();
        let dump = format!("{s:?}");
        assert!(!dump.contains("balance"), "{dump}");
        assert!(!dump.contains("123.456"), "{dump}");
    }

    #[test]
    fn documented_error_envelope_maps() {
        assert_eq!(
            run(StatusCode::BAD_REQUEST, BAD_REQUEST),
            Err(ProviderError::Vendor("Validation failed".into()))
        );
        assert_eq!(run(StatusCode::UNAUTHORIZED, ""), Err(ProviderError::Unauthorized));
        assert_eq!(run(StatusCode::FORBIDDEN, ""), Err(ProviderError::Unauthorized));
        let out = BAD_REQUEST
            .replace("400", "429")
            .replace("VALIDATION_ERROR", "INSUFFICIENT_CREDIT")
            .replace("Validation failed", "You have run out of credit");
        assert_eq!(
            run(StatusCode::TOO_MANY_REQUESTS, &out),
            Err(ProviderError::RateLimited {
                retry_after: None,
                message: Some("You have run out of credit".into())
            })
        );
        assert_eq!(
            run(StatusCode::GATEWAY_TIMEOUT, ""),
            Err(ProviderError::Vendor("The vendor returned HTTP 504.".into()))
        );
        assert_eq!(run(StatusCode::OK, "{}"), Err(malformed()));
    }
}
