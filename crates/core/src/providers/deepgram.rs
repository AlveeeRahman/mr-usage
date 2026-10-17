//! Deepgram project balance.
//!
//! Verified 2026-10-17 against
//! <https://developers.deepgram.com/reference/manage/billing/list> (and its
//! `.md` rendering, which carries the example body):
//! `GET https://api.deepgram.com/v1/projects/{project_id}/balances` with
//! `Authorization: Token <key>`. 200 body `{balances: [{balance_id, amount
//! (double), units ("USD"), purchase_order_id}]}`. The only documented error
//! is 400 with either the legacy `{err_code, err_msg, request_id}` or the
//! modern `{category, message, details, request_id}` shape; the page shows
//! no example body, so `deepgram_400.json` is the smallest body that matches
//! the legacy schema. 401/403 and 429 are mapped by status alone.
//!
//! Cost: the page lists no price. Deepgram bills per audio processed; this
//! Manage API call reads billing records and transcribes nothing.
//!
//! The project id is not a secret but is an identifier, so it lives in the
//! settings file as the provider's config field and is interpolated into the
//! URL after a character check. `balance_id`, `purchase_order_id` and
//! `request_id` are never deserialized.

use std::time::Duration;

use async_trait::async_trait;
use jiff::Timestamp;
use reqwest::StatusCode;
use secrecy::SecretString;
use serde::Deserialize;

use super::{cents, error_message, http_status_message, malformed};
use crate::net::{Auth, Http};
use crate::provider::{Category, ConfigField, Provider, ProviderError, Snapshot, Unit};
use crate::redact;

pub struct Deepgram;

const ENDPOINT: &str = "https://api.deepgram.com/v1/projects/<project id>/balances";
const HOST: &str = "api.deepgram.com";

#[derive(Deserialize)]
struct Body {
    balances: Option<Vec<Balance>>,
}

/// Amount and units only: no balance id, no purchase order.
#[derive(Deserialize)]
struct Balance {
    amount: Option<f64>,
    units: Option<String>,
}

/// The legacy error shape `{err_code, err_msg, request_id}`.
fn legacy_error_message(body: &[u8]) -> Option<String> {
    #[derive(Deserialize)]
    struct Legacy {
        err_msg: Option<String>,
    }
    let legacy: Legacy = serde_json::from_slice(body).ok()?;
    let clean = redact::vendor_text(&legacy.err_msg?, super::MAX_VENDOR_TEXT);
    (!clean.is_empty()).then_some(clean)
}

fn vendor_message(body: &[u8]) -> Option<String> {
    error_message(body).or_else(|| legacy_error_message(body))
}

/// The real URL for the user's project id, or why it cannot be built.
pub(crate) fn endpoint_for(config: Option<&str>) -> Result<String, ProviderError> {
    let id = config
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| ProviderError::Unsupported("Enter the project id from the Deepgram console in this provider's settings.".into()))?;
    if !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err(ProviderError::Unsupported(
            "The Deepgram project id may only contain letters, digits and dashes.".into(),
        ));
    }
    Ok(format!("https://{HOST}/v1/projects/{id}/balances"))
}

#[async_trait]
impl Provider for Deepgram {
    fn id(&self) -> &'static str {
        "deepgram"
    }

    fn display_name(&self) -> &'static str {
        "Deepgram"
    }

    fn docs_url(&self) -> &'static str {
        "https://developers.deepgram.com/reference/manage/billing/list"
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
        Some("Adds up the project's outstanding US dollar balances. Needs the project id from the console.")
    }

    fn config_field(&self) -> Option<ConfigField> {
        Some(ConfigField {
            label: "Project id",
            hint: "The project id from the Deepgram console. Not a key.",
            placeholder: "4a8e…",
        })
    }

    async fn fetch(&self, key: &SecretString, config: Option<&str>, http: &Http) -> Result<Snapshot, ProviderError> {
        let url = endpoint_for(config)?;
        let response = http
            .get(
                &url,
                HOST,
                Auth::Header {
                    name: "authorization",
                    prefix: "Token ",
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
    let balances = parsed.balances.unwrap_or_default();
    let mut usd_cents: u64 = 0;
    let mut usd_count = 0usize;
    let mut other_count = 0usize;
    for b in &balances {
        if b.units.as_deref().is_some_and(|u| u.eq_ignore_ascii_case("usd")) {
            usd_cents = usd_cents.saturating_add(cents(b.amount).unwrap_or(0));
            usd_count += 1;
        } else {
            other_count += 1;
        }
    }
    let mut extra = Vec::new();
    if usd_count > 1 {
        extra.push(("Balances".to_owned(), format!("{usd_count} added together")));
    }
    if other_count > 0 {
        extra.push(("Not counted".to_owned(), format!("{other_count} not in US dollars")));
    }
    // No USD balance at all: either nothing is outstanding (zero) or every
    // balance is in a unit this card cannot show (none).
    let remaining = if usd_count > 0 || other_count == 0 { Some(usd_cents) } else { None };
    Ok(Snapshot {
        taken_at: now,
        remaining,
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

    const OK: &str = include_str!("../../tests/fixtures/deepgram_200.json");
    const BAD_REQUEST: &str = include_str!("../../tests/fixtures/deepgram_400.json");

    fn run(status: StatusCode, body: &str) -> Result<Snapshot, ProviderError> {
        parse(status, None, body.as_bytes(), Timestamp::UNIX_EPOCH)
    }

    #[test]
    fn documented_200_parses_in_cents() {
        let s = run(StatusCode::OK, OK).unwrap();
        assert_eq!(s.remaining, Some(125_075));
        assert_eq!(s.limit, None);
        assert_eq!(s.period_end, None);
        assert!(s.extra.is_empty());
    }

    #[test]
    fn identifiers_do_not_survive() {
        let s = run(StatusCode::OK, OK).unwrap();
        let dump = format!("{s:?}");
        for echoed in ["b7f3c9d2-4a1e-4f8b-9c3d-2e5a7f8b9c1d", "PO-2024-04567", "b7f3c9d2"] {
            assert!(!dump.contains(echoed), "{dump}");
        }
    }

    #[test]
    fn several_balances_add_up_and_other_units_are_set_aside() {
        let body = r#"{"balances":[
            {"balance_id":"a","amount":10.5,"units":"USD","purchase_order_id":"x"},
            {"balance_id":"b","amount":2.25,"units":"usd"},
            {"balance_id":"c","amount":500,"units":"minutes"}
        ]}"#;
        let s = run(StatusCode::OK, body).unwrap();
        assert_eq!(s.remaining, Some(1275));
        assert_eq!(
            s.extra,
            vec![
                ("Balances".to_owned(), "2 added together".to_owned()),
                ("Not counted".to_owned(), "1 not in US dollars".to_owned()),
            ]
        );
    }

    #[test]
    fn empty_list_is_zero_and_only_foreign_units_is_none() {
        assert_eq!(run(StatusCode::OK, r#"{"balances":[]}"#).unwrap().remaining, Some(0));
        assert_eq!(run(StatusCode::OK, "{}").unwrap().remaining, Some(0));
        let s = run(StatusCode::OK, r#"{"balances":[{"amount":5,"units":"EUR"}]}"#).unwrap();
        assert_eq!(s.remaining, None);
    }

    #[test]
    fn documented_400_surfaces_the_legacy_message() {
        assert_eq!(
            run(StatusCode::BAD_REQUEST, BAD_REQUEST),
            Err(ProviderError::Vendor("Invalid credentials.".into()))
        );
        let modern = r#"{"category":"INVALID_AUTH","message":"Bad token.","details":"d","request_id":"r"}"#;
        assert_eq!(
            run(StatusCode::BAD_REQUEST, modern),
            Err(ProviderError::Vendor("Bad token.".into()))
        );
    }

    #[test]
    fn auth_and_rate_limit_statuses() {
        assert_eq!(run(StatusCode::UNAUTHORIZED, ""), Err(ProviderError::Unauthorized));
        assert_eq!(run(StatusCode::FORBIDDEN, ""), Err(ProviderError::Unauthorized));
        let e = parse(
            StatusCode::TOO_MANY_REQUESTS,
            Some(Duration::from_secs(9)),
            b"",
            Timestamp::UNIX_EPOCH,
        )
        .unwrap_err();
        assert_eq!(
            e,
            ProviderError::RateLimited {
                retry_after: Some(Duration::from_secs(9)),
                message: None
            }
        );
        assert_eq!(run(StatusCode::OK, "<html>"), Err(malformed()));
    }

    #[test]
    fn project_id_builds_the_url_or_explains_why_not() {
        assert_eq!(
            endpoint_for(Some(" 123456-7890-1234-5678-901234 ")).unwrap(),
            "https://api.deepgram.com/v1/projects/123456-7890-1234-5678-901234/balances"
        );
        assert!(matches!(endpoint_for(None), Err(ProviderError::Unsupported(_))));
        assert!(matches!(endpoint_for(Some("   ")), Err(ProviderError::Unsupported(_))));
        for bad in ["../v1/keys", "abc/def", "abc?x=1", "abc def", "a_b"] {
            assert!(matches!(endpoint_for(Some(bad)), Err(ProviderError::Unsupported(_))), "{bad}");
        }
    }

    #[test]
    fn built_url_stays_on_the_allowlist() {
        let url = reqwest::Url::parse(&endpoint_for(Some("abc-123")).unwrap()).unwrap();
        assert!(crate::net::check_allowed(&url, HOST).is_ok());
    }
}
