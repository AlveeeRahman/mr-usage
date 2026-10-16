//! Scrubbing for anything that might reach a log, an error message or the UI.
//!
//! The rules are deliberately greedy. Over-redacting a log line costs nothing;
//! leaking a key costs the owner money.

use std::io::{self, Write};
use std::sync::{Arc, Mutex};

use tracing_subscriber::fmt::MakeWriter;

const REDACTED: &str = "[redacted]";

/// Words after which the next token is treated as a secret.
const MARKERS: &[&str] = &[
    "api_key",
    "apikey",
    "api-key",
    "x-api-key",
    "access_token",
    "token",
    "secret",
    "password",
    "bearer",
    "authorization",
    "proxy-authorization",
];

/// HTTP auth scheme names. After a marker, a scheme word is redacted and the
/// redaction carries on to the credential that follows it
/// (`Authorization: Basic <value>`).
const AUTH_SCHEMES: &[&str] = &["basic", "digest", "negotiate", "ntlm"];

/// Prefixes of vendor key formats. Firecrawl documents `fc-`, Elicit documents
/// `elk_live_` (docs/SOURCES.md, Elicit row, 2026-10-16). TODO(verify): the
/// Firecrawl `fc-` prefix and `elk_test_` have no recorded source.
const KEY_PREFIXES: &[&str] = &["fc-", "elk_live_", "elk_test_"];

/// A bare token at least this long that mixes letters and digits is treated as
/// a key. SerpApi keys are 64 hex characters. TODO(verify): no source is
/// recorded for that length; the rule does not depend on it.
const LONG_TOKEN: usize = 32;

/// Remove the userinfo, query string and fragment from a URL-shaped string.
pub fn redact_url(raw: &str) -> String {
    let (scheme, rest) = match raw.find("://") {
        Some(i) => (&raw[..i + 3], &raw[i + 3..]),
        None => ("", raw),
    };
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, tail) = rest.split_at(authority_end);
    let host = match authority.rfind('@') {
        Some(i) => &authority[i + 1..],
        None => authority,
    };
    let (path, had_query) = match tail.find(['?', '#']) {
        Some(i) => (&tail[..i], true),
        None => (tail, false),
    };
    let mut out = String::with_capacity(scheme.len() + host.len() + path.len() + 12);
    out.push_str(scheme);
    out.push_str(host);
    out.push_str(path);
    if had_query {
        out.push('?');
        out.push_str(REDACTED);
    }
    out
}

/// Scrub free text: URLs lose their query strings, values after secret
/// markers are replaced, and key-shaped tokens are replaced.
pub fn scrub(text: &str) -> String {
    scrub_tokens(&scrub_urls(text))
}

fn scrub_urls(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = find_url_start(rest) {
        out.push_str(&rest[..start]);
        let candidate = &rest[start..];
        let end = candidate
            .find(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | '<' | '>' | '`' | ')' | ']' | '}' | ','))
            .unwrap_or(candidate.len());
        out.push_str(&redact_url(&candidate[..end]));
        rest = &candidate[end..];
    }
    out.push_str(rest);
    out
}

fn find_url_start(text: &str) -> Option<usize> {
    let lower = text.to_ascii_lowercase();
    match (lower.find("https://"), lower.find("http://")) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '~' | '+' | '/')
}

fn scrub_tokens(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut redact_next = false;
    let mut chars = text.char_indices().peekable();
    while let Some(&(start, c)) = chars.peek() {
        if is_word_char(c) {
            let mut end = start;
            while let Some(&(i, ch)) = chars.peek() {
                if !is_word_char(ch) {
                    break;
                }
                end = i + ch.len_utf8();
                chars.next();
            }
            let word = &text[start..end];
            // Never touch our own placeholder, so scrubbing is idempotent.
            let placeholder = word == "redacted" && out.ends_with('[');
            // Marker words go too, so a log grep for `api_key` or
            // `Authorization` comes back empty.
            if !placeholder && (redact_next || is_marker(word) || looks_like_key(word)) {
                out.push_str(REDACTED);
            } else {
                out.push_str(word);
            }
            redact_next = is_marker(word) || (redact_next && is_auth_scheme(word));
        } else {
            out.push(c);
            chars.next();
            // A newline ends any "marker: value" pair.
            if c == '\n' {
                redact_next = false;
            }
        }
    }
    out
}

fn is_marker(word: &str) -> bool {
    let lower = word.to_ascii_lowercase();
    MARKERS.iter().any(|m| lower == *m)
}

fn is_auth_scheme(word: &str) -> bool {
    let lower = word.to_ascii_lowercase();
    AUTH_SCHEMES.iter().any(|s| lower == *s)
}

fn has_key_prefix(word: &str) -> bool {
    KEY_PREFIXES.iter().any(|p| word.starts_with(p) && word.len() >= p.len() + 8)
}

/// Each `/`-separated segment is checked on its own, so a key inside a URL
/// path or after a slash is still caught, while a long file path made of
/// short segments is left alone.
fn looks_like_key(word: &str) -> bool {
    has_key_prefix(word)
        || word.split('/').any(|seg| {
            has_key_prefix(seg)
                || (seg.len() >= LONG_TOKEN && seg.chars().any(|c| c.is_ascii_digit()) && seg.chars().any(|c| c.is_ascii_alphabetic()))
        })
}

/// Strip control characters, collapse whitespace, truncate and scrub a string
/// that came from a vendor before it is shown to the user.
pub fn vendor_text(raw: &str, max_chars: usize) -> String {
    let mut cleaned = String::with_capacity(raw.len().min(max_chars * 4));
    let mut last_space = false;
    for c in raw.chars() {
        let c = if c.is_control() || c.is_whitespace() { ' ' } else { c };
        if c == ' ' {
            if last_space {
                continue;
            }
            last_space = true;
        } else {
            last_space = false;
        }
        cleaned.push(c);
    }
    // Scrub before truncating: a cut that lands inside a key would leave a
    // fragment too short for the key-shape rules to recognise.
    let scrubbed = scrub(cleaned.trim());
    if scrubbed.chars().count() > max_chars {
        let mut s: String = scrubbed.chars().take(max_chars).collect();
        s.push('…');
        s
    } else {
        scrubbed
    }
}

/// A `MakeWriter` that scrubs each formatted log event before it reaches the
/// wrapped writer. Install it as the only writer of the only subscriber.
#[derive(Clone)]
pub struct RedactingMakeWriter<M> {
    inner: M,
}

impl<M> RedactingMakeWriter<M> {
    pub fn new(inner: M) -> Self {
        Self { inner }
    }
}

impl<'a, M> MakeWriter<'a> for RedactingMakeWriter<M>
where
    M: MakeWriter<'a>,
{
    type Writer = RedactingWriter<M::Writer>;

    fn make_writer(&'a self) -> Self::Writer {
        RedactingWriter {
            inner: self.inner.make_writer(),
            buf: Vec::with_capacity(256),
        }
    }
}

/// Buffers one log event and writes the scrubbed text when dropped.
pub struct RedactingWriter<W: Write> {
    inner: W,
    buf: Vec<u8>,
}

impl<W: Write> Write for RedactingWriter<W> {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.buf.extend_from_slice(data);
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<W: Write> Drop for RedactingWriter<W> {
    fn drop(&mut self) {
        let text = String::from_utf8_lossy(&self.buf);
        let clean = scrub(&text);
        // Nowhere to report a failed log write; dropping it is the only option.
        let _ = self.inner.write_all(clean.as_bytes());
        let _ = self.inner.flush();
        zeroize::Zeroize::zeroize(&mut self.buf);
    }
}

/// An in-memory log sink for tests.
#[derive(Clone, Default)]
pub struct CaptureWriter(pub Arc<Mutex<Vec<u8>>>);

impl CaptureWriter {
    pub fn contents(&self) -> String {
        match self.0.lock() {
            Ok(guard) => String::from_utf8_lossy(&guard).into_owned(),
            Err(poisoned) => String::from_utf8_lossy(&poisoned.into_inner()).into_owned(),
        }
    }
}

impl Write for CaptureWriter {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        match self.0.lock() {
            Ok(mut guard) => guard.extend_from_slice(data),
            Err(poisoned) => poisoned.into_inner().extend_from_slice(data),
        }
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for CaptureWriter {
    type Writer = CaptureWriter;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SERP_KEY: &str = "0f1e2d3c4b5a69788796a5b4c3d2e1f00f1e2d3c4b5a69788796a5b4c3d2e1f0";
    const FC_KEY: &str = "fc-4f2a9c0d7e1b4a3c8d5e6f7a8b9c0d1e";

    #[test]
    fn url_query_is_removed() {
        let url = format!("https://serpapi.com/account.json?api_key={SERP_KEY}");
        let out = redact_url(&url);
        assert_eq!(out, "https://serpapi.com/account.json?[redacted]");
    }

    #[test]
    fn url_userinfo_is_removed() {
        assert_eq!(
            redact_url("https://user:pw@example.com/a?b=c#d"),
            "https://example.com/a?[redacted]"
        );
    }

    #[test]
    fn url_inside_text_is_scrubbed() {
        let text = format!("error sending request for url (https://serpapi.com/account.json?api_key={SERP_KEY}): timed out");
        let out = scrub(&text);
        assert!(!out.contains(SERP_KEY), "{out}");
        assert!(out.contains("https://serpapi.com/account.json?[redacted]"), "{out}");
    }

    #[test]
    fn bearer_and_authorization_are_scrubbed() {
        let text = format!("Authorization: Bearer {FC_KEY}");
        let out = scrub(&text);
        assert!(!out.contains(FC_KEY), "{out}");
        assert!(!out.contains("Authorization"), "{out}");
    }

    #[test]
    fn key_value_pairs_are_scrubbed() {
        for text in [
            format!("api_key={SERP_KEY}"),
            format!("{{\"api_key\": \"{SERP_KEY}\"}}"),
            "token=short".to_owned(),
        ] {
            let out = scrub(&text);
            assert!(!out.contains(SERP_KEY) && !out.contains("short"), "{out}");
        }
    }

    #[test]
    fn bare_key_shaped_tokens_are_scrubbed() {
        let out = scrub(&format!("saw {FC_KEY} and {SERP_KEY} and elk_live_abcdefgh1234"));
        assert!(!out.contains(FC_KEY));
        assert!(!out.contains(SERP_KEY));
        assert!(!out.contains("elk_live_abcdefgh1234"));
    }

    #[test]
    fn keys_after_a_slash_are_scrubbed() {
        for text in [
            format!("https://elicit.com/api/v2/{SERP_KEY}"),
            format!("path/{SERP_KEY}"),
            format!("x/{FC_KEY}"),
        ] {
            let out = scrub(&text);
            assert!(!out.contains(SERP_KEY) && !out.contains(FC_KEY), "{out}");
        }
        // A file path made of short segments is not mistaken for a key.
        let path = "/usr/lib/x86_64-linux-gnu/libwebkit2gtk-4.1.so.0";
        assert_eq!(scrub(path), path);
    }

    #[test]
    fn auth_scheme_does_not_end_redaction() {
        for text in ["Authorization: Basic dXNlcjpwYXNz", "Proxy-Authorization: Digest abc123"] {
            let out = scrub(text);
            assert!(!out.contains("dXNlcjpwYXNz") && !out.contains("abc123"), "{out}");
        }
    }

    #[test]
    fn truncation_cannot_leave_a_key_fragment() {
        // The key starts 20 characters before the cut, so truncating first
        // would leave a 20-character fragment that no rule recognises.
        let text = format!("{} {SERP_KEY}", "a".repeat(179));
        let out = vendor_text(&text, 200);
        assert!(!out.contains(&SERP_KEY[..20]), "{out}");
    }

    #[test]
    fn ordinary_text_survives() {
        let text = "Could not find credit usage information";
        assert_eq!(scrub(text), text);
    }

    #[test]
    fn scrub_is_idempotent() {
        let once = scrub(&format!("api_key={SERP_KEY}"));
        assert_eq!(scrub(&once), once);
    }

    #[test]
    fn vendor_text_strips_controls_and_truncates() {
        let out = vendor_text("bad\u{1b}[31m\nthing\t\there", 100);
        assert_eq!(out, "bad [31m thing here");
        let long = "x".repeat(500);
        assert_eq!(vendor_text(&long, 10).chars().count(), 11);
    }

    #[test]
    fn log_line_with_fake_request_does_not_contain_key() {
        let capture = CaptureWriter::default();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(RedactingMakeWriter::new(capture.clone()))
            .with_ansi(false)
            .with_max_level(tracing::Level::TRACE)
            .finish();
        let url = format!("https://serpapi.com/account.json?api_key={SERP_KEY}");
        tracing::subscriber::with_default(subscriber, || {
            tracing::warn!(url = %url, "request failed");
            tracing::debug!("Authorization: Bearer {FC_KEY}");
            tracing::error!(api_key = SERP_KEY, "structured field");
        });
        let out = capture.contents();
        assert!(out.contains("request failed"), "{out}");
        assert!(!out.contains(SERP_KEY), "{out}");
        assert!(!out.contains(FC_KEY), "{out}");
        assert!(!out.contains("api_key"), "{out}");
        assert!(!out.contains("Authorization"), "{out}");
    }
}
