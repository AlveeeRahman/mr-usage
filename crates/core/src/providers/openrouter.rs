//! OpenRouter key credit limit and spend.
//!
//! Verified 2026-10-16 against
//! <https://openrouter.ai/docs/api/api-reference/api-keys/get-current-api-key>:
//! `GET https://openrouter.ai/api/v1/key`, `Authorization: Bearer sk-or-v1-...`.
//! 200 body `{data: {limit, limit_remaining, limit_reset, usage, usage_daily,
//! usage_weekly, usage_monthly, is_free_tier, effective_budget: {limit_usd,
//! remaining_usd, spend_usd, reset_interval, resets_at} | null,
//! free_model_daily_requests: {limit, remaining, used} | null, label,
//! creator_user_id, workspace_id, ...}}`. Money is in US dollars. `limit` and
//! `limit_remaining` are null when the key has no credit limit. Errors are
//! `{"error": {"code": <number>, "message": "<text>"}}` with 401 and 500
//! documented.
//!
//! `label`, `creator_user_id`, `workspace_id`, `organization_id` and the
//! budget `guardrail_id` are identifiers and are never deserialized. The
//! `label` in particular holds the first and last characters of the key.
//!
//! Cost: this reads the key's own record and is not a model call. The docs
//! do not state a price; none of the pricing pages mention it.
//! TODO(verify): confirm with a live key that `usage` does not move.

use std::time::Duration;

use async_trait::async_trait;
use jiff::Timestamp;
use reqwest::StatusCode;
use secrecy::SecretString;
use serde::Deserialize;

use super::{cents, count, dollars, error_message, http_status_message, malformed};
use crate::net::{Auth, Http};
use crate::provider::{Category, Provider, ProviderError, Snapshot, Unit, parse_timestamp};

pub struct OpenRouter;

const ENDPOINT: &str = "https://openrouter.ai/api/v1/key";
const HOST: &str = "openrouter.ai";

#[derive(Deserialize)]
struct Body {
    data: Data,
}

/// Numbers and dates only. See the module doc for what is left out.
#[derive(Deserialize)]
struct Data {
    limit: Option<f64>,
    limit_remaining: Option<f64>,
    limit_reset: Option<String>,
    usage: Option<f64>,
    usage_monthly: Option<f64>,
    is_free_tier: Option<bool>,
    effective_budget: Option<Budget>,
    free_model_daily_requests: Option<FreeRequests>,
}

#[derive(Deserialize)]
struct Budget {
    limit_usd: Option<f64>,
    remaining_usd: Option<f64>,
    reset_interval: Option<String>,
    resets_at: Option<String>,
}

#[derive(Deserialize)]
struct FreeRequests {
    limit: Option<f64>,
    used: Option<f64>,
}

#[async_trait]
impl Provider for OpenRouter {
    fn id(&self) -> &'static str {
        "openrouter"
    }

    fn display_name(&self) -> &'static str {
        "OpenRouter"
    }

    fn docs_url(&self) -> &'static str {
        "https://openrouter.ai/docs/api/api-reference/api-keys/get-current-api-key"
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
        Category::Llm
    }

    fn min_poll_interval(&self) -> Duration {
        Duration::from_secs(60)
    }

    fn note(&self) -> Option<&'static str> {
        Some("Shows this key's credit limit. A key without a limit shows spend only.")
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
    let data = parsed.data;
    let mut extra = Vec::new();
    let mut period_end = None;

    // The key's own credit limit comes first, then an organization budget.
    let (remaining, limit) = match (cents(data.limit_remaining), cents(data.limit)) {
        (Some(r), Some(l)) => {
            if let Some(reset) = data.limit_reset.as_deref().filter(|r| !r.is_empty()) {
                extra.push(("Limit resets".to_owned(), reset.to_owned()));
            }
            (Some(r), Some(l))
        }
        _ => match data
            .effective_budget
            .as_ref()
            .and_then(|b| Some((cents(b.remaining_usd)?, cents(b.limit_usd)?)))
        {
            Some((r, l)) => {
                extra.push(("Counting".to_owned(), "the workspace budget; this key has no limit".to_owned()));
                (Some(r), Some(l))
            }
            None => {
                extra.push(("Key limit".to_owned(), "none".to_owned()));
                (None, None)
            }
        },
    };
    if let Some(b) = &data.effective_budget {
        if let Some(at) = b.resets_at.as_deref().and_then(parse_timestamp) {
            period_end = Some(at);
        }
        if let (Some(r), Some(l)) = (cents(b.remaining_usd), cents(b.limit_usd)) {
            let interval = b.reset_interval.as_deref().filter(|i| !i.is_empty()).unwrap_or("no reset");
            extra.push(("Budget".to_owned(), format!("{} of {} left, {interval}", dollars(r), dollars(l))));
        }
    }
    // This month's spend, or the key's lifetime usage when the monthly figure
    // is missing. A key with no limit and no budget shows it as its headline.
    let spent = cents(data.usage_monthly).or_else(|| cents(data.usage));
    if let Some(f) = data.free_model_daily_requests
        && let (Some(used), Some(l)) = (count(f.used), count(f.limit).filter(|l| *l > 0))
    {
        extra.push(("Free-model requests today".to_owned(), format!("{used} of {l}")));
    }
    if data.is_free_tier == Some(true) {
        extra.push(("Tier".to_owned(), "free".to_owned()));
    }
    Ok(Snapshot {
        taken_at: now,
        remaining,
        limit,
        period_end,
        spent,
        spend_per_hour: None,
        extra,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &str = include_str!("../../tests/fixtures/openrouter_200.json");
    const BAD_KEY: &str = include_str!("../../tests/fixtures/openrouter_401.json");
    const SERVER: &str = include_str!("../../tests/fixtures/openrouter_500.json");

    fn run(status: StatusCode, body: &str) -> Result<Snapshot, ProviderError> {
        parse(status, None, body.as_bytes(), Timestamp::UNIX_EPOCH)
    }

    #[test]
    fn documented_200_parses_in_cents() {
        let s = run(StatusCode::OK, OK).unwrap();
        assert_eq!(s.remaining, Some(7450));
        assert_eq!(s.limit, Some(10000));
        assert_eq!(s.period_end.unwrap().to_string(), "2026-10-12T00:00:00Z");
        assert_eq!(
            s.extra,
            vec![
                ("Limit resets".to_owned(), "monthly".to_owned()),
                ("Budget".to_owned(), "$17.50 of $50.00 left, weekly".to_owned()),
                ("Free-model requests today".to_owned(), "12 of 50".to_owned()),
            ]
        );
    }

    #[test]
    fn identifiers_do_not_survive() {
        let s = run(StatusCode::OK, OK).unwrap();
        let dump = format!("{s:?}");
        for echoed in [
            "sk-or-v1-au7",
            "user_2dHFtVWx2n56w6HkM0000000000",
            "0df9e665-d932-5740-b2c7-b52af166bc11",
        ] {
            assert!(!dump.contains(echoed), "{dump}");
        }
    }

    #[test]
    fn unlimited_key_falls_back_to_budget_then_to_spend_only() {
        let no_limit = OK
            .replace("\"limit\": 100,", "\"limit\": null,")
            .replace("\"limit_remaining\": 74.5,", "\"limit_remaining\": null,");
        let s = run(StatusCode::OK, &no_limit).unwrap();
        assert_eq!((s.remaining, s.limit), (Some(1750), Some(5000)));
        assert!(s.extra.iter().any(|(k, _)| k == "Counting"));

        let no_budget = no_limit.replace("\"effective_budget\": {", "\"effective_budget\": null, \"x\": {");
        let s = run(StatusCode::OK, &no_budget).unwrap();
        assert_eq!((s.remaining, s.limit), (None, None));
        assert_eq!(s.period_end, None);
        assert!(s.extra.contains(&("Key limit".to_owned(), "none".to_owned())));
        assert_eq!(s.spent, Some(2550));
    }

    #[test]
    fn documented_errors_map() {
        assert_eq!(run(StatusCode::UNAUTHORIZED, BAD_KEY), Err(ProviderError::Unauthorized));
        assert_eq!(
            run(StatusCode::INTERNAL_SERVER_ERROR, SERVER),
            Err(ProviderError::Vendor("Internal Server Error".into()))
        );
        assert_eq!(
            run(StatusCode::TOO_MANY_REQUESTS, ""),
            Err(ProviderError::RateLimited {
                retry_after: None,
                message: None
            })
        );
    }
}
