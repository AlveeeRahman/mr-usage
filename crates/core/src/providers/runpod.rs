//! RunPod account balance and burn rate.
//!
//! Verified 2026-10-17 against <https://graphql-spec.runpod.io/> (RunPod
//! GraphQL API Spec 1.1.0): endpoint `https://api.runpod.io/graphql`, header
//! `Authorization: Bearer <RUNPOD_API_KEY>`. The `myself` query returns a
//! `User` with `clientBalance: Float`, `currentSpendPerHr: Float` and
//! `spendLimit: Int`, all in US dollars. The call is
//! `POST https://api.runpod.io/graphql` with the fixed body
//! `{"query":"query { myself { clientBalance currentSpendPerHr spendLimit } }"}`.
//! <https://docs.runpod.io/sdks/graphql/configurations> (modified 2026-08-19)
//! shows the same endpoint with the key as a `?api_key=` query parameter
//! instead; the header documented by the spec is used because it keeps the
//! key out of the URL. That page also says the GraphQL API "is deprecated and
//! will be retired in early 2027".
//!
//! Cost: neither page states a price. Reading the account record starts no
//! pod and is not metered.
//!
//! Error shapes: neither page documents an error body. GraphQL servers
//! answer a rejected query with HTTP 200 and `{"errors": [{"message": …}],
//! "data": null}`, so `runpod_200_unauthorized.json` is the smallest such
//! body, built here rather than copied. An error whose message mentions
//! authentication or authorization maps to `Unauthorized`; any other message
//! is passed through as a vendor error. Non-200 statuses are mapped from the
//! status alone.
//!
//! `runpod_200.json` is the documented `myself` example (every string is
//! `"abc123"`, numbers are `987.65`/`123`) cut down to the scalars around the
//! three fields the query asks for. `id`, `authId`, `email`,
//! `stripeSavedPaymentLast4`, `stripeAccountId` and the host-side balances are
//! never deserialized.

use std::time::Duration;

use async_trait::async_trait;
use jiff::Timestamp;
use reqwest::StatusCode;
use secrecy::SecretString;
use serde::Deserialize;

use super::{MAX_VENDOR_TEXT, cents, dollars, error_message, http_status_message, malformed};
use crate::net::{Auth, Http};
use crate::provider::{Category, Provider, ProviderError, Snapshot, Unit};
use crate::redact;

pub struct RunPod;

const ENDPOINT: &str = "https://api.runpod.io/graphql";
const HOST: &str = "api.runpod.io";
/// Fixed query. Carries no key and asks for numbers only.
const QUERY: &str = r#"{"query":"query { myself { clientBalance currentSpendPerHr spendLimit } }"}"#;

#[derive(Deserialize)]
struct Body {
    data: Option<Data>,
    errors: Option<Vec<GraphqlError>>,
}

#[derive(Deserialize)]
struct Data {
    myself: Option<Myself>,
}

/// Numbers only. See the module doc for what is left out.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Myself {
    client_balance: Option<f64>,
    current_spend_per_hr: Option<f64>,
    spend_limit: Option<f64>,
}

#[derive(Deserialize)]
struct GraphqlError {
    message: Option<String>,
}

#[async_trait]
impl Provider for RunPod {
    fn id(&self) -> &'static str {
        "runpod"
    }

    fn display_name(&self) -> &'static str {
        "RunPod"
    }

    fn docs_url(&self) -> &'static str {
        "https://graphql-spec.runpod.io/"
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
        Some("Shows the prepaid balance and the current hourly burn. RunPod says this GraphQL API retires in early 2027.")
    }

    async fn fetch(&self, key: &SecretString, _config: Option<&str>, http: &Http) -> Result<Snapshot, ProviderError> {
        let response = http.post_json(ENDPOINT, HOST, Auth::Bearer(key), &[], QUERY).await?;
        parse(response.status, response.retry_after, &response.body, Timestamp::now())
    }
}

/// The first GraphQL error message, scrubbed, or `None`.
fn graphql_error(errors: &[GraphqlError]) -> Option<String> {
    let text = errors.iter().find_map(|e| e.message.as_deref()).filter(|m| !m.trim().is_empty())?;
    let clean = redact::vendor_text(text, MAX_VENDOR_TEXT);
    (!clean.is_empty()).then_some(clean)
}

fn is_auth_failure(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    [
        "unauthorized",
        "unauthenticated",
        "authentication",
        "not authenticated",
        "invalid api key",
        "forbidden",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
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
    if let Some(errors) = parsed.errors.as_deref().filter(|e| !e.is_empty()) {
        let message = graphql_error(errors).unwrap_or_else(|| "RunPod rejected the query.".to_owned());
        return Err(if is_auth_failure(&message) {
            ProviderError::Unauthorized
        } else {
            ProviderError::Vendor(message)
        });
    }
    let me = parsed.data.and_then(|d| d.myself).ok_or_else(malformed)?;
    let balance = me.client_balance.filter(|b| b.is_finite()).ok_or_else(malformed)?;
    let mut extra = Vec::new();
    if balance < 0.0
        && let Some(owed) = cents(Some(-balance))
    {
        extra.push(("Owed".to_owned(), dollars(owed)));
    }
    if let Some(limit) = cents(me.spend_limit).filter(|l| *l > 0) {
        extra.push(("Spend limit".to_owned(), dollars(limit)));
    }
    Ok(Snapshot {
        taken_at: now,
        remaining: Some(cents(Some(balance)).unwrap_or(0)),
        limit: None,
        period_end: None,
        spent: None,
        spend_per_hour: cents(me.current_spend_per_hr),
        unit: None,
        extra,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &str = include_str!("../../tests/fixtures/runpod_200.json");
    const REJECTED: &str = include_str!("../../tests/fixtures/runpod_200_unauthorized.json");

    fn run(status: StatusCode, body: &str) -> Result<Snapshot, ProviderError> {
        parse(status, None, body.as_bytes(), Timestamp::UNIX_EPOCH)
    }

    #[test]
    fn query_is_fixed_json_without_a_key() {
        let v: serde_json::Value = serde_json::from_str(QUERY).unwrap();
        assert_eq!(v["query"], "query { myself { clientBalance currentSpendPerHr spendLimit } }");
    }

    #[test]
    fn documented_200_parses_in_cents() {
        let s = run(StatusCode::OK, OK).unwrap();
        assert_eq!(s.remaining, Some(98_765));
        assert_eq!(s.spend_per_hour, Some(98_765));
        assert_eq!(s.limit, None);
        assert_eq!(s.period_end, None);
        assert_eq!(s.extra, vec![("Spend limit".to_owned(), "$123.00".to_owned())]);
    }

    #[test]
    fn zero_spend_limit_is_not_shown() {
        let body = OK.replace("\"spendLimit\": 123", "\"spendLimit\": 0");
        assert!(run(StatusCode::OK, &body).unwrap().extra.is_empty());
    }

    #[test]
    fn negative_balance_shows_zero_and_owed() {
        let body = OK.replace("\"clientBalance\": 987.65", "\"clientBalance\": -2.5");
        let s = run(StatusCode::OK, &body).unwrap();
        assert_eq!(s.remaining, Some(0));
        assert!(s.extra.contains(&("Owed".to_owned(), "$2.50".to_owned())));
    }

    #[test]
    fn identifiers_do_not_survive() {
        let body = OK
            .replace("\"id\": \"abc123\"", "\"id\": \"user_7f3a9b2c\"")
            .replace("\"authId\": \"abc123\"", "\"authId\": \"auth0|5f1e2d3c\"")
            .replace("\"email\": \"abc123\"", "\"email\": \"demo@example.com\"")
            .replace("\"stripeSavedPaymentLast4\": \"abc123\"", "\"stripeSavedPaymentLast4\": \"4242\"")
            .replace("\"stripeAccountId\": \"abc123\"", "\"stripeAccountId\": \"acct_1ABCdef\"");
        let s = run(StatusCode::OK, &body).unwrap();
        let dump = format!("{s:?}");
        for echoed in [
            "user_7f3a9b2c",
            "auth0|",
            "demo@example.com",
            "4242",
            "acct_1ABCdef",
            "abc123",
            "12345",
        ] {
            assert!(!dump.contains(echoed), "{dump}");
        }
    }

    #[test]
    fn graphql_auth_error_on_200_is_unauthorized() {
        assert_eq!(run(StatusCode::OK, REJECTED), Err(ProviderError::Unauthorized));
        let other = REJECTED.replace("Unauthorized", "Cannot query field 'nope' on type 'User'.");
        assert_eq!(
            run(StatusCode::OK, &other),
            Err(ProviderError::Vendor("Cannot query field 'nope' on type 'User'.".into()))
        );
    }

    #[test]
    fn missing_data_is_malformed() {
        assert_eq!(run(StatusCode::OK, r#"{"data": {"myself": null}}"#), Err(malformed()));
        assert_eq!(run(StatusCode::OK, r#"{"data": {"myself": {}}}"#), Err(malformed()));
        assert_eq!(run(StatusCode::OK, "<html>"), Err(malformed()));
    }

    #[test]
    fn http_statuses_map() {
        assert_eq!(run(StatusCode::UNAUTHORIZED, ""), Err(ProviderError::Unauthorized));
        assert_eq!(run(StatusCode::FORBIDDEN, ""), Err(ProviderError::Unauthorized));
        let e = parse(
            StatusCode::TOO_MANY_REQUESTS,
            Some(Duration::from_secs(9)),
            br#"{"message": "Too many requests"}"#,
            Timestamp::UNIX_EPOCH,
        )
        .unwrap_err();
        assert_eq!(
            e,
            ProviderError::RateLimited {
                retry_after: Some(Duration::from_secs(9)),
                message: Some("Too many requests".into())
            }
        );
        assert_eq!(
            run(StatusCode::BAD_GATEWAY, "<html>"),
            Err(ProviderError::Vendor("The vendor returned HTTP 502.".into()))
        );
    }
}
