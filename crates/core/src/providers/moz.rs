//! Moz API quota rows.
//!
//! Verified 2026-10-17 against <https://moz.com/api/docs/methods/QuotaLookupAction>:
//! `POST https://api.moz.com/jsonrpc` with the header `x-moz-token: <key>`
//! and the JSON-RPC 2.0 body `{"jsonrpc": "2.0", "id": "<opaque>", "method":
//! "quota.lookup", "params": {"data": {"path": "api.limits.data.rows"}}}`.
//! The page's "Quota Usage" section says the method "Does not require any
//! quota", so the call is free. The documented result is `quota: {path,
//! account_id, allotted, used, reset, report, overage, period_start,
//! period_reset}` with the period bounds as Unix seconds. The page documents
//! no error shape; a JSON-RPC 2.0 `error` member (`{code, message}`) is
//! treated as a vendor error, and HTTP 401/403 as a rejected key.
//!
//! The docs' example `id` is a UUID. The app sends a fixed opaque id because
//! the server only mirrors it back. TODO(verify): the page states no minimum
//! id length.
//!
//! The documented example shows the bare `{quota: ...}` object, while a
//! JSON-RPC response wraps it as `{result: {quota: ...}}`; the parser accepts
//! both. `account_id` and `path` are never deserialized.

use std::time::Duration;

use async_trait::async_trait;
use jiff::Timestamp;
use reqwest::StatusCode;
use secrecy::SecretString;
use serde::Deserialize;
use serde::de::IgnoredAny;

use super::{count, error_message, http_status_message, malformed};
use crate::net::{Auth, Http};
use crate::provider::{Category, Provider, ProviderError, Snapshot, Unit};

pub struct Moz;

const ENDPOINT: &str = "https://api.moz.com/jsonrpc";
const HOST: &str = "api.moz.com";
/// The quota behind the production Data API. Fixed; the key never enters it.
const REQUEST: &str =
    r#"{"jsonrpc":"2.0","id":"mr-usage-quota-lookup-0001","method":"quota.lookup","params":{"data":{"path":"api.limits.data.rows"}}}"#;

#[derive(Deserialize)]
struct Body {
    result: Option<RpcResult>,
    quota: Option<Quota>,
    error: Option<IgnoredAny>,
}

#[derive(Deserialize)]
struct RpcResult {
    quota: Option<Quota>,
}

/// Counters, the overage flag and the period bounds. `path` and
/// `account_id` are left out.
#[derive(Deserialize)]
struct Quota {
    allotted: Option<f64>,
    used: Option<f64>,
    overage: Option<bool>,
    period_start: Option<i64>,
    period_reset: Option<i64>,
}

#[async_trait]
impl Provider for Moz {
    fn id(&self) -> &'static str {
        "moz"
    }

    fn display_name(&self) -> &'static str {
        "Moz"
    }

    fn docs_url(&self) -> &'static str {
        "https://moz.com/api/docs/methods/QuotaLookupAction"
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
        Some("Counts rows of the production Data API quota (api.limits.data.rows). The lookup itself uses no quota.")
    }

    async fn fetch(&self, key: &SecretString, _config: Option<&str>, http: &Http) -> Result<Snapshot, ProviderError> {
        let auth = Auth::Header {
            name: "x-moz-token",
            prefix: "",
            value: key,
        };
        let response = http.post_json(ENDPOINT, HOST, auth, &[], REQUEST).await?;
        parse(response.status, response.retry_after, &response.body, Timestamp::now())
    }
}

fn unix_seconds(secs: Option<i64>) -> Option<Timestamp> {
    secs.and_then(|s| Timestamp::from_second(s).ok())
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
    if parsed.error.is_some() {
        return Err(ProviderError::Vendor(
            error_message(body).unwrap_or_else(|| "Moz returned a JSON-RPC error.".into()),
        ));
    }
    let quota = parsed.result.and_then(|r| r.quota).or(parsed.quota).ok_or_else(malformed)?;
    let allotted = count(quota.allotted);
    let used = count(quota.used);
    let remaining = match (allotted, used) {
        (Some(a), Some(u)) => Some(a.saturating_sub(u)),
        _ => None,
    };
    let mut extra = Vec::new();
    if let Some(start) = unix_seconds(quota.period_start) {
        extra.push(("Period start".to_owned(), start.strftime("%Y-%m-%d").to_string()));
    }
    if let Some(overage) = quota.overage {
        let word = if overage { "enabled" } else { "not enabled" };
        extra.push(("Overage".to_owned(), word.to_owned()));
    }
    Ok(Snapshot {
        taken_at: now,
        remaining,
        limit: allotted,
        period_end: unix_seconds(quota.period_reset),
        spent: None,
        spend_per_hour: None,
        unit: None,
        extra,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &str = include_str!("../../tests/fixtures/moz_200.json");

    fn run(status: StatusCode, body: &str) -> Result<Snapshot, ProviderError> {
        parse(status, None, body.as_bytes(), Timestamp::UNIX_EPOCH)
    }

    #[test]
    fn documented_200_parses() {
        let s = run(StatusCode::OK, OK).unwrap();
        assert_eq!(s.remaining, Some(50));
        assert_eq!(s.limit, Some(100));
        assert_eq!(s.period_end.unwrap().to_string(), "2024-06-01T07:00:00Z");
        assert_eq!(
            s.extra,
            vec![
                ("Period start".to_owned(), "2024-05-01".to_owned()),
                ("Overage".to_owned(), "not enabled".to_owned()),
            ]
        );
    }

    #[test]
    fn jsonrpc_envelope_parses_the_same() {
        let wrapped = format!(r#"{{"jsonrpc":"2.0","id":"mr-usage-quota-lookup-0001","result":{OK}}}"#);
        assert_eq!(run(StatusCode::OK, &wrapped), run(StatusCode::OK, OK));
    }

    #[test]
    fn account_id_and_path_do_not_survive() {
        let s = run(StatusCode::OK, OK).unwrap();
        let dump = format!("{s:?}");
        assert!(!dump.contains("123456789"), "{dump}");
        assert!(!dump.contains("api.limits"), "{dump}");
    }

    #[test]
    fn jsonrpc_error_on_200_is_a_vendor_error() {
        let body = r#"{"jsonrpc":"2.0","id":"x","error":{"code":-32600,"message":"Invalid Request"}}"#;
        assert_eq!(run(StatusCode::OK, body), Err(ProviderError::Vendor("Invalid Request".into())));
    }

    #[test]
    fn statuses_map() {
        assert_eq!(run(StatusCode::UNAUTHORIZED, ""), Err(ProviderError::Unauthorized));
        assert_eq!(run(StatusCode::FORBIDDEN, ""), Err(ProviderError::Unauthorized));
        assert_eq!(
            parse(
                StatusCode::TOO_MANY_REQUESTS,
                Some(Duration::from_secs(3)),
                b"",
                Timestamp::UNIX_EPOCH
            ),
            Err(ProviderError::RateLimited {
                retry_after: Some(Duration::from_secs(3)),
                message: None
            })
        );
        assert_eq!(
            run(StatusCode::INTERNAL_SERVER_ERROR, "<html>"),
            Err(ProviderError::Vendor("The vendor returned HTTP 500.".into()))
        );
        assert_eq!(run(StatusCode::OK, "{}"), Err(malformed()));
    }

    #[test]
    fn request_body_is_well_formed_and_carries_no_key() {
        let v: serde_json::Value = serde_json::from_str(REQUEST).unwrap();
        assert_eq!(v["method"], "quota.lookup");
        assert_eq!(v["params"]["data"]["path"], "api.limits.data.rows");
        assert!(v["id"].as_str().unwrap().len() >= 24);
    }
}
