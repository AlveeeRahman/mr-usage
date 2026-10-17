//! DigitalOcean customer balance.
//!
//! Verified 2026-10-17 against
//! <https://docs.digitalocean.com/reference/api/digitalocean/#tag/Billing/operation/balance_get>
//! ("Get Customer Balance"): `GET https://api.digitalocean.com/v2/customers/my/balance`,
//! `Authorization: Bearer <token>`, required scope `billing:read`. 200 body
//! `{account_balance, generated_at, month_to_date_balance, month_to_date_usage}`
//! where the three amounts are decimal strings in US dollars ("12.23") and
//! `generated_at` is RFC 3339. `account_balance` is "Current balance of the
//! customer's most recent billing activity. Does not reflect
//! month_to_date_usage"; `month_to_date_usage` is "Amount used in the current
//! billing period as of the generated_at time". Error bodies are
//! `{"id": "<code>", "message": "<text>", "request_id"?: "<uuid>"}` with 401
//! (`unauthorized`), 404, 429 (`too_many_requests`) and 500 documented.
//!
//! Cost: the page states no price. The API is rate limited (5000 requests per
//! hour, 250 per minute, reported in `ratelimit-*` headers), not billed.
//!
//! Sign convention: a positive `account_balance` is money owed to
//! DigitalOcean, a negative one is credit. The card shows credit as what is
//! left and the month's usage as spend. The Billing overview on the same page
//! says invoices are generated on the first of each month, so the period ends
//! at the start of the next UTC month. `request_id` is never deserialized.

use std::time::Duration;

use async_trait::async_trait;
use jiff::Timestamp;
use reqwest::StatusCode;
use secrecy::SecretString;
use serde::Deserialize;

use super::{cents, dollars, error_message, http_status_message, malformed};
use crate::net::{Auth, Http};
use crate::provider::{Category, Provider, ProviderError, Snapshot, Unit, parse_timestamp};

pub struct DigitalOcean;

const ENDPOINT: &str = "https://api.digitalocean.com/v2/customers/my/balance";
const HOST: &str = "api.digitalocean.com";

/// Amounts arrive as decimal strings. No identifiers are documented in the
/// 200 body; the error `request_id` is not deserialized.
#[derive(Deserialize)]
struct Body {
    account_balance: Option<String>,
    month_to_date_usage: Option<String>,
    month_to_date_balance: Option<String>,
    generated_at: Option<String>,
}

fn amount(raw: Option<&str>) -> Option<f64> {
    raw?.trim().parse::<f64>().ok().filter(|n| n.is_finite())
}

/// Midnight UTC on the first day of the month after `now`.
pub(crate) fn start_of_next_utc_month(now: Timestamp) -> Option<Timestamp> {
    let date = now.to_zoned(jiff::tz::TimeZone::UTC).date();
    let next = date.first_of_month().checked_add(jiff::Span::new().months(1)).ok()?;
    next.to_zoned(jiff::tz::TimeZone::UTC).ok().map(|z| z.timestamp())
}

#[async_trait]
impl Provider for DigitalOcean {
    fn id(&self) -> &'static str {
        "digitalocean"
    }

    fn display_name(&self) -> &'static str {
        "DigitalOcean"
    }

    fn docs_url(&self) -> &'static str {
        "https://docs.digitalocean.com/reference/api/digitalocean/#tag/Billing/operation/balance_get"
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
        Category::Compute
    }

    fn min_poll_interval(&self) -> Duration {
        Duration::from_secs(60)
    }

    fn note(&self) -> Option<&'static str> {
        Some("The token needs the billing:read scope. Shows credit left and this month's usage; a balance owed appears as a line.")
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
    let balance = amount(parsed.account_balance.as_deref());
    let usage = amount(parsed.month_to_date_usage.as_deref());
    if balance.is_none() && usage.is_none() {
        return Err(malformed());
    }
    let mut extra = Vec::new();
    let remaining = match balance {
        Some(b) if b < 0.0 => cents(Some(-b)),
        Some(b) if b > 0.0 => {
            if let Some(owed) = cents(Some(b)) {
                extra.push(("Owed".to_owned(), dollars(owed)));
            }
            None
        }
        _ => None,
    };
    if let Some(mtd) = cents(amount(parsed.month_to_date_balance.as_deref())) {
        extra.push(("Month to date balance".to_owned(), dollars(mtd)));
    }
    if let Some(at) = parsed.generated_at.as_deref().and_then(parse_timestamp) {
        extra.push(("Balances as of".to_owned(), at.strftime("%Y-%m-%d %H:%M UTC").to_string()));
    }
    Ok(Snapshot {
        taken_at: now,
        remaining,
        limit: None,
        period_end: start_of_next_utc_month(now),
        spent: cents(usage),
        spend_per_hour: None,
        unit: None,
        extra,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &str = include_str!("../../tests/fixtures/digitalocean_200.json");
    const BAD_KEY: &str = include_str!("../../tests/fixtures/digitalocean_401.json");
    const NOT_FOUND: &str = include_str!("../../tests/fixtures/digitalocean_404.json");
    const LIMITED: &str = include_str!("../../tests/fixtures/digitalocean_429.json");
    const SERVER: &str = include_str!("../../tests/fixtures/digitalocean_500.json");

    fn run(status: StatusCode, body: &str) -> Result<Snapshot, ProviderError> {
        parse(status, None, body.as_bytes(), Timestamp::UNIX_EPOCH)
    }

    #[test]
    fn documented_200_parses_in_cents() {
        // The example owes $12.23, so nothing is left and the debt is a line.
        let s = run(StatusCode::OK, OK).unwrap();
        assert_eq!(s.remaining, None);
        assert_eq!(s.limit, None);
        assert_eq!(s.spent, Some(1121));
        assert_eq!(s.period_end.unwrap().to_string(), "1970-02-01T00:00:00Z");
        assert_eq!(
            s.extra,
            vec![
                ("Owed".to_owned(), "$12.23".to_owned()),
                ("Month to date balance".to_owned(), "$23.44".to_owned()),
                ("Balances as of".to_owned(), "2019-07-09 15:01 UTC".to_owned()),
            ]
        );
    }

    #[test]
    fn negative_balance_is_credit_left() {
        let body = OK.replace("\"account_balance\": \"12.23\"", "\"account_balance\": \"-50.00\"");
        let s = run(StatusCode::OK, &body).unwrap();
        assert_eq!(s.remaining, Some(5000));
        assert!(!s.extra.iter().any(|(k, _)| k == "Owed"));
    }

    #[test]
    fn period_ends_at_the_next_utc_month() {
        let now: Timestamp = "2026-10-17T02:30:00Z".parse().unwrap();
        assert_eq!(start_of_next_utc_month(now).unwrap().to_string(), "2026-11-01T00:00:00Z");
        let dec: Timestamp = "2026-12-31T23:59:59Z".parse().unwrap();
        assert_eq!(start_of_next_utc_month(dec).unwrap().to_string(), "2027-01-01T00:00:00Z");
    }

    #[test]
    fn identifiers_do_not_survive() {
        // The 200 body documents no identifiers; make sure an undocumented
        // one, and the error envelope's request_id, would still be dropped.
        let body = OK.replace(
            "\"account_balance\": \"12.23\"",
            "\"account_balance\": \"12.23\", \"email\": \"demo@example.com\", \"request_id\": \"4d9d8375-3c56-4925-a3e7-eb137fed17e9\"",
        );
        let s = run(StatusCode::OK, &body).unwrap();
        let dump = format!("{s:?}");
        for echoed in ["demo@example.com", "4d9d8375"] {
            assert!(!dump.contains(echoed), "{dump}");
        }
    }

    #[test]
    fn unparseable_amounts_are_malformed() {
        assert_eq!(run(StatusCode::OK, r#"{"account_balance": "n/a"}"#), Err(malformed()));
        assert_eq!(run(StatusCode::OK, "<html>"), Err(malformed()));
    }

    #[test]
    fn documented_errors_map() {
        assert_eq!(run(StatusCode::UNAUTHORIZED, BAD_KEY), Err(ProviderError::Unauthorized));
        assert_eq!(run(StatusCode::FORBIDDEN, BAD_KEY), Err(ProviderError::Unauthorized));
        assert_eq!(
            run(StatusCode::NOT_FOUND, NOT_FOUND),
            Err(ProviderError::Vendor("The resource you requested could not be found.".into()))
        );
        assert_eq!(
            run(StatusCode::INTERNAL_SERVER_ERROR, SERVER),
            Err(ProviderError::Vendor("Unexpected server-side error".into()))
        );
        let e = parse(
            StatusCode::TOO_MANY_REQUESTS,
            Some(Duration::from_secs(12)),
            LIMITED.as_bytes(),
            Timestamp::UNIX_EPOCH,
        )
        .unwrap_err();
        assert_eq!(
            e,
            ProviderError::RateLimited {
                retry_after: Some(Duration::from_secs(12)),
                message: Some("API rate limit exceeded.".into())
            }
        );
        assert_eq!(
            run(StatusCode::BAD_GATEWAY, "<html>"),
            Err(ProviderError::Vendor("The vendor returned HTTP 502.".into()))
        );
    }
}
