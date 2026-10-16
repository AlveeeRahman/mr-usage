//! Hunter account requests.
//!
//! Verified 2026-10-16 against <https://hunter.io/api-documentation/v2>
//! ("Account Information" and "Authentication"): `GET
//! https://api.hunter.io/v2/account`. "This call is free." The key may be
//! passed as the `api_key` query parameter, the `X-API-KEY` header or
//! `Authorization: Bearer <key>`; the app sends the header. 200 body
//! `{data: {first_name, last_name, email, plan_name, plan_level, reset_date,
//! team_id, requests: {credits?: {used, available, remaining}, searches:
//! {...}, verifications: {...}}, calls: {...deprecated}}}`. Errors are
//! `{"errors": [{"id", "code", "details"}]}`. Status meanings from the
//! "Errors" table: 401 no valid key, 403 "You have reached the rate limit",
//! 429 "You have reached your usage limit".
//!
//! Per the docs, `requests.credits` exists only for accounts with a single
//! credit pool; `searches` and `verifications` are then approximate views of
//! that pool, rounded down to whole requests. Without `credits` they are
//! separate quotas. The headline is always `searches.remaining` of
//! `searches.available`, which is a real count in both cases.
//!
//! The documented 200 example is not valid JSON: `"reset_date": 2026-10-24`
//! is unquoted. `tests/fixtures/hunter_200_as_documented.json` keeps it as
//! published and the test checks the parser rejects it;
//! `hunter_200.json` quotes the date and nothing else.
//!
//! `first_name`, `last_name`, `email` and `team_id` are never deserialized.

use std::time::Duration;

use async_trait::async_trait;
use jiff::Timestamp;
use reqwest::StatusCode;
use secrecy::SecretString;
use serde::Deserialize;

use super::{MAX_VENDOR_TEXT, count, http_status_message, malformed};
use crate::net::{Auth, Http};
use crate::provider::{Provider, ProviderError, Snapshot, Unit, parse_date};
use crate::redact;

pub struct Hunter;

const ENDPOINT: &str = "https://api.hunter.io/v2/account";
const HOST: &str = "api.hunter.io";

#[derive(Deserialize)]
struct Body {
    data: Data,
}

/// Plan, reset date and the request counters. No identity fields.
#[derive(Deserialize)]
struct Data {
    plan_name: Option<String>,
    reset_date: Option<String>,
    requests: Requests,
}

#[derive(Deserialize)]
struct Requests {
    credits: Option<Pool>,
    searches: Option<Pool>,
    verifications: Option<Pool>,
}

#[derive(Deserialize)]
struct Pool {
    available: Option<f64>,
    remaining: Option<f64>,
}

/// Hunter's error envelope: `{"errors": [{"id", "code", "details"}]}`.
fn hunter_error(body: &[u8]) -> Option<String> {
    #[derive(Deserialize)]
    struct Item {
        details: Option<String>,
        id: Option<String>,
    }
    #[derive(Deserialize)]
    struct Envelope {
        errors: Vec<Item>,
    }
    let envelope: Envelope = serde_json::from_slice(body).ok()?;
    let first = envelope.errors.into_iter().next()?;
    let text = first.details.or(first.id)?;
    let clean = redact::vendor_text(&text, MAX_VENDOR_TEXT);
    (!clean.is_empty()).then_some(clean)
}

fn pool_pair(pool: &Pool) -> Option<(u64, u64)> {
    Some((count(pool.remaining)?, count(pool.available)?))
}

#[async_trait]
impl Provider for Hunter {
    fn id(&self) -> &'static str {
        "hunter"
    }

    fn display_name(&self) -> &'static str {
        "Hunter"
    }

    fn docs_url(&self) -> &'static str {
        "https://hunter.io/api-documentation/v2#account"
    }

    fn endpoint(&self) -> &'static str {
        ENDPOINT
    }

    fn allowed_host(&self) -> &'static str {
        HOST
    }

    fn unit(&self) -> Unit {
        Unit::Searches
    }

    fn min_poll_interval(&self) -> Duration {
        Duration::from_secs(60)
    }

    fn note(&self) -> Option<&'static str> {
        Some("Hunter documents this call as free. On a single-credit-pool plan the search count is Hunter's approximate view of that pool.")
    }

    async fn fetch(&self, key: &SecretString, http: &Http) -> Result<Snapshot, ProviderError> {
        let response = http.get(ENDPOINT, HOST, Auth::Bearer(key)).await?;
        parse(response.status, response.retry_after, &response.body, Timestamp::now())
    }
}

pub(crate) fn parse(status: StatusCode, retry_after: Option<Duration>, body: &[u8], now: Timestamp) -> Result<Snapshot, ProviderError> {
    match status {
        StatusCode::UNAUTHORIZED => return Err(ProviderError::Unauthorized),
        // Hunter's 403 is its rate limit and 429 is an exhausted quota.
        StatusCode::FORBIDDEN | StatusCode::TOO_MANY_REQUESTS => {
            return Err(ProviderError::RateLimited {
                retry_after,
                message: hunter_error(body),
            });
        }
        s if !s.is_success() => {
            return Err(ProviderError::Vendor(hunter_error(body).unwrap_or_else(|| http_status_message(s))));
        }
        _ => {}
    }
    let parsed: Body = serde_json::from_slice(body).map_err(|_| malformed())?;
    let data = parsed.data;
    let searches = data.requests.searches.as_ref().and_then(pool_pair).ok_or_else(malformed)?;
    let mut extra = Vec::new();
    if let Some(plan) = data.plan_name.as_deref().filter(|p| !p.is_empty()) {
        extra.push(("Plan".to_owned(), plan.to_owned()));
    }
    if let Some((r, a)) = data.requests.credits.as_ref().and_then(pool_pair) {
        extra.push((
            "Credits".to_owned(),
            format!("{r} of {a} left (searches above are an approximate view)"),
        ));
    }
    if let Some((r, a)) = data.requests.verifications.as_ref().and_then(pool_pair) {
        extra.push(("Verifications".to_owned(), format!("{r} of {a} left")));
    }
    Ok(Snapshot {
        taken_at: now,
        remaining: Some(searches.0),
        limit: Some(searches.1).filter(|a| *a > 0),
        period_end: data.reset_date.as_deref().and_then(parse_date),
        extra,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &str = include_str!("../../tests/fixtures/hunter_200.json");
    const AS_DOCUMENTED: &str = include_str!("../../tests/fixtures/hunter_200_as_documented.json");
    const ERROR: &str = include_str!("../../tests/fixtures/hunter_400.json");

    fn run(status: StatusCode, body: &str) -> Result<Snapshot, ProviderError> {
        parse(status, None, body.as_bytes(), Timestamp::UNIX_EPOCH)
    }

    #[test]
    fn documented_200_with_the_date_quoted_parses() {
        let s = run(StatusCode::OK, OK).unwrap();
        assert_eq!(s.remaining, Some(9450));
        assert_eq!(s.limit, Some(10000));
        assert_eq!(s.period_end.unwrap().to_string(), "2026-10-24T00:00:00Z");
        assert_eq!(
            s.extra,
            vec![
                ("Plan".to_owned(), "Growth".to_owned()),
                (
                    "Credits".to_owned(),
                    "9450 of 10000 left (searches above are an approximate view)".to_owned()
                ),
                ("Verifications".to_owned(), "18900 of 20000 left".to_owned()),
            ]
        );
    }

    #[test]
    fn documented_example_as_published_is_not_json() {
        assert_eq!(run(StatusCode::OK, AS_DOCUMENTED), Err(malformed()));
    }

    #[test]
    fn identity_fields_do_not_survive() {
        let s = run(StatusCode::OK, OK).unwrap();
        let dump = format!("{s:?}");
        for echoed in ["Antoine", "Finkelstein", "antoine@hunter.io"] {
            assert!(!dump.contains(echoed), "{dump}");
        }
    }

    #[test]
    fn separate_quotas_without_a_credit_pool() {
        let body = OK.replace(
            r#""credits": {
        "used": 550.0,
        "available": 10000.0,
        "remaining": 9450.0
      },"#,
            "",
        );
        let s = run(StatusCode::OK, &body).unwrap();
        assert_eq!(s.remaining, Some(9450));
        assert!(!s.extra.iter().any(|(k, _)| k == "Credits"));
    }

    #[test]
    fn documented_error_envelope_maps() {
        assert_eq!(run(StatusCode::UNAUTHORIZED, ERROR), Err(ProviderError::Unauthorized));
        assert_eq!(
            run(StatusCode::FORBIDDEN, ERROR),
            Err(ProviderError::RateLimited {
                retry_after: None,
                message: Some("You are missing the domain parameter".into())
            })
        );
        assert_eq!(
            run(StatusCode::TOO_MANY_REQUESTS, ERROR),
            Err(ProviderError::RateLimited {
                retry_after: None,
                message: Some("You are missing the domain parameter".into())
            })
        );
        assert_eq!(
            run(StatusCode::BAD_REQUEST, ERROR),
            Err(ProviderError::Vendor("You are missing the domain parameter".into()))
        );
    }
}
