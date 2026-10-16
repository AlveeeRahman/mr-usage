//! Automated form of the keyring leak test. A global allocator, installed only
//! in this test binary, scans every freed heap block for the secret before
//! releasing it. A hit means a copy of the secret was dropped without being
//! zeroized.
//!
//! Every allocation is zero-filled (`alloc_zeroed`), so scanning a block never
//! reads uninitialized memory.

#![allow(unsafe_code, clippy::unwrap_used, clippy::panic)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use jiff::Timestamp;
use mr_usage_core::keystore::memory::MemoryKeyStore;
use mr_usage_core::keystore::{self, KeyStore, OsKeyStore};
use mr_usage_core::net::Http;
use mr_usage_core::poller::{Poller, ProviderView};
use mr_usage_core::provider::{Provider, ProviderError, Snapshot, Unit};
use mr_usage_core::settings::Settings;
use mr_usage_core::wipe::SystemClock;
use secrecy::{ExposeSecret, SecretString};

/// Twenty bytes, the same secret the upstream test uses.
const PATTERN: &[u8] = b"super-duper-password";

static ARMED: AtomicBool = AtomicBool::new(false);
static HITS: AtomicUsize = AtomicUsize::new(0);
static SERIAL: Mutex<()> = Mutex::new(());

struct ScanningAllocator;

// SAFETY: forwards to `System`; the extra work in `dealloc` only reads the
// block being freed, which is valid for `layout.size()` bytes and fully
// initialized because every allocation is zeroed.
unsafe impl GlobalAlloc for ScanningAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if ARMED.load(Ordering::SeqCst) && layout.size() >= PATTERN.len() {
            let block = unsafe { std::slice::from_raw_parts(ptr, layout.size()) };
            if block.windows(PATTERN.len()).any(|w| w == PATTERN) {
                HITS.fetch_add(1, Ordering::SeqCst);
            }
        }
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: ScanningAllocator = ScanningAllocator;

fn secret_bytes() -> Vec<u8> {
    b"s u p e r - d u p e r - p a s s w o r d".iter().step_by(2).copied().collect()
}

fn secret() -> SecretString {
    let s = String::from_utf8(secret_bytes()).unwrap();
    SecretString::from(s)
}

fn scan<F: FnOnce()>(f: F) -> usize {
    HITS.store(0, Ordering::SeqCst);
    ARMED.store(true, Ordering::SeqCst);
    f();
    ARMED.store(false, Ordering::SeqCst);
    HITS.load(Ordering::SeqCst)
}

struct EchoProvider;

#[async_trait]
impl Provider for EchoProvider {
    fn id(&self) -> &'static str {
        "echo"
    }
    fn display_name(&self) -> &'static str {
        "Echo"
    }
    fn docs_url(&self) -> &'static str {
        "https://example.com/"
    }
    fn endpoint(&self) -> &'static str {
        "https://example.com/"
    }
    fn allowed_host(&self) -> &'static str {
        "example.com"
    }
    fn unit(&self) -> Unit {
        Unit::Credits
    }
    fn min_poll_interval(&self) -> Duration {
        Duration::from_secs(60)
    }
    async fn fetch(&self, key: &SecretString, _http: &Http) -> Result<Snapshot, ProviderError> {
        Ok(Snapshot {
            taken_at: Timestamp::UNIX_EPOCH,
            remaining: Some(key.expose_secret().len() as u64),
            limit: None,
            period_end: None,
            extra: vec![],
        })
    }
}

#[test]
fn scanner_detects_an_unzeroized_copy() {
    let _serial = SERIAL.lock().unwrap();
    let hits = scan(|| {
        let plain = String::from_utf8(secret_bytes()).unwrap();
        drop(plain);
    });
    assert!(hits >= 1, "the scanner failed to see a plain String being freed");
}

#[test]
fn wrapper_and_poller_leave_no_secret_in_freed_memory() {
    let _serial = SERIAL.lock().unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let http = Http::new().unwrap();
    let hits = scan(|| {
        let store = Arc::new(MemoryKeyStore::default());
        // The same steps `save_key`, `key_hint` and a poll take.
        let normalized = keystore::normalize_key(secret()).unwrap();
        store.set("echo", &normalized).unwrap();
        drop(normalized);
        let hint = keystore::hint(&store.get("echo").unwrap().unwrap());
        assert_eq!(hint, "••••word");

        let settings = Settings {
            enabled: vec!["echo".into()],
            ..Settings::default()
        };
        let poller = Poller::new(
            vec![Arc::new(EchoProvider) as Arc<dyn Provider>],
            store.clone(),
            http,
            Arc::new(SystemClock),
            Arc::new(|_: &ProviderView| {}),
            &settings,
        );
        runtime.block_on(async {
            poller.load_key_presence().await;
            let view = poller.refresh("echo").await.unwrap();
            assert_eq!(view.latest.unwrap().remaining, Some(20));
        });
        store.delete("echo").unwrap();
        drop(poller);
        drop(store);
    });
    drop(runtime);
    assert_eq!(hits, 0, "secret bytes found in {hits} freed block(s)");
}

/// The same check against the real OS credential store. On Linux this needs a
/// running, unlocked Secret Service. Run with:
/// `cargo test -p mr-usage-core --test leak -- --ignored --test-threads=1`
#[test]
#[ignore = "needs a real OS credential store"]
fn os_store_leaves_no_secret_in_freed_memory() {
    let _serial = SERIAL.lock().unwrap();
    // Initialise the platform store outside the scanned window.
    let _ = OsKeyStore.get("mr-usage-leak-warmup");
    let hits = scan(|| {
        let store = OsKeyStore;
        store.set("mr-usage-leak-test", &secret()).unwrap();
        drop(store.get("mr-usage-leak-test").unwrap());
        store.delete("mr-usage-leak-test").unwrap();
    });
    assert_eq!(hits, 0, "secret bytes found in {hits} freed block(s)");
}
