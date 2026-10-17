//! Runway organization credit balance.
//!
//! Verified 2026-10-16 against <https://docs.dev.runwayml.com/api/>
//! ("Organization" → "Get organization information"): `GET
//! https://api.dev.runwayml.com/v1/organization`, `Authorization: Bearer
//! <key>`, plus the required header `X-Runway-Version: 2024-11-06` ("This
//! field must be set to the exact value"). 200 body `{tier:
//! {maxMonthlyCreditSpend, models: {...}}, creditBalance, usage: {models:
//! {...}}}`. The page's example is generated from the schema, so every
//! number is the placeholder 9007199254740991; the fixture keeps it. The
//! page documents 400 ("The request was rejected") and 429 ("You have
//! exceeded the rate limit for this endpoint") without example bodies.
//!
//! Runway credits have no plan ceiling to count down from, so `limit` is
//! `None` and the card shows the balance alone. The docs do not say whether
//! this call costs credits. TODO(verify): confirm with a live key that
//! `creditBalance` does not move.

use std::time::Duration;

use async_trait::async_trait;
use jiff::Timestamp;
use reqwest::StatusCode;
use secrecy::SecretString;
use serde::Deserialize;

use super::{count, error_message, http_status_message, malformed};
use crate::net::{Auth, Http};
use crate::provider::{Category, Provider, ProviderError, Snapshot, Unit};

pub struct Runway;

const ENDPOINT: &str = "https://api.dev.runwayml.com/v1/organization";
const HOST: &str = "api.dev.runwayml.com";
const VERSION_HEADER: (&str, &str) = ("X-Runway-Version", "2024-11-06");

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Body {
    credit_balance: f64,
    tier: Option<Tier>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Tier {
    max_monthly_credit_spend: Option<f64>,
}

#[async_trait]
impl Provider for Runway {
    fn id(&self) -> &'static str {
        "runway"
    }

    fn display_name(&self) -> &'static str {
        "Runway"
    }

    fn docs_url(&self) -> &'static str {
        "https://docs.dev.runwayml.com/api/#tag/Organization/paths/~1v1~1organization/get"
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
        Category::Media
    }

    fn min_poll_interval(&self) -> Duration {
        Duration::from_secs(60)
    }

    fn note(&self) -> Option<&'static str> {
        Some("Shows the organization's credit balance. Runway does not document whether this call costs credits.")
    }

    async fn fetch(&self, key: &SecretString, _config: Option<&str>, http: &Http) -> Result<Snapshot, ProviderError> {
        let response = http.get_with_headers(ENDPOINT, HOST, Auth::Bearer(key), &[VERSION_HEADER]).await?;
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
    let balance = count(Some(parsed.credit_balance)).ok_or_else(malformed)?;
    let mut extra = Vec::new();
    if let Some(cap) = parsed.tier.and_then(|t| count(t.max_monthly_credit_spend)) {
        extra.push(("Monthly spend cap".to_owned(), format!("{cap} credits")));
    }
    Ok(Snapshot {
        taken_at: now,
        remaining: Some(balance),
        limit: None,
        period_end: None,
        spent: None,
        spend_per_hour: None,
        unit: None,
        extra,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &str = include_str!("../../tests/fixtures/runway_200.json");

    fn run(status: StatusCode, body: &str) -> Result<Snapshot, ProviderError> {
        parse(status, None, body.as_bytes(), Timestamp::UNIX_EPOCH)
    }

    #[test]
    fn documented_placeholder_example_parses() {
        let s = run(StatusCode::OK, OK).unwrap();
        assert_eq!(s.remaining, Some(9007199254740991));
        assert_eq!(s.limit, None);
        assert_eq!(
            s.extra,
            vec![("Monthly spend cap".to_owned(), "9007199254740991 credits".to_owned())]
        );
    }

    #[test]
    fn a_small_balance_parses() {
        let s = run(
            StatusCode::OK,
            r#"{"creditBalance": 1250, "tier": {"maxMonthlyCreditSpend": 50000, "models": {}}, "usage": {"models": {}}}"#,
        )
        .unwrap();
        assert_eq!(s.remaining, Some(1250));
    }

    #[test]
    fn documented_statuses_map() {
        assert_eq!(run(StatusCode::UNAUTHORIZED, ""), Err(ProviderError::Unauthorized));
        assert_eq!(
            parse(
                StatusCode::TOO_MANY_REQUESTS,
                Some(Duration::from_secs(30)),
                b"",
                Timestamp::UNIX_EPOCH
            ),
            Err(ProviderError::RateLimited {
                retry_after: Some(Duration::from_secs(30)),
                message: None
            })
        );
        assert_eq!(
            run(StatusCode::BAD_REQUEST, r#"{"error":"The request was rejected."}"#),
            Err(ProviderError::Vendor("The request was rejected.".into()))
        );
    }

    #[test]
    fn version_header_is_the_documented_exact_value() {
        assert_eq!(VERSION_HEADER, ("X-Runway-Version", "2024-11-06"));
    }
}
