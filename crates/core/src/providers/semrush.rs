//! Semrush Standard API unit balance.
//!
//! Verified 2026-10-17 against
//! <https://developer.semrush.com/api/v3/get-started/api-units-balance/>:
//! `GET www.semrush.com/users/countapiunits.html?key=<key>`, described as a
//! "free" request. The documented response is CSV with a single value, the
//! example being `1,000` (remaining API units, with a thousands separator).
//! The page documents no error body for this endpoint; it only says that
//! other endpoints answer a shortage of units with `ERROR 132` or HTTP 403.
//!
//! The key travels in the query string because the page offers nothing else.
//! The page writes the URL with `http://`; the app only ever speaks HTTPS to
//! the same host. TODO(verify): confirm with a live key that the HTTPS host
//! answers.
//!
//! A non-numeric 200 body is an error line. One that mentions the key is
//! reported as a rejected key, anything else as a vendor error with the text
//! scrubbed. Nothing in the body is an identifier; the balance is the whole
//! response.

use std::time::Duration;

use async_trait::async_trait;
use jiff::Timestamp;
use reqwest::StatusCode;
use secrecy::SecretString;

use super::{MAX_VENDOR_TEXT, http_status_message, malformed};
use crate::net::{Auth, Http};
use crate::provider::{Category, Provider, ProviderError, Snapshot, Unit};
use crate::redact;

pub struct Semrush;

const ENDPOINT: &str = "https://www.semrush.com/users/countapiunits.html";
const HOST: &str = "www.semrush.com";

#[async_trait]
impl Provider for Semrush {
    fn id(&self) -> &'static str {
        "semrush"
    }

    fn display_name(&self) -> &'static str {
        "Semrush"
    }

    fn docs_url(&self) -> &'static str {
        "https://developer.semrush.com/api/v3/get-started/api-units-balance/"
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
        Category::Seo
    }

    fn min_poll_interval(&self) -> Duration {
        Duration::from_secs(60)
    }

    fn note(&self) -> Option<&'static str> {
        Some("Shows the Standard API unit balance. The Trends API has its own limits that this call does not cover.")
    }

    async fn fetch(&self, key: &SecretString, _config: Option<&str>, http: &Http) -> Result<Snapshot, ProviderError> {
        let response = http.get(ENDPOINT, HOST, Auth::Query { name: "key", value: key }).await?;
        parse(response.status, response.retry_after, &response.body, Timestamp::now())
    }
}

/// `1,000` or `1000` as a count. Anything else is not a balance.
fn units(text: &str) -> Option<u64> {
    let digits: String = text.chars().filter(|c| *c != ',').collect();
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

pub(crate) fn parse(status: StatusCode, retry_after: Option<Duration>, body: &[u8], now: Timestamp) -> Result<Snapshot, ProviderError> {
    let raw = String::from_utf8_lossy(body);
    let text = redact::vendor_text(&raw, MAX_VENDOR_TEXT);
    let message = (!text.is_empty()).then(|| text.clone());
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => return Err(ProviderError::Unauthorized),
        StatusCode::TOO_MANY_REQUESTS => return Err(ProviderError::RateLimited { retry_after, message }),
        s if !s.is_success() => return Err(ProviderError::Vendor(message.unwrap_or_else(|| http_status_message(s)))),
        _ => {}
    }
    match units(raw.trim()) {
        Some(remaining) => Ok(Snapshot {
            taken_at: now,
            remaining: Some(remaining),
            limit: None,
            period_end: None,
            spent: None,
            spend_per_hour: None,
            unit: None,
            extra: Vec::new(),
        }),
        None if text.is_empty() => Err(malformed()),
        None if raw.to_ascii_lowercase().contains("key") => Err(ProviderError::Unauthorized),
        None => Err(ProviderError::Vendor(text)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &str = include_str!("../../tests/fixtures/semrush_200.json");

    fn run(status: StatusCode, body: &str) -> Result<Snapshot, ProviderError> {
        parse(status, None, body.as_bytes(), Timestamp::UNIX_EPOCH)
    }

    #[test]
    fn documented_200_parses_with_thousands_separator() {
        let s = run(StatusCode::OK, OK).unwrap();
        assert_eq!(s.remaining, Some(1000));
        assert_eq!(s.limit, None);
        assert_eq!(s.period_end, None);
        assert!(s.extra.is_empty());
    }

    #[test]
    fn plain_integer_parses_too() {
        assert_eq!(run(StatusCode::OK, "2500000\n").unwrap().remaining, Some(2_500_000));
        assert_eq!(run(StatusCode::OK, "0").unwrap().remaining, Some(0));
    }

    /// Semrush reports errors as a text line. This shape is not documented
    /// for the balance endpoint; the test pins the rule, not the vendor text.
    #[test]
    fn error_line_mentioning_the_key_is_unauthorized() {
        assert_eq!(
            run(StatusCode::OK, "ERROR 120 :: WRONG KEY - ID PAIR"),
            Err(ProviderError::Unauthorized)
        );
    }

    #[test]
    fn other_error_lines_are_vendor_errors() {
        assert_eq!(
            run(StatusCode::OK, "ERROR 132 :: LIMITS EXCEEDED"),
            Err(ProviderError::Vendor("ERROR 132 :: LIMITS EXCEEDED".into()))
        );
        assert_eq!(run(StatusCode::OK, "   \n"), Err(malformed()));
        assert_eq!(run(StatusCode::OK, "1.5"), Err(ProviderError::Vendor("1.5".into())));
    }

    #[test]
    fn statuses_map() {
        assert_eq!(run(StatusCode::UNAUTHORIZED, ""), Err(ProviderError::Unauthorized));
        assert_eq!(run(StatusCode::FORBIDDEN, "nope"), Err(ProviderError::Unauthorized));
        assert_eq!(
            run(StatusCode::TOO_MANY_REQUESTS, "slow down"),
            Err(ProviderError::RateLimited {
                retry_after: None,
                message: Some("slow down".into())
            })
        );
        assert_eq!(
            run(StatusCode::BAD_GATEWAY, ""),
            Err(ProviderError::Vendor("The vendor returned HTTP 502.".into()))
        );
    }
}
