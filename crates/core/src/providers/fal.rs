//! fal.ai account credit balance.
//!
//! Verified 2026-10-17 against <https://fal.ai/docs/platform-apis/v1/account/billing>:
//! `GET https://api.fal.ai/v1/account/billing?expand=credits`, header
//! `Authorization: Key <api key>` (fal's own scheme, not Bearer). Without
//! `expand=credits` the body has no `credits` object. 200 body
//! `{username, credits: {current_balance: 24.5, currency: "USD"}}` with the
//! balance a JSON number in whole currency units. `username` is an account
//! identifier and is never deserialized; `currency` is read as an enum.
//!
//! Permissions: the page requires `billing:usage:read`, included in the
//! `BILLING`, `READONLY` and `FULL` key presets. A key without it gets 403
//! "Access denied", so 403 maps to `Unsupported` with a hint to make a key
//! on one of those presets; 401 "Authentication required" stays
//! `Unauthorized`.
//!
//! Cost: the page lists no price; the call is an account lookup, not a model
//! request, and consumes no credits.
//!
//! Errors: `{"error": {"type", "message"}}` with 400 `validation_error`,
//! 401 and 403 `authorization_error`, 429 `rate_limited` and 500
//! `server_error` documented. No rate-limit figure is published, so the floor
//! is the global minute.
//!
//! Fixtures `fal_200.json`, `fal_400.json`, `fal_401.json`, `fal_403.json`,
//! `fal_429.json` and `fal_500.json` are the page's examples verbatim.

use std::time::Duration;

use async_trait::async_trait;
use jiff::Timestamp;
use reqwest::StatusCode;
use secrecy::SecretString;
use serde::Deserialize;

use super::{cents, error_message, http_status_message, malformed};
use crate::net::{Auth, Http};
use crate::provider::{Category, Provider, ProviderError, Snapshot, Unit};

pub struct Fal;

const ENDPOINT: &str = "https://api.fal.ai/v1/account/billing?expand=credits";
const HOST: &str = "api.fal.ai";

/// `username` is deliberately absent.
#[derive(Deserialize)]
struct Body {
    credits: Option<Credits>,
}

#[derive(Deserialize)]
struct Credits {
    current_balance: Option<f64>,
    currency: Option<Currency>,
}

#[derive(Deserialize, Clone, Copy)]
enum Currency {
    #[serde(rename = "USD")]
    Usd,
    #[serde(other)]
    Other,
}

#[async_trait]
impl Provider for Fal {
    fn id(&self) -> &'static str {
        "fal"
    }

    fn display_name(&self) -> &'static str {
        "fal.ai"
    }

    fn docs_url(&self) -> &'static str {
        "https://fal.ai/docs/platform-apis/v1/account/billing"
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
        Some("Needs a key on the READONLY or BILLING preset (permission billing:usage:read).")
    }

    async fn fetch(&self, key: &SecretString, _config: Option<&str>, http: &Http) -> Result<Snapshot, ProviderError> {
        let response = http
            .get(
                ENDPOINT,
                HOST,
                Auth::Header {
                    name: "authorization",
                    prefix: "Key ",
                    value: key,
                },
            )
            .await?;
        parse(response.status, response.retry_after, &response.body, Timestamp::now())
    }
}

pub(crate) fn parse(status: StatusCode, retry_after: Option<Duration>, body: &[u8], now: Timestamp) -> Result<Snapshot, ProviderError> {
    match status {
        StatusCode::UNAUTHORIZED => return Err(ProviderError::Unauthorized),
        // The key is real but lacks billing:usage:read.
        StatusCode::FORBIDDEN => {
            return Err(ProviderError::Unsupported(
                "This key cannot read billing. Create a key with the READONLY or BILLING preset in the fal dashboard.".into(),
            ));
        }
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
    let credits = parsed.credits.ok_or_else(malformed)?;
    let unit = match credits.currency {
        Some(Currency::Usd) | None => Unit::Usd,
        Some(Currency::Other) => return Err(malformed()),
    };
    let remaining = cents(credits.current_balance.map(|b| b.max(0.0))).ok_or_else(malformed)?;
    let mut snapshot = Snapshot::at(now);
    snapshot.unit = Some(unit);
    snapshot.remaining = Some(remaining);
    if credits.current_balance.is_some_and(|b| b < 0.0) {
        snapshot
            .extra
            .push(("Balance".to_owned(), "below zero; top up to keep running".to_owned()));
    }
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &str = include_str!("../../tests/fixtures/fal_200.json");
    const BAD_REQUEST: &str = include_str!("../../tests/fixtures/fal_400.json");
    const NO_AUTH: &str = include_str!("../../tests/fixtures/fal_401.json");
    const DENIED: &str = include_str!("../../tests/fixtures/fal_403.json");
    const LIMITED: &str = include_str!("../../tests/fixtures/fal_429.json");
    const SERVER: &str = include_str!("../../tests/fixtures/fal_500.json");

    fn run(status: StatusCode, body: &str) -> Result<Snapshot, ProviderError> {
        parse(status, None, body.as_bytes(), Timestamp::UNIX_EPOCH)
    }

    #[test]
    fn documented_200_parses_in_cents() {
        let s = run(StatusCode::OK, OK).unwrap();
        assert_eq!(s.remaining, Some(2450));
        assert_eq!(s.limit, None);
        assert_eq!(s.unit, Some(Unit::Usd));
        assert!(s.extra.is_empty());
    }

    #[test]
    fn identifiers_do_not_survive() {
        let s = run(StatusCode::OK, OK).unwrap();
        let dump = format!("{s:?}");
        for echoed in ["my-team", "username", "USD", "current_balance"] {
            assert!(!dump.contains(echoed), "{dump}");
        }
    }

    #[test]
    fn documented_errors_map() {
        assert_eq!(run(StatusCode::UNAUTHORIZED, NO_AUTH), Err(ProviderError::Unauthorized));
        assert!(matches!(run(StatusCode::FORBIDDEN, DENIED), Err(ProviderError::Unsupported(m)) if m.contains("READONLY")));
        assert_eq!(
            run(StatusCode::BAD_REQUEST, BAD_REQUEST),
            Err(ProviderError::Vendor("Invalid request parameters".into()))
        );
        assert_eq!(
            run(StatusCode::INTERNAL_SERVER_ERROR, SERVER),
            Err(ProviderError::Vendor("An unexpected error occurred".into()))
        );
        assert_eq!(
            parse(
                StatusCode::TOO_MANY_REQUESTS,
                Some(Duration::from_secs(30)),
                LIMITED.as_bytes(),
                Timestamp::UNIX_EPOCH
            ),
            Err(ProviderError::RateLimited {
                retry_after: Some(Duration::from_secs(30)),
                message: Some("Rate limit exceeded".into())
            })
        );
        assert_eq!(
            run(StatusCode::BAD_GATEWAY, "<html>"),
            Err(ProviderError::Vendor("The vendor returned HTTP 502.".into()))
        );
    }

    #[test]
    fn missing_credits_or_foreign_currency_is_malformed() {
        assert_eq!(run(StatusCode::OK, r#"{"username":"my-team"}"#), Err(malformed()));
        assert_eq!(run(StatusCode::OK, &OK.replace("\"USD\"", "\"EUR\"")), Err(malformed()));
        assert_eq!(run(StatusCode::OK, "not json"), Err(malformed()));
    }

    #[test]
    fn negative_balance_shows_zero_with_a_caveat() {
        let s = run(StatusCode::OK, &OK.replace("24.5", "-1.25")).unwrap();
        assert_eq!(s.remaining, Some(0));
        assert_eq!(s.extra.len(), 1);
    }
}
