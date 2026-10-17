//! Black Forest Labs credits.
//!
//! Verified 2026-10-17 against
//! <https://docs.bfl.ml/api-reference/get-the-users-credits>:
//! `GET https://api.bfl.ai/v1/credits` with the key in the `x-key` header
//! (the page's only authorization). 200 body `{"credits": 123}` where
//! `credits` is a required number, "User's current credit balance". The page
//! documents no error bodies; 401/403 and 429 are mapped by status and other
//! failures show `error_message` when the body has one.
//!
//! Cost: the page lists no price. It reads the balance and generates nothing.
//!
//! Nothing in the documented body identifies the account.

use std::time::Duration;

use async_trait::async_trait;
use jiff::Timestamp;
use reqwest::StatusCode;
use secrecy::SecretString;
use serde::Deserialize;

use super::{count, error_message, http_status_message, malformed};
use crate::net::{Auth, Http};
use crate::provider::{Category, Provider, ProviderError, Snapshot, Unit};

pub struct BlackForestLabs;

const ENDPOINT: &str = "https://api.bfl.ai/v1/credits";
const HOST: &str = "api.bfl.ai";

#[derive(Deserialize)]
struct Body {
    credits: Option<f64>,
}

#[async_trait]
impl Provider for BlackForestLabs {
    fn id(&self) -> &'static str {
        "bfl"
    }

    fn display_name(&self) -> &'static str {
        "Black Forest Labs"
    }

    fn docs_url(&self) -> &'static str {
        "https://docs.bfl.ml/api-reference/get-the-users-credits"
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

    async fn fetch(&self, key: &SecretString, _config: Option<&str>, http: &Http) -> Result<Snapshot, ProviderError> {
        let response = http
            .get(
                ENDPOINT,
                HOST,
                Auth::Header {
                    name: "x-key",
                    prefix: "",
                    value: key,
                },
            )
            .await?;
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

    const OK: &str = include_str!("../../tests/fixtures/bfl_200.json");

    fn run(status: StatusCode, body: &str) -> Result<Snapshot, ProviderError> {
        parse(status, None, body.as_bytes(), Timestamp::UNIX_EPOCH)
    }

    #[test]
    fn documented_200_parses() {
        let s = run(StatusCode::OK, OK).unwrap();
        assert_eq!(s.remaining, Some(123));
        assert_eq!(s.limit, None);
        assert_eq!(s.period_end, None);
        assert!(s.extra.is_empty());
    }

    #[test]
    fn nothing_but_numbers_survives() {
        let s = run(StatusCode::OK, OK).unwrap();
        let dump = format!("{s:?}");
        assert!(!dump.contains("api-key"), "{dump}");
        assert_eq!(s.extra, Vec::<(String, String)>::new());
    }

    #[test]
    fn fractional_credits_round_down_with_the_exact_figure_as_an_extra() {
        let s = run(StatusCode::OK, r#"{"credits": 99.75}"#).unwrap();
        assert_eq!(s.remaining, Some(99));
        assert_eq!(s.extra, vec![("Exact balance".to_owned(), "99.75".to_owned())]);
    }

    #[test]
    fn statuses_map() {
        assert_eq!(run(StatusCode::UNAUTHORIZED, ""), Err(ProviderError::Unauthorized));
        assert_eq!(run(StatusCode::FORBIDDEN, ""), Err(ProviderError::Unauthorized));
        let e = parse(
            StatusCode::TOO_MANY_REQUESTS,
            Some(Duration::from_secs(4)),
            br#"{"detail":"Too Many Requests"}"#,
            Timestamp::UNIX_EPOCH,
        )
        .unwrap_err();
        assert_eq!(
            e,
            ProviderError::RateLimited {
                retry_after: Some(Duration::from_secs(4)),
                message: None
            }
        );
        assert_eq!(
            run(StatusCode::INTERNAL_SERVER_ERROR, r#"{"message":"boom"}"#),
            Err(ProviderError::Vendor("boom".into()))
        );
        assert_eq!(run(StatusCode::OK, "{}"), Err(malformed()));
        assert_eq!(run(StatusCode::OK, "<html>"), Err(malformed()));
    }
}
