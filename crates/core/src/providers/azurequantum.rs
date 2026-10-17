//! Azure Quantum workspace quotas.
//!
//! Verified 2026-10-17 against
//! <https://learn.microsoft.com/en-us/rest/api/azurequantum/dataplane/quotas/list?view=rest-azurequantum-dataplane-2026-01-15-preview>
//! ("Quotas - List"): `GET {endpoint}/subscriptions/{subscriptionId}/resourceGroups/{resourceGroupName}/providers/Microsoft.Quantum/workspaces/{workspaceName}/quotas?api-version=2026-01-15-preview`
//! where `{endpoint}` is `https://{region}.quantum.azure.com`. The user pastes
//! that whole address up to the workspace name as the "Workspace address"
//! setting and the provider appends `/quotas?api-version=…`. Security lists
//! two schemes: Entra OAuth2 and `x-ms-quantum-api-key` ("Azure Api-Key
//! Authentication", type apiKey, in header). The key travels in that header,
//! with no prefix. 200 body `{value: [{dimension, scope, providerId,
//! utilization, holds, limit, period}], nextLink}`; `utilization`, `holds` and
//! `limit` are floats, `period` is `Monthly` or `None` (concurrent quotas),
//! `scope` is `Workspace` or `Subscription`. "Other Status Codes" return
//! `Azure.Core.Foundations.ErrorResponse`: `{"error": {code, message, target?,
//! details?, innererror?}}` with an `x-ms-error-code` header. The page shows
//! no example error body, so `azurequantum_401.json` is the smallest body that
//! matches that schema, built here rather than copied.
//!
//! Cost: the page states no price. Listing quotas submits no job; Azure
//! Quantum bills job execution, not its REST reads.
//!
//! Card: the `*_hours` row with the largest limit, in seconds. Remaining is
//! `limit - utilization - holds`. `MeterPeriod` is "Based on calendar year", so
//! a `Monthly` period ends at the start of the next UTC month. Other rows
//! become extras. `providerId` is a vendor name, not an account identifier,
//! but it is not needed and is not deserialized. The subscription id, resource
//! group and workspace name live only in the user's setting and the URL; the
//! Snapshot never sees them. `nextLink` is ignored: one page is read.

use std::time::Duration;

use async_trait::async_trait;
use jiff::Timestamp;
use reqwest::{StatusCode, Url};
use secrecy::SecretString;
use serde::Deserialize;

use super::digitalocean::start_of_next_utc_month;
use super::{error_message, http_status_message, malformed};
use crate::net::{Auth, Http, check_allowed};
use crate::provider::{Category, ConfigField, Provider, ProviderError, Snapshot, Unit};

pub struct AzureQuantum;

const ENDPOINT: &str = "https://<workspace address>/quotas?api-version=2026-01-15-preview";
const HOST: &str = "*.quantum.azure.com";
const API_VERSION: &str = "2026-01-15-preview";
const KEY_HEADER: &str = "x-ms-quantum-api-key";
const SECONDS_PER_HOUR: f64 = 3600.0;

#[derive(Deserialize)]
struct Body {
    value: Vec<Row>,
}

/// Numbers plus the three enumerations that say what a row counts.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Row {
    dimension: Option<String>,
    scope: Option<String>,
    utilization: Option<f64>,
    holds: Option<f64>,
    limit: Option<f64>,
    period: Option<String>,
}

/// `https://<ws>.<region>.quantum.azure.com/subscriptions/…/workspaces/<ws>/quotas?api-version=…`
/// from the user's setting, or why it cannot be used.
pub(crate) fn build_endpoint(config: Option<&str>) -> Result<String, ProviderError> {
    let raw = config
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .ok_or_else(|| ProviderError::Unsupported("Set the workspace address from the Azure portal before adding the key.".into()))?;
    if !raw.starts_with("https://") {
        return Err(ProviderError::Unsupported("The workspace address must start with https://.".into()));
    }
    let base = raw.trim_end_matches('/');
    let url = Url::parse(base).map_err(|_| ProviderError::Unsupported("The workspace address is not a valid URL.".into()))?;
    if url.query().is_some() || url.fragment().is_some() {
        return Err(ProviderError::Unsupported(
            "The workspace address should end at the workspace name, with no query string.".into(),
        ));
    }
    if check_allowed(&url, HOST).is_err() {
        return Err(ProviderError::Unsupported(
            "The workspace address must be a host under quantum.azure.com.".into(),
        ));
    }
    if !url.path().contains("/providers/Microsoft.Quantum/workspaces/") {
        return Err(ProviderError::Unsupported(
            "The workspace address should include /subscriptions/…/providers/Microsoft.Quantum/workspaces/<name>.".into(),
        ));
    }
    Ok(format!("{base}/quotas?api-version={API_VERSION}"))
}

/// `free_combined_job_hours` becomes `free combined job hours`.
fn words(dimension: &str) -> String {
    dimension.replace(['_', '-'], " ")
}

/// A float as a short figure: whole numbers without decimals, others with two.
fn figure(n: f64) -> String {
    if !n.is_finite() || n < 0.0 {
        "?".to_owned()
    } else if n.fract() == 0.0 {
        format!("{}", n as u64)
    } else {
        format!("{n:.2}")
    }
}

fn hours_to_seconds(hours: f64) -> Option<u64> {
    (hours.is_finite() && hours >= 0.0).then(|| (hours * SECONDS_PER_HOUR).floor() as u64)
}

#[async_trait]
impl Provider for AzureQuantum {
    fn id(&self) -> &'static str {
        "azurequantum"
    }

    fn display_name(&self) -> &'static str {
        "Azure Quantum"
    }

    fn docs_url(&self) -> &'static str {
        "https://learn.microsoft.com/en-us/rest/api/azurequantum/dataplane/quotas/list?view=rest-azurequantum-dataplane-2026-01-15-preview"
    }

    fn endpoint(&self) -> &'static str {
        ENDPOINT
    }

    fn allowed_host(&self) -> &'static str {
        HOST
    }

    fn unit(&self) -> Unit {
        Unit::Seconds
    }

    fn category(&self) -> Category {
        Category::Quantum
    }

    fn min_poll_interval(&self) -> Duration {
        Duration::from_secs(60)
    }

    fn note(&self) -> Option<&'static str> {
        Some("Shows the largest job-hours quota in seconds. The key is a workspace API key, sent in the x-ms-quantum-api-key header.")
    }

    fn config_field(&self) -> Option<ConfigField> {
        Some(ConfigField {
            label: "Workspace address",
            hint: "The workspace's data-plane address and resource path from the Azure portal, starting with https:// and ending at the workspace name. Not a key.",
            placeholder: "https://myws.eastus.quantum.azure.com/subscriptions/…/resourceGroups/…/providers/Microsoft.Quantum/workspaces/myws",
        })
    }

    async fn fetch(&self, key: &SecretString, config: Option<&str>, http: &Http) -> Result<Snapshot, ProviderError> {
        let endpoint = build_endpoint(config)?;
        let response = http
            .get(
                &endpoint,
                HOST,
                Auth::Header {
                    name: KEY_HEADER,
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
    let rows: Vec<&Row> = parsed.value.iter().filter(|r| r.limit.is_some_and(f64::is_finite)).collect();

    // Headline: the hours quota with the largest limit.
    let headline = rows
        .iter()
        .copied()
        .filter(|r| r.dimension.as_deref().is_some_and(|d| d.to_ascii_lowercase().contains("hours")))
        .max_by(|a, b| a.limit.unwrap_or(0.0).total_cmp(&b.limit.unwrap_or(0.0)));

    let mut snapshot = Snapshot::at(now);
    for row in &rows {
        let used = row.utilization.unwrap_or(0.0).max(0.0) + row.holds.unwrap_or(0.0).max(0.0);
        let limit = row.limit.unwrap_or(0.0);
        let dimension = row.dimension.as_deref().unwrap_or("quota");
        let scope = row.scope.as_deref().map(str::to_ascii_lowercase);
        if headline.is_some_and(|h| std::ptr::eq(h, *row)) {
            snapshot.remaining = hours_to_seconds((limit - used).max(0.0));
            snapshot.limit = hours_to_seconds(limit);
            if row.period.as_deref() == Some("Monthly") {
                snapshot.period_end = start_of_next_utc_month(now);
            }
            let mut label = words(dimension);
            if let Some(s) = &scope {
                label.push_str(&format!(", {s}"));
            }
            snapshot.extra.push(("Counting".to_owned(), label));
            if row.holds.is_some_and(|h| h > 0.0) {
                snapshot
                    .extra
                    .push(("On hold".to_owned(), format!("{} hours", figure(row.holds.unwrap_or(0.0)))));
            }
            continue;
        }
        let mut label = words(dimension);
        if let Some(s) = &scope {
            label.push_str(&format!(", {s}"));
        }
        snapshot.extra.push((label, format!("{} of {}", figure(used), figure(limit))));
    }
    if snapshot.remaining.is_none() && snapshot.extra.is_empty() {
        return Err(malformed());
    }
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &str = include_str!("../../tests/fixtures/azurequantum_200.json");
    const BAD_KEY: &str = include_str!("../../tests/fixtures/azurequantum_401.json");
    const WS: &str = "https://myws.eastus.quantum.azure.com/subscriptions/00000000-0000-0000-0000-000000000001/resourceGroups/my-resource-group/providers/Microsoft.Quantum/workspaces/my-workspace";

    fn run(status: StatusCode, body: &str) -> Result<Snapshot, ProviderError> {
        parse(status, None, body.as_bytes(), "2026-10-17T02:30:00Z".parse().unwrap())
    }

    #[test]
    fn documented_200_parses_in_seconds() {
        let s = run(StatusCode::OK, OK).unwrap();
        // 15 h limit, 0.001756 h used, 0.016667 h held: 14.98158 h left.
        assert_eq!(s.limit, Some(54_000));
        assert_eq!(s.remaining, Some(53_933));
        assert_eq!(s.period_end.unwrap().to_string(), "2026-11-01T00:00:00Z");
        assert_eq!(
            s.extra,
            vec![
                ("Counting".to_owned(), "free combined job hours, subscription".to_owned()),
                ("On hold".to_owned(), "0.02 hours".to_owned()),
                ("free fpga job hours, workspace".to_owned(), "0 of 0.25".to_owned()),
                ("free combined job hours, workspace".to_owned(), "0.00 of 1".to_owned()),
                ("concurrent cpu jobs, workspace".to_owned(), "0 of 10".to_owned()),
                ("concurrent fpga jobs, workspace".to_owned(), "0 of 1".to_owned()),
            ]
        );
    }

    #[test]
    fn no_hours_row_leaves_only_extras() {
        let body = OK.replace("job_hours", "job_units");
        let s = run(StatusCode::OK, &body).unwrap();
        assert_eq!((s.remaining, s.limit, s.period_end), (None, None, None));
        assert_eq!(s.extra.len(), 5);
    }

    #[test]
    fn empty_or_unparseable_body_is_malformed() {
        assert_eq!(run(StatusCode::OK, r#"{"value": [], "nextLink": null}"#), Err(malformed()));
        assert_eq!(run(StatusCode::OK, "<html>"), Err(malformed()));
    }

    #[test]
    fn identifiers_do_not_survive() {
        let body = OK.replace(
            "\"providerId\": \"Microsoft\"",
            "\"providerId\": \"ionq\", \"workspaceId\": \"ws-7f3a9b2c\"",
        );
        let s = run(StatusCode::OK, &body).unwrap();
        let dump = format!("{s:?}");
        for echoed in [
            "ionq",
            "ws-7f3a9b2c",
            "00000000-0000",
            "my-resource-group",
            "my-workspace",
            "Microsoft",
        ] {
            assert!(!dump.contains(echoed), "{dump}");
        }
    }

    #[test]
    fn documented_error_shapes_map() {
        assert_eq!(run(StatusCode::UNAUTHORIZED, BAD_KEY), Err(ProviderError::Unauthorized));
        assert_eq!(run(StatusCode::FORBIDDEN, BAD_KEY), Err(ProviderError::Unauthorized));
        let not_found = r#"{"error": {"code": "ResourceNotFound", "message": "Workspace not found.", "target": "workspaceName"}}"#;
        assert_eq!(
            run(StatusCode::NOT_FOUND, not_found),
            Err(ProviderError::Vendor("Workspace not found.".into()))
        );
        let e = parse(
            StatusCode::TOO_MANY_REQUESTS,
            Some(Duration::from_secs(5)),
            br#"{"error": {"code": "TooManyRequests", "message": "Slow down."}}"#,
            Timestamp::UNIX_EPOCH,
        )
        .unwrap_err();
        assert_eq!(
            e,
            ProviderError::RateLimited {
                retry_after: Some(Duration::from_secs(5)),
                message: Some("Slow down.".into())
            }
        );
        assert_eq!(
            run(StatusCode::BAD_GATEWAY, "<html>"),
            Err(ProviderError::Vendor("The vendor returned HTTP 502.".into()))
        );
    }

    #[test]
    fn endpoint_is_built_from_the_workspace_address() {
        assert_eq!(
            build_endpoint(Some(WS)).unwrap(),
            format!("{WS}/quotas?api-version=2026-01-15-preview")
        );
        assert_eq!(
            build_endpoint(Some(&format!("  {WS}/ "))).unwrap(),
            format!("{WS}/quotas?api-version=2026-01-15-preview")
        );
    }

    #[test]
    fn bad_workspace_addresses_are_refused() {
        for bad in [
            "",
            "http://myws.eastus.quantum.azure.com/subscriptions/x/providers/Microsoft.Quantum/workspaces/w",
            "https://quantum.azure.com/subscriptions/x/providers/Microsoft.Quantum/workspaces/w",
            "https://evil.example/subscriptions/x/providers/Microsoft.Quantum/workspaces/w",
            "https://myws.eastus.quantum.azure.com.evil.example/providers/Microsoft.Quantum/workspaces/w",
            "https://myws.eastus.quantum.azure.com/",
            "https://myws.eastus.quantum.azure.com/subscriptions/x/providers/Microsoft.Quantum/workspaces/w?api-version=1",
        ] {
            assert!(matches!(build_endpoint(Some(bad)), Err(ProviderError::Unsupported(_))), "{bad}");
        }
        assert!(matches!(build_endpoint(None), Err(ProviderError::Unsupported(_))));
    }

    #[test]
    fn built_endpoint_passes_the_allowlist() {
        let url = Url::parse(&build_endpoint(Some(WS)).unwrap()).unwrap();
        assert!(check_allowed(&url, HOST).is_ok());
    }
}
