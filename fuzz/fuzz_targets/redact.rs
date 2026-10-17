//! The redaction rules against arbitrary text.
//!
//! Must hold for any input: no panic; `scrub` and `redact_url` are
//! idempotent (a line scrubbed once is unchanged by a second pass, which the
//! log writer relies on because it re-scrubs text vendor_text already
//! handled); and `vendor_text` never exceeds its character budget by more
//! than the ellipsis and never passes a control character through.
#![no_main]

use libfuzzer_sys::fuzz_target;
use mr_usage_core::redact::{redact_url, scrub, vendor_text};

fuzz_target!(|data: &[u8]| {
    let [budget, rest @ ..] = data else { return };
    let Ok(text) = std::str::from_utf8(rest) else { return };
    let once = scrub(text);
    assert_eq!(scrub(&once), once, "scrub is not idempotent");
    let url = redact_url(text);
    assert_eq!(redact_url(&url), url, "redact_url is not idempotent");
    let max = usize::from(*budget);
    let shown = vendor_text(text, max);
    assert!(shown.chars().count() <= max + 1, "vendor_text exceeded its budget");
    assert!(!shown.chars().any(char::is_control), "vendor_text passed a control character through");
});
