//! DeepSeek account balance.
//!
//! Verified 2026-10-17 against <https://api-docs.deepseek.com/api/get-user-balance>:
//! `GET https://api.deepseek.com/user/balance`, `Authorization: Bearer <key>`.
//! 200 body `{is_available: bool, balance_infos: [{currency: "CNY" | "USD",
//! total_balance, granted_balance, topped_up_balance}]}` where every amount
//! is a decimal string in whole currency units ("110.00"). `total_balance`
//! is the sum of the granted (promotional, expiring) and topped-up balances.
//! The currency is per account, so the snapshot overrides the unit to
//! `Unit::Cny` or `Unit::Usd` from the first entry.
//!
//! Cost: the page lists no price and the call is not a model request; the
//! pricing page prices tokens only. It consumes no balance.
//!
//! Errors: the balance page documents only the 200 shape. The platform-wide
//! error table (<https://api-docs.deepseek.com/quick_start/error_codes>,
//! read the same day) lists 400 "Invalid Format", 401 "Authentication
//! Fails", 402 "Insufficient Balance", 422, 429 "Rate Limit Reached", 500
//! and 503, but shows no body. 401 maps to `Unauthorized`, 429 to
//! `RateLimited`, the rest to `Vendor` with whatever `{"error"|"message"}`
//! text the body carries (the API is OpenAI-compatible, so an
//! `{"error": {"message", ...}}` body is the expected form but is not
//! documented).
//!
//! The fixture `deepseek_200.json` is the page's example verbatim. No error
//! example body is published, so the error tests use the status alone and a
//! small body in the undocumented OpenAI form, which is a smallest-body
//! guess rather than a copy.

use std::time::Duration;

use async_trait::async_trait;
use jiff::Timestamp;
use reqwest::StatusCode;
use secrecy::SecretString;
use serde::Deserialize;

use super::{cents, dollars, error_message, http_status_message, malformed};
use crate::net::{Auth, Http};
use crate::provider::{Category, Provider, ProviderError, Snapshot, Unit};

pub struct DeepSeek;

const ENDPOINT: &str = "https://api.deepseek.com/user/balance";
const HOST: &str = "api.deepseek.com";

#[derive(Deserialize)]
struct Body {
    is_available: Option<bool>,
    #[serde(default)]
    balance_infos: Vec<BalanceInfo>,
}

/// Amounts only. The currency is read as an enum so no vendor string is kept.
#[derive(Deserialize)]
struct BalanceInfo {
    currency: Option<Currency>,
    total_balance: Option<String>,
    granted_balance: Option<String>,
    topped_up_balance: Option<String>,
}

#[derive(Deserialize, Clone, Copy)]
enum Currency {
    #[serde(rename = "CNY")]
    Cny,
    #[serde(rename = "USD")]
    Usd,
    #[serde(other)]
    Other,
}

/// A documented decimal string such as `"110.00"` as a number.
fn decimal(raw: Option<&str>) -> Option<f64> {
    raw?.trim().parse::<f64>().ok()
}

/// `¥12.34` or `$12.34` from minor units.
fn money(unit: Unit, minor: u64) -> String {
    match unit {
        Unit::Cny => format!("¥{}.{:02}", minor / 100, minor % 100),
        _ => dollars(minor),
    }
}

#[async_trait]
impl Provider for DeepSeek {
    fn id(&self) -> &'static str {
        "deepseek"
    }

    fn display_name(&self) -> &'static str {
        "DeepSeek"
    }

    fn docs_url(&self) -> &'static str {
        "https://api-docs.deepseek.com/api/get-user-balance"
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
        Some("Shows the account balance in the currency DeepSeek bills you in, yuan or dollars.")
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
    let mut snapshot = Snapshot::at(now);
    if let Some(info) = parsed.balance_infos.first() {
        let unit = match info.currency {
            Some(Currency::Cny) => Unit::Cny,
            Some(Currency::Usd) | None => Unit::Usd,
            Some(Currency::Other) => return Err(malformed()),
        };
        snapshot.unit = Some(unit);
        snapshot.remaining = cents(decimal(info.total_balance.as_deref()));
        if let Some(granted) = cents(decimal(info.granted_balance.as_deref())) {
            snapshot.extra.push(("Granted".to_owned(), money(unit, granted)));
        }
        if let Some(topped) = cents(decimal(info.topped_up_balance.as_deref())) {
            snapshot.extra.push(("Topped up".to_owned(), money(unit, topped)));
        }
    }
    if parsed.is_available == Some(false) {
        snapshot.extra.push(("Inference".to_owned(), "blocked, balance too low".to_owned()));
    }
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &str = include_str!("../../tests/fixtures/deepseek_200.json");

    fn run(status: StatusCode, body: &str) -> Result<Snapshot, ProviderError> {
        parse(status, None, body.as_bytes(), Timestamp::UNIX_EPOCH)
    }

    #[test]
    fn documented_200_parses_in_fen_with_cny_override() {
        let s = run(StatusCode::OK, OK).unwrap();
        assert_eq!(s.remaining, Some(11000));
        assert_eq!(s.limit, None);
        assert_eq!(s.unit, Some(Unit::Cny));
        assert_eq!(
            s.extra,
            vec![
                ("Granted".to_owned(), "¥10.00".to_owned()),
                ("Topped up".to_owned(), "¥100.00".to_owned()),
            ]
        );
    }

    #[test]
    fn usd_account_overrides_to_dollars() {
        let body = OK.replace("\"CNY\"", "\"USD\"");
        let s = run(StatusCode::OK, &body).unwrap();
        assert_eq!(s.unit, Some(Unit::Usd));
        assert_eq!(s.remaining, Some(11000));
        assert!(s.extra.contains(&("Granted".to_owned(), "$10.00".to_owned())));
    }

    #[test]
    fn unavailable_balance_adds_a_caveat() {
        let body = OK.replace("\"is_available\": true", "\"is_available\": false");
        let s = run(StatusCode::OK, &body).unwrap();
        assert!(s.extra.contains(&("Inference".to_owned(), "blocked, balance too low".to_owned())));
    }

    #[test]
    fn identifiers_do_not_survive() {
        let s = run(StatusCode::OK, OK).unwrap();
        let dump = format!("{s:?}");
        for echoed in ["CNY", "currency", "is_available"] {
            assert!(!dump.contains(echoed), "{dump}");
        }
    }

    #[test]
    fn documented_errors_map() {
        assert_eq!(run(StatusCode::UNAUTHORIZED, ""), Err(ProviderError::Unauthorized));
        assert_eq!(run(StatusCode::FORBIDDEN, ""), Err(ProviderError::Unauthorized));
        let body = r#"{"error":{"message":"Rate limit reached","type":"rate_limit_error","param":null,"code":"rate_limit_reached"}}"#;
        assert_eq!(
            parse(
                StatusCode::TOO_MANY_REQUESTS,
                Some(Duration::from_secs(3)),
                body.as_bytes(),
                Timestamp::UNIX_EPOCH
            ),
            Err(ProviderError::RateLimited {
                retry_after: Some(Duration::from_secs(3)),
                message: Some("Rate limit reached".into())
            })
        );
        let body =
            r#"{"error":{"message":"Insufficient Balance","type":"invalid_request_error","param":null,"code":"invalid_request_error"}}"#;
        assert_eq!(
            run(StatusCode::PAYMENT_REQUIRED, body),
            Err(ProviderError::Vendor("Insufficient Balance".into()))
        );
        assert_eq!(
            run(StatusCode::SERVICE_UNAVAILABLE, "<html>"),
            Err(ProviderError::Vendor("The vendor returned HTTP 503.".into()))
        );
    }

    #[test]
    fn unparseable_200_is_malformed() {
        assert_eq!(run(StatusCode::OK, "not json"), Err(malformed()));
        assert_eq!(run(StatusCode::OK, &OK.replace("\"CNY\"", "\"EUR\"")), Err(malformed()));
    }
}
