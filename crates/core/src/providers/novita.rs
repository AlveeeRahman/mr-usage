//! Novita AI account balance.
//!
//! Verified 2026-10-17 against
//! <https://docs.novita.ai/api-reference/basic-get-user-balance>:
//! `GET https://api.novita.ai/openapi/v1/billing/balance/detail`,
//! `Authorization: Bearer <key>`. 200 body `{availableBalance, cashBalance,
//! creditLimit, pendingCharges, outstandingInvoices}`, every value a string
//! integer in units of 1/10000 US dollar (`"10000"` is $1.00). `cashBalance`
//! is what was topped up, `creditLimit` the most the account may owe,
//! `availableBalance` what can be spent (top-up plus credit), and
//! `outstandingInvoices` what is owed. The page says `pendingCharges`
//! currently defaults to `0`.
//!
//! Cost: the page lists no price and the call is a billing lookup, not a
//! model request. It consumes no balance.
//!
//! Errors: the page documents no error bodies and no rate limit. Statuses are
//! mapped generically: 401/403 to `Unauthorized`, 429 to `RateLimited`, the
//! rest to `Vendor` with whatever `{"error"|"message"}` text the body holds.
//!
//! The fixture `novita_200.json` is the page's 200 example verbatim.

use std::time::Duration;

use async_trait::async_trait;
use jiff::Timestamp;
use reqwest::StatusCode;
use secrecy::SecretString;
use serde::Deserialize;

use super::{cents, dollars, error_message, http_status_message, malformed};
use crate::net::{Auth, Http};
use crate::provider::{Category, Provider, ProviderError, Snapshot, Unit};

pub struct Novita;

const ENDPOINT: &str = "https://api.novita.ai/openapi/v1/billing/balance/detail";
const HOST: &str = "api.novita.ai";

/// Amounts only; the page publishes nothing else.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Body {
    available_balance: Option<String>,
    cash_balance: Option<String>,
    credit_limit: Option<String>,
    pending_charges: Option<String>,
    outstanding_invoices: Option<String>,
}

/// A documented string in 1/10000 USD as whole cents.
fn ten_thousandths(raw: Option<&str>) -> Option<u64> {
    let n = raw?.trim().parse::<f64>().ok()?;
    cents(Some(n / 10_000.0))
}

#[async_trait]
impl Provider for Novita {
    fn id(&self) -> &'static str {
        "novita"
    }

    fn display_name(&self) -> &'static str {
        "Novita AI"
    }

    fn docs_url(&self) -> &'static str {
        "https://docs.novita.ai/api-reference/basic-get-user-balance"
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
        Some("Shows the spendable balance. The limit, when shown, is the credit line Novita lets you owe.")
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
    let remaining = ten_thousandths(parsed.available_balance.as_deref());
    if remaining.is_none() {
        return Err(malformed());
    }
    let mut snapshot = Snapshot::at(now);
    snapshot.unit = Some(Unit::Usd);
    snapshot.remaining = remaining;
    snapshot.limit = ten_thousandths(parsed.credit_limit.as_deref()).filter(|l| *l > 0);
    snapshot.spent = ten_thousandths(parsed.pending_charges.as_deref());
    if let Some(cash) = ten_thousandths(parsed.cash_balance.as_deref()) {
        snapshot.extra.push(("Topped up".to_owned(), dollars(cash)));
    }
    if let Some(owed) = ten_thousandths(parsed.outstanding_invoices.as_deref()).filter(|o| *o > 0) {
        snapshot.extra.push(("Owed".to_owned(), dollars(owed)));
    }
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &str = include_str!("../../tests/fixtures/novita_200.json");

    fn run(status: StatusCode, body: &str) -> Result<Snapshot, ProviderError> {
        parse(status, None, body.as_bytes(), Timestamp::UNIX_EPOCH)
    }

    #[test]
    fn documented_200_parses_in_cents() {
        let s = run(StatusCode::OK, OK).unwrap();
        assert_eq!(s.remaining, Some(10000));
        assert_eq!(s.limit, Some(2000));
        assert_eq!(s.spent, Some(0));
        assert_eq!(s.unit, Some(Unit::Usd));
        assert_eq!(s.extra, vec![("Topped up".to_owned(), "$80.00".to_owned())]);
    }

    #[test]
    fn zero_credit_limit_is_a_plain_balance_and_debt_shows() {
        let body = OK
            .replace("\"creditLimit\": \"200000\"", "\"creditLimit\": \"0\"")
            .replace("\"outstandingInvoices\": \"0\"", "\"outstandingInvoices\": \"12345\"");
        let s = run(StatusCode::OK, &body).unwrap();
        assert_eq!(s.limit, None);
        assert!(s.extra.contains(&("Owed".to_owned(), "$1.23".to_owned())));
    }

    #[test]
    fn identifiers_do_not_survive() {
        let s = run(StatusCode::OK, OK).unwrap();
        let dump = format!("{s:?}");
        for echoed in ["availableBalance", "1000000", "800000"] {
            assert!(!dump.contains(echoed), "{dump}");
        }
    }

    #[test]
    fn error_statuses_map() {
        assert_eq!(run(StatusCode::UNAUTHORIZED, ""), Err(ProviderError::Unauthorized));
        assert_eq!(run(StatusCode::FORBIDDEN, ""), Err(ProviderError::Unauthorized));
        assert_eq!(
            run(StatusCode::INTERNAL_SERVER_ERROR, r#"{"message":"boom"}"#),
            Err(ProviderError::Vendor("boom".into()))
        );
        assert_eq!(
            run(StatusCode::BAD_GATEWAY, "<html>"),
            Err(ProviderError::Vendor("The vendor returned HTTP 502.".into()))
        );
        assert_eq!(
            parse(
                StatusCode::TOO_MANY_REQUESTS,
                Some(Duration::from_secs(9)),
                b"",
                Timestamp::UNIX_EPOCH
            ),
            Err(ProviderError::RateLimited {
                retry_after: Some(Duration::from_secs(9)),
                message: None
            })
        );
    }

    #[test]
    fn unparseable_200_is_malformed() {
        assert_eq!(run(StatusCode::OK, "not json"), Err(malformed()));
        assert_eq!(run(StatusCode::OK, r#"{"cashBalance":"1"}"#), Err(malformed()));
    }
}
