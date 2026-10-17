//! Entry points for the fuzz targets in `fuzz/`. Compiled only with the
//! `fuzzing` feature, which the app never enables, so nothing here is in a
//! shipped binary.
//!
//! The targets feed arbitrary bytes to the code that handles vendor
//! responses and to the redaction rules. Both take input from outside the
//! app's control: a vendor can change its API, and a log line can carry
//! anything a vendor sends. The checks are that nothing panics and that the
//! guarantees the rest of the crate relies on hold for every input.

use std::time::Duration;

use jiff::Timestamp;
use reqwest::StatusCode;

use crate::provider::{ProviderError, Snapshot};
use crate::providers::*;

type Parser = fn(StatusCode, Option<Duration>, &[u8], Timestamp) -> Result<Snapshot, ProviderError>;

/// Every provider parser with the common signature. Brave reads rate-limit
/// headers as well and is handled by [`parse`] separately; IBM Quantum has
/// no parser because the app cannot call its API.
const PARSERS: &[Parser] = &[
    ahrefs::parse,
    apify::parse,
    azurequantum::parse,
    bfl::parse,
    brightdata::parse,
    context7::parse,
    crawlbase::parse,
    dataforseo::parse,
    deepgram::parse,
    deepseek::parse,
    did::parse,
    digitalocean::parse,
    elevenlabs::parse,
    elicit::parse,
    fal::parse,
    firecrawl::parse,
    heygen::parse,
    hunter::parse,
    keywordseverywhere::parse,
    linkup::parse,
    luma::parse,
    majestic::parse,
    mangools::parse,
    moonshot::parse,
    moz::parse,
    novita::parse,
    openrouter::parse,
    recraft::parse,
    runpod::parse,
    runway::parse,
    scrapingbee::parse,
    searchapi::parse,
    semrush::parse,
    seranking::parse,
    serpapi::parse,
    serpstat::parse,
    siliconflow::parse,
    stability::parse,
    tavily::parse,
    vast::parse,
    vercelgateway::parse,
    xai::parse,
];

/// HTTP statuses worth exercising. The parsers branch on 401, 403, 429 and
/// the success range, so a short list reaches every branch far faster than
/// a random 16-bit value would.
const STATUSES: &[u16] = &[200, 201, 204, 400, 401, 402, 403, 404, 409, 422, 429, 500, 502, 503];

/// Number of parsers [`parse`] can select, Brave included.
pub fn parser_count() -> usize {
    PARSERS.len() + 1
}

/// Run one provider parser. `selector` picks the parser (wrapping), `status`
/// picks an HTTP status from a fixed list (wrapping), and `body` is the
/// response. For Brave, the first three lines of `body` become the three
/// rate-limit headers and the rest is the response body.
pub fn parse(selector: u8, status: u8, body: &[u8]) -> Result<Snapshot, ProviderError> {
    let status = StatusCode::from_u16(STATUSES[usize::from(status) % STATUSES.len()]).unwrap_or(StatusCode::OK);
    let now = Timestamp::UNIX_EPOCH;
    let retry_after = Some(Duration::from_secs(u64::from(selector)));
    let index = usize::from(selector) % parser_count();
    match PARSERS.get(index) {
        Some(p) => p(status, retry_after, body, now),
        None => {
            let mut lines = body.splitn(4, |b| *b == b'\n');
            let mut header = |name: &str| {
                let value = lines.next().map(|l| String::from_utf8_lossy(l).into_owned()).unwrap_or_default();
                (name.to_owned(), value)
            };
            let headers = [
                header("x-ratelimit-limit"),
                header("x-ratelimit-remaining"),
                header("x-ratelimit-reset"),
            ];
            let rest = lines.next().unwrap_or_default();
            brave::parse(status, retry_after, &headers, rest, now)
        }
    }
}

/// The shared error-body reader, which every parser falls back to.
pub fn error_message(body: &[u8]) -> Option<String> {
    super::providers::error_message(body)
}
