//! Firecrawl credit usage.
//!
//! Verified 2026-10-16 against
//! <https://docs.firecrawl.dev/api-reference/endpoint/credit-usage>:
//! `GET https://api.firecrawl.dev/v2/team/credit-usage`, `Authorization: Bearer`,
//! 200 body `{success, data: {remainingCredits, planCredits, billingPeriodStart,
//! billingPeriodEnd}}`, 404/500 bodies `{success: false, error}`.
//!
//! Cost: <https://docs.firecrawl.dev/billing> says every API call consumes
//! credits but its per-endpoint table does not list this endpoint. The
//! open-source API server settles it (firecrawl/firecrawl, commit
//! 386330419c6924cc5d733e59abca9b857241acc4, 2026-10-15, `apps/api/src`):
//! `routes/v2.ts` mounts `/team/credit-usage` behind
//! `authMiddleware(RateLimiterMode.Account)`, `controllers/v2/credit-usage.ts`
//! only reads the balance (`getTeamBalance` in `services/autumn/usage.ts`), and
//! `billTeam(` is called from the scrape, crawl, search, map, extract and
//! agent controllers but not from this one. The Account rate limiter allows
//! 1000 requests per minute. So the call is free and the floor is the global
//! one minute. Confirmed on a live account on 2026-10-17: `remainingCredits`
//! did not move across polls with no other usage.

use std::time::Duration;

use async_trait::async_trait;
use jiff::Timestamp;
use reqwest::StatusCode;
use secrecy::SecretString;
use serde::Deserialize;

use super::{error_message, http_status_message, malformed};
use crate::net::{Auth, Http};
use crate::provider::{Provider, ProviderError, Snapshot, Unit, parse_timestamp};

pub struct Firecrawl;

const ENDPOINT: &str = "https://api.firecrawl.dev/v2/team/credit-usage";
const HOST: &str = "api.firecrawl.dev";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Body {
    success: bool,
    data: Option<Data>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Data {
    // Signed: pay-as-you-go accounts can run past zero into an overage allowance.
    remaining_credits: Option<i64>,
    plan_credits: Option<i64>,
    billing_period_start: Option<String>,
    billing_period_end: Option<String>,
}

#[async_trait]
impl Provider for Firecrawl {
    fn id(&self) -> &'static str {
        "firecrawl"
    }

    fn display_name(&self) -> &'static str {
        "Firecrawl"
    }

    fn docs_url(&self) -> &'static str {
        "https://docs.firecrawl.dev/api-reference/endpoint/credit-usage"
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
        Some("Free to call: Firecrawl's open-source API server bills nothing for it, and a live account confirmed it.")
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
                message: error_message(body),
            });
        }
        s if !s.is_success() => {
            return Err(ProviderError::Vendor(error_message(body).unwrap_or_else(|| http_status_message(s))));
        }
        _ => {}
    }
    let parsed: Body = serde_json::from_slice(body).map_err(|_| malformed())?;
    if !parsed.success {
        return Err(ProviderError::Vendor(
            error_message(body).unwrap_or_else(|| "Firecrawl reported success: false.".into()),
        ));
    }
    let data = parsed.data.ok_or_else(malformed)?;
    let mut extra = Vec::new();
    if let Some(start) = data.billing_period_start.as_deref().and_then(parse_timestamp) {
        extra.push(("Billing period start".to_owned(), start.strftime("%Y-%m-%d").to_string()));
    }
    if let Some(r) = data.remaining_credits.filter(|r| *r < 0) {
        extra.push(("Overage".to_owned(), format!("{} credits", r.unsigned_abs())));
    }
    Ok(Snapshot {
        taken_at: now,
        remaining: data.remaining_credits.map(|r| r.max(0).unsigned_abs()),
        limit: data.plan_credits.map(|p| p.max(0).unsigned_abs()),
        period_end: data.billing_period_end.as_deref().and_then(parse_timestamp),
        extra,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &str = include_str!("../../tests/fixtures/firecrawl_200.json");
    const NOT_FOUND: &str = include_str!("../../tests/fixtures/firecrawl_404.json");
    const SERVER: &str = include_str!("../../tests/fixtures/firecrawl_500.json");

    #[test]
    fn documented_200_parses() {
        let s = parse(StatusCode::OK, None, OK.as_bytes(), Timestamp::UNIX_EPOCH).unwrap();
        assert_eq!(s.remaining, Some(1000));
        assert_eq!(s.limit, Some(500_000));
        assert_eq!(s.period_end.unwrap().to_string(), "2025-01-31T23:59:59Z");
        assert_eq!(s.extra, vec![("Billing period start".to_owned(), "2025-01-01".to_owned())]);
    }

    #[test]
    fn documented_404_surfaces_vendor_text() {
        let e = parse(StatusCode::NOT_FOUND, None, NOT_FOUND.as_bytes(), Timestamp::UNIX_EPOCH).unwrap_err();
        assert_eq!(e, ProviderError::Vendor("Could not find credit usage information".into()));
    }

    #[test]
    fn documented_500_surfaces_vendor_text() {
        let e = parse(StatusCode::INTERNAL_SERVER_ERROR, None, SERVER.as_bytes(), Timestamp::UNIX_EPOCH).unwrap_err();
        assert_eq!(e, ProviderError::Vendor("Internal server error while fetching credit usage".into()));
    }

    #[test]
    fn success_false_on_200_is_an_error() {
        let e = parse(StatusCode::OK, None, NOT_FOUND.as_bytes(), Timestamp::UNIX_EPOCH).unwrap_err();
        assert!(matches!(e, ProviderError::Vendor(_)));
    }

    #[test]
    fn auth_and_rate_limit_statuses() {
        assert_eq!(
            parse(StatusCode::UNAUTHORIZED, None, b"", Timestamp::UNIX_EPOCH),
            Err(ProviderError::Unauthorized)
        );
        let e = parse(
            StatusCode::TOO_MANY_REQUESTS,
            Some(Duration::from_secs(7)),
            b"",
            Timestamp::UNIX_EPOCH,
        )
        .unwrap_err();
        assert_eq!(
            e,
            ProviderError::RateLimited {
                retry_after: Some(Duration::from_secs(7)),
                message: None
            }
        );
    }

    #[test]
    fn non_json_error_body_gets_status_text() {
        let e = parse(StatusCode::BAD_GATEWAY, None, b"<html>bad gateway</html>", Timestamp::UNIX_EPOCH).unwrap_err();
        assert_eq!(e, ProviderError::Vendor("The vendor returned HTTP 502.".into()));
    }
}
