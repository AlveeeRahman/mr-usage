//! The only HTTP path in the app. Providers get an `Http`, never a raw
//! `reqwest::Client`, so the host allowlist and redaction cannot be skipped.

use std::sync::Once;
use std::time::Duration;

use reqwest::header::{AUTHORIZATION, HeaderValue, RETRY_AFTER};
use reqwest::redirect::Policy;
use reqwest::{StatusCode, Url};
use secrecy::{ExposeSecret, SecretString};
use zeroize::Zeroizing;

use crate::provider::ProviderError;
use crate::redact;

pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Usage responses are a few hundred bytes. Anything far larger is refused.
pub const MAX_BODY_BYTES: usize = 64 * 1024;
const MAX_RETRY_AFTER: Duration = Duration::from_secs(24 * 60 * 60);

/// How a provider presents its key.
pub enum Auth<'a> {
    /// `Authorization: Bearer <key>`.
    Bearer(&'a SecretString),
    /// `?<name>=<key>`. Only for vendors whose documentation offers nothing else.
    Query { name: &'static str, value: &'a SecretString },
}

/// A response with the body copied into a zeroizing buffer.
pub struct RawResponse {
    pub status: StatusCode,
    pub retry_after: Option<Duration>,
    pub body: Zeroizing<Vec<u8>>,
}

pub struct Http {
    client: reqwest::Client,
}

fn install_crypto_provider() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        // Err means a provider is already installed, which is fine.
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

impl Http {
    pub fn new() -> Result<Self, ProviderError> {
        install_crypto_provider();
        let client = reqwest::Client::builder()
            .tls_backend_rustls()
            .tls_version_min(reqwest::tls::Version::TLS_1_2)
            .https_only(true)
            .redirect(Policy::none())
            .timeout(REQUEST_TIMEOUT)
            .connect_timeout(CONNECT_TIMEOUT)
            .user_agent(concat!("mr-usage/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(transport_error)?;
        Ok(Self { client })
    }

    /// GET `endpoint`, refusing to send unless its host is `allowed_host`.
    pub async fn get(&self, endpoint: &'static str, allowed_host: &'static str, auth: Auth<'_>) -> Result<RawResponse, ProviderError> {
        self.get_with_headers(endpoint, allowed_host, auth, &[]).await
    }

    /// `get` plus fixed, non-secret headers such as a vendor API version.
    pub async fn get_with_headers(
        &self,
        endpoint: &'static str,
        allowed_host: &'static str,
        auth: Auth<'_>,
        headers: &[(&'static str, &'static str)],
    ) -> Result<RawResponse, ProviderError> {
        let mut url = Url::parse(endpoint).map_err(|_| ProviderError::Transport("invalid endpoint".into()))?;
        check_allowed(&url, allowed_host)?;

        let mut request = self.client.get(url.clone());
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        match auth {
            Auth::Bearer(key) => {
                let secret = key.expose_secret();
                let mut text = Zeroizing::new(String::with_capacity(7 + secret.len()));
                text.push_str("Bearer ");
                text.push_str(secret);
                let mut value = HeaderValue::from_str(&text)
                    .map_err(|_| ProviderError::Transport("the key contains characters that cannot be sent".into()))?;
                value.set_sensitive(true);
                request = request.header(AUTHORIZATION, value);
            }
            Auth::Query { name, value } => {
                url.query_pairs_mut().append_pair(name, value.expose_secret());
                check_allowed(&url, allowed_host)?;
                request = self.client.get(url);
            }
        }

        let mut response = request.send().await.map_err(transport_error)?;
        let status = response.status();
        let retry_after = response
            .headers()
            .get(RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse::<u64>().ok())
            .map(|secs| Duration::from_secs(secs).min(MAX_RETRY_AFTER));

        if response.content_length().is_some_and(|len| len > MAX_BODY_BYTES as u64) {
            return Err(ProviderError::Transport("response too large".into()));
        }
        // Allocated once at full size so it never reallocates and leaves an
        // unzeroized copy behind.
        let mut body = Zeroizing::new(Vec::with_capacity(MAX_BODY_BYTES));
        while let Some(chunk) = response.chunk().await.map_err(transport_error)? {
            if body.len() + chunk.len() > MAX_BODY_BYTES {
                return Err(ProviderError::Transport("response too large".into()));
            }
            body.extend_from_slice(&chunk);
        }
        Ok(RawResponse { status, retry_after, body })
    }
}

/// Refuse any URL that is not `https://<allowed_host>/` on the default port
/// with no userinfo.
pub fn check_allowed(url: &Url, allowed_host: &str) -> Result<(), ProviderError> {
    let ok = url.scheme() == "https"
        && url.host_str() == Some(allowed_host)
        && url.port_or_known_default() == Some(443)
        && url.username().is_empty()
        && url.password().is_none();
    if ok {
        Ok(())
    } else {
        Err(ProviderError::Transport(format!(
            "refused to contact {} (not on the allowlist)",
            redact::redact_url(url.as_str())
        )))
    }
}

/// Convert a reqwest error without letting the request URL into the message.
pub fn transport_error(err: reqwest::Error) -> ProviderError {
    let err = err.without_url();
    let summary = if err.is_timeout() {
        "timed out"
    } else if err.is_connect() {
        "could not connect"
    } else if err.is_redirect() {
        "unexpected redirect"
    } else if err.is_body() || err.is_decode() {
        "could not read the response"
    } else if err.is_builder() {
        "could not build the request"
    } else {
        "request failed"
    };
    let mut message = String::from(summary);
    let mut source = std::error::Error::source(&err);
    let mut depth = 0;
    while let Some(cause) = source {
        if depth == 4 {
            break;
        }
        message.push_str(": ");
        message.push_str(&cause.to_string());
        source = cause.source();
        depth += 1;
    }
    ProviderError::Transport(redact::vendor_text(&message, 300))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowlist_accepts_exact_host() {
        let url = Url::parse("https://api.firecrawl.dev/v2/team/credit-usage").unwrap();
        assert!(check_allowed(&url, "api.firecrawl.dev").is_ok());
    }

    #[test]
    fn allowlist_rejects_everything_else() {
        for raw in [
            "http://api.firecrawl.dev/v2/team/credit-usage",
            "https://api.firecrawl.dev.evil.example/v2",
            "https://evil.example/?h=api.firecrawl.dev",
            "https://api.firecrawl.dev:8443/v2",
            "https://user@api.firecrawl.dev/v2",
        ] {
            let url = Url::parse(raw).unwrap();
            assert!(check_allowed(&url, "api.firecrawl.dev").is_err(), "{raw}");
        }
    }

    #[test]
    fn allowlist_error_does_not_echo_query() {
        let url = Url::parse("https://evil.example/x?api_key=0123456789abcdef0123456789abcdef").unwrap();
        let err = check_allowed(&url, "serpapi.com").unwrap_err().to_string();
        assert!(!err.contains("0123456789abcdef"), "{err}");
    }

    #[tokio::test]
    async fn get_refuses_hosts_off_the_allowlist_before_sending() {
        let http = Http::new().unwrap();
        let key = SecretString::from("fc-not-a-real-key-000000".to_owned());
        let result = http.get("https://example.com/", "api.firecrawl.dev", Auth::Bearer(&key)).await;
        assert!(matches!(result, Err(ProviderError::Transport(m)) if m.contains("allowlist")));
    }

    #[test]
    fn client_builds_with_ring_provider() {
        assert!(Http::new().is_ok());
    }
}
