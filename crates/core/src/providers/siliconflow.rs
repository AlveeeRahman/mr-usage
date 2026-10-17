//! SiliconFlow account balance.
//!
//! Verified 2026-10-17 against
//! <https://docs.siliconflow.com/en/api-reference/userinfo/get-user-info>:
//! `GET https://api.siliconflow.com/v1/user/info`, `Authorization: Bearer
//! <key>`. 200 body `{code: 20000, message: "OK", status: true, data: {id,
//! name, image, email, isAdmin, balance, status, introduction, role,
//! chargeBalance, totalBalance}}`. The three amounts are decimal strings;
//! on this `.com` host the account is billed in US dollars. `balance` is the
//! granted (promotional) balance, `chargeBalance` what was topped up, and
//! `totalBalance` their sum. `id`, `name`, `image`, `email`, `introduction`
//! and `role` are identifiers and are never deserialized.
//!
//! Cost: the page lists no price; the call is an account lookup, not a model
//! request, and consumes no balance.
//!
//! Errors: the page lists 400, 401, 404, 429, 503 and 504, each with a bare
//! JSON string body shown only as the placeholder `"<string>"`. A bare
//! string carries no `error`/`message` field, so those statuses fall back
//! to the HTTP status text. 401 maps to `Unauthorized`, 429 to
//! `RateLimited`.
//!
//! Fixtures: `siliconflow_200.json` is the page's 200 example verbatim
//! (placeholder identifiers included, so a test can prove none survive);
//! `siliconflow_401.json` is the page's placeholder error body verbatim.

use std::time::Duration;

use async_trait::async_trait;
use jiff::Timestamp;
use reqwest::StatusCode;
use secrecy::SecretString;
use serde::Deserialize;

use super::{cents, dollars, error_message, http_status_message, malformed};
use crate::net::{Auth, Http};
use crate::provider::{Category, Provider, ProviderError, Snapshot, Unit};

pub struct SiliconFlow;

const ENDPOINT: &str = "https://api.siliconflow.com/v1/user/info";
const HOST: &str = "api.siliconflow.com";

#[derive(Deserialize)]
struct Body {
    code: Option<i64>,
    status: Option<bool>,
    data: Option<Data>,
}

/// Amounts only. See the module doc for what is left out.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Data {
    balance: Option<String>,
    charge_balance: Option<String>,
    total_balance: Option<String>,
}

/// A documented decimal string such as `"88.88"` as a number.
fn decimal(raw: Option<&str>) -> Option<f64> {
    raw?.trim().parse::<f64>().ok()
}

#[async_trait]
impl Provider for SiliconFlow {
    fn id(&self) -> &'static str {
        "siliconflow"
    }

    fn display_name(&self) -> &'static str {
        "SiliconFlow"
    }

    fn docs_url(&self) -> &'static str {
        "https://docs.siliconflow.com/en/api-reference/userinfo/get-user-info"
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
        Some("For keys from siliconflow.com (billed in dollars). Users have reported the figures differing from the console.")
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
    if parsed.status == Some(false) || parsed.code.is_some_and(|c| c != 20000) {
        return Err(ProviderError::Vendor(
            error_message(body).unwrap_or_else(|| "SiliconFlow reported a failed request.".into()),
        ));
    }
    let data = parsed.data.ok_or_else(malformed)?;
    let remaining = cents(decimal(data.total_balance.as_deref())).ok_or_else(malformed)?;
    let mut snapshot = Snapshot::at(now);
    snapshot.unit = Some(Unit::Usd);
    snapshot.remaining = Some(remaining);
    if let Some(granted) = cents(decimal(data.balance.as_deref())) {
        snapshot.extra.push(("Granted".to_owned(), dollars(granted)));
    }
    if let Some(topped) = cents(decimal(data.charge_balance.as_deref())) {
        snapshot.extra.push(("Topped up".to_owned(), dollars(topped)));
    }
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &str = include_str!("../../tests/fixtures/siliconflow_200.json");
    const ERR: &str = include_str!("../../tests/fixtures/siliconflow_401.json");

    fn run(status: StatusCode, body: &str) -> Result<Snapshot, ProviderError> {
        parse(status, None, body.as_bytes(), Timestamp::UNIX_EPOCH)
    }

    #[test]
    fn documented_200_parses_in_cents() {
        let s = run(StatusCode::OK, OK).unwrap();
        assert_eq!(s.remaining, Some(8888));
        assert_eq!(s.limit, None);
        assert_eq!(s.unit, Some(Unit::Usd));
        assert_eq!(
            s.extra,
            vec![
                ("Granted".to_owned(), "$0.88".to_owned()),
                ("Topped up".to_owned(), "$88.00".to_owned()),
            ]
        );
    }

    #[test]
    fn identifiers_do_not_survive() {
        let s = run(StatusCode::OK, OK).unwrap();
        let dump = format!("{s:?}");
        for echoed in ["userid", "username", "user_avatar_image_url", "user_email_address", "normal"] {
            assert!(!dump.contains(echoed), "{dump}");
        }
    }

    #[test]
    fn documented_errors_map() {
        assert_eq!(run(StatusCode::UNAUTHORIZED, ERR), Err(ProviderError::Unauthorized));
        assert_eq!(run(StatusCode::FORBIDDEN, ERR), Err(ProviderError::Unauthorized));
        assert_eq!(
            run(StatusCode::NOT_FOUND, ERR),
            Err(ProviderError::Vendor("The vendor returned HTTP 404.".into()))
        );
        assert_eq!(
            run(StatusCode::SERVICE_UNAVAILABLE, ERR),
            Err(ProviderError::Vendor("The vendor returned HTTP 503.".into()))
        );
        assert_eq!(
            run(StatusCode::GATEWAY_TIMEOUT, ""),
            Err(ProviderError::Vendor("The vendor returned HTTP 504.".into()))
        );
        assert_eq!(
            parse(
                StatusCode::TOO_MANY_REQUESTS,
                Some(Duration::from_secs(10)),
                ERR.as_bytes(),
                Timestamp::UNIX_EPOCH
            ),
            Err(ProviderError::RateLimited {
                retry_after: Some(Duration::from_secs(10)),
                message: None
            })
        );
    }

    #[test]
    fn failed_status_on_200_is_a_vendor_error() {
        let body = OK.replace("\"status\": true", "\"status\": false");
        assert_eq!(run(StatusCode::OK, &body), Err(ProviderError::Vendor("OK".into())));
        let body = OK.replace("\"code\": 20000", "\"code\": 50000");
        assert!(matches!(run(StatusCode::OK, &body), Err(ProviderError::Vendor(_))));
    }

    #[test]
    fn unparseable_200_is_malformed() {
        assert_eq!(run(StatusCode::OK, "not json"), Err(malformed()));
        assert_eq!(run(StatusCode::OK, r#"{"code":20000,"status":true}"#), Err(malformed()));
        assert_eq!(
            run(
                StatusCode::OK,
                &OK.replace("\"totalBalance\": \"88.88\"", "\"totalBalance\": \"lots\"")
            ),
            Err(malformed())
        );
    }
}
