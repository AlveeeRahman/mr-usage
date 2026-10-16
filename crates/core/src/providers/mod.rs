//! One module per provider. To add a vendor, add a module, implement
//! `Provider`, and list it in `registry`.

use std::sync::Arc;

use serde::Deserialize;

use crate::provider::{Provider, ProviderError};
use crate::redact;

pub mod elicit;
pub mod firecrawl;
pub mod serpapi;

/// Every provider the app knows about, in display order.
pub fn registry() -> Vec<Arc<dyn Provider>> {
    vec![Arc::new(firecrawl::Firecrawl), Arc::new(serpapi::SerpApi), Arc::new(elicit::Elicit)]
}

/// Longest vendor error string shown to the user.
pub(crate) const MAX_VENDOR_TEXT: usize = 200;

/// Pull a human-readable message out of an error body. Handles the three
/// shapes the vendors document: `{"error": "text"}`,
/// `{"error": {"message": "text"}}` and `{"message": "text"}`.
pub(crate) fn error_message(body: &[u8]) -> Option<String> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum ErrorField {
        Text(String),
        Object { message: Option<String>, code: Option<String> },
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
        ) => c,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_ids_are_unique_and_stable() {
        let ids: Vec<_> = registry().iter().map(|p| p.id()).collect();
        assert_eq!(ids, ["firecrawl", "serpapi", "elicit"]);
    }

    #[test]
    fn every_provider_endpoint_is_on_its_own_allowlist() {
        for p in registry() {
            let url = reqwest::Url::parse(p.endpoint()).unwrap();
            assert!(crate::net::check_allowed(&url, p.allowed_host()).is_ok(), "{}", p.id());
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
            error_message(br#"{"error":"Rate limit exceeded","message":"Slow down."}"#).as_deref(),
            Some("Rate limit exceeded. Slow down.")
        );
        assert_eq!(error_message(b"<html>"), None);
        assert_eq!(
            error_code(br#"{"error":{"code":"usage_unavailable","message":"m"}}"#).as_deref(),
            Some("usage_unavailable")
        );
    }
}
