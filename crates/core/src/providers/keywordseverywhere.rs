//! Keywords Everywhere credit balance.
//!
//! Verified 2026-10-17 against
//! <https://api.keywordseverywhere.com/docs/miscellaneous/get_credits.md>
//! ("Get Credit Balance"): `GET {{v1.server}}/account/credits` with
//! `Authorization: Bearer <key>` and `Accept: application/json`. The page
//! writes the host as the template `{{v1.server}}`; the app uses
//! `https://api.keywordseverywhere.com/v1`, the host the documentation itself
//! is served from. TODO(verify): confirm with a live key.
//!
//! Documented statuses: 200 "A successful request" and 401 "API key is
//! missing or is invalid" (no body shown). The 200 body is "an array
//! containing your account's credit balance as the only element", the sample
//! being `[95597755]`. The page states no price for the call. Nothing in the
//! body identifies the account.

use std::time::Duration;

use async_trait::async_trait;
use jiff::Timestamp;
use reqwest::StatusCode;
use secrecy::SecretString;

use super::{count, error_message, http_status_message, malformed};
use crate::net::{Auth, Http};
use crate::provider::{Category, Provider, ProviderError, Snapshot, Unit};

pub struct KeywordsEverywhere;

const ENDPOINT: &str = "https://api.keywordseverywhere.com/v1/account/credits";
const HOST: &str = "api.keywordseverywhere.com";

#[async_trait]
impl Provider for KeywordsEverywhere {
    fn id(&self) -> &'static str {
        "keywordseverywhere"
    }

    fn display_name(&self) -> &'static str {
        "Keywords Everywhere"
    }

    fn docs_url(&self) -> &'static str {
        "https://api.keywordseverywhere.com/docs/miscellaneous/get_credits.md"
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
        Some("Credits are prepaid and do not reset, so there is no renewal date.")
    }

    async fn fetch(&self, key: &SecretString, _config: Option<&str>, http: &Http) -> Result<Snapshot, ProviderError> {
        let response = http
            .get_with_headers(ENDPOINT, HOST, Auth::Bearer(key), &[("accept", "application/json")])
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
                message: error_message(body),
            });
        }
        s if !s.is_success() => {
            return Err(ProviderError::Vendor(error_message(body).unwrap_or_else(|| http_status_message(s))));
        }
        _ => {}
    }
    let parsed: Vec<f64> = serde_json::from_slice(body).map_err(|_| malformed())?;
    let remaining = count(parsed.first().copied()).ok_or_else(malformed)?;
    Ok(Snapshot {
        taken_at: now,
        remaining: Some(remaining),
        limit: None,
        period_end: None,
        spent: None,
        spend_per_hour: None,
        unit: None,
        extra: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &str = include_str!("../../tests/fixtures/keywordseverywhere_200.json");

    fn run(status: StatusCode, body: &str) -> Result<Snapshot, ProviderError> {
        parse(status, None, body.as_bytes(), Timestamp::UNIX_EPOCH)
    }

    #[test]
    fn documented_200_parses() {
        let s = run(StatusCode::OK, OK).unwrap();
        assert_eq!(s.remaining, Some(95_597_755));
        assert_eq!(s.limit, None);
        assert_eq!(s.period_end, None);
        assert!(s.extra.is_empty());
    }

    #[test]
    fn only_the_number_survives() {
        let s = run(StatusCode::OK, OK).unwrap();
        assert_eq!(format!("{s:?}").matches("95597755").count(), 1);
    }

    #[test]
    fn documented_401_is_unauthorized() {
        assert_eq!(run(StatusCode::UNAUTHORIZED, ""), Err(ProviderError::Unauthorized));
        assert_eq!(run(StatusCode::FORBIDDEN, ""), Err(ProviderError::Unauthorized));
    }

    #[test]
    fn other_statuses_map() {
        assert_eq!(
            parse(
                StatusCode::TOO_MANY_REQUESTS,
                Some(Duration::from_secs(9)),
                b"",
                Timestamp::UNIX_EPOCH
            ),
            Err(ProviderError::RateLimited {
                retry_after: Some(Duration::from_secs(9)),
                message: None
            })
        );
        assert_eq!(
            run(StatusCode::INTERNAL_SERVER_ERROR, r#"{"message":"oops"}"#),
            Err(ProviderError::Vendor("oops".into()))
        );
    }

    #[test]
    fn unexpected_bodies_are_malformed() {
        assert_eq!(run(StatusCode::OK, "[]"), Err(malformed()));
        assert_eq!(run(StatusCode::OK, "{}"), Err(malformed()));
        assert_eq!(run(StatusCode::OK, "[-5]"), Err(malformed()));
    }
}
