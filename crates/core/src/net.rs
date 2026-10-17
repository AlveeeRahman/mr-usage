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
    /// `<name>: <prefix><key>`, for vendors with their own header such as
    /// `xi-api-key` or `x-key`, or a scheme other than Bearer.
    Header {
        name: &'static str,
        prefix: &'static str,
        value: &'a SecretString,
    },
    /// `?<name>=<key>`. Only for vendors whose documentation offers nothing else.
    Query { name: &'static str, value: &'a SecretString },
}

/// A response with the body copied into a zeroizing buffer.
pub struct RawResponse {
    pub status: StatusCode,
    pub retry_after: Option<Duration>,
    pub body: Zeroizing<Vec<u8>>,
    /// Every header whose name contains `ratelimit`, lower-cased name and
    /// trimmed value. Brave Search reports its quota this way.
    pub rate_limit: Vec<(String, String)>,
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
    pub async fn get(&self, endpoint: &str, allowed_host: &str, auth: Auth<'_>) -> Result<RawResponse, ProviderError> {
        self.send(reqwest::Method::GET, endpoint, allowed_host, auth, &[], None).await
    }

    /// `get` plus fixed, non-secret headers such as a vendor API version.
    pub async fn get_with_headers(
        &self,
        endpoint: &str,
        allowed_host: &str,
        auth: Auth<'_>,
        headers: &[(&'static str, &'static str)],
    ) -> Result<RawResponse, ProviderError> {
        self.send(reqwest::Method::GET, endpoint, allowed_host, auth, headers, None).await
    }

    /// POST a fixed JSON document, for vendors whose usage query is GraphQL.
    /// The body is a constant chosen by the provider and never carries the key.
    pub async fn post_json(
        &self,
        endpoint: &str,
        allowed_host: &str,
        auth: Auth<'_>,
        headers: &[(&'static str, &'static str)],
        body: &'static str,
    ) -> Result<RawResponse, ProviderError> {
        self.send(reqwest::Method::POST, endpoint, allowed_host, auth, headers, Some(body))
            .await
    }

    async fn send(
        &self,
        method: reqwest::Method,
        endpoint: &str,
        allowed_host: &str,
        auth: Auth<'_>,
        headers: &[(&'static str, &'static str)],
        body: Option<&'static str>,
    ) -> Result<RawResponse, ProviderError> {
        let mut url = Url::parse(endpoint).map_err(|_| ProviderError::Transport("invalid endpoint".into()))?;
        check_allowed(&url, allowed_host)?;

        let mut secret_header: Option<(&'static str, HeaderValue)> = None;
        match auth {
            Auth::Bearer(key) => {
                secret_header = Some((AUTHORIZATION.as_str(), sensitive_value("Bearer ", key)?));
            }
            Auth::Header { name, prefix, value } => {
                secret_header = Some((name, sensitive_value(prefix, value)?));
            }
            Auth::Query { name, value } => {
                url.query_pairs_mut().append_pair(name, value.expose_secret());
                check_allowed(&url, allowed_host)?;
            }
        }

        let mut request = self.client.request(method, url);
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        if let Some((name, value)) = secret_header {
            request = request.header(name, value);
        }
        if let Some(body) = body {
            request = request.header(reqwest::header::CONTENT_TYPE, "application/json").body(body);
        }

        let mut response = request.send().await.map_err(transport_error)?;
        let status = response.status();
        let retry_after = response
            .headers()
            .get(RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse::<u64>().ok())
            .map(|secs| Duration::from_secs(secs).min(MAX_RETRY_AFTER));
        let rate_limit = response
            .headers()
            .iter()
            .filter(|(name, _)| name.as_str().contains("ratelimit"))
            .filter_map(|(name, value)| Some((name.as_str().to_owned(), value.to_str().ok()?.trim().to_owned())))
            .collect();

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
        Ok(RawResponse {
            status,
            retry_after,
            body,
            rate_limit,
        })
    }
}

/// `<prefix><key>` as a header value marked sensitive, built in a zeroizing
/// buffer so no plain copy of the key is left on the heap.
fn sensitive_value(prefix: &str, key: &SecretString) -> Result<HeaderValue, ProviderError> {
    let secret = key.expose_secret();
    let mut text = Zeroizing::new(String::with_capacity(prefix.len() + secret.len()));
    text.push_str(prefix);
    text.push_str(secret);
    let mut value =
        HeaderValue::from_str(&text).map_err(|_| ProviderError::Transport("the key contains characters that cannot be sent".into()))?;
    value.set_sensitive(true);
    Ok(value)
}

/// Refuse any URL that is not `https://<allowed_host>/` on the default port
/// with no userinfo. An allowlist entry of the form `*.example.com` accepts
/// any host below `example.com`, for vendors with one host per workspace.
pub fn check_allowed(url: &Url, allowed_host: &str) -> Result<(), ProviderError> {
    let host_ok = match (url.host_str(), allowed_host.strip_prefix("*.")) {
        (Some(host), Some(suffix)) => {
            host.len() > suffix.len() + 1 && host.ends_with(suffix) && host.as_bytes()[host.len() - suffix.len() - 1] == b'.'
        }
        (Some(host), None) => host == allowed_host,
        (None, _) => false,
    };
    let ok = url.scheme() == "https"
        && host_ok
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
    fn allowlist_suffix_accepts_one_or_more_labels_below_it_only() {
        for ok in ["https://ws1.eastus.quantum.azure.com/q", "https://a.quantum.azure.com/"] {
            assert!(check_allowed(&Url::parse(ok).unwrap(), "*.quantum.azure.com").is_ok(), "{ok}");
        }
        for bad in [
            "https://quantum.azure.com/",
            "https://evilquantum.azure.com/",
            "https://quantum.azure.com.evil.example/",
        ] {
            assert!(check_allowed(&Url::parse(bad).unwrap(), "*.quantum.azure.com").is_err(), "{bad}");
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
