//! Ahrefs API units.
//!
//! Verified 2026-10-17 against
//! <https://docs.ahrefs.com/en/api/reference/subscription-info/get-limits-and-usage>:
//! `GET https://api.ahrefs.com/v3/subscription-info/limits-and-usage?output=json`
//! with `Authorization: Bearer <key>` (the header format is spelled out on
//! <https://docs.ahrefs.com/en/api/docs/api-keys-creation-and-management>,
//! read the same day). The page says "Requests to this endpoint are free and
//! do not consume any API units." 200 body
//! `{limits_and_usage: {api_key_expiration_date (date-time), subscription
//! (string), units_limit_api_key (integer or null; null = unlimited),
//! units_limit_workspace (integer or null), units_usage_api_key (integer),
//! units_usage_workspace (integer or null), usage_reset_date (date)}}`.
//! Documented error statuses are 400, 401, 403, 429 and 500, each with the
//! body `{error: string}`.
//!
//! The page documents the schema but shows no example response, so
//! `tests/fixtures/ahrefs_200.json` and the `ahrefs_401/429/500` fixtures
//! are the smallest bodies that match the documented schema, not copies of
//! vendor examples.
//!
//! The card prefers the key's own pair (`units_limit_api_key` and
//! `units_usage_api_key`) when the key has a limit and falls back to the
//! workspace pair when it does not. `subscription` is a plan name and is not
//! deserialized: only numbers and dates are.

use std::time::Duration;

use async_trait::async_trait;
use jiff::Timestamp;
use reqwest::StatusCode;
use secrecy::SecretString;
use serde::Deserialize;

use super::{count, error_message, http_status_message, malformed};
use crate::net::{Auth, Http};
use crate::provider::{Category, Provider, ProviderError, Snapshot, Unit, parse_date, parse_timestamp};

pub struct Ahrefs;

const ENDPOINT: &str = "https://api.ahrefs.com/v3/subscription-info/limits-and-usage?output=json";
const HOST: &str = "api.ahrefs.com";

#[derive(Deserialize)]
struct Body {
    limits_and_usage: LimitsAndUsage,
}

/// Counters and dates only. `subscription` (the plan name) is left out.
#[derive(Deserialize)]
struct LimitsAndUsage {
    api_key_expiration_date: Option<String>,
    units_limit_api_key: Option<f64>,
    units_limit_workspace: Option<f64>,
    units_usage_api_key: Option<f64>,
    units_usage_workspace: Option<f64>,
    usage_reset_date: Option<String>,
}

#[async_trait]
impl Provider for Ahrefs {
    fn id(&self) -> &'static str {
        "ahrefs"
    }

    fn display_name(&self) -> &'static str {
        "Ahrefs"
    }

    fn docs_url(&self) -> &'static str {
        "https://docs.ahrefs.com/en/api/reference/subscription-info/get-limits-and-usage"
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
        Some("Counts API units. Shows this key's own monthly limit when it has one, otherwise the workspace's.")
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
    let u = parsed.limits_and_usage;
    let key_limit = count(u.units_limit_api_key);
    let key_used = count(u.units_usage_api_key);
    let workspace_limit = count(u.units_limit_workspace);
    let workspace_used = count(u.units_usage_workspace);
    let mut extra = Vec::new();

    // The key's own monthly limit comes first; a key without one counts the
    // workspace pool. Nothing limited at all leaves only the usage to show.
    let (remaining, limit, spent) = match key_limit {
        Some(l) => {
            let used = key_used.ok_or_else(malformed)?;
            if let (Some(wl), Some(wu)) = (workspace_limit, workspace_used) {
                extra.push(("Workspace".to_owned(), format!("{wu} of {wl} used")));
            }
            (Some(l.saturating_sub(used)), Some(l), None)
        }
        None => {
            extra.push(("Key limit".to_owned(), "none".to_owned()));
            if let Some(used) = key_used {
                extra.push(("Used by this key".to_owned(), used.to_string()));
            }
            match (workspace_limit, workspace_used) {
                (Some(wl), Some(wu)) => (Some(wl.saturating_sub(wu)), Some(wl), None),
                (wl, wu) => (None, wl, wu.or(key_used)),
            }
        }
    };
    if let Some(expires) = u.api_key_expiration_date.as_deref().and_then(parse_timestamp) {
        extra.push(("Key expires".to_owned(), expires.strftime("%Y-%m-%d").to_string()));
    }
    Ok(Snapshot {
        taken_at: now,
        remaining,
        limit,
        period_end: u.usage_reset_date.as_deref().and_then(parse_date),
        spent,
        spend_per_hour: None,
        unit: None,
        extra,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &str = include_str!("../../tests/fixtures/ahrefs_200.json");
    const BAD_KEY: &str = include_str!("../../tests/fixtures/ahrefs_401.json");
    const LIMITED: &str = include_str!("../../tests/fixtures/ahrefs_429.json");
    const SERVER: &str = include_str!("../../tests/fixtures/ahrefs_500.json");

    fn run(status: StatusCode, body: &str) -> Result<Snapshot, ProviderError> {
        parse(status, None, body.as_bytes(), Timestamp::UNIX_EPOCH)
    }

    #[test]
    fn documented_schema_200_parses() {
        let s = run(StatusCode::OK, OK).unwrap();
        assert_eq!(s.remaining, Some(487_655));
        assert_eq!(s.limit, Some(500_000));
        assert_eq!(s.period_end.unwrap().to_string(), "2026-11-01T00:00:00Z");
        assert_eq!(s.spent, None);
        assert_eq!(
            s.extra,
            vec![
                ("Workspace".to_owned(), "456789 of 2000000 used".to_owned()),
                ("Key expires".to_owned(), "2027-01-01".to_owned()),
            ]
        );
    }

    #[test]
    fn unlimited_key_counts_the_workspace() {
        let body = OK.replace("\"units_limit_api_key\": 500000", "\"units_limit_api_key\": null");
        let s = run(StatusCode::OK, &body).unwrap();
        assert_eq!((s.remaining, s.limit), (Some(1_543_211), Some(2_000_000)));
        assert!(s.extra.contains(&("Key limit".to_owned(), "none".to_owned())));
        assert!(s.extra.contains(&("Used by this key".to_owned(), "12345".to_owned())));
    }

    #[test]
    fn nothing_limited_shows_usage_only() {
        let body = OK
            .replace("\"units_limit_api_key\": 500000", "\"units_limit_api_key\": null")
            .replace("\"units_limit_workspace\": 2000000", "\"units_limit_workspace\": null")
            .replace("\"units_usage_workspace\": 456789", "\"units_usage_workspace\": null");
        let s = run(StatusCode::OK, &body).unwrap();
        assert_eq!((s.remaining, s.limit), (None, None));
        assert_eq!(s.spent, Some(12345));
    }

    #[test]
    fn plan_name_does_not_survive() {
        let s = run(StatusCode::OK, OK).unwrap();
        let dump = format!("{s:?}");
        assert!(!dump.contains("Enterprise"), "{dump}");
    }

    #[test]
    fn documented_errors_map() {
        assert_eq!(run(StatusCode::UNAUTHORIZED, BAD_KEY), Err(ProviderError::Unauthorized));
        assert_eq!(run(StatusCode::FORBIDDEN, BAD_KEY), Err(ProviderError::Unauthorized));
        assert_eq!(
            run(StatusCode::TOO_MANY_REQUESTS, LIMITED),
            Err(ProviderError::RateLimited {
                retry_after: None,
                message: Some("Too many requests".into())
            })
        );
        assert_eq!(
            run(StatusCode::INTERNAL_SERVER_ERROR, SERVER),
            Err(ProviderError::Vendor("Internal server error".into()))
        );
        assert_eq!(
            run(StatusCode::BAD_REQUEST, r#"{"error": "Bad request"}"#),
            Err(ProviderError::Vendor("Bad request".into()))
        );
    }

    #[test]
    fn unexpected_body_is_malformed() {
        assert_eq!(run(StatusCode::OK, "{}"), Err(malformed()));
        assert_eq!(run(StatusCode::OK, "<html>"), Err(malformed()));
    }
}
