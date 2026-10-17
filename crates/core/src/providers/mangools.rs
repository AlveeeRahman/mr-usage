//! Mangools (KWFinder, SERPChecker, SERPWatcher, LinkMiner, SiteProfiler)
//! resource limits.
//!
//! Verified 2026-10-17 against <https://apidocs.mangools.com/kwfinder/get-limits/>:
//! `GET https://api.mangools.com/v3/kwfinder/limits` with the header
//! `x-access-token: <key>` (the page stresses it is not a Bearer header).
//! 200 body holds, among plan metadata, `resources: {<pool>: {limit,
//! remaining, reset (seconds, or null)}}` for pools such as `serps`,
//! `related-keywords`, `trends`, `links` and `tracked_keywords`. The only
//! documented error is 429 with a `text/html` "429 Too Many Requests" page.
//! The page does not state a price for the call; it describes it as being
//! for "quota checks before sending expensive requests", and the example
//! response's own `remaining` figures are the measure of consumption.
//!
//! The page warns that authentication is optional here: a missing or unknown
//! token gets HTTP 200 "with nothing remaining in any pool rather than HTTP
//! 401". So a body in which no pool has a positive limit is reported as a
//! rejected key. A real account whose pools are merely exhausted still has
//! positive limits and is shown as such.
//!
//! The headline is the `serps` pool (SERP lookups). Other documented pools
//! appear as extras under plain-word labels; pools the docs do not name are
//! only counted, so no vendor string reaches the card. `highestPlan`,
//! `platformPlan` and `limit_options` are never deserialized.

use std::collections::BTreeMap;
use std::time::Duration;

use async_trait::async_trait;
use jiff::{SignedDuration, Timestamp};
use reqwest::StatusCode;
use secrecy::SecretString;
use serde::Deserialize;

use super::{count, error_message, http_status_message, malformed};
use crate::net::{Auth, Http};
use crate::provider::{Category, Provider, ProviderError, Snapshot, Unit};

pub struct Mangools;

const ENDPOINT: &str = "https://api.mangools.com/v3/kwfinder/limits";
const HOST: &str = "api.mangools.com";
const HEADLINE: &str = "serps";
/// Plain-word labels for the pools the docs list, in the order they appear.
const LABELS: &[(&str, &str)] = &[
    ("related-keywords", "Related keywords"),
    ("trends", "Trends"),
    ("tracked_keywords", "Tracked keywords"),
    ("links", "Backlink rows"),
    ("sch-url-metrics", "SERPChecker URL metrics"),
    ("kw-url-metrics", "KWFinder URL metrics"),
    ("lm-url-metrics", "LinkMiner URL metrics"),
    ("sp-overview", "SiteProfiler overviews"),
    ("lm-suggest", "LinkMiner suggestions"),
    ("ai-watcher-prompts", "AI Watcher prompts"),
    ("ai-watcher-monitors", "AI Watcher monitors"),
    ("checked-keywords-ranks", "Checked keyword ranks"),
];

#[derive(Deserialize)]
struct Body {
    resources: Option<BTreeMap<String, Pool>>,
}

#[derive(Deserialize)]
struct Pool {
    limit: Option<f64>,
    remaining: Option<f64>,
    reset: Option<f64>,
}

#[async_trait]
impl Provider for Mangools {
    fn id(&self) -> &'static str {
        "mangools"
    }

    fn display_name(&self) -> &'static str {
        "Mangools"
    }

    fn docs_url(&self) -> &'static str {
        "https://apidocs.mangools.com/kwfinder/get-limits/"
    }

    fn endpoint(&self) -> &'static str {
        ENDPOINT
    }

    fn allowed_host(&self) -> &'static str {
        HOST
    }

    fn unit(&self) -> Unit {
        Unit::Requests
    }

    fn category(&self) -> Category {
        Category::Seo
    }

    fn min_poll_interval(&self) -> Duration {
        Duration::from_secs(60)
    }

    fn note(&self) -> Option<&'static str> {
        Some(
            "Headline is the SERP lookup pool. Mangools answers an unknown token with empty pools, so a bad key shows as rejected only when every pool is empty.",
        )
    }

    async fn fetch(&self, key: &SecretString, _config: Option<&str>, http: &Http) -> Result<Snapshot, ProviderError> {
        let auth = Auth::Header {
            name: "x-access-token",
            prefix: "",
            value: key,
        };
        let response = http.get(ENDPOINT, HOST, auth).await?;
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
    let pools = parsed.resources.unwrap_or_default();
    if !pools.values().any(|p| count(p.limit).is_some_and(|l| l > 0)) {
        return Err(ProviderError::Unauthorized);
    }
    let headline = pools.get(HEADLINE);
    let period_end = headline
        .and_then(|p| count(p.reset))
        .and_then(|secs| i64::try_from(secs).ok())
        .and_then(|secs| now.checked_add(SignedDuration::from_secs(secs)).ok());
    let mut extra = Vec::new();
    let mut unnamed = 0u32;
    for (name, pool) in &pools {
        if name == HEADLINE {
            continue;
        }
        let (Some(remaining), Some(limit)) = (count(pool.remaining), count(pool.limit)) else {
            continue;
        };
        match LABELS.iter().find(|(key, _)| *key == name) {
            Some((_, label)) => extra.push(((*label).to_owned(), format!("{remaining} of {limit}"))),
            None => unnamed += 1,
        }
    }
    if unnamed > 0 {
        extra.push(("Other pools".to_owned(), unnamed.to_string()));
    }
    Ok(Snapshot {
        taken_at: now,
        remaining: headline.and_then(|p| count(p.remaining)),
        limit: headline.and_then(|p| count(p.limit)),
        period_end,
        spent: None,
        spend_per_hour: None,
        unit: None,
        extra,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &str = include_str!("../../tests/fixtures/mangools_200.json");
    const LIMITED: &str = include_str!("../../tests/fixtures/mangools_429.json");

    fn run(status: StatusCode, body: &str) -> Result<Snapshot, ProviderError> {
        parse(status, None, body.as_bytes(), Timestamp::UNIX_EPOCH)
    }

    #[test]
    fn documented_200_parses() {
        let s = run(StatusCode::OK, OK).unwrap();
        assert_eq!(s.remaining, Some(496));
        assert_eq!(s.limit, Some(500));
        assert_eq!(s.period_end.unwrap().to_string(), "1970-01-01T02:09:29Z");
        assert!(s.extra.contains(&("Backlink rows".to_owned(), "99500 of 100000".to_owned())));
        assert!(s.extra.contains(&("Tracked keywords".to_owned(), "1651 of 1651".to_owned())));
        assert!(s.extra.contains(&("AI Watcher monitors".to_owned(), "2 of 3".to_owned())));
        assert_eq!(s.extra.len(), 12);
        assert!(!s.extra.iter().any(|(k, _)| k == "Other pools"));
    }

    #[test]
    fn plan_strings_do_not_survive() {
        let s = run(StatusCode::OK, OK).unwrap();
        let dump = format!("{s:?}");
        for echoed in ["premium", "BASIC", "orange", "tooltip"] {
            assert!(!dump.contains(echoed), "{echoed}: {dump}");
        }
    }

    #[test]
    fn unknown_pool_names_are_counted_not_shown() {
        let body = OK.replace("\"trends\"", "\"secret-project-x\"");
        let s = run(StatusCode::OK, &body).unwrap();
        assert!(!format!("{s:?}").contains("secret-project-x"));
        assert!(s.extra.contains(&("Other pools".to_owned(), "1".to_owned())));
    }

    #[test]
    fn empty_pools_mean_a_rejected_token() {
        assert_eq!(run(StatusCode::OK, "{}"), Err(ProviderError::Unauthorized));
        assert_eq!(run(StatusCode::OK, r#"{"resources":{}}"#), Err(ProviderError::Unauthorized));
        assert_eq!(
            run(StatusCode::OK, r#"{"resources":{"serps":{"limit":0,"remaining":0,"reset":null}}}"#),
            Err(ProviderError::Unauthorized)
        );
    }

    #[test]
    fn exhausted_pools_are_still_a_reading() {
        let s = run(StatusCode::OK, r#"{"resources":{"serps":{"limit":500,"remaining":0,"reset":10}}}"#).unwrap();
        assert_eq!((s.remaining, s.limit), (Some(0), Some(500)));
    }

    #[test]
    fn documented_429_html_maps() {
        assert_eq!(
            run(StatusCode::TOO_MANY_REQUESTS, LIMITED),
            Err(ProviderError::RateLimited {
                retry_after: None,
                message: None
            })
        );
    }

    #[test]
    fn other_statuses_map() {
        assert_eq!(run(StatusCode::UNAUTHORIZED, ""), Err(ProviderError::Unauthorized));
        assert_eq!(
            run(StatusCode::INTERNAL_SERVER_ERROR, ""),
            Err(ProviderError::Vendor("The vendor returned HTTP 500.".into()))
        );
        assert_eq!(run(StatusCode::OK, "<html>"), Err(malformed()));
    }
}
