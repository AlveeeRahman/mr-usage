//! Luma Dream Machine credit balance.
//!
//! Verified 2026-10-17 against <https://docs.lumalabs.ai/reference/getcredits>
//! (and its `.md` rendering, which carries the OpenAPI definition):
//! `GET https://api.lumalabs.ai/dream-machine/v1/credits`, security
//! `BearerAuth` (`Authorization: Bearer <key>`). 200 schema `Credits:
//! {credit_balance: number (float), "Available credits balance in USD
//! cents"}`, required. Every other status is the `default` response with
//! schema `Error: {detail: string}` and the example
//! `{"detail": "Invalid API key is provided"}`. The page shows no example 200
//! body, so `luma_200.json` is the smallest body that matches the schema;
//! `luma_error.json` is the schema's own example.
//!
//! Cost: the page lists no price. It reads the balance and generates nothing.
//!
//! The page carries a banner pointing new integrations to the Luma API
//! Platform and the Luma Agents API; this endpoint is the one documented for
//! Dream Machine keys. Nothing in the documented body identifies the account.

use std::time::Duration;

use async_trait::async_trait;
use jiff::Timestamp;
use reqwest::StatusCode;
use secrecy::SecretString;
use serde::Deserialize;

use super::{error_message, http_status_message, malformed};
use crate::net::{Auth, Http};
use crate::provider::{Category, Provider, ProviderError, Snapshot, Unit};
use crate::redact;

pub struct Luma;

const ENDPOINT: &str = "https://api.lumalabs.ai/dream-machine/v1/credits";
const HOST: &str = "api.lumalabs.ai";

#[derive(Deserialize)]
struct Body {
    credit_balance: Option<f64>,
}

/// The documented `Error` shape: `{detail: string}`.
fn detail_message(body: &[u8]) -> Option<String> {
    #[derive(Deserialize)]
    struct Envelope {
        detail: Option<String>,
    }
    let envelope: Envelope = serde_json::from_slice(body).ok()?;
    let clean = redact::vendor_text(&envelope.detail?, super::MAX_VENDOR_TEXT);
    (!clean.is_empty()).then_some(clean)
}

fn vendor_message(body: &[u8]) -> Option<String> {
    detail_message(body).or_else(|| error_message(body))
}

#[async_trait]
impl Provider for Luma {
    fn id(&self) -> &'static str {
        "luma"
    }

    fn display_name(&self) -> &'static str {
        "Luma"
    }

    fn docs_url(&self) -> &'static str {
        "https://docs.lumalabs.ai/reference/getcredits"
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
        Category::Media
    }

    fn min_poll_interval(&self) -> Duration {
        Duration::from_secs(60)
    }

    fn note(&self) -> Option<&'static str> {
        Some("For Dream Machine API keys. Luma's newer API Platform keys are documented elsewhere.")
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
                message: vendor_message(body),
            });
        }
        s if !s.is_success() => {
            return Err(ProviderError::Vendor(
                vendor_message(body).unwrap_or_else(|| http_status_message(s)),
            ));
        }
        _ => {}
    }
    let parsed: Body = serde_json::from_slice(body).map_err(|_| malformed())?;
    // Already in cents: round to the nearest whole cent.
    let balance = parsed.credit_balance.filter(|c| c.is_finite()).ok_or_else(malformed)?;
    let mut snapshot = Snapshot::at(now);
    snapshot.remaining = Some(balance.max(0.0).round() as u64);
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &str = include_str!("../../tests/fixtures/luma_200.json");
    const ERROR: &str = include_str!("../../tests/fixtures/luma_error.json");

    fn run(status: StatusCode, body: &str) -> Result<Snapshot, ProviderError> {
        parse(status, None, body.as_bytes(), Timestamp::UNIX_EPOCH)
    }

    #[test]
    fn schema_shaped_200_parses_as_cents() {
        let s = run(StatusCode::OK, OK).unwrap();
        assert_eq!(s.remaining, Some(1235));
        assert_eq!(s.limit, None);
        assert_eq!(s.unit, None);
        assert!(s.extra.is_empty());
        assert_eq!(run(StatusCode::OK, r#"{"credit_balance": 0}"#).unwrap().remaining, Some(0));
        assert_eq!(run(StatusCode::OK, r#"{"credit_balance": -3.2}"#).unwrap().remaining, Some(0));
    }

    #[test]
    fn nothing_but_the_balance_survives() {
        let s = run(StatusCode::OK, OK).unwrap();
        assert_eq!(format!("{s:?}").matches("1235").count(), 1);
    }

    #[test]
    fn documented_error_body_maps_by_status() {
        assert_eq!(run(StatusCode::UNAUTHORIZED, ERROR), Err(ProviderError::Unauthorized));
        assert_eq!(run(StatusCode::FORBIDDEN, ERROR), Err(ProviderError::Unauthorized));
        assert_eq!(
            run(StatusCode::BAD_REQUEST, ERROR),
            Err(ProviderError::Vendor("Invalid API key is provided".into()))
        );
        let e = parse(
            StatusCode::TOO_MANY_REQUESTS,
            Some(Duration::from_secs(6)),
            ERROR.as_bytes(),
            Timestamp::UNIX_EPOCH,
        )
        .unwrap_err();
        assert_eq!(
            e,
            ProviderError::RateLimited {
                retry_after: Some(Duration::from_secs(6)),
                message: Some("Invalid API key is provided".into())
            }
        );
    }

    #[test]
    fn malformed_bodies_are_rejected() {
        assert_eq!(run(StatusCode::OK, "{}"), Err(malformed()));
        assert_eq!(run(StatusCode::OK, "<html>"), Err(malformed()));
        assert_eq!(
            run(StatusCode::SERVICE_UNAVAILABLE, "<html>"),
            Err(ProviderError::Vendor("The vendor returned HTTP 503.".into()))
        );
    }
}
