//! xAI prepaid credit balance for one team.
//!
//! Verified 2026-10-17 against
//! <https://docs.x.ai/developers/rest-api-reference/management/billing>
//! ("List prepaid credit balance and balance changes"):
//! `GET https://management-api.x.ai/v1/billing/teams/{team_id}/prepaid/balance`
//! with a Management API key as `Authorization: Bearer <key>`. The guide
//! <https://docs.x.ai/developers/management-api-guide> (read the same day)
//! says the management key is separate from the inference API key, comes
//! from Console -> Settings -> Management Keys, and that the base URL
//! `https://management-api.x.ai` differs from the inference API. 200 body
//! `{changes: [{teamId, changeOrigin, topupStatus, amount: {val}, invoiceId,
//! invoiceNumber, createTime, paymentProcessor: {kind}}], total: {val}}`
//! where `total` is a "Representation of USD Cents" as a string. The ledger
//! is inverted: the documented $10 purchase appears as `"amount": {"val":
//! "-1000"}` and the resulting balance as `"total": {"val": "-1000"}`, so a
//! negative total is credit in hand and a positive one is owed. Only
//! `total.val` is deserialized; `changes` holds a team id, invoice ids and
//! invoice numbers and is never read.
//!
//! The team id comes from the config field, is checked against
//! `[A-Za-z0-9_-]` and spliced into the path. The documented example is a
//! UUID.
//!
//! Cost: the page lists no price; the management API is not metered as
//! inference and consumes no credits.
//!
//! Errors: the page documents no error bodies or statuses. 401 and 403 map
//! to `Unauthorized`, 429 to `RateLimited`, the rest to `Vendor` with any
//! `{"error"|"message"}` text the body carries.
//!
//! The fixture `xai_200.json` is the page's 200 example verbatim.

use std::time::Duration;

use async_trait::async_trait;
use jiff::Timestamp;
use reqwest::StatusCode;
use secrecy::SecretString;
use serde::Deserialize;

use super::{dollars, error_message, http_status_message, malformed};
use crate::net::{Auth, Http};
use crate::provider::{Category, ConfigField, Provider, ProviderError, Snapshot, Unit};

pub struct XAi;

const ENDPOINT: &str = "https://management-api.x.ai/v1/billing/teams/<team id>/prepaid/balance";
const HOST: &str = "management-api.x.ai";

/// `changes` is deliberately absent.
#[derive(Deserialize)]
struct Body {
    total: Option<Money>,
}

#[derive(Deserialize)]
struct Money {
    val: Option<String>,
}

/// The real URL for one team, or why it cannot be built.
pub(crate) fn endpoint_for(config: Option<&str>) -> Result<String, ProviderError> {
    let team = config
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .ok_or_else(|| ProviderError::Unsupported("Enter the team id from the xAI console in this provider's settings.".into()))?;
    if !team.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-') {
        return Err(ProviderError::Unsupported(
            "The team id may only contain letters, digits, '-' and '_'.".into(),
        ));
    }
    Ok(format!("https://{HOST}/v1/billing/teams/{team}/prepaid/balance"))
}

#[async_trait]
impl Provider for XAi {
    fn id(&self) -> &'static str {
        "xai"
    }

    fn display_name(&self) -> &'static str {
        "xAI"
    }

    fn docs_url(&self) -> &'static str {
        "https://docs.x.ai/developers/rest-api-reference/management/billing"
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
        Some("Needs a Management API key, not an inference key, plus the team id. Shows prepaid credit only.")
    }

    fn config_field(&self) -> Option<ConfigField> {
        Some(ConfigField {
            label: "Team id",
            hint: "The team id from the xAI console. Not a key.",
            placeholder: "team_…",
        })
    }

    async fn fetch(&self, key: &SecretString, config: Option<&str>, http: &Http) -> Result<Snapshot, ProviderError> {
        let endpoint = endpoint_for(config)?;
        let response = http.get(&endpoint, HOST, Auth::Bearer(key)).await?;
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
    let ledger: i64 = parsed
        .total
        .and_then(|t| t.val)
        .and_then(|v| v.trim().parse().ok())
        .ok_or_else(malformed)?;
    let mut snapshot = Snapshot::at(now);
    snapshot.unit = Some(Unit::Usd);
    // Inverted ledger: credit in hand is negative, debt is positive.
    snapshot.remaining = Some(ledger.min(0).unsigned_abs());
    if ledger > 0 {
        snapshot.extra.push(("Owed".to_owned(), dollars(ledger.unsigned_abs())));
    }
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &str = include_str!("../../tests/fixtures/xai_200.json");

    fn run(status: StatusCode, body: &str) -> Result<Snapshot, ProviderError> {
        parse(status, None, body.as_bytes(), Timestamp::UNIX_EPOCH)
    }

    #[test]
    fn documented_200_parses_the_inverted_ledger_in_cents() {
        let s = run(StatusCode::OK, OK).unwrap();
        assert_eq!(s.remaining, Some(1000));
        assert_eq!(s.limit, None);
        assert_eq!(s.unit, Some(Unit::Usd));
        assert!(s.extra.is_empty());
    }

    #[test]
    fn positive_total_is_debt() {
        let body = r#"{"changes":[],"total":{"val":"250"}}"#;
        let s = run(StatusCode::OK, body).unwrap();
        assert_eq!(s.remaining, Some(0));
        assert_eq!(s.extra, vec![("Owed".to_owned(), "$2.50".to_owned())]);
    }

    #[test]
    fn identifiers_do_not_survive() {
        let s = run(StatusCode::OK, OK).unwrap();
        let dump = format!("{s:?}");
        for echoed in [
            "65c1e471-205f-4566-9c5a-07198bcdf4ce",
            "7v7blf6c1G2g34OdI4N5tD1CKCdag4ZYLhn5vTBLIMM=",
            "062-446-653-166",
            "PURCHASE",
            "STRIPE",
        ] {
            assert!(!dump.contains(echoed), "{dump}");
        }
    }

    #[test]
    fn error_statuses_map() {
        assert_eq!(run(StatusCode::UNAUTHORIZED, ""), Err(ProviderError::Unauthorized));
        assert_eq!(run(StatusCode::FORBIDDEN, ""), Err(ProviderError::Unauthorized));
        assert_eq!(
            run(StatusCode::NOT_FOUND, r#"{"error":"team not found"}"#),
            Err(ProviderError::Vendor("team not found".into()))
        );
        assert_eq!(
            run(StatusCode::INTERNAL_SERVER_ERROR, "<html>"),
            Err(ProviderError::Vendor("The vendor returned HTTP 500.".into()))
        );
        assert_eq!(
            parse(
                StatusCode::TOO_MANY_REQUESTS,
                Some(Duration::from_secs(2)),
                b"",
                Timestamp::UNIX_EPOCH
            ),
            Err(ProviderError::RateLimited {
                retry_after: Some(Duration::from_secs(2)),
                message: None
            })
        );
    }

    #[test]
    fn unparseable_200_is_malformed() {
        assert_eq!(run(StatusCode::OK, "not json"), Err(malformed()));
        assert_eq!(run(StatusCode::OK, r#"{"changes":[]}"#), Err(malformed()));
        assert_eq!(run(StatusCode::OK, r#"{"total":{"val":"ten"}}"#), Err(malformed()));
    }

    #[test]
    fn team_id_builds_the_url_or_is_refused() {
        assert_eq!(
            endpoint_for(Some("65c1e471-205f-4566-9c5a-07198bcdf4ce")).unwrap(),
            "https://management-api.x.ai/v1/billing/teams/65c1e471-205f-4566-9c5a-07198bcdf4ce/prepaid/balance"
        );
        assert!(matches!(endpoint_for(None), Err(ProviderError::Unsupported(m)) if m.contains("team id")));
        assert!(matches!(endpoint_for(Some("  ")), Err(ProviderError::Unsupported(_))));
        for bad in ["a/b", "x?y=1", "team id", "../other", "t#1"] {
            assert!(matches!(endpoint_for(Some(bad)), Err(ProviderError::Unsupported(_))), "{bad}");
        }
        let url = reqwest::Url::parse(&endpoint_for(Some("team_1")).unwrap()).unwrap();
        assert!(crate::net::check_allowed(&url, HOST).is_ok());
    }
}
