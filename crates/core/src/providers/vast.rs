//! Vast.ai account balance.
//!
//! Verified 2026-10-17 against
//! <https://docs.vast.ai/api-reference/accounts/show-user> ("show user"):
//! `GET https://console.vast.ai/api/v0/users/current`, `Authorization: Bearer
//! <key>` ("API key must be provided in the Authorization header"). 200 body
//! `{id, key_id, email, balance, ssh_key, sid}` where `balance` is a float in
//! US dollars ("The current balance of the user"). 401 and 500 bodies are
//! `{"success": false, "error": "<string>", "msg": "<string>"}`. The page shows
//! no 429, so a 429 is mapped from the status alone.
//!
//! Cost: the page states no price. The call reads the account record and is
//! not a rental or a search; nothing on the page says it is billed.
//!
//! `id`, `key_id`, `email`, `ssh_key` and `sid` are identifiers and are never
//! deserialized; `Body` has only `balance`. The documented examples are
//! generated from the schema, so every string is `"<string>"` and every number
//! is `123`; the fixtures keep them exactly as published.

use std::time::Duration;

use async_trait::async_trait;
use jiff::Timestamp;
use reqwest::StatusCode;
use secrecy::SecretString;
use serde::Deserialize;

use super::{MAX_VENDOR_TEXT, cents, dollars, http_status_message, malformed};
use crate::net::{Auth, Http};
use crate::provider::{Category, Provider, ProviderError, Snapshot, Unit};
use crate::redact;

pub struct Vast;

const ENDPOINT: &str = "https://console.vast.ai/api/v0/users/current";
const HOST: &str = "console.vast.ai";

/// Only the balance. See the module doc for what is left out.
#[derive(Deserialize)]
struct Body {
    balance: Option<f64>,
}

/// Vast's error envelope: `error` is a short code, `msg` the sentence.
#[derive(Deserialize)]
struct ErrorBody {
    error: Option<String>,
    msg: Option<String>,
}

/// `msg` when present, else `error`, scrubbed. `None` for a non-JSON body.
fn vast_error_message(body: &[u8]) -> Option<String> {
    let parsed: ErrorBody = serde_json::from_slice(body).ok()?;
    let text = match (parsed.msg, parsed.error) {
        (Some(m), _) if !m.trim().is_empty() => m,
        (_, Some(e)) => e,
        _ => return None,
    };
    let clean = redact::vendor_text(&text, MAX_VENDOR_TEXT);
    (!clean.is_empty()).then_some(clean)
}

#[async_trait]
impl Provider for Vast {
    fn id(&self) -> &'static str {
        "vast"
    }

    fn display_name(&self) -> &'static str {
        "Vast.ai"
    }

    fn docs_url(&self) -> &'static str {
        "https://docs.vast.ai/api-reference/accounts/show-user"
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
        Category::Compute
    }

    fn min_poll_interval(&self) -> Duration {
        Duration::from_secs(60)
    }

    fn note(&self) -> Option<&'static str> {
        Some("Shows the account's prepaid balance. Vast.ai does not report a limit or a renewal date.")
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
                message: vast_error_message(body),
            });
        }
        s if !s.is_success() => {
            return Err(ProviderError::Vendor(
                vast_error_message(body).unwrap_or_else(|| http_status_message(s)),
            ));
        }
        _ => {}
    }
    let parsed: Body = serde_json::from_slice(body).map_err(|_| malformed())?;
    let balance = parsed.balance.filter(|b| b.is_finite()).ok_or_else(malformed)?;
    let mut extra = Vec::new();
    // A negative balance is money owed: the card shows zero left and says so.
    if balance < 0.0
        && let Some(owed) = cents(Some(-balance))
    {
        extra.push(("Owed".to_owned(), dollars(owed)));
    }
    Ok(Snapshot {
        taken_at: now,
        remaining: Some(cents(Some(balance)).unwrap_or(0)),
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

    const OK: &str = include_str!("../../tests/fixtures/vast_200.json");
    const BAD_KEY: &str = include_str!("../../tests/fixtures/vast_401.json");
    const SERVER: &str = include_str!("../../tests/fixtures/vast_500.json");

    fn run(status: StatusCode, body: &str) -> Result<Snapshot, ProviderError> {
        parse(status, None, body.as_bytes(), Timestamp::UNIX_EPOCH)
    }

    #[test]
    fn documented_200_parses_in_cents() {
        let s = run(StatusCode::OK, OK).unwrap();
        assert_eq!(s.remaining, Some(12300));
        assert_eq!(s.limit, None);
        assert_eq!(s.period_end, None);
        assert!(s.extra.is_empty());
    }

    #[test]
    fn fractional_balance_rounds_to_cents() {
        let body = OK.replace("\"balance\": 123", "\"balance\": 17.456");
        assert_eq!(run(StatusCode::OK, &body).unwrap().remaining, Some(1746));
    }

    #[test]
    fn negative_balance_shows_zero_and_owed() {
        let body = OK.replace("\"balance\": 123", "\"balance\": -4.5");
        let s = run(StatusCode::OK, &body).unwrap();
        assert_eq!(s.remaining, Some(0));
        assert_eq!(s.extra, vec![("Owed".to_owned(), "$4.50".to_owned())]);
    }

    #[test]
    fn identifiers_do_not_survive() {
        // The documented example is schema-generated; fill it with realistic
        // identifiers so the test proves something.
        let body = OK
            .replace("\"id\": 123", "\"id\": 48213")
            .replace("\"key_id\": 123", "\"key_id\": 9971")
            .replace("\"email\": \"<string>\"", "\"email\": \"demo@example.com\"")
            .replace(
                "\"ssh_key\": \"<string>\"",
                "\"ssh_key\": \"ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIDemoKey user@host\"",
            )
            .replace("\"sid\": \"<string>\"", "\"sid\": \"sid-7f3a9b2c\"");
        let s = run(StatusCode::OK, &body).unwrap();
        let dump = format!("{s:?}");
        for echoed in [
            "48213",
            "9971",
            "demo@example.com",
            "ssh-ed25519",
            "AAAAC3Nza",
            "sid-7f3a9b2c",
            "<string>",
        ] {
            assert!(!dump.contains(echoed), "{dump}");
        }
    }

    #[test]
    fn missing_balance_is_malformed() {
        assert_eq!(run(StatusCode::OK, r#"{"id": 1}"#), Err(malformed()));
        assert_eq!(run(StatusCode::OK, "<html>"), Err(malformed()));
    }

    #[test]
    fn documented_401_is_unauthorized() {
        assert_eq!(run(StatusCode::UNAUTHORIZED, BAD_KEY), Err(ProviderError::Unauthorized));
        assert_eq!(run(StatusCode::FORBIDDEN, BAD_KEY), Err(ProviderError::Unauthorized));
    }

    #[test]
    fn documented_500_keeps_vendor_text() {
        let body = SERVER.replace("\"msg\": \"<string>\"", "\"msg\": \"Internal error, try again.\"");
        assert_eq!(
            run(StatusCode::INTERNAL_SERVER_ERROR, &body),
            Err(ProviderError::Vendor("Internal error, try again.".into()))
        );
        // The schema-generated example falls back to the `error` code, then to the status.
        assert_eq!(
            run(StatusCode::INTERNAL_SERVER_ERROR, SERVER),
            Err(ProviderError::Vendor("<string>".into()))
        );
        assert_eq!(
            run(StatusCode::BAD_GATEWAY, "<html>"),
            Err(ProviderError::Vendor("The vendor returned HTTP 502.".into()))
        );
    }

    #[test]
    fn rate_limit_status_maps() {
        let e = parse(
            StatusCode::TOO_MANY_REQUESTS,
            Some(Duration::from_secs(30)),
            br#"{"success": false, "error": "rate_limited", "msg": "Too many requests"}"#,
            Timestamp::UNIX_EPOCH,
        )
        .unwrap_err();
        assert_eq!(
            e,
            ProviderError::RateLimited {
                retry_after: Some(Duration::from_secs(30)),
                message: Some("Too many requests".into())
            }
        );
    }
}
