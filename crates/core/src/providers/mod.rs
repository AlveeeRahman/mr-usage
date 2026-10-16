//! One module per provider. To add a vendor, add a module, implement
//! `Provider`, and list it in `registry`.

use std::sync::Arc;

use serde::Deserialize;

use crate::provider::{Provider, ProviderError};
use crate::redact;

pub mod apify;
pub mod context7;
pub mod elicit;
pub mod firecrawl;
pub mod hunter;
pub mod openrouter;
pub mod runway;
pub mod serpapi;
pub mod tavily;

/// Every provider the app knows about, in display order.
pub fn registry() -> Vec<Arc<dyn Provider>> {
    vec![
        Arc::new(firecrawl::Firecrawl),
        Arc::new(serpapi::SerpApi),
        Arc::new(elicit::Elicit),
        Arc::new(context7::Context7),
        Arc::new(tavily::Tavily),
        Arc::new(openrouter::OpenRouter),
        Arc::new(apify::Apify),
        Arc::new(hunter::Hunter),
        Arc::new(runway::Runway),
    ]
}

/// Longest vendor error string shown to the user.
pub(crate) const MAX_VENDOR_TEXT: usize = 200;

/// Pull a human-readable message out of an error body. Handles the shapes
/// the vendors document: `{"error": "text"}`, `{"error": {"message": "text"}}`
/// (with `code` as a string or, at OpenRouter, a number) and
/// `{"message": "text"}`.
pub(crate) fn error_message(body: &[u8]) -> Option<String> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum ErrorField {
        Text(String),
        Object {
            message: Option<String>,
            code: Option<serde_json::Value>,
        },
    }
    #[derive(Deserialize)]
    struct Envelope {
        error: Option<ErrorField>,
        message: Option<String>,
    }
    let envelope: Envelope = serde_json::from_slice(body).ok()?;
    let text = match (envelope.error, envelope.message) {
        (Some(ErrorField::Object { message: Some(m), .. }), _) => m,
        (
            Some(ErrorField::Object {
                message: None,
                code: Some(c),
            }),
            _,
        ) => match c {
            serde_json::Value::String(s) => s,
            other => other.to_string(),
        },
        (Some(ErrorField::Text(t)), Some(m)) => format!("{t}. {m}"),
        (Some(ErrorField::Text(t)), None) => t,
        (_, Some(m)) => m,
        _ => return None,
    };
    let clean = redact::vendor_text(&text, MAX_VENDOR_TEXT);
    (!clean.is_empty()).then_some(clean)
}

/// The `error.code` field of an `{"error": {"code": ...}}` body, if present.
pub(crate) fn error_code(body: &[u8]) -> Option<String> {
    #[derive(Deserialize)]
    struct Inner {
        code: Option<String>,
    }
    #[derive(Deserialize)]
    struct Envelope {
        error: Option<serde_json::Value>,
    }
    let envelope: Envelope = serde_json::from_slice(body).ok()?;
    let inner: Inner = serde_json::from_value(envelope.error?).ok()?;
    inner.code
}

/// Fallback when an error body has no message we can read.
pub(crate) fn http_status_message(status: reqwest::StatusCode) -> String {
    format!("The vendor returned HTTP {}.", status.as_u16())
}

pub(crate) fn malformed() -> ProviderError {
    ProviderError::Vendor("The vendor's response did not match its documentation.".into())
}

/// A non-negative JSON number as a whole count. Fractions round down.
pub(crate) fn count(v: Option<f64>) -> Option<u64> {
    v.filter(|n| n.is_finite() && *n >= 0.0).map(|n| n.floor() as u64)
}

/// A non-negative dollar amount as whole cents, rounded to the nearest cent.
pub(crate) fn cents(usd: Option<f64>) -> Option<u64> {
    usd.filter(|n| n.is_finite() && *n >= 0.0).map(|n| (n * 100.0).round() as u64)
}

/// `$12.34` from cents.
pub(crate) fn dollars(cents: u64) -> String {
    format!("${}.{:02}", cents / 100, cents % 100)
}

/// A count with thousands separators is the frontend's job; here a plain
/// integer string is enough for `extra` values.
pub(crate) fn whole(n: f64) -> String {
    if n.is_finite() && n >= 0.0 {
        (n.floor() as u64).to_string()
    } else {
        "?".to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_ids_are_unique_and_stable() {
        let ids: Vec<_> = registry().iter().map(|p| p.id()).collect();
        assert_eq!(
            ids,
            [
                "firecrawl",
                "serpapi",
                "elicit",
                "context7",
                "tavily",
                "openrouter",
                "apify",
                "hunter",
                "runway"
            ]
        );
    }

    #[test]
    fn every_provider_endpoint_is_on_its_own_allowlist() {
        for p in registry() {
            let url = reqwest::Url::parse(p.endpoint()).unwrap();
            assert!(crate::net::check_allowed(&url, p.allowed_host()).is_ok(), "{}", p.id());
        }
    }

    #[test]
    fn every_provider_has_a_docs_page_and_a_floor_of_at_least_a_minute() {
        for p in registry() {
            assert!(p.docs_url().starts_with("https://"), "{}", p.id());
            assert!(p.min_poll_interval().as_secs() >= 60, "{}", p.id());
        }
    }

    #[test]
    fn error_shapes() {
        assert_eq!(error_message(br#"{"error":"plain"}"#).as_deref(), Some("plain"));
        assert_eq!(
            error_message(br#"{"error":{"code":"c","message":"nested"}}"#).as_deref(),
            Some("nested")
        );
        assert_eq!(
            error_message(br#"{"error":{"code":401,"message":"Missing Authentication header"}}"#).as_deref(),
            Some("Missing Authentication header")
        );
        assert_eq!(error_message(br#"{"error":{"code":503}}"#).as_deref(), Some("503"));
        assert_eq!(
            error_message(br#"{"error":"Rate limit exceeded","message":"Slow down."}"#).as_deref(),
            Some("Rate limit exceeded. Slow down.")
        );
        assert_eq!(error_message(b"<html>"), None);
        assert_eq!(
            error_code(br#"{"error":{"code":"usage_unavailable","message":"m"}}"#).as_deref(),
            Some("usage_unavailable")
        );
    }

    #[test]
    fn number_helpers() {
        assert_eq!(count(Some(9450.9)), Some(9450));
        assert_eq!(count(Some(-1.0)), None);
        assert_eq!(count(Some(f64::NAN)), None);
        assert_eq!(cents(Some(74.5)), Some(7450));
        assert_eq!(cents(Some(0.005)), Some(1));
        assert_eq!(cents(None), None);
        assert_eq!(dollars(7450), "$74.50");
        assert_eq!(dollars(5), "$0.05");
        assert_eq!(whole(500.784475), "500");
    }
}
