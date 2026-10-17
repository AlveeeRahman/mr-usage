//! The shared error-body reader against arbitrary bytes.
//!
//! Must hold for any input: no panic, and any message it returns is
//! non-empty, within the vendor-text budget (200 characters plus an
//! ellipsis) and free of control characters.
#![no_main]

use libfuzzer_sys::fuzz_target;
use mr_usage_core::fuzz;

fuzz_target!(|data: &[u8]| {
    if let Some(message) = fuzz::error_message(data) {
        assert!(!message.is_empty());
        assert!(message.chars().count() <= 201, "message longer than the vendor-text budget");
        assert!(!message.chars().any(char::is_control), "control character in message");
    }
});
