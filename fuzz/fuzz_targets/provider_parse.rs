//! Every provider parser against arbitrary status codes and bodies.
//!
//! Must hold for any input: no panic, and an error carries a short, clean
//! sentence (non-empty, no control characters, within the vendor-text budget
//! plus the fixed prefixes the error types add). Error text is not required
//! to be scrub-stable: a static sentence may legitimately contain a marker
//! word such as "token", and the log writer over-redacting it costs nothing.
#![no_main]

use libfuzzer_sys::fuzz_target;
use mr_usage_core::fuzz;

/// `MAX_VENDOR_TEXT` (200) plus the ellipsis, plus the longest fixed prefix
/// an error variant adds in front of vendor text.
const MAX_ERROR_CHARS: usize = 201 + 40;

fuzz_target!(|data: &[u8]| {
    let [selector, status, body @ ..] = data else { return };
    if let Err(e) = fuzz::parse(*selector, *status, body) {
        let text = e.to_string();
        assert!(!text.is_empty(), "empty error text");
        assert!(text.chars().count() <= MAX_ERROR_CHARS, "error text over budget: {text:?}");
        assert!(!text.chars().any(char::is_control), "control character in error text: {text:?}");
    }
});
