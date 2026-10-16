//! Thin wrapper over the OS credential store.
//!
//! Uses `keyring` 4.2.0 with its `v1` feature (verified 2026-10-16,
//! <https://docs.rs/keyring/4.2.0/keyring/v1/index.html>): Keychain Services on
//! macOS, Credential Manager on Windows, the Secret Service (through zbus, with
//! an encrypted session) on Linux and the BSDs. There is no fallback store. If
//! the platform store is missing, the user gets an error with instructions.

use secrecy::{ExposeSecret, SecretString};
use zeroize::Zeroize;

use crate::SERVICE;

/// Keys longer than this are refused. Real vendor keys are well under 200.
pub const MAX_KEY_BYTES: usize = 512;

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum KeystoreError {
    #[error("{0}")]
    Unavailable(String),
    #[error("The credential store refused the request: {0}")]
    Failure(String),
    #[error("{0}")]
    InvalidKey(&'static str),
}

pub trait KeyStore: Send + Sync {
    fn set(&self, account: &str, key: &SecretString) -> Result<(), KeystoreError>;
    fn get(&self, account: &str) -> Result<Option<SecretString>, KeystoreError>;
    fn delete(&self, account: &str) -> Result<(), KeystoreError>;
}

/// Where keys live on this OS, for the footer line.
pub fn store_label() -> &'static str {
    if cfg!(target_os = "macos") {
        "macOS Keychain"
    } else if cfg!(target_os = "windows") {
        "Windows Credential Manager"
    } else {
        "the Secret Service (GNOME Keyring, KWallet or KeePassXC)"
    }
}

/// Trim surrounding whitespace and reject keys that are empty, too long, or
/// contain whitespace or control characters. The input is zeroized.
pub fn normalize_key(raw: SecretString) -> Result<SecretString, KeystoreError> {
    let trimmed = raw.expose_secret().trim();
    if trimmed.is_empty() {
        return Err(KeystoreError::InvalidKey("The key is empty."));
    }
    if trimmed.len() > MAX_KEY_BYTES {
        return Err(KeystoreError::InvalidKey("The key is too long."));
    }
    if trimmed.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(KeystoreError::InvalidKey("The key contains spaces or control characters."));
    }
    Ok(exact_secret(trimmed))
}

/// Copy into a `SecretString` whose buffer has exactly the needed capacity, so
/// the `String` to `Box<str>` conversion inside secrecy does not reallocate and
/// leave an unzeroized copy behind.
fn exact_secret(value: &str) -> SecretString {
    let mut exact = String::with_capacity(value.len());
    exact.push_str(value);
    SecretString::from(exact)
}

/// Wrap a string the store returned, then wipe the original buffer.
fn take_secret(mut raw: String) -> SecretString {
    let secret = exact_secret(&raw);
    raw.zeroize();
    secret
}

/// A masked hint such as `••••3f9a`. Short keys reveal nothing.
pub fn hint(key: &SecretString) -> String {
    let k = key.expose_secret();
    let count = k.chars().count();
    if count < 12 {
        return "••••".to_owned();
    }
    let tail: String = k.chars().skip(count - 4).collect();
    format!("••••{tail}")
}

/// The real OS credential store.
pub struct OsKeyStore;

impl OsKeyStore {
    fn entry(account: &str) -> Result<keyring::Entry, KeystoreError> {
        keyring::Entry::new(SERVICE, account).map_err(map_error)
    }
}

impl KeyStore for OsKeyStore {
    fn set(&self, account: &str, key: &SecretString) -> Result<(), KeystoreError> {
        Self::entry(account)?.set_password(key.expose_secret()).map_err(map_error)
    }

    fn get(&self, account: &str) -> Result<Option<SecretString>, KeystoreError> {
        match Self::entry(account)?.get_password() {
            Ok(raw) => Ok(Some(take_secret(raw))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(map_error(e)),
        }
    }

    fn delete(&self, account: &str) -> Result<(), KeystoreError> {
        match Self::entry(account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(map_error(e)),
        }
    }
}

const NO_STORE: &str = if cfg!(any(target_os = "macos", target_os = "windows")) {
    "The system credential store is not available."
} else {
    "No Secret Service is running. Install and unlock GNOME Keyring, KWallet or KeePassXC \
     (anything that provides org.freedesktop.secrets), then restart Mr. Usage. \
     Mr. Usage will not fall back to storing keys anywhere else."
};

/// Map a keyring error to a message that cannot contain secret bytes. Two
/// variants carry the raw secret; those buffers are wiped here and never
/// formatted.
fn map_error(err: keyring::Error) -> KeystoreError {
    match err {
        keyring::Error::NoDefaultStore => KeystoreError::Unavailable(NO_STORE.to_owned()),
        keyring::Error::NoStorageAccess(_) => {
            KeystoreError::Unavailable("The credential store is locked or access was denied. Unlock it and try again.".to_owned())
        }
        keyring::Error::PlatformFailure(e) => KeystoreError::Failure(crate::redact::vendor_text(&e.to_string(), 200)),
        keyring::Error::BadEncoding(mut bytes) => {
            bytes.zeroize();
            KeystoreError::Failure("the stored key is not valid UTF-8".to_owned())
        }
        keyring::Error::BadDataFormat(mut bytes, _) => {
            bytes.zeroize();
            KeystoreError::Failure("the stored key is in an unexpected format".to_owned())
        }
        keyring::Error::NoEntry => KeystoreError::Failure("no key is stored".to_owned()),
        keyring::Error::TooLong(..) => KeystoreError::InvalidKey("The key is too long for this credential store."),
        keyring::Error::Ambiguous(_) => KeystoreError::Failure("more than one matching key is stored".to_owned()),
        _ => KeystoreError::Failure("unexpected credential store error".to_owned()),
    }
}

/// In-memory store for tests. Not compiled into the app.
#[cfg(any(test, feature = "test-util"))]
pub mod memory {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use secrecy::{ExposeSecret, SecretString};

    use super::{KeyStore, KeystoreError, exact_secret};

    #[derive(Default)]
    pub struct MemoryKeyStore {
        keys: Mutex<HashMap<String, SecretString>>,
    }

    impl MemoryKeyStore {
        fn lock(&self) -> Result<std::sync::MutexGuard<'_, HashMap<String, SecretString>>, KeystoreError> {
            self.keys.lock().map_err(|_| KeystoreError::Failure("poisoned".into()))
        }
    }

    impl KeyStore for MemoryKeyStore {
        fn set(&self, account: &str, key: &SecretString) -> Result<(), KeystoreError> {
            self.lock()?.insert(account.to_owned(), exact_secret(key.expose_secret()));
            Ok(())
        }

        fn get(&self, account: &str) -> Result<Option<SecretString>, KeystoreError> {
            Ok(self.lock()?.get(account).map(|k| exact_secret(k.expose_secret())))
        }

        fn delete(&self, account: &str) -> Result<(), KeystoreError> {
            self.lock()?.remove(account);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &str) -> SecretString {
        SecretString::from(v.to_owned())
    }

    #[test]
    fn normalize_trims_and_validates() {
        assert_eq!(normalize_key(s("  fc-abc123  \n")).unwrap().expose_secret(), "fc-abc123");
        assert!(normalize_key(s("   ")).is_err());
        assert!(normalize_key(s("fc abc")).is_err());
        assert!(normalize_key(s("fc\u{0}abc")).is_err());
        assert!(normalize_key(s(&"a".repeat(MAX_KEY_BYTES + 1))).is_err());
    }

    #[test]
    fn hint_masks() {
        assert_eq!(hint(&s("fc-0123456789abcdef")), "••••cdef");
        assert_eq!(hint(&s("short")), "••••");
    }

    #[test]
    fn memory_store_round_trip() {
        let store = memory::MemoryKeyStore::default();
        store.set("firecrawl", &s("fc-1")).unwrap();
        assert_eq!(store.get("firecrawl").unwrap().unwrap().expose_secret(), "fc-1");
        store.delete("firecrawl").unwrap();
        assert!(store.get("firecrawl").unwrap().is_none());
    }

    #[test]
    fn error_messages_never_carry_secret_bytes() {
        let secret = b"super-duper-password".to_vec();
        let e = map_error(keyring::Error::BadEncoding(secret));
        assert!(!e.to_string().contains("super-duper"));
    }

    /// Needs a running Secret Service, Keychain or Credential Manager.
    /// Run with `cargo test -p mr-usage-core -- --ignored os_store`.
    #[test]
    #[ignore = "needs a real OS credential store"]
    fn os_store_round_trip() {
        let store = OsKeyStore;
        let account = "mr-usage-selftest";
        store.set(account, &s("fc-selftest-0000000000")).unwrap();
        assert_eq!(store.get(account).unwrap().unwrap().expose_secret(), "fc-selftest-0000000000");
        store.delete(account).unwrap();
        assert!(store.get(account).unwrap().is_none());
    }
}
