//! DataForSEO account balance.
//!
//! Verified 2026-10-17 against <https://docs.dataforseo.com/v3/appendix/user_data/>:
//! `GET https://api.dataforseo.com/v3/appendix/user_data` with HTTP Basic
//! auth, `Authorization: Basic base64(login:password)`. The page is marked
//! "Your account will not be charged for using this API". 200 body
//! `{version, status_code, status_message, time, cost, tasks_count,
//! tasks_error, tasks: [{id, status_code, status_message, time, cost,
//! result_count, path, data, result: [{login, timezone, rates, money: {total,
//! balance, limits: {day, minute}, statistics: {day, minute}}, price, ...}]}]}`
//! with every amount in US dollars.
//!
//! Errors, from <https://docs.dataforseo.com/v3/appendix/errors/> (read the
//! same day): the servers "always return the 200 HTTP response code" except
//! 401 Unauthorized, 402 Payment Required, 404 Not Found and 500 Internal
//! Server Error. Otherwise `status_code` and `status_message` carry the
//! outcome: 20000 is success, 40100 "you are not authorized to access this
//! resource", 40200 "payment required", 40202 "the rate-limit per minute has
//! been exceeded".
//!
//! The user_data page's example response is drawn by a script and did not
//! come through as text, so `tests/fixtures/dataforseo_200.json` is the
//! smallest body that matches the documented field table, with the `money`
//! figures chosen for the test; `dataforseo_200_40100.json` pairs the
//! documented 40100 message with the documented envelope. The field table
//! describes `money.limits.day`/`minute` as nested per-function tables; the
//! parser reads them only when they are plain numbers.
//! TODO(verify): the live shape of `money.limits` and `money.statistics`.
//!
//! The key is pasted as `login:password`. The app base64-encodes it straight
//! into a `SecretString` and sends it as the Basic credential; a key without
//! a colon is refused before any request is made. `login`, `timezone`, the
//! task `id`, `rates` and `price` are never deserialized.

use std::time::Duration;

use async_trait::async_trait;
use base64::prelude::*;
use jiff::Timestamp;
use reqwest::StatusCode;
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;

use super::{MAX_VENDOR_TEXT, cents, dollars, error_message, http_status_message, malformed};
use crate::net::{Auth, Http};
use crate::provider::{Category, Provider, ProviderError, Snapshot, Unit};
use crate::redact;

pub struct DataForSeo;

const ENDPOINT: &str = "https://api.dataforseo.com/v3/appendix/user_data";
const HOST: &str = "api.dataforseo.com";
const OK_CODE: i64 = 20000;
const UNAUTHORIZED_CODE: i64 = 40100;
const RATE_LIMIT_CODE: i64 = 40202;

#[derive(Deserialize)]
struct Body {
    status_code: Option<i64>,
    status_message: Option<String>,
    tasks: Option<Vec<Task>>,
}

#[derive(Deserialize)]
struct Task {
    status_code: Option<i64>,
    status_message: Option<String>,
    result: Option<Vec<ResultItem>>,
}

/// Only the money section. `login` and `timezone` are left out.
#[derive(Deserialize)]
struct ResultItem {
    money: Option<Money>,
}

#[derive(Deserialize)]
struct Money {
    total: Option<f64>,
    balance: Option<f64>,
    limits: Option<Grouped>,
    statistics: Option<Grouped>,
}

/// Documented as either a number or a per-function table; only a number is read.
#[derive(Deserialize)]
struct Grouped {
    day: Option<serde_json::Value>,
    minute: Option<serde_json::Value>,
}

#[async_trait]
impl Provider for DataForSeo {
    fn id(&self) -> &'static str {
        "dataforseo"
    }

    fn display_name(&self) -> &'static str {
        "DataForSEO"
    }

    fn docs_url(&self) -> &'static str {
        "https://docs.dataforseo.com/v3/appendix/user_data/"
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
        Category::Seo
    }

    fn min_poll_interval(&self) -> Duration {
        Duration::from_secs(60)
    }

    fn note(&self) -> Option<&'static str> {
        Some("Paste the API login and password as one key in the form login:password. The balance call is free.")
    }

    async fn fetch(&self, key: &SecretString, _config: Option<&str>, http: &Http) -> Result<Snapshot, ProviderError> {
        let credential = basic_credential(key)?;
        let auth = Auth::Header {
            name: "authorization",
            prefix: "Basic ",
            value: &credential,
        };
        let response = http.get(ENDPOINT, HOST, auth).await?;
        parse(response.status, response.retry_after, &response.body, Timestamp::now())
    }
}

/// `base64(login:password)` as a secret. The encoded string is moved into
/// the `SecretString`, which zeroizes it on drop; no plain copy is kept.
pub(crate) fn basic_credential(key: &SecretString) -> Result<SecretString, ProviderError> {
    let raw = key.expose_secret();
    let well_formed = raw.find(':').is_some_and(|i| i > 0 && i + 1 < raw.len());
    if !well_formed {
        return Err(ProviderError::Unsupported(
            "DataForSEO needs the API login and password pasted as one key in the form login:password.".into(),
        ));
    }
    Ok(SecretString::from(BASE64_STANDARD.encode(raw.as_bytes())))
}

fn status_error(code: Option<i64>, message: Option<&str>) -> Option<ProviderError> {
    let code = code?;
    if code == OK_CODE {
        return None;
    }
    let message = message.map(|m| redact::vendor_text(m, MAX_VENDOR_TEXT)).filter(|m| !m.is_empty());
    Some(match code {
        UNAUTHORIZED_CODE => ProviderError::Unauthorized,
        RATE_LIMIT_CODE => ProviderError::RateLimited {
            retry_after: None,
            message,
        },
        _ => ProviderError::Vendor(message.unwrap_or_else(|| format!("DataForSEO reported status {code}."))),
    })
}

fn amount(v: Option<&serde_json::Value>) -> Option<u64> {
    cents(v.and_then(serde_json::Value::as_f64))
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
    if let Some(e) = status_error(parsed.status_code, parsed.status_message.as_deref()) {
        return Err(e);
    }
    let task = parsed.tasks.and_then(|mut t| t.drain(..).next()).ok_or_else(malformed)?;
    if let Some(e) = status_error(task.status_code, task.status_message.as_deref()) {
        return Err(e);
    }
    let money = task
        .result
        .and_then(|mut r| r.drain(..).next())
        .and_then(|r| r.money)
        .ok_or_else(malformed)?;
    let mut extra = Vec::new();
    if let Some(total) = cents(money.total) {
        extra.push(("Deposited in total".to_owned(), dollars(total)));
    }
    if let Some(today) = money.statistics.as_ref().and_then(|s| amount(s.day.as_ref())) {
        extra.push(("Spent today".to_owned(), dollars(today)));
    }
    if let Some(limits) = &money.limits {
        if let Some(day) = amount(limits.day.as_ref()) {
            extra.push(("Daily spending limit".to_owned(), dollars(day)));
        }
        if let Some(minute) = amount(limits.minute.as_ref()) {
            extra.push(("Per-minute spending limit".to_owned(), dollars(minute)));
        }
    }
    Ok(Snapshot {
        taken_at: now,
        remaining: cents(money.balance),
        limit: None,
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

    const OK: &str = include_str!("../../tests/fixtures/dataforseo_200.json");
    const BAD_LOGIN: &str = include_str!("../../tests/fixtures/dataforseo_200_40100.json");

    fn run(status: StatusCode, body: &str) -> Result<Snapshot, ProviderError> {
        parse(status, None, body.as_bytes(), Timestamp::UNIX_EPOCH)
    }

    #[test]
    fn documented_schema_200_parses_in_cents() {
        let s = run(StatusCode::OK, OK).unwrap();
        assert_eq!(s.remaining, Some(51_234));
        assert_eq!(s.limit, None);
        assert_eq!(s.period_end, None);
        assert_eq!(
            s.extra,
            vec![
                ("Deposited in total".to_owned(), "$1000.00".to_owned()),
                ("Spent today".to_owned(), "$12.50".to_owned()),
                ("Daily spending limit".to_owned(), "$500.00".to_owned()),
                ("Per-minute spending limit".to_owned(), "$50.00".to_owned()),
            ]
        );
    }

    #[test]
    fn login_and_task_id_do_not_survive() {
        let s = run(StatusCode::OK, OK).unwrap();
        let dump = format!("{s:?}");
        for echoed in ["demo@example.com", "10171630-1535-0411-0000-6e0d5a0f8a9c", "0.1.20250526"] {
            assert!(!dump.contains(echoed), "{echoed}: {dump}");
        }
    }

    #[test]
    fn nested_limit_tables_are_skipped_not_fatal() {
        let body = OK.replace(
            r#""limits": { "day": 500, "minute": 50 }"#,
            r#""limits": { "day": { "serp": 10 }, "minute": {} }"#,
        );
        let s = run(StatusCode::OK, &body).unwrap();
        assert_eq!(s.remaining, Some(51_234));
        assert!(!s.extra.iter().any(|(k, _)| k.contains("limit")));
    }

    #[test]
    fn documented_internal_codes_map() {
        assert_eq!(run(StatusCode::OK, BAD_LOGIN), Err(ProviderError::Unauthorized));
        let limited = BAD_LOGIN.replace("40100", "40202").replace(
            "you are not authorized to access this resource. see your login details here: https://app.dataforseo.com/api-dashboard .",
            "the rate-limit per minute has been exceeded.",
        );
        assert_eq!(
            run(StatusCode::OK, &limited),
            Err(ProviderError::RateLimited {
                retry_after: None,
                message: Some("the rate-limit per minute has been exceeded.".into())
            })
        );
        let task_failed = OK.replacen(
            r#""status_code": 20000,
      "status_message": "Ok.""#,
            r#""status_code": 40200,
      "status_message": "payment required.""#,
            1,
        );
        assert_eq!(
            run(StatusCode::OK, &task_failed),
            Err(ProviderError::Vendor("payment required.".into()))
        );
    }

    #[test]
    fn documented_http_statuses_map() {
        assert_eq!(run(StatusCode::UNAUTHORIZED, ""), Err(ProviderError::Unauthorized));
        assert_eq!(
            run(StatusCode::PAYMENT_REQUIRED, ""),
            Err(ProviderError::Vendor("The vendor returned HTTP 402.".into()))
        );
        assert_eq!(
            run(StatusCode::NOT_FOUND, ""),
            Err(ProviderError::Vendor("The vendor returned HTTP 404.".into()))
        );
        assert_eq!(
            run(StatusCode::INTERNAL_SERVER_ERROR, ""),
            Err(ProviderError::Vendor("The vendor returned HTTP 500.".into()))
        );
        assert_eq!(
            run(StatusCode::TOO_MANY_REQUESTS, ""),
            Err(ProviderError::RateLimited {
                retry_after: None,
                message: None
            })
        );
        assert_eq!(run(StatusCode::OK, "{}"), Err(malformed()));
    }

    #[test]
    fn credential_is_basic_base64_of_login_and_password() {
        let key = SecretString::from("login:password".to_owned());
        assert_eq!(basic_credential(&key).unwrap().expose_secret(), "bG9naW46cGFzc3dvcmQ=");
    }

    #[test]
    fn key_without_a_colon_is_refused_before_any_request() {
        for bad in ["nocolon", ":password", "login:", ""] {
            let key = SecretString::from(bad.to_owned());
            assert!(
                matches!(basic_credential(&key), Err(ProviderError::Unsupported(m)) if m.contains("login:password")),
                "{bad}"
            );
        }
    }
}
