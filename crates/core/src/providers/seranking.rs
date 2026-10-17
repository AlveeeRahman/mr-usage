//! SE Ranking Data API units.
//!
//! Verified 2026-10-17 against <https://seranking.com/api/data/account-system/>
//! ("Get subscription details"): `GET
//! https://api.seranking.com/v1/account/subscription?output=json` with the
//! header `Authorization: Token <key>`. The page states "Cost: 0 credits".
//! 200 body `{subscription_info: {status ("active" | "inactive" | "expired"
//! | "disabled"), start_date, expiration_date ("YYYY-MM-DD HH:MM:SS"),
//! units_limit, units_left}}`. The page documents no error shape; HTTP
//! 401/403 are treated as a rejected key and other failures as vendor errors.
//!
//! The dates carry no zone and are read as UTC. `status` is kept only when it
//! is one of the four documented words, so no free text reaches the card.

use std::time::Duration;

use async_trait::async_trait;
use jiff::Timestamp;
use reqwest::StatusCode;
use secrecy::SecretString;
use serde::Deserialize;

use super::{count, error_message, http_status_message, malformed};
use crate::net::{Auth, Http};
use crate::provider::{Category, Provider, ProviderError, Snapshot, Unit};

pub struct SeRanking;

const ENDPOINT: &str = "https://api.seranking.com/v1/account/subscription?output=json";
const HOST: &str = "api.seranking.com";
const STATUSES: &[&str] = &["active", "inactive", "expired", "disabled"];

#[derive(Deserialize)]
struct Body {
    subscription_info: Info,
}

#[derive(Deserialize)]
struct Info {
    status: Option<String>,
    start_date: Option<String>,
    expiration_date: Option<String>,
    units_limit: Option<f64>,
    units_left: Option<f64>,
}

#[async_trait]
impl Provider for SeRanking {
    fn id(&self) -> &'static str {
        "seranking"
    }

    fn display_name(&self) -> &'static str {
        "SE Ranking"
    }

    fn docs_url(&self) -> &'static str {
        "https://seranking.com/api/data/account-system/"
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
        Some("Counts Data API units for the subscription; the renewal date is its expiration date.")
    }

    async fn fetch(&self, key: &SecretString, _config: Option<&str>, http: &Http) -> Result<Snapshot, ProviderError> {
        let auth = Auth::Header {
            name: "authorization",
            prefix: "Token ",
            value: key,
        };
        let response = http.get(ENDPOINT, HOST, auth).await?;
        parse(response.status, response.retry_after, &response.body, Timestamp::now())
    }
}

/// `2024-02-18 14:20:02` read as UTC.
fn parse_utc_datetime(raw: &str) -> Option<Timestamp> {
    let civil: jiff::civil::DateTime = raw.trim().replacen(' ', "T", 1).parse().ok()?;
    civil.to_zoned(jiff::tz::TimeZone::UTC).ok().map(|z| z.timestamp())
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
    let info = parsed.subscription_info;
    let mut extra = Vec::new();
    if let Some(status) = info.status.as_deref().and_then(|s| STATUSES.iter().find(|known| **known == s)) {
        extra.push(("Plan status".to_owned(), (*status).to_owned()));
    }
    if let Some(start) = info.start_date.as_deref().and_then(parse_utc_datetime) {
        extra.push(("Subscription start".to_owned(), start.strftime("%Y-%m-%d").to_string()));
    }
    Ok(Snapshot {
        taken_at: now,
        remaining: count(info.units_left),
        limit: count(info.units_limit),
        period_end: info.expiration_date.as_deref().and_then(parse_utc_datetime),
        spent: None,
        spend_per_hour: None,
        unit: None,
        extra,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &str = include_str!("../../tests/fixtures/seranking_200.json");

    fn run(status: StatusCode, body: &str) -> Result<Snapshot, ProviderError> {
        parse(status, None, body.as_bytes(), Timestamp::UNIX_EPOCH)
    }

    #[test]
    fn documented_200_parses() {
        let s = run(StatusCode::OK, OK).unwrap();
        assert_eq!(s.remaining, Some(4_975_033));
        assert_eq!(s.limit, Some(5_000_000));
        assert_eq!(s.period_end.unwrap().to_string(), "2024-02-18T14:20:02Z");
        assert_eq!(
            s.extra,
            vec![
                ("Plan status".to_owned(), "active".to_owned()),
                ("Subscription start".to_owned(), "2024-01-18".to_owned()),
            ]
        );
    }

    #[test]
    fn unknown_status_text_does_not_survive() {
        let body = OK.replace("\"active\"", "\"owner: demo@example.com\"");
        let s = run(StatusCode::OK, &body).unwrap();
        let dump = format!("{s:?}");
        assert!(!dump.contains("demo@example.com"), "{dump}");
        assert!(!s.extra.iter().any(|(k, _)| k == "Plan status"));
    }

    #[test]
    fn unparseable_dates_are_none() {
        let body = OK.replace("2024-02-18 14:20:02", "soon");
        assert_eq!(run(StatusCode::OK, &body).unwrap().period_end, None);
    }

    #[test]
    fn statuses_map() {
        assert_eq!(run(StatusCode::UNAUTHORIZED, ""), Err(ProviderError::Unauthorized));
        assert_eq!(run(StatusCode::FORBIDDEN, ""), Err(ProviderError::Unauthorized));
        assert_eq!(
            run(StatusCode::TOO_MANY_REQUESTS, r#"{"message":"Too many requests"}"#),
            Err(ProviderError::RateLimited {
                retry_after: None,
                message: Some("Too many requests".into())
            })
        );
        assert_eq!(
            run(StatusCode::INTERNAL_SERVER_ERROR, ""),
            Err(ProviderError::Vendor("The vendor returned HTTP 500.".into()))
        );
        assert_eq!(run(StatusCode::OK, "{}"), Err(malformed()));
    }
}
