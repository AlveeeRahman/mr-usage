//! Context7 request usage.
//!
//! Verified 2026-10-16 against
//! <https://context7.com/docs/api-reference/usage/get-api-usage>:
//! `GET https://context7.com/api/v2/usage`, `Authorization: Bearer ctx7sk...`,
//! optional `?days=1..90` for the length of `teamspace.daily` (default 30).
//! 200 body `{plan, usageResetsAt, member: {requests, includedInPlan},
//! teamspace: {extraRequests, extraCostUsd, spendingLimitUsd, daily: [...]}}`.
//! `plan` is one of `free`, `pro`, `enterprise`, `payg`; on `payg`
//! `includedInPlan` is 0 and every request is billed. `usageResetsAt` is the
//! first day of next month, UTC. Errors are `{error, message}` with 400, 401,
//! 429 and 500 documented. The page says "Calls to this endpoint do not count
//! as requests", so the floor is the global one minute.
//!
//! The app asks for `days=1` because it only needs the month-to-date numbers,
//! which keeps the body small. The daily rows are not deserialized.

use std::time::Duration;

use async_trait::async_trait;
use jiff::Timestamp;
use reqwest::StatusCode;
use secrecy::SecretString;
use serde::Deserialize;

use super::{cents, count, dollars, error_message, http_status_message, malformed};
use crate::net::{Auth, Http};
use crate::provider::{Category, Provider, ProviderError, Snapshot, Unit, parse_timestamp};

pub struct Context7;

const ENDPOINT: &str = "https://context7.com/api/v2/usage?days=1";
const HOST: &str = "context7.com";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Body {
    plan: Option<String>,
    usage_resets_at: Option<String>,
    member: Member,
    teamspace: Option<Teamspace>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Member {
    requests: f64,
    included_in_plan: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Teamspace {
    extra_requests: Option<f64>,
    extra_cost_usd: Option<f64>,
    spending_limit_usd: Option<f64>,
}

#[async_trait]
impl Provider for Context7 {
    fn id(&self) -> &'static str {
        "context7"
    }

    fn display_name(&self) -> &'static str {
        "Context7"
    }

    fn docs_url(&self) -> &'static str {
        "https://context7.com/docs/api-reference/usage/get-api-usage"
    }

    fn endpoint(&self) -> &'static str {
        ENDPOINT
    }

    fn allowed_host(&self) -> &'static str {
        HOST
    }

    fn unit(&self) -> Unit {
        Unit::Requests
    }

    fn category(&self) -> Category {
        Category::Data
    }

    fn min_poll_interval(&self) -> Duration {
        Duration::from_secs(60)
    }

    fn note(&self) -> Option<&'static str> {
        Some("Context7 documents this call as free: it does not count as a request.")
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
    let used = count(Some(parsed.member.requests)).ok_or_else(malformed)?;
    let included = count(Some(parsed.member.included_in_plan)).ok_or_else(malformed)?;
    let mut extra = Vec::new();
    if let Some(plan) = parsed.plan.as_deref().filter(|p| !p.is_empty()) {
        extra.push(("Plan".to_owned(), plan.to_owned()));
    }
    // Pay-as-you-go includes nothing, so "0 of 0 left" would be misleading.
    let (remaining, limit) = if included == 0 {
        extra.push(("Requests this month".to_owned(), used.to_string()));
        (None, None)
    } else {
        (Some(included.saturating_sub(used)), Some(included))
    };
    if let Some(team) = parsed.teamspace {
        if let Some(n) = count(team.extra_requests).filter(|n| *n > 0) {
            extra.push(("Extra requests".to_owned(), n.to_string()));
        }
        if let Some(cost) = cents(team.extra_cost_usd).filter(|c| *c > 0) {
            let cap = cents(team.spending_limit_usd).map(dollars).unwrap_or_else(|| "no cap".to_owned());
            extra.push(("Extra cost".to_owned(), format!("{} of {cap}", dollars(cost))));
        }
    }
    Ok(Snapshot {
        taken_at: now,
        remaining,
        limit,
        period_end: parsed.usage_resets_at.as_deref().and_then(parse_timestamp),
        spent: None,
        spend_per_hour: None,
        unit: None,
        extra,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRO: &str = include_str!("../../tests/fixtures/context7_200_pro.json");
    const FREE: &str = include_str!("../../tests/fixtures/context7_200_free.json");
    const BAD_KEY: &str = include_str!("../../tests/fixtures/context7_401.json");
    const LIMITED: &str = include_str!("../../tests/fixtures/context7_429.json");
    const SERVER: &str = include_str!("../../tests/fixtures/context7_500.json");

    fn run(status: StatusCode, body: &str) -> Result<Snapshot, ProviderError> {
        parse(status, None, body.as_bytes(), Timestamp::UNIX_EPOCH)
    }

    #[test]
    fn documented_pro_example_past_included_requests() {
        let s = run(StatusCode::OK, PRO).unwrap();
        assert_eq!(s.remaining, Some(0));
        assert_eq!(s.limit, Some(2000));
        assert_eq!(s.period_end.unwrap().to_string(), "2026-11-01T00:00:00Z");
        assert_eq!(
            s.extra,
            vec![
                ("Plan".to_owned(), "pro".to_owned()),
                ("Extra requests".to_owned(), "340".to_owned()),
                ("Extra cost".to_owned(), "$1.70 of $50.00".to_owned()),
            ]
        );
    }

    #[test]
    fn documented_free_example() {
        let s = run(StatusCode::OK, FREE).unwrap();
        assert_eq!(s.remaining, Some(588));
        assert_eq!(s.limit, Some(1000));
        assert_eq!(s.extra, vec![("Plan".to_owned(), "free".to_owned())]);
    }

    #[test]
    fn pay_as_you_go_has_no_count() {
        let body = PRO
            .replace("\"includedInPlan\": 2000", "\"includedInPlan\": 0")
            .replace("\"pro\"", "\"payg\"");
        let s = run(StatusCode::OK, &body).unwrap();
        assert_eq!(s.remaining, None);
        assert_eq!(s.limit, None);
        assert!(s.extra.contains(&("Requests this month".to_owned(), "2340".to_owned())));
    }

    #[test]
    fn documented_errors_map() {
        assert_eq!(run(StatusCode::UNAUTHORIZED, BAD_KEY), Err(ProviderError::Unauthorized));
        assert_eq!(
            run(StatusCode::TOO_MANY_REQUESTS, LIMITED),
            Err(ProviderError::RateLimited {
                retry_after: None,
                message: Some("rate_limit_exceeded. Rate limit exceeded. Please try again later.".into())
            })
        );
        assert_eq!(
            run(StatusCode::INTERNAL_SERVER_ERROR, SERVER),
            Err(ProviderError::Vendor(
                "internal_error. An error occurred while processing your request".into()
            ))
        );
    }

    #[test]
    fn daily_rows_are_not_needed() {
        let body = r#"{"plan":"free","usageResetsAt":"2026-11-01T00:00:00.000Z","member":{"requests":1,"includedInPlan":1000}}"#;
        let s = run(StatusCode::OK, body).unwrap();
        assert_eq!(s.remaining, Some(999));
    }
}
