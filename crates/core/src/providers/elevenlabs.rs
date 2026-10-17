//! ElevenLabs character quota.
//!
//! Verified 2026-10-17 against
//! <https://elevenlabs.io/docs/api-reference/user/subscription/get> (and its
//! `.md` rendering, which carries the example body):
//! `GET https://api.elevenlabs.io/v1/user/subscription` with the key in the
//! `xi-api-key` header. 200 body `{tier, character_count, character_limit,
//! next_character_count_reset_unix (integer or null, seconds), status,
//! current_overage: {amount: string, currency}, open_invoices, next_invoice,
//! ...}`. The only documented error is 422 `{detail: [{loc, msg, type}]}`;
//! the page shows no example body for it, so `elevenlabs_422.json` is the
//! smallest body that matches that schema. 401/403 and 429 have no documented
//! body and are mapped by status alone.
//!
//! Cost: the page lists no price. This reads the subscription record and
//! synthesises nothing, so it cannot consume characters.
//!
//! `open_invoices`, `next_invoice`, `pending_change` and the voice slot
//! counters are not deserialized. Nothing in the documented body is an
//! account identifier, but the invoice objects are left out all the same.

use std::time::Duration;

use async_trait::async_trait;
use jiff::Timestamp;
use reqwest::StatusCode;
use secrecy::SecretString;
use serde::Deserialize;

use super::{cents, dollars, error_message, http_status_message, malformed};
use crate::net::{Auth, Http};
use crate::provider::{Category, Provider, ProviderError, Snapshot, Unit};
use crate::redact;

pub struct ElevenLabs;

const ENDPOINT: &str = "https://api.elevenlabs.io/v1/user/subscription";
const HOST: &str = "api.elevenlabs.io";

/// Counts, the reset time, the plan tier and the overage. No invoices.
#[derive(Deserialize)]
struct Body {
    tier: Option<String>,
    character_count: Option<i64>,
    character_limit: Option<i64>,
    next_character_count_reset_unix: Option<i64>,
    status: Option<String>,
    current_overage: Option<Price>,
}

/// ElevenLabs' `Price`: the amount is a decimal string.
#[derive(Deserialize)]
struct Price {
    amount: Option<String>,
    currency: Option<String>,
}

/// The 422 body: `{detail: [{loc, msg, type}]}`. Returns the first `msg`.
fn validation_message(body: &[u8]) -> Option<String> {
    #[derive(Deserialize)]
    struct Item {
        msg: Option<String>,
    }
    #[derive(Deserialize)]
    struct Envelope {
        detail: Option<Vec<Item>>,
    }
    let envelope: Envelope = serde_json::from_slice(body).ok()?;
    let msg = envelope.detail?.into_iter().find_map(|i| i.msg)?;
    let clean = redact::vendor_text(&msg, super::MAX_VENDOR_TEXT);
    (!clean.is_empty()).then_some(clean)
}

fn vendor_message(body: &[u8]) -> Option<String> {
    error_message(body).or_else(|| validation_message(body))
}

fn non_negative(v: Option<i64>) -> Option<u64> {
    v.map(|n| n.max(0).unsigned_abs())
}

#[async_trait]
impl Provider for ElevenLabs {
    fn id(&self) -> &'static str {
        "elevenlabs"
    }

    fn display_name(&self) -> &'static str {
        "ElevenLabs"
    }

    fn docs_url(&self) -> &'static str {
        "https://elevenlabs.io/docs/api-reference/user/subscription/get"
    }

    fn endpoint(&self) -> &'static str {
        ENDPOINT
    }

    fn allowed_host(&self) -> &'static str {
        HOST
    }

    fn unit(&self) -> Unit {
        Unit::Characters
    }

    fn category(&self) -> Category {
        Category::Media
    }

    fn min_poll_interval(&self) -> Duration {
        Duration::from_secs(60)
    }

    fn note(&self) -> Option<&'static str> {
        Some("Counts characters in the current billing period. Overage spend shows as an extra when usage-based billing is on.")
    }

    async fn fetch(&self, key: &SecretString, _config: Option<&str>, http: &Http) -> Result<Snapshot, ProviderError> {
        let response = http
            .get(
                ENDPOINT,
                HOST,
                Auth::Header {
                    name: "xi-api-key",
                    prefix: "",
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
    let limit = non_negative(parsed.character_limit).ok_or_else(malformed)?;
    let used = non_negative(parsed.character_count).ok_or_else(malformed)?;
    let mut extra = Vec::new();
    if let Some(tier) = parsed.tier.as_deref().filter(|t| !t.is_empty()) {
        extra.push(("Tier".to_owned(), redact::vendor_text(tier, 40)));
    }
    if let Some(status) = parsed.status.as_deref().filter(|s| !s.is_empty() && *s != "active") {
        extra.push(("Status".to_owned(), status.replace('_', " ")));
    }
    if let Some(overage) = parsed.current_overage.as_ref().and_then(overage_text) {
        extra.push(("Overage".to_owned(), overage));
    }
    Ok(Snapshot {
        taken_at: now,
        remaining: Some(limit.saturating_sub(used)),
        limit: Some(limit),
        period_end: parsed
            .next_character_count_reset_unix
            .and_then(|secs| Timestamp::from_second(secs).ok()),
        spent: None,
        spend_per_hour: None,
        unit: None,
        extra,
    })
}

/// `$1.20` for a US dollar overage, `1.20 EUR` otherwise, `None` when zero.
fn overage_text(price: &Price) -> Option<String> {
    let amount: f64 = price.amount.as_deref()?.trim().parse().ok()?;
    if !amount.is_finite() || amount <= 0.0 {
        return None;
    }
    let currency = price.currency.as_deref().unwrap_or("usd");
    if currency.eq_ignore_ascii_case("usd") {
        cents(Some(amount)).map(dollars)
    } else {
        Some(format!("{amount:.2} {}", currency.to_ascii_uppercase()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &str = include_str!("../../tests/fixtures/elevenlabs_200.json");
    const INVALID: &str = include_str!("../../tests/fixtures/elevenlabs_422.json");

    fn run(status: StatusCode, body: &str) -> Result<Snapshot, ProviderError> {
        parse(status, None, body.as_bytes(), Timestamp::UNIX_EPOCH)
    }

    #[test]
    fn documented_200_parses() {
        let s = run(StatusCode::OK, OK).unwrap();
        assert_eq!(s.remaining, Some(9000));
        assert_eq!(s.limit, Some(10000));
        assert_eq!(s.period_end, Some(Timestamp::from_second(1738356858).unwrap()));
        assert_eq!(s.period_end.unwrap().to_string(), "2025-01-31T20:54:18Z");
        assert_eq!(s.extra, vec![("Tier".to_owned(), "starter".to_owned())]);
        assert_eq!(s.spent, None);
        assert_eq!(s.unit, None);
    }

    #[test]
    fn invoice_details_do_not_survive() {
        let s = run(StatusCode::OK, OK).unwrap();
        let dump = format!("{s:?}");
        for echoed in ["processing", "succeeded", "monthly_period", "amount_due", "discount", "coupon"] {
            assert!(!dump.contains(echoed), "{dump}");
        }
    }

    #[test]
    fn null_reset_is_none_and_overage_shows() {
        let body = OK
            .replace(
                "\"next_character_count_reset_unix\": 1738356858",
                "\"next_character_count_reset_unix\": null",
            )
            .replace("\"amount\": \"0\"", "\"amount\": \"1.2\"")
            .replace("\"status\": \"active\"", "\"status\": \"past_due\"");
        let s = run(StatusCode::OK, &body).unwrap();
        assert_eq!(s.period_end, None);
        assert_eq!(
            s.extra,
            vec![
                ("Tier".to_owned(), "starter".to_owned()),
                ("Status".to_owned(), "past due".to_owned()),
                ("Overage".to_owned(), "$1.20".to_owned()),
            ]
        );
    }

    #[test]
    fn usage_past_the_limit_clamps_to_zero() {
        let body = OK.replace("\"character_count\": 1000", "\"character_count\": 12000");
        let s = run(StatusCode::OK, &body).unwrap();
        assert_eq!(s.remaining, Some(0));
    }

    #[test]
    fn documented_422_surfaces_the_validation_message() {
        assert_eq!(
            run(StatusCode::UNPROCESSABLE_ENTITY, INVALID),
            Err(ProviderError::Vendor("Field required".into()))
        );
    }

    #[test]
    fn auth_and_rate_limit_statuses() {
        assert_eq!(run(StatusCode::UNAUTHORIZED, ""), Err(ProviderError::Unauthorized));
        assert_eq!(run(StatusCode::FORBIDDEN, ""), Err(ProviderError::Unauthorized));
        let e = parse(
            StatusCode::TOO_MANY_REQUESTS,
            Some(Duration::from_secs(3)),
            b"",
            Timestamp::UNIX_EPOCH,
        )
        .unwrap_err();
        assert_eq!(
            e,
            ProviderError::RateLimited {
                retry_after: Some(Duration::from_secs(3)),
                message: None
            }
        );
    }

    #[test]
    fn malformed_bodies_are_rejected() {
        assert_eq!(run(StatusCode::OK, "<html>"), Err(malformed()));
        assert_eq!(run(StatusCode::OK, r#"{"tier":"starter"}"#), Err(malformed()));
        assert_eq!(
            run(StatusCode::BAD_GATEWAY, "<html>"),
            Err(ProviderError::Vendor("The vendor returned HTTP 502.".into()))
        );
    }
}
