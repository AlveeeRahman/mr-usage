//! Recraft credits.
//!
//! Verified 2026-10-17 against
//! <https://www.recraft.ai/docs/api-reference/endpoints> ("Get user
//! information"): `GET https://external.api.recraft.ai/v1/users/me`,
//! `Authorization: Bearer RECRAFT_API_TOKEN`. 200 body `{credits: 1000,
//! email, id, name}`. The page documents no error bodies; 401/403 and 429 are
//! mapped by status and other failures show `error_message` when the body
//! has one.
//!
//! Cost: the page's endpoint table lists a price for every call and shows
//! "—" for `GET /users/me`, so the docs price it at nothing.
//!
//! `email`, `id` and `name` are never deserialized; only `credits` is read.

use std::time::Duration;

use async_trait::async_trait;
use jiff::Timestamp;
use reqwest::StatusCode;
use secrecy::SecretString;
use serde::Deserialize;

use super::{count, error_message, http_status_message, malformed};
use crate::net::{Auth, Http};
use crate::provider::{Category, Provider, ProviderError, Snapshot, Unit};

pub struct Recraft;

const ENDPOINT: &str = "https://external.api.recraft.ai/v1/users/me";
const HOST: &str = "external.api.recraft.ai";

/// The credit count alone. See the module doc for what is left out.
#[derive(Deserialize)]
struct Body {
    credits: Option<f64>,
}

#[async_trait]
impl Provider for Recraft {
    fn id(&self) -> &'static str {
        "recraft"
    }

    fn display_name(&self) -> &'static str {
        "Recraft"
    }

    fn docs_url(&self) -> &'static str {
        "https://www.recraft.ai/docs/api-reference/endpoints"
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
        Some("Free to call: Recraft's endpoint table lists no price for it.")
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
                message: error_message(body),
            });
        }
        s if !s.is_success() => {
            return Err(ProviderError::Vendor(error_message(body).unwrap_or_else(|| http_status_message(s))));
        }
        _ => {}
    }
    let parsed: Body = serde_json::from_slice(body).map_err(|_| malformed())?;
    let mut snapshot = Snapshot::at(now);
    snapshot.remaining = Some(count(parsed.credits).ok_or_else(malformed)?);
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &str = include_str!("../../tests/fixtures/recraft_200.json");

    fn run(status: StatusCode, body: &str) -> Result<Snapshot, ProviderError> {
        parse(status, None, body.as_bytes(), Timestamp::UNIX_EPOCH)
    }

    #[test]
    fn documented_200_parses() {
        let s = run(StatusCode::OK, OK).unwrap();
        assert_eq!(s.remaining, Some(1000));
        assert_eq!(s.limit, None);
        assert_eq!(s.period_end, None);
        assert!(s.extra.is_empty());
    }

    #[test]
    fn email_id_and_name_do_not_survive() {
        let s = run(StatusCode::OK, OK).unwrap();
        let dump = format!("{s:?}");
        for echoed in [
            "test@example.com",
            "c18a1988-45e7-4c00-82c4-4ad7d3dbce3a",
            "c18a1988",
            "Recraft Test",
        ] {
            assert!(!dump.contains(echoed), "{dump}");
        }
    }

    #[test]
    fn statuses_map() {
        assert_eq!(run(StatusCode::UNAUTHORIZED, ""), Err(ProviderError::Unauthorized));
        assert_eq!(run(StatusCode::FORBIDDEN, ""), Err(ProviderError::Unauthorized));
        let e = parse(
            StatusCode::TOO_MANY_REQUESTS,
            Some(Duration::from_secs(8)),
            br#"{"message":"rate limit exceeded"}"#,
            Timestamp::UNIX_EPOCH,
        )
        .unwrap_err();
        assert_eq!(
            e,
            ProviderError::RateLimited {
                retry_after: Some(Duration::from_secs(8)),
                message: Some("rate limit exceeded".into())
            }
        );
        assert_eq!(
            run(StatusCode::INTERNAL_SERVER_ERROR, ""),
            Err(ProviderError::Vendor("The vendor returned HTTP 500.".into()))
        );
    }

    #[test]
    fn malformed_bodies_are_rejected() {
        assert_eq!(run(StatusCode::OK, "{}"), Err(malformed()));
        assert_eq!(run(StatusCode::OK, r#"{"credits": -5}"#), Err(malformed()));
        assert_eq!(run(StatusCode::OK, "<html>"), Err(malformed()));
    }
}
