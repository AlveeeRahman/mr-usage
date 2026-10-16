//! Tavily credit usage.
//!
//! Verified 2026-10-16 against
//! <https://docs.tavily.com/documentation/api-reference/endpoint/usage>:
//! `GET https://api.tavily.com/usage`, `Authorization: Bearer tvly-...`.
//! 200 body `{key: {usage, limit, search_usage, extract_usage, crawl_usage,
//! map_usage, research_usage}, account: {current_plan, plan_usage, plan_limit,
//! paygo_usage, paygo_limit, search_usage, ...}}`. 401 and 429 bodies are
//! `{"detail": {"error": "<text>"}}`. The optional `X-Project-ID` header is
//! not sent.
//!
//! Cost: <https://docs.tavily.com/documentation/api-credits> lists search,
//! extract, map, crawl and research as the calls that consume credits. The
//! usage call is not among them, so the floor is the global one minute.
//! TODO(verify): confirm with a live key that `plan_usage` does not move when
//! only this endpoint is called.

use std::time::Duration;

use async_trait::async_trait;
use jiff::Timestamp;
use reqwest::StatusCode;
use secrecy::SecretString;
use serde::Deserialize;

use super::{MAX_VENDOR_TEXT, count, http_status_message, malformed};
use crate::net::{Auth, Http};
use crate::provider::{Provider, ProviderError, Snapshot, Unit};
use crate::redact;

pub struct Tavily;

const ENDPOINT: &str = "https://api.tavily.com/usage";
const HOST: &str = "api.tavily.com";

#[derive(Deserialize)]
struct Body {
    key: Option<KeyUsage>,
    account: Account,
}

#[derive(Deserialize)]
struct KeyUsage {
    usage: Option<f64>,
    limit: Option<f64>,
}

#[derive(Deserialize)]
struct Account {
    current_plan: Option<String>,
    plan_usage: Option<f64>,
    plan_limit: Option<f64>,
    paygo_usage: Option<f64>,
    paygo_limit: Option<f64>,
}

/// Tavily's error envelope: `{"detail": {"error": "<text>"}}`.
fn detail_error(body: &[u8]) -> Option<String> {
    #[derive(Deserialize)]
    struct Detail {
        error: Option<String>,
    }
    #[derive(Deserialize)]
    struct Envelope {
        detail: Option<Detail>,
    }
    let envelope: Envelope = serde_json::from_slice(body).ok()?;
    let text = envelope.detail?.error?;
    let clean = redact::vendor_text(&text, MAX_VENDOR_TEXT);
    (!clean.is_empty()).then_some(clean)
}

#[async_trait]
impl Provider for Tavily {
    fn id(&self) -> &'static str {
        "tavily"
    }

    fn display_name(&self) -> &'static str {
        "Tavily"
    }

    fn docs_url(&self) -> &'static str {
        "https://docs.tavily.com/documentation/api-reference/endpoint/usage"
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

    fn min_poll_interval(&self) -> Duration {
        Duration::from_secs(60)
    }

    fn note(&self) -> Option<&'static str> {
        Some("Tavily's pricing page lists only search, extract, map, crawl and research as credit-consuming calls.")
    }

    async fn fetch(&self, key: &SecretString, http: &Http) -> Result<Snapshot, ProviderError> {
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
                message: detail_error(body),
            });
        }
        s if !s.is_success() => {
            return Err(ProviderError::Vendor(detail_error(body).unwrap_or_else(|| http_status_message(s))));
        }
        _ => {}
    }
    let parsed: Body = serde_json::from_slice(body).map_err(|_| malformed())?;
    let account = parsed.account;
    let mut extra = Vec::new();
    if let Some(plan) = account.current_plan.as_deref().filter(|p| !p.is_empty()) {
        extra.push(("Plan".to_owned(), plan.to_owned()));
    }
    let used = count(account.plan_usage);
    let limit = count(account.plan_limit).filter(|l| *l > 0);
    let remaining = match (used, limit) {
        (Some(u), Some(l)) => Some(l.saturating_sub(u)),
        _ => None,
    };
    if remaining.is_none()
        && let Some(u) = used
    {
        extra.push(("Plan usage".to_owned(), u.to_string()));
    }
    if let Some(key) = parsed.key
        && let (Some(u), Some(l)) = (count(key.usage), count(key.limit).filter(|l| *l > 0))
    {
        extra.push(("This key".to_owned(), format!("{u} of {l} used")));
    }
    if let (Some(u), Some(l)) = (count(account.paygo_usage), count(account.paygo_limit).filter(|l| *l > 0)) {
        extra.push(("Pay-as-you-go".to_owned(), format!("{u} of {l} used")));
    }
    Ok(Snapshot {
        taken_at: now,
        remaining,
        limit,
        period_end: None,
        extra,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &str = include_str!("../../tests/fixtures/tavily_200.json");
    const BAD_KEY: &str = include_str!("../../tests/fixtures/tavily_401.json");
    const LIMITED: &str = include_str!("../../tests/fixtures/tavily_429.json");

    fn run(status: StatusCode, body: &str) -> Result<Snapshot, ProviderError> {
        parse(status, None, body.as_bytes(), Timestamp::UNIX_EPOCH)
    }

    #[test]
    fn documented_200_parses() {
        let s = run(StatusCode::OK, OK).unwrap();
        assert_eq!(s.remaining, Some(14500));
        assert_eq!(s.limit, Some(15000));
        assert_eq!(s.period_end, None);
        assert_eq!(
            s.extra,
            vec![
                ("Plan".to_owned(), "Bootstrap".to_owned()),
                ("This key".to_owned(), "150 of 1000 used".to_owned()),
                ("Pay-as-you-go".to_owned(), "25 of 100 used".to_owned()),
            ]
        );
    }

    #[test]
    fn missing_plan_limit_gives_usage_only() {
        let body = OK.replace("\"plan_limit\": 15000,", "");
        let s = run(StatusCode::OK, &body).unwrap();
        assert_eq!(s.remaining, None);
        assert_eq!(s.limit, None);
        assert!(s.extra.contains(&("Plan usage".to_owned(), "500".to_owned())));
    }

    #[test]
    fn documented_errors_map() {
        assert_eq!(run(StatusCode::UNAUTHORIZED, BAD_KEY), Err(ProviderError::Unauthorized));
        assert_eq!(
            run(StatusCode::TOO_MANY_REQUESTS, LIMITED),
            Err(ProviderError::RateLimited {
                retry_after: None,
                message: Some("Your request has been blocked due to excessive requests. Please reduce the rate of requests.".into())
            })
        );
        assert_eq!(
            run(StatusCode::BAD_GATEWAY, "<html>"),
            Err(ProviderError::Vendor("The vendor returned HTTP 502.".into()))
        );
    }
}
