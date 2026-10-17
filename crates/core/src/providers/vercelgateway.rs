//! Vercel AI Gateway credit balance.
//!
//! Verified 2026-10-17 against
//! <https://vercel.com/docs/ai-gateway/sdks-and-apis/rest-api> ("Check credit
//! balance"): `GET https://ai-gateway.vercel.sh/v1/credits`, `Authorization:
//! Bearer <AI Gateway API key>` (a Vercel OIDC token also works; this app
//! sends only the key). 200 body `{"balance": "95.50", "total_used": "4.50"}`
//! with both amounts as decimal strings in US dollars. `balance` is the
//! team's remaining AI Gateway Credits; `total_used` is lifetime spend, not
//! a billing-period figure, so the card labels it "Used so far".
//!
//! Cost: the page lists no price for this endpoint; it is a lookup, not an
//! inference request, and consumes no credits. Only the `/v1/models
//! ?include_availability` variant documents a rate limit (429 with
//! `Retry-After: 60`), so the floor stays at the global minute.
//!
//! Errors: `{"error": {"message": "<text>", "type": "<type>"}}` with 400,
//! 401 (bad key or token), 403 (endpoint needs a paid plan), 404, 500 and
//! 503 documented. 401 and 403 map to `Unauthorized`, 429 to `RateLimited`,
//! the rest to `Vendor`.
//!
//! Fixtures: `vercelgateway_200.json` is the page's sample response
//! verbatim; `vercelgateway_400.json` is the page's generic "Error response"
//! example verbatim.

use std::time::Duration;

use async_trait::async_trait;
use jiff::Timestamp;
use reqwest::StatusCode;
use secrecy::SecretString;
use serde::Deserialize;

use super::{cents, dollars, error_message, http_status_message, malformed};
use crate::net::{Auth, Http};
use crate::provider::{Category, Provider, ProviderError, Snapshot, Unit};

pub struct VercelGateway;

const ENDPOINT: &str = "https://ai-gateway.vercel.sh/v1/credits";
const HOST: &str = "ai-gateway.vercel.sh";

/// Both documented fields are amounts; nothing else is published.
#[derive(Deserialize)]
struct Body {
    balance: Option<String>,
    total_used: Option<String>,
}

/// A documented decimal string such as `"95.50"` as a number.
fn decimal(raw: Option<&str>) -> Option<f64> {
    raw?.trim().parse::<f64>().ok()
}

#[async_trait]
impl Provider for VercelGateway {
    fn id(&self) -> &'static str {
        "vercelgateway"
    }

    fn display_name(&self) -> &'static str {
        "Vercel AI Gateway"
    }

    fn docs_url(&self) -> &'static str {
        "https://vercel.com/docs/ai-gateway/sdks-and-apis/rest-api"
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
        Some("Shows the team's credit balance. \"Used so far\" is lifetime spend, not this month's.")
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
    let remaining = cents(decimal(parsed.balance.as_deref()));
    if remaining.is_none() {
        return Err(malformed());
    }
    let mut snapshot = Snapshot::at(now);
    snapshot.unit = Some(Unit::Usd);
    snapshot.remaining = remaining;
    // Lifetime spend. Kept in `spent` so the card can show it, and labelled
    // so nobody reads it as this period's figure.
    snapshot.spent = cents(decimal(parsed.total_used.as_deref()));
    if let Some(used) = snapshot.spent {
        snapshot.extra.push(("Used so far".to_owned(), dollars(used)));
    }
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &str = include_str!("../../tests/fixtures/vercelgateway_200.json");
    const BAD_REQUEST: &str = include_str!("../../tests/fixtures/vercelgateway_400.json");

    fn run(status: StatusCode, body: &str) -> Result<Snapshot, ProviderError> {
        parse(status, None, body.as_bytes(), Timestamp::UNIX_EPOCH)
    }

    #[test]
    fn documented_200_parses_in_cents() {
        let s = run(StatusCode::OK, OK).unwrap();
        assert_eq!(s.remaining, Some(9550));
        assert_eq!(s.spent, Some(450));
        assert_eq!(s.limit, None);
        assert_eq!(s.unit, Some(Unit::Usd));
        assert_eq!(s.extra, vec![("Used so far".to_owned(), "$4.50".to_owned())]);
    }

    #[test]
    fn identifiers_do_not_survive() {
        let s = run(StatusCode::OK, OK).unwrap();
        let dump = format!("{s:?}");
        for echoed in ["total_used", "balance", "95.50"] {
            assert!(!dump.contains(echoed), "{dump}");
        }
    }

    #[test]
    fn documented_errors_map() {
        assert_eq!(run(StatusCode::UNAUTHORIZED, BAD_REQUEST), Err(ProviderError::Unauthorized));
        assert_eq!(run(StatusCode::FORBIDDEN, BAD_REQUEST), Err(ProviderError::Unauthorized));
        assert_eq!(
            run(StatusCode::BAD_REQUEST, BAD_REQUEST),
            Err(ProviderError::Vendor("Invalid request body".into()))
        );
        assert_eq!(
            run(StatusCode::SERVICE_UNAVAILABLE, "<html>"),
            Err(ProviderError::Vendor("The vendor returned HTTP 503.".into()))
        );
        assert_eq!(
            parse(
                StatusCode::TOO_MANY_REQUESTS,
                Some(Duration::from_secs(60)),
                BAD_REQUEST.as_bytes(),
                Timestamp::UNIX_EPOCH
            ),
            Err(ProviderError::RateLimited {
                retry_after: Some(Duration::from_secs(60)),
                message: Some("Invalid request body".into())
            })
        );
    }

    #[test]
    fn unparseable_200_is_malformed() {
        assert_eq!(run(StatusCode::OK, "not json"), Err(malformed()));
        assert_eq!(run(StatusCode::OK, r#"{"balance":"lots"}"#), Err(malformed()));
    }
}
