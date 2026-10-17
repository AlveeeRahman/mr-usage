//! Majestic API resource units.
//!
//! Verified 2026-10-17 against
//! <https://developer-support.majestic.com/api/commands/get-subscription-info.shtml>
//! and the API Reference Guide <https://developer-support.majestic.com/api/>:
//! `GET https://api.majestic.com/api/json?cmd=GetSubscriptionInfo&app_api_key=<key>`.
//! The command page says "this command uses no resources", so the call is
//! free. The guide names `https://api.majestic.com/...` as the live endpoint
//! and `https://developer.majestic.com/...` as the development sandbox; the
//! command page's sample URLs use the sandbox, but the app uses the live host
//! so the figures belong to the real subscription.
//!
//! 200 body: `Code` ("OK" on success), `ErrorMessage`, `FullError`, the unit
//! totals `TotalAnalysisResUnits`, `TotalRetrievalResUnits`,
//! `TotalIndexItemInfoResUnits` and `TotalDetailedReportsPerPeriodRemaining`,
//! `SubscriptionIsActive`, and `DataTables.Subscriptions.Data[]` with
//! `Starts`, `Expires` ("YYYY-MM-DD HH:MM:SS") and per-subscription units.
//! Errors arrive as HTTP 200 with a `Code` other than "OK" and the text in
//! `ErrorMessage`; the pages list no specific codes. A code or message that
//! mentions the key or authorization is reported as a rejected key, anything
//! else as a vendor error. TODO(verify): the exact `Code` for a bad key.
//!
//! The key travels in the query string because that is the documented way;
//! `net::Http` never lets the URL reach an error or a log. The headline is
//! `TotalRetrievalResUnits`: retrieval units are spent whenever backlinks
//! are returned, which is what most commands do. Analysis and index-item
//! units are extras. The renewal date is the earliest `Expires`, when the
//! first block of units lapses. `ServerName`, `UniqueIndexID`, `CustomerID`,
//! `SubscriptionID`, plan codes and titles are never deserialized.

use std::time::Duration;

use async_trait::async_trait;
use jiff::Timestamp;
use reqwest::StatusCode;
use secrecy::SecretString;
use serde::Deserialize;

use super::{MAX_VENDOR_TEXT, count, error_message, http_status_message, malformed};
use crate::net::{Auth, Http};
use crate::provider::{Category, Provider, ProviderError, Snapshot, Unit};
use crate::redact;

pub struct Majestic;

const ENDPOINT: &str = "https://api.majestic.com/api/json?cmd=GetSubscriptionInfo";
const HOST: &str = "api.majestic.com";

/// Status, the unit totals and the subscription table. Server, index, plan
/// and customer identifiers are left out.
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Body {
    code: Option<String>,
    error_message: Option<String>,
    total_analysis_res_units: Option<f64>,
    total_retrieval_res_units: Option<f64>,
    total_index_item_info_res_units: Option<f64>,
    total_detailed_reports_per_period_remaining: Option<f64>,
    subscription_is_active: Option<serde_json::Value>,
    data_tables: Option<DataTables>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct DataTables {
    subscriptions: Option<Table>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Table {
    data: Option<Vec<Subscription>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Subscription {
    expires: Option<String>,
}

#[async_trait]
impl Provider for Majestic {
    fn id(&self) -> &'static str {
        "majestic"
    }

    fn display_name(&self) -> &'static str {
        "Majestic"
    }

    fn docs_url(&self) -> &'static str {
        "https://developer-support.majestic.com/api/commands/get-subscription-info.shtml"
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
        Some(
            "Headline is retrieval units; analysis and index-item units are listed below. The renewal date is the earliest subscription expiry.",
        )
    }

    async fn fetch(&self, key: &SecretString, _config: Option<&str>, http: &Http) -> Result<Snapshot, ProviderError> {
        let auth = Auth::Query {
            name: "app_api_key",
            value: key,
        };
        let response = http.get(ENDPOINT, HOST, auth).await?;
        parse(response.status, response.retry_after, &response.body, Timestamp::now())
    }
}

/// `2017-10-25 15:05:40` read as UTC.
fn parse_utc_datetime(raw: &str) -> Option<Timestamp> {
    let civil: jiff::civil::DateTime = raw.trim().replacen(' ', "T", 1).parse().ok()?;
    civil.to_zoned(jiff::tz::TimeZone::UTC).ok().map(|z| z.timestamp())
}

/// `1`, `"1"` or `true` means active; the docs show a number in JSON and a
/// string in XML.
fn is_active(v: &serde_json::Value) -> Option<bool> {
    match v {
        serde_json::Value::Bool(b) => Some(*b),
        serde_json::Value::Number(n) => Some(n.as_f64()? != 0.0),
        serde_json::Value::String(s) => Some(s == "1" || s.eq_ignore_ascii_case("true")),
        _ => None,
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
    let code = parsed.code.ok_or_else(malformed)?;
    if code != "OK" {
        let message = parsed
            .error_message
            .as_deref()
            .map(str::trim)
            .filter(|m| !m.is_empty())
            .unwrap_or(&code);
        let lower = format!("{code} {message}").to_ascii_lowercase();
        if lower.contains("key") || lower.contains("auth") {
            return Err(ProviderError::Unauthorized);
        }
        let text = redact::vendor_text(message, MAX_VENDOR_TEXT);
        return Err(ProviderError::Vendor(if text.is_empty() {
            "Majestic returned an error.".into()
        } else {
            text
        }));
    }
    let mut extra = Vec::new();
    if let Some(n) = count(parsed.total_analysis_res_units) {
        extra.push(("Analysis units".to_owned(), n.to_string()));
    }
    if let Some(n) = count(parsed.total_index_item_info_res_units) {
        extra.push(("Index item units".to_owned(), n.to_string()));
    }
    if let Some(n) = count(parsed.total_detailed_reports_per_period_remaining) {
        extra.push(("Detailed reports left".to_owned(), n.to_string()));
    }
    let subscriptions = parsed
        .data_tables
        .and_then(|d| d.subscriptions)
        .and_then(|t| t.data)
        .unwrap_or_default();
    if !subscriptions.is_empty() {
        extra.push(("Subscriptions".to_owned(), subscriptions.len().to_string()));
    }
    if let Some(active) = parsed.subscription_is_active.as_ref().and_then(is_active) {
        let word = if active { "yes" } else { "no" };
        extra.push(("Active subscription".to_owned(), word.to_owned()));
    }
    let period_end = subscriptions
        .iter()
        .filter_map(|s| s.expires.as_deref().and_then(parse_utc_datetime))
        .min();
    Ok(Snapshot {
        taken_at: now,
        remaining: count(parsed.total_retrieval_res_units),
        limit: None,
        period_end,
        spent: None,
        spend_per_hour: None,
        unit: None,
        extra,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &str = include_str!("../../tests/fixtures/majestic_200.json");

    fn run(status: StatusCode, body: &str) -> Result<Snapshot, ProviderError> {
        parse(status, None, body.as_bytes(), Timestamp::UNIX_EPOCH)
    }

    #[test]
    fn documented_200_parses() {
        let s = run(StatusCode::OK, OK).unwrap();
        assert_eq!(s.remaining, Some(19_462_890));
        assert_eq!(s.limit, None);
        assert_eq!(s.period_end.unwrap().to_string(), "2017-10-25T15:05:40Z");
        assert_eq!(
            s.extra,
            vec![
                ("Analysis units".to_owned(), "97094051".to_owned()),
                ("Index item units".to_owned(), "504986".to_owned()),
                ("Detailed reports left".to_owned(), "1010".to_owned()),
                ("Subscriptions".to_owned(), "2".to_owned()),
                ("Active subscription".to_owned(), "yes".to_owned()),
            ]
        );
    }

    #[test]
    fn identifiers_do_not_survive() {
        let s = run(StatusCode::OK, OK).unwrap();
        let dump = format!("{s:?}");
        for echoed in [
            "SHADOJAGUAR",
            "20170904134254",
            "895472",
            "61975",
            "SUB_MAJESTIC_250",
            "Majestic Platinum",
            "1.0.6495",
        ] {
            assert!(!dump.contains(echoed), "{echoed}: {dump}");
        }
    }

    #[test]
    fn error_code_mentioning_the_key_is_unauthorized() {
        let body = r#"{"Code":"InvalidAPIKey","ErrorMessage":"","FullError":""}"#;
        assert_eq!(run(StatusCode::OK, body), Err(ProviderError::Unauthorized));
        let body = r#"{"Code":"Error","ErrorMessage":"The supplied API key is not valid.","FullError":""}"#;
        assert_eq!(run(StatusCode::OK, body), Err(ProviderError::Unauthorized));
    }

    #[test]
    fn other_error_codes_are_vendor_errors() {
        let body = r#"{"Code":"Error","ErrorMessage":"Index unavailable","FullError":"stack"}"#;
        assert_eq!(run(StatusCode::OK, body), Err(ProviderError::Vendor("Index unavailable".into())));
        let body = r#"{"Code":"Busy","ErrorMessage":"","FullError":""}"#;
        assert_eq!(run(StatusCode::OK, body), Err(ProviderError::Vendor("Busy".into())));
    }

    #[test]
    fn missing_code_is_malformed() {
        assert_eq!(run(StatusCode::OK, "{}"), Err(malformed()));
        assert_eq!(run(StatusCode::OK, "<html>"), Err(malformed()));
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
            run(StatusCode::BAD_GATEWAY, ""),
            Err(ProviderError::Vendor("The vendor returned HTTP 502.".into()))
        );
    }
}
