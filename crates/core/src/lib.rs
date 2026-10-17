//! Core logic for Mr. Usage.
//!
//! Everything security-relevant lives here so it can be tested with plain
//! `cargo test` and audited without a webview in the picture.

#[cfg(feature = "fuzzing")]
pub mod fuzz;
pub mod keystore;
pub mod net;
pub mod poller;
pub mod provider;
pub mod providers;
pub mod redact;
pub mod settings;
pub mod wipe;

/// Keystore service name. Every key is stored under this service with the
/// provider id as the account name.
pub const SERVICE: &str = "mr-usage";
