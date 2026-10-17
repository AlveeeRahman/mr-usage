//! HeyGen balance, subscription credits or metered spend.
//!
//! Verified 2026-10-17 against <https://developers.heygen.com/user-profile>:
//! `GET https://api.heygen.com/v3/users/me` with the key in the `X-Api-Key`
//! header. 200 body `{data: {username, email, first_name, last_name,
//! billing_type, wallet, subscription, usage_based}}`. `billing_type` is
//! `"wallet"`, `"subscription"` or `"usage_based"` and says which of the three
//! objects is populated:
//! `wallet: {currency: "usd" | "credits", remaining_balance, auto_reload:
//! {enabled, threshold_usd, amount_usd}}`;
//! `subscription: {plan, credits: {premium_credits: {remaining, resets_at},
//! add_on_credits: {remaining}}}`;
//! `usage_based: {spending_current_usd, spending_cap_usd}`.
//! The page documents no error bodies; 401/403 and 429 are mapped by status
//! and other failures show `error_message` when the body has one.
//!
//! Cost: the page lists no price and describes the call as the way to
//! "check your account status or remaining quota before making API calls".
//!
//! `username`, `email`, `first_name` and `last_name` are never deserialized.
//! The card's unit follows the account: a US dollar wallet or metered spend
//! overrides the provider's `Credits` with `Usd`.

use std::time::Duration;

use async_trait::async_trait;
use jiff::Timestamp;
use reqwest::StatusCode;
use secrecy::SecretString;
use serde::Deserialize;

use super::{cents, count, error_message, http_status_message, malformed};
use crate::net::{Auth, Http};
use crate::provider::{Category, Provider, ProviderError, Snapshot, Unit, parse_timestamp};
use crate::redact;

pub struct HeyGen;

const ENDPOINT: &str = "https://api.heygen.com/v3/users/me";
const HOST: &str = "api.heygen.com";

#[derive(Deserialize)]
struct Body {
    data: Data,
}

/// The billing objects only. No username, email or names.
#[derive(Deserialize)]
struct Data {
    billing_type: Option<String>,
    wallet: Option<Wallet>,
    subscription: Option<Subscription>,
    usage_based: Option<UsageBased>,
}

#[derive(Deserialize)]
struct Wallet {
    currency: Option<String>,
    remaining_balance: Option<f64>,
    auto_reload: Option<AutoReload>,
}

#[derive(Deserialize)]
struct AutoReload {
    enabled: Option<bool>,
}

#[derive(Deserialize)]
struct Subscription {
    plan: Option<String>,
    credits: Option<SubscriptionCredits>,
}

#[derive(Deserialize)]
struct SubscriptionCredits {
    premium_credits: Option<CreditPool>,
    add_on_credits: Option<CreditPool>,
}

#[derive(Deserialize)]
struct CreditPool {
    remaining: Option<f64>,
    resets_at: Option<String>,
}

#[derive(Deserialize)]
struct UsageBased {
    spending_current_usd: Option<f64>,
    spending_cap_usd: Option<f64>,
}

#[async_trait]
impl Provider for HeyGen {
    fn id(&self) -> &'static str {
        "heygen"
    }

    fn display_name(&self) -> &'static str {
        "HeyGen"
    }

    fn docs_url(&self) -> &'static str {
        "https://developers.heygen.com/user-profile"
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
        Category::Media
    }

    fn min_poll_interval(&self) -> Duration {
        Duration::from_secs(60)
    }

    fn note(&self) -> Option<&'static str> {
        Some("Shows the wallet balance, the subscription's premium credits or the metered spend, whichever the account uses.")
    }

    async fn fetch(&self, key: &SecretString, _config: Option<&str>, http: &Http) -> Result<Snapshot, ProviderError> {
        let response = http
            .get(
                ENDPOINT,
                HOST,
                Auth::Header {
                    name: "x-api-key",
                    prefix: "",
                    value: key,
                },
            )
            .await?;
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
    let mut snapshot = Snapshot::at(now);
    // `billing_type` names the populated object; fall back to whichever is
    // present when it is missing.
    let kind = data.billing_type.as_deref().unwrap_or(if data.wallet.is_some() {
        "wallet"
    } else if data.subscription.is_some() {
        "subscription"
    } else {
        "usage_based"
    });
    match kind {
        "wallet" => {
            let wallet = data.wallet.ok_or_else(malformed)?;
            if wallet.currency.as_deref().is_some_and(|c| c.eq_ignore_ascii_case("usd")) {
                snapshot.remaining = cents(wallet.remaining_balance);
                snapshot.unit = Some(Unit::Usd);
            } else {
                snapshot.remaining = count(wallet.remaining_balance);
                snapshot.unit = Some(Unit::Credits);
            }
            snapshot.extra.push(("Billing".to_owned(), "prepaid wallet".to_owned()));
            if let Some(enabled) = wallet.auto_reload.and_then(|a| a.enabled) {
                snapshot
                    .extra
                    .push(("Auto-reload".to_owned(), if enabled { "on" } else { "off" }.to_owned()));
            }
        }
        "subscription" => {
            let sub = data.subscription.ok_or_else(malformed)?;
            let credits = sub.credits.unwrap_or(SubscriptionCredits {
                premium_credits: None,
                add_on_credits: None,
            });
            snapshot.unit = Some(Unit::Credits);
            if let Some(premium) = credits.premium_credits {
                snapshot.remaining = count(premium.remaining);
                snapshot.period_end = premium.resets_at.as_deref().and_then(parse_timestamp);
            }
            if let Some(plan) = sub.plan.as_deref().filter(|p| !p.is_empty()) {
                snapshot
                    .extra
                    .push(("Plan".to_owned(), redact::vendor_text(plan, 40).replace('_', " ")));
            }
            if let Some(add_on) = credits.add_on_credits.and_then(|a| count(a.remaining)) {
                snapshot.extra.push(("Add-on credits".to_owned(), add_on.to_string()));
            }
        }
        "usage_based" => {
            let usage = data.usage_based.ok_or_else(malformed)?;
            snapshot.unit = Some(Unit::Usd);
            snapshot.spent = cents(usage.spending_current_usd);
            snapshot.limit = cents(usage.spending_cap_usd);
            snapshot.extra.push(("Billing".to_owned(), "metered".to_owned()));
            if snapshot.limit.is_none() {
                snapshot.extra.push(("Spending cap".to_owned(), "none".to_owned()));
            }
        }
        _ => return Err(malformed()),
    }
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &str = include_str!("../../tests/fixtures/heygen_200.json");

    fn run(status: StatusCode, body: &str) -> Result<Snapshot, ProviderError> {
        parse(status, None, body.as_bytes(), Timestamp::UNIX_EPOCH)
    }

    #[test]
    fn documented_wallet_parses_in_cents() {
        let s = run(StatusCode::OK, OK).unwrap();
        assert_eq!(s.remaining, Some(4250));
        assert_eq!(s.unit, Some(Unit::Usd));
        assert_eq!(s.limit, None);
        assert_eq!(s.spent, None);
        assert_eq!(
            s.extra,
            vec![
                ("Billing".to_owned(), "prepaid wallet".to_owned()),
                ("Auto-reload".to_owned(), "off".to_owned()),
            ]
        );
    }

    #[test]
    fn identifiers_do_not_survive() {
        let s = run(StatusCode::OK, OK).unwrap();
        let dump = format!("{s:?}");
        for echoed in ["jane_doe", "jane@example.com", "Jane", "Doe"] {
            assert!(!dump.contains(echoed), "{dump}");
        }
    }

    #[test]
    fn credit_wallet_counts_credits() {
        let body = OK
            .replace("\"currency\": \"usd\"", "\"currency\": \"credits\"")
            .replace("42.50", "17.9");
        let s = run(StatusCode::OK, &body).unwrap();
        assert_eq!(s.remaining, Some(17));
        assert_eq!(s.unit, Some(Unit::Credits));
    }

    #[test]
    fn subscription_shows_premium_credits_and_reset() {
        let body = r#"{"data":{"username":"u","email":"e@x.y","first_name":null,"last_name":null,
            "billing_type":"subscription","wallet":null,
            "subscription":{"plan":"business_plus","credits":{
                "premium_credits":{"remaining":120,"resets_at":"2026-11-01T00:00:00Z"},
                "add_on_credits":{"remaining":30}}},
            "usage_based":null}}"#;
        let s = run(StatusCode::OK, body).unwrap();
        assert_eq!(s.remaining, Some(120));
        assert_eq!(s.unit, Some(Unit::Credits));
        assert_eq!(s.period_end.unwrap().to_string(), "2026-11-01T00:00:00Z");
        assert_eq!(
            s.extra,
            vec![
                ("Plan".to_owned(), "business plus".to_owned()),
                ("Add-on credits".to_owned(), "30".to_owned()),
            ]
        );
        assert!(!format!("{s:?}").contains("e@x.y"));
    }

    #[test]
    fn usage_based_is_a_spend_card() {
        let body = r#"{"data":{"username":"u","billing_type":"usage_based","wallet":null,"subscription":null,
            "usage_based":{"spending_current_usd":12.345,"spending_cap_usd":100}}}"#;
        let s = run(StatusCode::OK, body).unwrap();
        assert_eq!(s.spent, Some(1235));
        assert_eq!(s.limit, Some(10000));
        assert_eq!(s.remaining, None);
        assert_eq!(s.unit, Some(Unit::Usd));
        let no_cap = body.replace(",\"spending_cap_usd\":100", ",\"spending_cap_usd\":null");
        let s = run(StatusCode::OK, &no_cap).unwrap();
        assert_eq!(s.limit, None);
        assert!(s.extra.contains(&("Spending cap".to_owned(), "none".to_owned())));
    }

    #[test]
    fn unknown_billing_type_or_missing_object_is_malformed() {
        assert_eq!(
            run(StatusCode::OK, r#"{"data":{"billing_type":"wallet","wallet":null}}"#),
            Err(malformed())
        );
        assert_eq!(run(StatusCode::OK, r#"{"data":{"billing_type":"barter"}}"#), Err(malformed()));
        assert_eq!(run(StatusCode::OK, "<html>"), Err(malformed()));
    }

    #[test]
    fn auth_rate_limit_and_other_statuses() {
        assert_eq!(run(StatusCode::UNAUTHORIZED, ""), Err(ProviderError::Unauthorized));
        assert_eq!(run(StatusCode::FORBIDDEN, ""), Err(ProviderError::Unauthorized));
        let e = parse(
            StatusCode::TOO_MANY_REQUESTS,
            Some(Duration::from_secs(2)),
            br#"{"message":"Too many requests"}"#,
            Timestamp::UNIX_EPOCH,
        )
        .unwrap_err();
        assert_eq!(
            e,
            ProviderError::RateLimited {
                retry_after: Some(Duration::from_secs(2)),
                message: Some("Too many requests".into())
            }
        );
        assert_eq!(
            run(StatusCode::INTERNAL_SERVER_ERROR, ""),
            Err(ProviderError::Vendor("The vendor returned HTTP 500.".into()))
        );
    }
}
