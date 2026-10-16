//! Port of keyring-rs's leak test (`examples/leak-test/main.rs` in
//! open-source-cooperative/keyring-rs, read 2026-10-16), run against Mr.
//! Usage's own keystore wrapper instead of a raw `keyring_core::Entry`.
//!
//! It stores a secret, reads it back, drops every client copy, then waits
//! while `scripts/leak-test-linux.sh` dumps the process and greps the dump for
//! `super-duper-password`. Any hit means the secret survived somewhere in
//! memory: in our wrapper, in keyring, or in the platform client.
//!
//! The secret is assembled from an interleaved byte string so the literal
//! never appears in the binary, exactly as the upstream test does.

#![allow(clippy::print_stderr)]

use std::process::ExitCode;
use std::time::Duration;

use mr_usage_core::keystore::{KeyStore, OsKeyStore};
use secrecy::SecretString;
use zeroize::Zeroize;

const ACCOUNT: &str = "mr-usage-leak-test";

fn deinterleave(master: &[u8]) -> Vec<u8> {
    master.iter().step_by(2).copied().collect()
}

fn main() -> ExitCode {
    let mut bytes = deinterleave(b"s u p e r - d u p e r - p a s s w o r d");
    let secret = match String::from_utf8(bytes.clone()) {
        Ok(s) => SecretString::from(s),
        Err(_) => return ExitCode::FAILURE,
    };
    bytes.zeroize();

    let store = OsKeyStore;
    if let Err(e) = store.set(ACCOUNT, &secret) {
        eprintln!("set failed: {e}");
        return ExitCode::FAILURE;
    }
    drop(secret);
    match store.get(ACCOUNT) {
        Ok(Some(read_back)) => drop(read_back),
        Ok(None) => {
            eprintln!("get returned nothing");
            return ExitCode::FAILURE;
        }
        Err(e) => {
            eprintln!("get failed: {e}");
            return ExitCode::FAILURE;
        }
    }

    // Deliberately leaked marker, so the script can confirm its scan works.
    let ready = String::from_utf8(deinterleave(b"r e a d y - f o r - a - c l o s e - u p")).unwrap_or_default();
    eprintln!("Leaked string: {ready}");
    let delay = std::env::var("DELAY_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(7);
    std::thread::sleep(Duration::from_secs(delay));

    let _ = store.delete(ACCOUNT);
    eprintln!("Leak test program is exiting");
    ExitCode::SUCCESS
}
