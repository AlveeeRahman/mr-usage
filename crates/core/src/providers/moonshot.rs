//! Moonshot (Kimi) account balance.
//!
//! Verified 2026-10-17 against <https://platform.kimi.ai/docs/api/balance>:
//! `GET https://api.moonshot.ai/v1/users/me/balance`, `Authorization: Bearer
//! <key>`. 200 body `{code: 0, data: {available_balance, voucher_balance,
//! cash_balance}, scode: "0x0", status: true}` with the three amounts as
//! JSON numbers in US dollars on this host. `available_balance` is cash plus
//! vouchers; `cash_balance` may be negative when the account owes money, and
//! then `available_balance` equals `voucher_balance`. Inference stops when
//! `available_balance` is at or below zero. `code` is 0 on success.
//!
//! Cost: the page lists no price and the call is not a model request. It
//! consumes no balance.
//!
//! Errors: the page documents 401 and 500 with the body
//! `{"error": {"message", "type", "code"}}`, shown only as a schema
//! placeholder. The page also says keys from `platform.kimi.com` (the China
//! platform, `api.moonshot.cn`) are independent and fail with 401 here.
//!
//! The fixture `moonshot_200.json` is the page's "Response Example"
//! verbatim; `moonshot_401.json` is the page's placeholder error body
//! verbatim (its values are the literal string `<string>`).

use std::time::Duration;

use async_trait::async_trait;
use jiff::Timestamp;
use reqwest::StatusCode;
use secrecy::SecretString;
use serde::Deserialize;

use super::{cents, dollars, error_message, http_status_message, malformed};
use crate::net::{Auth, Http};
use crate::provider::{Category, Provider, ProviderError, Snapshot, Unit};

pub struct Moonshot;

const ENDPOINT: &str = "https://api.moonshot.ai/v1/users/me/balance";
const HOST: &str = "api.moonshot.ai";

#[derive(Deserialize)]
struct Body {
    code: Option<i64>,
    status: Option<bool>,
    data: Option<Data>,
}

/// Amounts only. `scode` is a vendor status string and is not read.
#[derive(Deserialize)]
struct Data {
    available_balance: Option<f64>,
    voucher_balance: Option<f64>,
    cash_balance: Option<f64>,
}

/// `$12.34` or `-$12.34` from a signed dollar amount.
fn signed_dollars(usd: f64) -> Option<String> {
    let minor = cents(Some(usd.abs()))?;
    Some(if usd < 0.0 {
        format!("-{}", dollars(minor))
    } else {
        dollars(minor)
    })
}

#[async_trait]
impl Provider for Moonshot {
    fn id(&self) -> &'static str {
        "moonshot"
    }

    fn display_name(&self) -> &'static str {
        "Moonshot (Kimi)"
    }

    fn docs_url(&self) -> &'static str {
        "https://platform.kimi.ai/docs/api/balance"
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
        Some(
            "For keys from platform.kimi.ai. Keys from platform.kimi.com or platform.moonshot.cn use a different host and are not supported.",
        )
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
    if parsed.status == Some(false) || parsed.code.is_some_and(|c| c != 0) {
        return Err(ProviderError::Vendor(
            error_message(body).unwrap_or_else(|| "Moonshot reported a failed request.".into()),
        ));
    }
    let data = parsed.data.ok_or_else(malformed)?;
    let mut snapshot = Snapshot::at(now);
    snapshot.unit = Some(Unit::Usd);
    // A negative available balance means inference is blocked; the card
    // shows zero and the Cash extra carries the debt.
    snapshot.remaining = cents(data.available_balance.map(|a| a.max(0.0)));
    if let Some(v) = data.voucher_balance.and_then(signed_dollars) {
        snapshot.extra.push(("Voucher".to_owned(), v));
    }
    if let Some(c) = data.cash_balance.and_then(signed_dollars) {
        snapshot.extra.push(("Cash".to_owned(), c));
    }
    if data.available_balance.is_some_and(|a| a <= 0.0) {
        snapshot.extra.push(("Inference".to_owned(), "blocked, balance too low".to_owned()));
    }
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &str = include_str!("../../tests/fixtures/moonshot_200.json");
    const ERR: &str = include_str!("../../tests/fixtures/moonshot_401.json");

    fn run(status: StatusCode, body: &str) -> Result<Snapshot, ProviderError> {
        parse(status, None, body.as_bytes(), Timestamp::UNIX_EPOCH)
    }

    #[test]
    fn documented_200_parses_in_cents() {
        let s = run(StatusCode::OK, OK).unwrap();
        assert_eq!(s.remaining, Some(4959));
        assert_eq!(s.limit, None);
        assert_eq!(s.unit, Some(Unit::Usd));
        assert_eq!(
            s.extra,
            vec![("Voucher".to_owned(), "$46.59".to_owned()), ("Cash".to_owned(), "$3.00".to_owned()),]
        );
    }

    #[test]
    fn negative_cash_shows_as_debt_and_zero_available_blocks() {
        let body = OK
            .replace("\"available_balance\": 49.58894", "\"available_balance\": 0")
            .replace("\"voucher_balance\": 46.58893", "\"voucher_balance\": 0")
            .replace("\"cash_balance\": 3.00001", "\"cash_balance\": -3.25");
        let s = run(StatusCode::OK, &body).unwrap();
        assert_eq!(s.remaining, Some(0));
        assert!(s.extra.contains(&("Cash".to_owned(), "-$3.25".to_owned())));
        assert!(s.extra.contains(&("Inference".to_owned(), "blocked, balance too low".to_owned())));
    }

    #[test]
    fn identifiers_do_not_survive() {
        let s = run(StatusCode::OK, OK).unwrap();
        let dump = format!("{s:?}");
        for echoed in ["0x0", "scode", "available_balance"] {
            assert!(!dump.contains(echoed), "{dump}");
        }
    }

    #[test]
    fn documented_errors_map() {
        assert_eq!(run(StatusCode::UNAUTHORIZED, ERR), Err(ProviderError::Unauthorized));
        assert_eq!(run(StatusCode::FORBIDDEN, ERR), Err(ProviderError::Unauthorized));
        assert!(matches!(run(StatusCode::INTERNAL_SERVER_ERROR, ERR), Err(ProviderError::Vendor(_))));
        assert_eq!(
            run(StatusCode::INTERNAL_SERVER_ERROR, "<html>"),
            Err(ProviderError::Vendor("The vendor returned HTTP 500.".into()))
        );
        assert_eq!(
            parse(
                StatusCode::TOO_MANY_REQUESTS,
                Some(Duration::from_secs(5)),
                b"",
                Timestamp::UNIX_EPOCH
            ),
            Err(ProviderError::RateLimited {
                retry_after: Some(Duration::from_secs(5)),
                message: None
            })
        );
    }

    #[test]
    fn failed_status_on_200_is_a_vendor_error() {
        let body = OK.replace("\"status\": true", "\"status\": false");
        assert!(matches!(run(StatusCode::OK, &body), Err(ProviderError::Vendor(_))));
        let body = OK.replace("\"code\": 0", "\"code\": 7");
        assert!(matches!(run(StatusCode::OK, &body), Err(ProviderError::Vendor(_))));
    }

    #[test]
    fn unparseable_200_is_malformed() {
        assert_eq!(run(StatusCode::OK, "not json"), Err(malformed()));
        assert_eq!(run(StatusCode::OK, r#"{"code":0,"status":true}"#), Err(malformed()));
    }
}
