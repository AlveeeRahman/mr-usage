//! The provider contract. A provider turns one vendor usage endpoint into a
//! `Snapshot` of plain numbers and dates. Nothing the vendor echoed back
//! survives `fetch`.

use std::time::Duration;

use async_trait::async_trait;
use jiff::Timestamp;
use secrecy::SecretString;
use serde::Serialize;

use crate::net::Http;

/// What `remaining` and `limit` count.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Unit {
    Credits,
    Searches,
    /// Elicit reports a percentage of plan usage, not a count.
    Percent,
}

/// One normalized reading. Holds no secrets and no account identifiers.
#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    pub taken_at: Timestamp,
    pub remaining: Option<u64>,
    pub limit: Option<u64>,
    pub period_end: Option<Timestamp>,
    /// Provider-specific label/value pairs for display only. Never secrets.
    pub extra: Vec<(String, String)>,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ProviderError {
    /// 401/403: the vendor rejected the key.
    #[error("The vendor rejected this API key.")]
    Unauthorized,
    #[error("Rate limited by the vendor{}", message.as_deref().map(|m| format!(": {m}")).unwrap_or_default())]
    RateLimited {
        retry_after: Option<Duration>,
        message: Option<String>,
    },
    /// The vendor returned a documented error. The text is already scrubbed.
    #[error("{0}")]
    Vendor(String),
    /// Network, TLS or timeout failure. Already redacted.
    #[error("Network error: {0}")]
    Transport(String),
    /// The provider has no usable endpoint for this account.
    #[error("{0}")]
    Unsupported(String),
}

impl ProviderError {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Unauthorized => "unauthorized",
            Self::RateLimited { .. } => "rateLimited",
            Self::Vendor(_) => "vendor",
            Self::Transport(_) => "transport",
            Self::Unsupported(_) => "unsupported",
        }
    }
}

#[async_trait]
pub trait Provider: Send + Sync {
    /// Stable id, also the keystore account name.
    fn id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    /// The documentation page the implementation was verified against.
    fn docs_url(&self) -> &'static str;
    /// The exact endpoint `fetch` calls, shown to the user before they save a key.
    fn endpoint(&self) -> &'static str;
    /// The only host this provider may contact.
    fn allowed_host(&self) -> &'static str;
    fn unit(&self) -> Unit;
    fn min_poll_interval(&self) -> Duration;
    /// A one-line caveat shown on the card, if any.
    fn note(&self) -> Option<&'static str> {
        None
    }
    async fn fetch(&self, key: &SecretString, http: &Http) -> Result<Snapshot, ProviderError>;
}

/// Parse an ISO 8601 / RFC 3339 timestamp. Returns `None` rather than failing
/// the whole snapshot when a vendor sends something unparseable.
pub fn parse_timestamp(raw: &str) -> Option<Timestamp> {
    raw.parse::<Timestamp>().ok()
}

/// Parse a calendar date such as `2026-11-16` and treat it as midnight UTC.
pub fn parse_date(raw: &str) -> Option<Timestamp> {
    let date: jiff::civil::Date = raw.parse().ok()?;
    date.to_zoned(jiff::tz::TimeZone::UTC).ok().map(|z| z.timestamp())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamps_parse() {
        assert!(parse_timestamp("2025-01-31T23:59:59Z").is_some());
        assert!(parse_timestamp("2025-01-31T23:59:59.123+02:00").is_some());
        assert!(parse_timestamp("string").is_none());
    }

    #[test]
    fn dates_parse() {
        let ts = parse_date("2026-11-16").expect("date");
        assert_eq!(ts.to_string(), "2026-11-16T00:00:00Z");
        assert!(parse_date("soon").is_none());
    }

    #[test]
    fn rate_limited_message_formats() {
        let e = ProviderError::RateLimited {
            retry_after: None,
            message: Some("Your account has run out of searches.".into()),
        };
        assert_eq!(e.to_string(), "Rate limited by the vendor: Your account has run out of searches.");
    }
}
