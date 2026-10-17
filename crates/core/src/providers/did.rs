//! D-ID credits.
//!
//! Verified 2026-10-17 against <https://docs.d-id.com/reference/getcredits>
//! (and its `.md` rendering, which carries the OpenAPI definition with the
//! examples): `GET https://api.d-id.com/credits`, security `basic` or
//! `bearer`; the page's sample sends `authorization: Basic <key>`, which is
//! how D-ID hands out API keys. 200 body `{credits: [{owner_id, created_at,
//! modified_at, remaining, total, expire_at, valid_from, product_id,
//! price_id, plan_group, product_billing_interval}], remaining, total}` with
//! `remaining` and `total` as doubles. The only documented error is 404
//! `{kind: "NotFoundError", description: "not credits found for user"}`
//! (schema `JsonError3`: `{kind, description, details, internal}`).
//! 401/403 and 429 are mapped by status alone.
//!
//! Cost: the page lists no price. This reads the credit pools and renders
//! nothing.
//!
//! `owner_id`, `product_id`, `price_id` and `plan_group` are identifiers and
//! are never deserialized. Only `remaining`, `total` and `expire_at` of each
//! pool are read, to find when the first pool that still has credits runs out.

use std::time::Duration;

use async_trait::async_trait;
use jiff::Timestamp;
use reqwest::StatusCode;
use secrecy::SecretString;
use serde::Deserialize;

use super::{count, error_message, http_status_message, malformed};
use crate::net::{Auth, Http};
use crate::provider::{Category, Provider, ProviderError, Snapshot, Unit, parse_timestamp};
use crate::redact;

pub struct DId;

const ENDPOINT: &str = "https://api.d-id.com/credits";
const HOST: &str = "api.d-id.com";

#[derive(Deserialize)]
struct Body {
    credits: Option<Vec<Pool>>,
    remaining: Option<f64>,
    total: Option<f64>,
}

/// Numbers and the expiry only. No owner, product or price ids.
#[derive(Deserialize)]
struct Pool {
    remaining: Option<f64>,
    expire_at: Option<String>,
}

/// The `JsonError3` shape: `{kind, description}`.
fn described_error(body: &[u8]) -> Option<String> {
    #[derive(Deserialize)]
    struct Envelope {
        description: Option<String>,
    }
    let envelope: Envelope = serde_json::from_slice(body).ok()?;
    let clean = redact::vendor_text(&envelope.description?, super::MAX_VENDOR_TEXT);
    (!clean.is_empty()).then_some(clean)
}

fn vendor_message(body: &[u8]) -> Option<String> {
    error_message(body).or_else(|| described_error(body))
}

#[async_trait]
impl Provider for DId {
    fn id(&self) -> &'static str {
        "did"
    }

    fn display_name(&self) -> &'static str {
        "D-ID"
    }

    fn docs_url(&self) -> &'static str {
        "https://docs.d-id.com/reference/getcredits"
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
        Some("The renewal date is when the first credit pool that still has credits expires.")
    }

    async fn fetch(&self, key: &SecretString, _config: Option<&str>, http: &Http) -> Result<Snapshot, ProviderError> {
        let response = http
            .get(
                ENDPOINT,
                HOST,
                Auth::Header {
                    name: "authorization",
                    prefix: "Basic ",
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
    let remaining = count(parsed.remaining).ok_or_else(malformed)?;
    let pools = parsed.credits.unwrap_or_default();
    let period_end = pools
        .iter()
        .filter(|p| p.remaining.is_some_and(|r| r > 0.0))
        .filter_map(|p| p.expire_at.as_deref().and_then(parse_timestamp))
        .min();
    let mut extra = Vec::new();
    if pools.len() > 1 {
        extra.push(("Credit pools".to_owned(), pools.len().to_string()));
    }
    Ok(Snapshot {
        taken_at: now,
        remaining: Some(remaining),
        limit: count(parsed.total),
        period_end,
        spent: None,
        spend_per_hour: None,
        unit: None,
        extra,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &str = include_str!("../../tests/fixtures/did_200.json");
    const NOT_FOUND: &str = include_str!("../../tests/fixtures/did_404.json");

    fn run(status: StatusCode, body: &str) -> Result<Snapshot, ProviderError> {
        parse(status, None, body.as_bytes(), Timestamp::UNIX_EPOCH)
    }

    #[test]
    fn documented_200_parses() {
        let s = run(StatusCode::OK, OK).unwrap();
        assert_eq!(s.remaining, Some(10000));
        assert_eq!(s.limit, Some(10000));
        assert_eq!(s.period_end.unwrap().to_string(), "2022-11-01T08:04:43.587Z");
        assert!(s.extra.is_empty());
    }

    #[test]
    fn owner_id_does_not_survive() {
        let s = run(StatusCode::OK, OK).unwrap();
        let dump = format!("{s:?}");
        assert!(!dump.contains("52907745-7672-470e-a803-a2f8feb52944"), "{dump}");
        assert!(!dump.contains("52907745"), "{dump}");
    }

    #[test]
    fn earliest_expiry_among_pools_with_credits_left_wins() {
        let body = r#"{"credits":[
            {"owner_id":"o","remaining":0,"total":100,"expire_at":"2026-01-01T00:00:00Z"},
            {"owner_id":"o","remaining":40,"total":100,"expire_at":"2026-06-01T00:00:00Z"},
            {"owner_id":"o","remaining":10,"total":50,"expire_at":"2026-03-01T00:00:00Z"}
        ],"remaining":50,"total":250}"#;
        let s = run(StatusCode::OK, body).unwrap();
        assert_eq!((s.remaining, s.limit), (Some(50), Some(250)));
        assert_eq!(s.period_end.unwrap().to_string(), "2026-03-01T00:00:00Z");
        assert_eq!(s.extra, vec![("Credit pools".to_owned(), "3".to_owned())]);
    }

    #[test]
    fn documented_404_surfaces_the_description() {
        assert_eq!(
            run(StatusCode::NOT_FOUND, NOT_FOUND),
            Err(ProviderError::Vendor("not credits found for user".into()))
        );
    }

    #[test]
    fn auth_rate_limit_and_malformed() {
        assert_eq!(run(StatusCode::UNAUTHORIZED, ""), Err(ProviderError::Unauthorized));
        assert_eq!(run(StatusCode::FORBIDDEN, ""), Err(ProviderError::Unauthorized));
        let e = parse(
            StatusCode::TOO_MANY_REQUESTS,
            Some(Duration::from_secs(5)),
            b"",
            Timestamp::UNIX_EPOCH,
        )
        .unwrap_err();
        assert_eq!(
            e,
            ProviderError::RateLimited {
                retry_after: Some(Duration::from_secs(5)),
                message: None
            }
        );
        assert_eq!(run(StatusCode::OK, "<html>"), Err(malformed()));
        assert_eq!(run(StatusCode::OK, r#"{"credits":[]}"#), Err(malformed()));
        assert_eq!(
            run(StatusCode::BAD_GATEWAY, ""),
            Err(ProviderError::Vendor("The vendor returned HTTP 502.".into()))
        );
    }
}
