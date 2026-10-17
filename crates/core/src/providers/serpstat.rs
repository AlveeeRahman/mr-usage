//! Serpstat API credits (lines).
//!
//! Verified 2026-10-17 against
//! <https://api-docs.serpstat.com/docs/serpstat-public-api/p2grcut8yurv0-get-stats>:
//! `POST https://api.serpstat.com/v4/?token=<key>` with the JSON body
//! `{"id": "<mirrored>", "method": "SerpstatLimitsProcedure.getStats"}`. The
//! page says "This method doesn't spent API credits." 200 body `{id, result:
//! {data: {max_lines, used_lines, left_lines, user_info: {user_id,
//! ga_client_Id, plan_id}, plugin_limits: {hasApiPlugin,
//! delayBetweenRequests, total, used, left}}, summary_info: {left_lines}}}`.
//! The page documents no error shape; a JSON-RPC style `error` member
//! (`{code, message}`) on a 200 is treated as a vendor error, or as a
//! rejected key when its message mentions the token, and HTTP 401/403 as a
//! rejected key.
//!
//! The key travels in the query string because that is the only documented
//! way. `net::Http` never lets the URL reach an error or a log. `user_info`
//! is never deserialized.
//!
//! The documented example is internally inconsistent (`left_lines` exceeds
//! `max_lines`); the parser reports the numbers as given.

use std::time::Duration;

use async_trait::async_trait;
use jiff::Timestamp;
use reqwest::StatusCode;
use secrecy::SecretString;
use serde::Deserialize;

use super::{count, error_message, http_status_message, malformed};
use crate::net::{Auth, Http};
use crate::provider::{Category, Provider, ProviderError, Snapshot, Unit};

pub struct Serpstat;

const ENDPOINT: &str = "https://api.serpstat.com/v4/";
const HOST: &str = "api.serpstat.com";
/// Fixed request body; the key never enters it.
const REQUEST: &str = r#"{"id":"1","method":"SerpstatLimitsProcedure.getStats"}"#;

#[derive(Deserialize)]
struct Body {
    result: Option<RpcResult>,
    error: Option<RpcError>,
}

/// Only the message, read raw to tell a rejected token from other errors
/// before the text is scrubbed (scrubbing removes the word "token").
#[derive(Deserialize)]
struct RpcError {
    message: Option<String>,
}

#[derive(Deserialize)]
struct RpcResult {
    data: Option<Data>,
}

/// Line counters and the browser-extension pool. `user_info` is left out.
#[derive(Deserialize)]
struct Data {
    max_lines: Option<f64>,
    used_lines: Option<f64>,
    left_lines: Option<f64>,
    plugin_limits: Option<Plugin>,
}

#[derive(Deserialize)]
struct Plugin {
    total: Option<f64>,
    left: Option<f64>,
}

#[async_trait]
impl Provider for Serpstat {
    fn id(&self) -> &'static str {
        "serpstat"
    }

    fn display_name(&self) -> &'static str {
        "Serpstat"
    }

    fn docs_url(&self) -> &'static str {
        "https://api-docs.serpstat.com/docs/serpstat-public-api/p2grcut8yurv0-get-stats"
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
        Category::Seo
    }

    fn min_poll_interval(&self) -> Duration {
        Duration::from_secs(60)
    }

    fn note(&self) -> Option<&'static str> {
        Some("Counts API credits (result lines). The stats call itself is free.")
    }

    async fn fetch(&self, key: &SecretString, _config: Option<&str>, http: &Http) -> Result<Snapshot, ProviderError> {
        let auth = Auth::Query { name: "token", value: key };
        let response = http.post_json(ENDPOINT, HOST, auth, &[], REQUEST).await?;
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
    if let Some(err) = parsed.error {
        if err.message.as_deref().is_some_and(|m| m.to_ascii_lowercase().contains("token")) {
            return Err(ProviderError::Unauthorized);
        }
        return Err(ProviderError::Vendor(
            error_message(body).unwrap_or_else(|| "Serpstat returned an error.".into()),
        ));
    }
    let data = parsed.result.and_then(|r| r.data).ok_or_else(malformed)?;
    let mut extra = Vec::new();
    if let Some(used) = count(data.used_lines) {
        extra.push(("Used this period".to_owned(), used.to_string()));
    }
    if let Some(p) = data.plugin_limits
        && let (Some(left), Some(total)) = (count(p.left), count(p.total))
    {
        extra.push(("Extension credits".to_owned(), format!("{left} of {total} left")));
    }
    Ok(Snapshot {
        taken_at: now,
        remaining: count(data.left_lines),
        limit: count(data.max_lines),
        period_end: None,
        spent: None,
        spend_per_hour: None,
        unit: None,
        extra,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &str = include_str!("../../tests/fixtures/serpstat_200.json");

    fn run(status: StatusCode, body: &str) -> Result<Snapshot, ProviderError> {
        parse(status, None, body.as_bytes(), Timestamp::UNIX_EPOCH)
    }

    #[test]
    fn documented_200_parses() {
        let s = run(StatusCode::OK, OK).unwrap();
        assert_eq!(s.remaining, Some(999_995));
        assert_eq!(s.limit, Some(100_000));
        assert_eq!(s.period_end, None);
        assert_eq!(
            s.extra,
            vec![
                ("Used this period".to_owned(), "5".to_owned()),
                ("Extension credits".to_owned(), "9990 of 10000 left".to_owned()),
            ]
        );
    }

    #[test]
    fn user_info_does_not_survive() {
        let s = run(StatusCode::OK, OK).unwrap();
        let dump = format!("{s:?}");
        for echoed in ["555", "12434534.544545", "2028", "\"123\""] {
            assert!(!dump.contains(echoed), "{echoed}: {dump}");
        }
    }

    #[test]
    fn error_member_on_200_maps() {
        let bad_token = r#"{"id":"1","error":{"code":32001,"message":"Invalid token"}}"#;
        assert_eq!(run(StatusCode::OK, bad_token), Err(ProviderError::Unauthorized));
        let other = r#"{"id":"1","error":{"code":32000,"message":"Something else"}}"#;
        assert_eq!(run(StatusCode::OK, other), Err(ProviderError::Vendor("Something else".into())));
    }

    #[test]
    fn statuses_map() {
        assert_eq!(run(StatusCode::UNAUTHORIZED, ""), Err(ProviderError::Unauthorized));
        assert_eq!(run(StatusCode::FORBIDDEN, ""), Err(ProviderError::Unauthorized));
        assert_eq!(
            run(StatusCode::TOO_MANY_REQUESTS, ""),
            Err(ProviderError::RateLimited {
                retry_after: None,
                message: None
            })
        );
        assert_eq!(
            run(StatusCode::SERVICE_UNAVAILABLE, "<html>"),
            Err(ProviderError::Vendor("The vendor returned HTTP 503.".into()))
        );
        assert_eq!(run(StatusCode::OK, r#"{"id":"1"}"#), Err(malformed()));
    }

    #[test]
    fn request_body_is_the_documented_call() {
        let v: serde_json::Value = serde_json::from_str(REQUEST).unwrap();
        assert_eq!(v["method"], "SerpstatLimitsProcedure.getStats");
    }
}
