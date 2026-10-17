//! Stability AI credit balance.
//!
//! Verified 2026-10-17 against the Apidog mirror of the official v1
//! reference, <https://stabilityai.apidog.io/api-6070374> ("balance"), whose
//! Getting Started page (<https://stabilityai.apidog.io/doc-455573>) says to
//! send the key in the `Authorization` header and that the API answers 429
//! past 150 requests every 10 seconds. `GET https://api.stability.ai/v1/user/balance`,
//! `Authorization: Bearer <key>`. 200 example `{"credits": 0.6336833840314097}`;
//! the mirror lists 401 and 500 and documents the error body as
//! `{id, name, message}` (<https://stabilityai.apidog.io/schema-1577756>).
//! The official page <https://platform.stability.ai/docs/api-reference> is a
//! single-page app that did not render as text, so the mirror is what was
//! read; the module's `docs_url` still points at the official reference.
//!
//! Cost: neither page lists a price for this call. It reads the balance and
//! generates nothing.
//!
//! The error `id` is a request identifier and is never shown; only `message`
//! is read, through `error_message`. Credits are fractional, so the card
//! shows the whole part and the exact figure as an extra.

use std::time::Duration;

use async_trait::async_trait;
use jiff::Timestamp;
use reqwest::StatusCode;
use secrecy::SecretString;
use serde::Deserialize;

use super::{count, error_message, http_status_message, malformed};
use crate::net::{Auth, Http};
use crate::provider::{Category, Provider, ProviderError, Snapshot, Unit};

pub struct Stability;

const ENDPOINT: &str = "https://api.stability.ai/v1/user/balance";
const HOST: &str = "api.stability.ai";

#[derive(Deserialize)]
struct Body {
    credits: Option<f64>,
}

#[async_trait]
impl Provider for Stability {
    fn id(&self) -> &'static str {
        "stability"
    }

    fn display_name(&self) -> &'static str {
        "Stability AI"
    }

    fn docs_url(&self) -> &'static str {
        "https://platform.stability.ai/docs/api-reference"
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
        Some("Credits are fractional; the card rounds down and lists the exact balance.")
    }

    async fn fetch(&self, key: &SecretString, _config: Option<&str>, http: &Http) -> Result<Snapshot, ProviderError> {
        let response = http.get(ENDPOINT, HOST, Auth::Bearer(key)).await?;
        parse(response.status, response.retry_after, &response.body, Timestamp::now())
    }
}

/// The exact fractional balance for the card's extras, `None` when whole.
fn exact_fraction(credits: f64) -> Option<String> {
    if credits.fract() == 0.0 {
        return None;
    }
    let text = format!("{credits:.4}");
    Some(text.trim_end_matches('0').trim_end_matches('.').to_owned())
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
    let credits = parsed.credits.filter(|c| c.is_finite()).ok_or_else(malformed)?;
    let mut snapshot = Snapshot::at(now);
    snapshot.remaining = count(Some(credits));
    if let Some(exact) = exact_fraction(credits) {
        snapshot.extra.push(("Exact balance".to_owned(), exact));
    }
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &str = include_str!("../../tests/fixtures/stability_200.json");
    const BAD_KEY: &str = include_str!("../../tests/fixtures/stability_401.json");

    fn run(status: StatusCode, body: &str) -> Result<Snapshot, ProviderError> {
        parse(status, None, body.as_bytes(), Timestamp::UNIX_EPOCH)
    }

    #[test]
    fn documented_200_rounds_down_and_keeps_the_exact_figure() {
        let s = run(StatusCode::OK, OK).unwrap();
        assert_eq!(s.remaining, Some(0));
        assert_eq!(s.limit, None);
        assert_eq!(s.extra, vec![("Exact balance".to_owned(), "0.6337".to_owned())]);
    }

    #[test]
    fn whole_balance_has_no_extra() {
        let s = run(StatusCode::OK, r#"{"credits": 250}"#).unwrap();
        assert_eq!(s.remaining, Some(250));
        assert!(s.extra.is_empty());
        let s = run(StatusCode::OK, r#"{"credits": 250.0}"#).unwrap();
        assert!(s.extra.is_empty());
    }

    #[test]
    fn documented_401_is_unauthorized_and_its_id_is_not_shown() {
        assert_eq!(run(StatusCode::UNAUTHORIZED, BAD_KEY), Err(ProviderError::Unauthorized));
        // The same body on a 500 shows the message and not the request id.
        // The redactor treats the word after "Authorization" as a credential.
        let e = run(StatusCode::INTERNAL_SERVER_ERROR, BAD_KEY).unwrap_err();
        assert_eq!(
            e,
            ProviderError::Vendor("Header parameter [redacted] [redacted] required, but not found".into())
        );
        assert!(!e.to_string().contains("296a972f"));
    }

    #[test]
    fn rate_limit_and_malformed() {
        let e = parse(
            StatusCode::TOO_MANY_REQUESTS,
            Some(Duration::from_secs(10)),
            b"",
            Timestamp::UNIX_EPOCH,
        )
        .unwrap_err();
        assert_eq!(
            e,
            ProviderError::RateLimited {
                retry_after: Some(Duration::from_secs(10)),
                message: None
            }
        );
        assert_eq!(run(StatusCode::OK, "{}"), Err(malformed()));
        assert_eq!(run(StatusCode::OK, "<html>"), Err(malformed()));
        assert_eq!(
            run(StatusCode::BAD_GATEWAY, ""),
            Err(ProviderError::Vendor("The vendor returned HTTP 502.".into()))
        );
    }
}
