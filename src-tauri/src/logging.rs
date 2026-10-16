//! One `tracing` subscriber, writing to stderr through the redaction layer.

use mr_usage_core::redact::{self, RedactingMakeWriter};
use tracing::Level;

/// Default level is `warn`. `MR_USAGE_LOG=debug` (or `info`, `trace`, `error`)
/// raises or lowers it; every level still passes through redaction.
pub fn init() {
    let level = match std::env::var("MR_USAGE_LOG").as_deref() {
        Ok("trace") => Level::TRACE,
        Ok("debug") => Level::DEBUG,
        Ok("info") => Level::INFO,
        Ok("error") => Level::ERROR,
        _ => Level::WARN,
    };
    let subscriber = tracing_subscriber::fmt()
        .with_writer(RedactingMakeWriter::new(std::io::stderr))
        .with_ansi(false)
        .with_target(false)
        .with_max_level(level)
        .finish();
    // Fails only if a subscriber is already set, which would be a bug in main.
    let _ = tracing::subscriber::set_global_default(subscriber);

    std::panic::set_hook(Box::new(|info| {
        let location = info.location().map(|l| format!("{}:{}", l.file(), l.line())).unwrap_or_default();
        let payload = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| (*s).to_owned())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_default();
        tracing::error!(location = %location, message = %redact::vendor_text(&payload, 200), "panic");
    }));
}
