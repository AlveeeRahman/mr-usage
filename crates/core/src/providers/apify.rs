//! Apify monthly usage against the account's limits.
//!
//! Verified 2026-10-16 against
//! <https://docs.apify.com/api/v2/users-me-limits-get>:
//! `GET https://api.apify.com/v2/users/me/limits`. Authentication per
//! <https://docs.apify.com/api/v2#authentication>: `Authorization: Bearer
//! <token>` (recommended) or a `token` query parameter (the docs call it less
//! secure); the app sends the header. 200 body `{data: {monthlyUsageCycle:
//! {startAt, endAt}, limits: {maxMonthlyUsageUsd, maxMonthlyActorComputeUnits,
//! maxMonthlyProxySerps, ...}, current: {monthlyUsageUsd,
//! monthlyActorComputeUnits, monthlyProxySerps, ...}}}`. Errors are
//! `{"error": {"type": "<slug>", "message": "<text>"}}` with 400, 401, 403,
//! 405 and 429 documented.
//!
//! The headline is dollars: how much of the monthly spending limit is left.
//! The same page is what the console's Limits page shows, and the docs do not
//! list it as billable platform usage. TODO(verify): confirm with a live
//! token that `monthlyUsageUsd` does not move when only this endpoint is
//! called.

use std::time::Duration;

use async_trait::async_trait;
use jiff::Timestamp;
use reqwest::StatusCode;
use secrecy::SecretString;
use serde::Deserialize;

use super::{cents, error_message, http_status_message, malformed, whole};
use crate::net::{Auth, Http};
use crate::provider::{Provider, ProviderError, Snapshot, Unit, parse_timestamp};

pub struct Apify;

const ENDPOINT: &str = "https://api.apify.com/v2/users/me/limits";
const HOST: &str = "api.apify.com";

#[derive(Deserialize)]
struct Body {
    data: Data,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Data {
    monthly_usage_cycle: Option<Cycle>,
    limits: Limits,
    current: Current,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Cycle {
    end_at: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Limits {
    max_monthly_usage_usd: Option<f64>,
    max_monthly_actor_compute_units: Option<f64>,
    max_monthly_proxy_serps: Option<f64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Current {
    monthly_usage_usd: Option<f64>,
    monthly_actor_compute_units: Option<f64>,
    monthly_proxy_serps: Option<f64>,
}

#[async_trait]
impl Provider for Apify {
    fn id(&self) -> &'static str {
        "apify"
    }

    fn display_name(&self) -> &'static str {
        "Apify"
    }

    fn docs_url(&self) -> &'static str {
        "https://docs.apify.com/api/v2/users-me-limits-get"
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

    fn min_poll_interval(&self) -> Duration {
        Duration::from_secs(60)
    }

    fn note(&self) -> Option<&'static str> {
        Some("Shows what is left of this month's spending limit, as on the console's Limits page.")
    }

    async fn fetch(&self, key: &SecretString, http: &Http) -> Result<Snapshot, ProviderError> {
        let response = http.get(ENDPOINT, HOST, Auth::Bearer(key)).await?;
        parse(response.status, response.retry_after, &response.body, Timestamp::now())
    }
}

pub(crate) fn parse(status: StatusCode, retry_after: Option<Duration>, body: &[u8], now: Timestamp) -> Result<Snapshot, ProviderError> {
    match status {
        StatusCode::UNAUTHORIZED => return Err(ProviderError::Unauthorized),
        // Apify's 403 is a valid token without the right permission.
        StatusCode::FORBIDDEN => {
            return Err(ProviderError::Unsupported(
                error_message(body).unwrap_or_else(|| "This token may not read the account's limits.".into()),
            ));
        }
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
    let data = parsed.data;
    let spent = cents(data.current.monthly_usage_usd).ok_or_else(malformed)?;
    let cap = cents(data.limits.max_monthly_usage_usd).ok_or_else(malformed)?;
    let mut extra = Vec::new();
    if let (Some(u), Some(l)) = (
        data.current.monthly_actor_compute_units,
        data.limits.max_monthly_actor_compute_units,
    ) {
        extra.push(("Compute units".to_owned(), format!("{} of {} used", whole(u), whole(l))));
    }
    if let (Some(u), Some(l)) = (data.current.monthly_proxy_serps, data.limits.max_monthly_proxy_serps) {
        extra.push(("Proxy SERPs".to_owned(), format!("{} of {} used", whole(u), whole(l))));
    }
    Ok(Snapshot {
        taken_at: now,
        remaining: Some(cap.saturating_sub(spent)),
        limit: Some(cap),
        period_end: data.monthly_usage_cycle.and_then(|c| c.end_at).as_deref().and_then(parse_timestamp),
        extra,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &str = include_str!("../../tests/fixtures/apify_200.json");
    const BAD_TOKEN: &str = include_str!("../../tests/fixtures/apify_401.json");
    const FORBIDDEN: &str = include_str!("../../tests/fixtures/apify_403.json");
    const LIMITED: &str = include_str!("../../tests/fixtures/apify_429.json");

    fn run(status: StatusCode, body: &str) -> Result<Snapshot, ProviderError> {
        parse(status, None, body.as_bytes(), Timestamp::UNIX_EPOCH)
    }

    #[test]
    fn documented_200_parses_in_cents() {
        let s = run(StatusCode::OK, OK).unwrap();
        assert_eq!(s.remaining, Some(25700));
        assert_eq!(s.limit, Some(30000));
        assert_eq!(s.period_end.unwrap().to_string(), "2022-11-01T23:59:59.999Z");
        assert_eq!(
            s.extra,
            vec![
                ("Compute units".to_owned(), "500 of 1000 used".to_owned()),
                ("Proxy SERPs".to_owned(), "34 of 50 used".to_owned()),
            ]
        );
    }

    #[test]
    fn overspend_clamps_at_zero() {
        let body = OK.replace("\"monthlyUsageUsd\": 43", "\"monthlyUsageUsd\": 301.5");
        let s = run(StatusCode::OK, &body).unwrap();
        assert_eq!(s.remaining, Some(0));
    }

    #[test]
    fn documented_errors_map() {
        assert_eq!(run(StatusCode::UNAUTHORIZED, BAD_TOKEN), Err(ProviderError::Unauthorized));
        assert_eq!(
            run(StatusCode::FORBIDDEN, FORBIDDEN),
            Err(ProviderError::Unsupported(
                "You do not have permission to perform this action.".into()
            ))
        );
        assert_eq!(
            run(StatusCode::TOO_MANY_REQUESTS, LIMITED),
            Err(ProviderError::RateLimited {
                retry_after: None,
                message: Some("You have exceeded the rate limit. Please try again later.".into())
            })
        );
    }
}
