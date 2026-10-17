//! Scheduling, backoff and the in-memory history ring buffers.
//!
//! The poller reads a key from the keystore at fetch time, hands it to the
//! provider and drops it. Keys are never stored in poller state.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use jiff::{SignedDuration, Timestamp};
use serde::Serialize;

use crate::keystore::{KeyStore, KeystoreError};
use crate::net::Http;
use crate::provider::{Category, ConfigField, Provider, ProviderError, Snapshot, Unit};
use crate::settings::Settings;
use crate::wipe::{Clock, MidnightWiper};

/// One day of samples at five-minute intervals.
pub const HISTORY_CAP: usize = 288;
/// How often the scheduler wakes to check due polls and the date.
pub const TICK: Duration = Duration::from_secs(15);
/// Minimum gap between manual refreshes of one provider.
pub const MANUAL_COOLDOWN: Duration = Duration::from_secs(60);
const MAX_BACKOFF: Duration = Duration::from_secs(60 * 60);

#[derive(Clone, Debug, PartialEq)]
enum LastError {
    Provider(ProviderError),
    Keystore(KeystoreError),
}

#[derive(Default)]
struct Runtime {
    history: VecDeque<Snapshot>,
    last_error: Option<LastError>,
    last_success: Option<Timestamp>,
    last_attempt: Option<Timestamp>,
    next_due: Option<Timestamp>,
    failures: u32,
    in_flight: bool,
    has_key: bool,
    /// Bumped whenever the key is saved or forgotten. A poll that started
    /// under an older generation used the old key; its result is discarded.
    generation: u64,
}

struct State {
    runtime: HashMap<&'static str, Runtime>,
    wiper: MidnightWiper,
    interval: Duration,
    enabled: Vec<String>,
    config: BTreeMap<String, String>,
}

/// What the frontend renders for one provider.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderView {
    pub id: &'static str,
    pub display_name: &'static str,
    pub docs_url: &'static str,
    pub endpoint: &'static str,
    pub unit: Unit,
    pub category: Category,
    pub note: Option<&'static str>,
    pub config_field: Option<ConfigField>,
    /// The provider's extra setting as the user typed it. Never a key.
    pub config: Option<String>,
    pub enabled: bool,
    pub has_key: bool,
    pub poll_interval_secs: u64,
    pub min_poll_interval_secs: u64,
    pub latest: Option<SnapshotView>,
    pub history: Vec<Point>,
    pub last_success_ms: Option<i64>,
    pub next_poll_ms: Option<i64>,
    pub error: Option<ErrorView>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotView {
    pub taken_at_ms: i64,
    pub remaining: Option<u64>,
    pub limit: Option<u64>,
    pub period_end_ms: Option<i64>,
    pub spent: Option<u64>,
    pub spend_per_hour: Option<u64>,
    pub extra: Vec<(String, String)>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Point {
    pub t: i64,
    pub remaining: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorView {
    pub kind: &'static str,
    pub message: String,
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum PollerError {
    #[error("Unknown provider.")]
    UnknownProvider,
    #[error("No key is stored for this provider.")]
    NoKey,
    #[error("Refreshed less than a minute ago. Try again shortly.")]
    TooSoon,
    #[error("A refresh is already running.")]
    Busy,
}

pub type Sink = Arc<dyn Fn(&ProviderView) + Send + Sync>;

pub struct Poller {
    providers: Vec<Arc<dyn Provider>>,
    keystore: Arc<dyn KeyStore>,
    http: Http,
    clock: Arc<dyn Clock>,
    sink: Sink,
    state: Mutex<State>,
}

fn after(ts: Timestamp, d: Duration) -> Timestamp {
    SignedDuration::try_from(d)
        .ok()
        .and_then(|sd| ts.checked_add(sd).ok())
        .unwrap_or(Timestamp::MAX)
}

fn millis(ts: Timestamp) -> i64 {
    ts.as_millisecond()
}

impl Poller {
    pub fn new(
        providers: Vec<Arc<dyn Provider>>,
        keystore: Arc<dyn KeyStore>,
        http: Http,
        clock: Arc<dyn Clock>,
        sink: Sink,
        settings: &Settings,
    ) -> Arc<Self> {
        let now = clock.now();
        let runtime = providers.iter().map(|p| (p.id(), Runtime::default())).collect();
        Arc::new(Self {
            providers,
            keystore,
            http,
            clock,
            sink,
            state: Mutex::new(State {
                runtime,
                wiper: MidnightWiper::new(&now),
                interval: settings.poll_interval(),
                enabled: settings.enabled.clone(),
                config: settings.provider_config.clone(),
            }),
        })
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        // A panic while holding the lock cannot leave secrets behind (none are
        // stored here), so recovering the data is safe.
        match self.state.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    fn provider(&self, id: &str) -> Option<&Arc<dyn Provider>> {
        self.providers.iter().find(|p| p.id() == id)
    }

    pub fn provider_ids(&self) -> Vec<&'static str> {
        self.providers.iter().map(|p| p.id()).collect()
    }

    fn interval_for(provider: &dyn Provider, global: Duration) -> Duration {
        global.max(provider.min_poll_interval())
    }

    /// Look up which providers have a stored key. Reads each key once and
    /// drops it immediately.
    pub async fn load_key_presence(&self) {
        for p in &self.providers {
            let id = p.id();
            let store = Arc::clone(&self.keystore);
            let present = tokio::task::spawn_blocking(move || store.get(id).map(|k| k.is_some())).await;
            let mut state = self.lock();
            if let Some(rt) = state.runtime.get_mut(id) {
                match present {
                    Ok(Ok(has)) => rt.has_key = has,
                    Ok(Err(e)) => rt.last_error = Some(LastError::Keystore(e)),
                    Err(_) => rt.last_error = Some(LastError::Keystore(KeystoreError::Failure("keystore task failed".into()))),
                }
            }
        }
    }

    pub fn views(&self) -> Vec<ProviderView> {
        let state = self.lock();
        self.providers.iter().filter_map(|p| Self::view(p.as_ref(), &state)).collect()
    }

    pub fn view_of(&self, id: &str) -> Option<ProviderView> {
        let state = self.lock();
        self.provider(id).and_then(|p| Self::view(p.as_ref(), &state))
    }

    fn view(p: &dyn Provider, state: &State) -> Option<ProviderView> {
        let rt = state.runtime.get(p.id())?;
        Some(ProviderView {
            id: p.id(),
            display_name: p.display_name(),
            docs_url: p.docs_url(),
            endpoint: p.endpoint(),
            unit: p.unit(),
            category: p.category(),
            note: p.note(),
            config_field: p.config_field(),
            config: state.config.get(p.id()).cloned(),
            enabled: state.enabled.iter().any(|e| e == p.id()),
            has_key: rt.has_key,
            poll_interval_secs: Self::interval_for(p, state.interval).as_secs(),
            min_poll_interval_secs: p.min_poll_interval().as_secs(),
            latest: rt.history.back().map(|s| SnapshotView {
                taken_at_ms: millis(s.taken_at),
                remaining: s.remaining,
                limit: s.limit,
                period_end_ms: s.period_end.map(millis),
                spent: s.spent,
                spend_per_hour: s.spend_per_hour,
                extra: s.extra.clone(),
            }),
            history: rt
                .history
                .iter()
                .map(|s| Point {
                    t: millis(s.taken_at),
                    remaining: s.remaining,
                })
                .collect(),
            last_success_ms: rt.last_success.map(millis),
            next_poll_ms: rt.next_due.map(millis),
            error: rt.last_error.as_ref().map(|e| match e {
                LastError::Provider(p) => ErrorView {
                    kind: p.kind(),
                    message: p.to_string(),
                },
                LastError::Keystore(k) => ErrorView {
                    kind: "keystore",
                    message: k.to_string(),
                },
            }),
        })
    }

    fn emit(&self, id: &str) {
        if let Some(view) = self.view_of(id) {
            (self.sink)(&view);
        }
    }

    pub fn apply_settings(&self, settings: &Settings) {
        let now = self.clock.now().timestamp();
        {
            let mut state = self.lock();
            state.interval = settings.poll_interval();
            state.enabled = settings.enabled.clone();
            state.config = settings.provider_config.clone();
            let State {
                runtime,
                interval,
                enabled,
                ..
            } = &mut *state;
            for provider in &self.providers {
                let id = provider.id();
                let Some(rt) = runtime.get_mut(id) else { continue };
                if !enabled.iter().any(|e| e == id) {
                    rt.next_due = None;
                    continue;
                }
                let stopped = matches!(
                    rt.last_error,
                    Some(LastError::Provider(ProviderError::Unauthorized | ProviderError::Unsupported(_)))
                );
                match (rt.next_due, rt.last_attempt) {
                    // Newly enabled providers poll on the next tick.
                    (None, _) if rt.has_key && !stopped => rt.next_due = Some(now),
                    // The next poll is one interval after the last one, under
                    // the new interval: moving from an hour to a minute does
                    // not wait up to an hour, and moving the other way stops
                    // polling fast at once. Backoff after failures stays.
                    (Some(_), Some(last)) if rt.failures == 0 => {
                        rt.next_due = Some(after(last, Self::interval_for(provider.as_ref(), *interval)));
                    }
                    _ => {}
                }
            }
        }
        for id in self.provider_ids() {
            self.emit(id);
        }
    }

    /// A new key was stored: forget the old account's history and poll soon.
    pub fn key_saved(&self, id: &str) {
        let now = self.clock.now().timestamp();
        {
            let mut state = self.lock();
            if let Some(rt) = state.runtime.get_mut(id) {
                *rt = Runtime {
                    has_key: true,
                    next_due: Some(now),
                    generation: rt.generation.wrapping_add(1),
                    ..Runtime::default()
                };
            }
        }
        self.emit(id);
    }

    /// The provider's extra setting changed: the next poll should use it now,
    /// and an error caused by the old value should not hold polling back.
    pub fn config_changed(&self, id: &str) {
        let now = self.clock.now().timestamp();
        {
            let mut state = self.lock();
            let enabled = state.enabled.iter().any(|e| e == id);
            if let Some(rt) = state.runtime.get_mut(id)
                && rt.has_key
                && enabled
            {
                rt.failures = 0;
                rt.last_error = None;
                rt.next_due = Some(now);
            }
        }
        self.emit(id);
    }

    pub fn key_forgotten(&self, id: &str) {
        {
            let mut state = self.lock();
            if let Some(rt) = state.runtime.get_mut(id) {
                *rt = Runtime {
                    generation: rt.generation.wrapping_add(1),
                    ..Runtime::default()
                };
            }
        }
        self.emit(id);
    }

    /// Clear every history buffer, latest reading and error. Keys stay in the
    /// OS store.
    pub fn wipe(&self) {
        {
            let mut state = self.lock();
            for rt in state.runtime.values_mut() {
                rt.history = VecDeque::new();
                rt.last_success = None;
                rt.last_error = None;
            }
        }
        for id in self.provider_ids() {
            self.emit(id);
        }
    }

    /// One scheduler step: wipe at a date change, then poll whatever is due.
    /// Due providers are polled at the same time, so one slow vendor cannot
    /// hold up the others or delay the next tick by more than its own
    /// timeout. Each poll owns its provider's `in_flight` flag, so a manual
    /// refresh of the same provider meanwhile is refused as `Busy`. The step
    /// returns once every poll it started has finished.
    pub async fn tick(self: &Arc<Self>) {
        let now_zoned = self.clock.now();
        let now = now_zoned.timestamp();
        let wipe = self.lock().wiper.should_wipe(&now_zoned);
        if wipe {
            tracing::info!("local date changed; wiping usage history");
            self.wipe();
        }
        let due: Vec<&'static str> = {
            let state = self.lock();
            self.providers
                .iter()
                .map(|p| p.id())
                .filter(|id| {
                    state.enabled.iter().any(|e| e == id)
                        && state
                            .runtime
                            .get(id)
                            .is_some_and(|rt| rt.has_key && !rt.in_flight && rt.next_due.is_some_and(|d| d <= now))
                })
                .collect()
        };
        let mut polls = tokio::task::JoinSet::new();
        for id in due {
            let poller = Arc::clone(self);
            polls.spawn(async move { poller.poll_one(id).await });
        }
        while let Some(joined) = polls.join_next().await {
            // Each poll records its own outcome in the provider's runtime. A
            // join error means the task panicked, which the lints keep out of
            // this crate's own code; it is logged rather than dropped.
            if let Err(e) = joined {
                tracing::warn!(error = %e, "poll task failed");
            }
        }
    }

    /// Run `tick` forever. Abort the task to stop.
    pub async fn run(self: Arc<Self>) {
        loop {
            self.tick().await;
            tokio::time::sleep(TICK).await;
        }
    }

    /// Manual refresh from the UI, rate-limited per provider.
    pub async fn refresh(&self, id: &str) -> Result<ProviderView, PollerError> {
        let now = self.clock.now().timestamp();
        {
            let state = self.lock();
            let rt = state.runtime.get(id).ok_or(PollerError::UnknownProvider)?;
            if !rt.has_key {
                return Err(PollerError::NoKey);
            }
            if rt.in_flight {
                return Err(PollerError::Busy);
            }
            if rt.last_attempt.is_some_and(|t| after(t, MANUAL_COOLDOWN) > now) {
                return Err(PollerError::TooSoon);
            }
        }
        self.poll_one(id).await
    }

    async fn poll_one(&self, id: &str) -> Result<ProviderView, PollerError> {
        let provider = Arc::clone(self.provider(id).ok_or(PollerError::UnknownProvider)?);
        let id = provider.id();
        let started = self.clock.now().timestamp();
        let (generation, config) = {
            let mut state = self.lock();
            let config = state.config.get(id).cloned();
            let rt = state.runtime.get_mut(id).ok_or(PollerError::UnknownProvider)?;
            if rt.in_flight {
                return Err(PollerError::Busy);
            }
            rt.in_flight = true;
            rt.last_attempt = Some(started);
            (rt.generation, config)
        };

        let store = Arc::clone(&self.keystore);
        let key = tokio::task::spawn_blocking(move || store.get(id))
            .await
            .unwrap_or_else(|_| Err(KeystoreError::Failure("keystore task failed".into())));

        let outcome: Result<Snapshot, LastError> = match key {
            Ok(Some(key)) => provider
                .fetch(&key, config.as_deref(), &self.http)
                .await
                .map_err(LastError::Provider),
            Ok(None) => Err(LastError::Keystore(KeystoreError::Failure("no key is stored".into()))),
            Err(e) => Err(LastError::Keystore(e)),
        };
        // `key` was moved into the match arm and is dropped (and zeroized) here.

        let finished = self.clock.now().timestamp();
        {
            let mut state = self.lock();
            if state.runtime.get(id).is_some_and(|rt| rt.generation != generation) {
                // The key was saved or forgotten while this request was out.
                // The result belongs to the old key, and the reset runtime
                // owns `in_flight` now, so leave everything as it is.
                drop(state);
                return self.view_of(id).ok_or(PollerError::UnknownProvider);
            }
            let interval = Self::interval_for(provider.as_ref(), state.interval);
            let enabled = state.enabled.iter().any(|e| e == id);
            let rt = state.runtime.get_mut(id).ok_or(PollerError::UnknownProvider)?;
            rt.in_flight = false;
            match outcome {
                Ok(snapshot) => {
                    rt.history.push_back(snapshot);
                    while rt.history.len() > HISTORY_CAP {
                        rt.history.pop_front();
                    }
                    rt.last_success = Some(finished);
                    rt.last_error = None;
                    rt.failures = 0;
                    rt.next_due = Some(after(finished, interval));
                }
                Err(err) => {
                    rt.failures = rt.failures.saturating_add(1);
                    let backoff = interval
                        .saturating_mul(1u32 << rt.failures.saturating_sub(1).min(16))
                        .min(MAX_BACKOFF.max(interval));
                    rt.next_due = match &err {
                        // A rejected key or unsupported account will not fix
                        // itself. Wait for a new key or a manual refresh.
                        LastError::Provider(ProviderError::Unauthorized | ProviderError::Unsupported(_)) => None,
                        LastError::Keystore(KeystoreError::Failure(m)) if m == "no key is stored" => {
                            rt.has_key = false;
                            None
                        }
                        LastError::Provider(ProviderError::RateLimited { retry_after, .. }) => {
                            Some(after(finished, retry_after.unwrap_or(backoff).max(backoff)))
                        }
                        _ => Some(after(finished, backoff)),
                    };
                    tracing::warn!(provider = id, kind = ?error_kind(&err), "poll failed");
                    rt.last_error = Some(err);
                }
            }
            if !enabled {
                rt.next_due = None;
            }
        }
        self.emit(id);
        self.view_of(id).ok_or(PollerError::UnknownProvider)
    }
}

fn error_kind(e: &LastError) -> &'static str {
    match e {
        LastError::Provider(p) => p.kind(),
        LastError::Keystore(_) => "keystore",
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use async_trait::async_trait;
    use jiff::Zoned;
    use secrecy::{ExposeSecret, SecretString};

    use super::*;
    use crate::keystore::memory::MemoryKeyStore;
    use crate::wipe::FakeClock;

    struct Mock {
        calls: AtomicUsize,
        result: Mutex<Result<u64, ProviderError>>,
    }

    impl Mock {
        fn new(result: Result<u64, ProviderError>) -> Arc<Self> {
            Arc::new(Self {
                calls: AtomicUsize::new(0),
                result: Mutex::new(result),
            })
        }
    }

    #[async_trait]
    impl Provider for Mock {
        fn id(&self) -> &'static str {
            "mock"
        }
        fn display_name(&self) -> &'static str {
            "Mock"
        }
        fn docs_url(&self) -> &'static str {
            "https://example.com/docs"
        }
        fn endpoint(&self) -> &'static str {
            "https://example.com/usage"
        }
        fn allowed_host(&self) -> &'static str {
            "example.com"
        }
        fn unit(&self) -> Unit {
            Unit::Credits
        }
        fn category(&self) -> Category {
            Category::Data
        }
        fn min_poll_interval(&self) -> Duration {
            Duration::from_secs(60)
        }
        async fn fetch(&self, key: &SecretString, _config: Option<&str>, _http: &Http) -> Result<Snapshot, ProviderError> {
            assert_eq!(key.expose_secret(), "mock-key-123");
            let n = self.calls.fetch_add(1, Ordering::SeqCst) as u64;
            let result = self.result.lock().unwrap().clone();
            result.map(|base| Snapshot {
                taken_at: Timestamp::UNIX_EPOCH,
                remaining: Some(base - n),
                limit: Some(base),
                period_end: None,
                spent: None,
                spend_per_hour: None,
                extra: vec![],
            })
        }
    }

    struct Harness {
        poller: Arc<Poller>,
        mock: Arc<Mock>,
        clock: Arc<FakeClock>,
        emitted: Arc<Mutex<Vec<ProviderView>>>,
    }

    async fn harness(result: Result<u64, ProviderError>, enabled: bool, with_key: bool) -> Harness {
        let mock = Mock::new(result);
        let store = Arc::new(MemoryKeyStore::default());
        if with_key {
            store.set("mock", &SecretString::from("mock-key-123".to_owned())).unwrap();
        }
        let clock = Arc::new(FakeClock::new("2026-10-16T12:00:00+00:00[UTC]".parse::<Zoned>().unwrap()));
        let emitted = Arc::new(Mutex::new(Vec::new()));
        let sink_store = Arc::clone(&emitted);
        let settings = Settings {
            poll_interval_secs: 300,
            enabled: if enabled { vec!["mock".into()] } else { vec![] },
            ..Settings::default()
        };
        let poller = Poller::new(
            vec![mock.clone() as Arc<dyn Provider>],
            store,
            Http::new().unwrap(),
            clock.clone(),
            Arc::new(move |v: &ProviderView| sink_store.lock().unwrap().push(v.clone())),
            &settings,
        );
        poller.load_key_presence().await;
        poller.apply_settings(&settings);
        Harness {
            poller,
            mock,
            clock,
            emitted,
        }
    }

    fn advance(clock: &FakeClock, secs: i64) {
        let next = clock.now().checked_add(SignedDuration::from_secs(secs)).unwrap();
        clock.set(next);
    }

    #[tokio::test]
    async fn polls_enabled_provider_and_records_history() {
        let h = harness(Ok(1000), true, true).await;
        h.poller.tick().await;
        let v = h.poller.view_of("mock").unwrap();
        assert_eq!(v.latest.as_ref().unwrap().remaining, Some(1000));
        assert_eq!(v.history.len(), 1);
        assert!(h.emitted.lock().unwrap().iter().any(|e| e.latest.is_some()));
        // Not due again until the interval passes.
        h.poller.tick().await;
        assert_eq!(h.mock.calls.load(Ordering::SeqCst), 1);
        advance(&h.clock, 300);
        h.poller.tick().await;
        assert_eq!(h.mock.calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn disabled_or_keyless_providers_are_not_polled() {
        let h = harness(Ok(10), false, true).await;
        h.poller.tick().await;
        assert_eq!(h.mock.calls.load(Ordering::SeqCst), 0);
        let h = harness(Ok(10), true, false).await;
        h.poller.tick().await;
        assert_eq!(h.mock.calls.load(Ordering::SeqCst), 0);
        assert!(!h.poller.view_of("mock").unwrap().has_key);
    }

    #[tokio::test]
    async fn history_is_capped() {
        let h = harness(Ok(100_000), true, true).await;
        // 60 s spacing keeps all samples on one date, so no midnight wipe.
        h.poller.apply_settings(&Settings {
            poll_interval_secs: 60,
            enabled: vec!["mock".into()],
            ..Settings::default()
        });
        for _ in 0..(HISTORY_CAP + 10) {
            h.poller.tick().await;
            advance(&h.clock, 60);
        }
        assert_eq!(h.poller.view_of("mock").unwrap().history.len(), HISTORY_CAP);
    }

    #[tokio::test]
    async fn midnight_wipes_history_with_injected_clock() {
        let h = harness(Ok(500), true, true).await;
        h.clock.set("2026-10-16T23:58:00+00:00[UTC]".parse().unwrap());
        h.poller.tick().await;
        assert_eq!(h.poller.view_of("mock").unwrap().history.len(), 1);
        // One minute later: same date, nothing due, history kept.
        h.clock.set("2026-10-16T23:59:00+00:00[UTC]".parse().unwrap());
        h.poller.tick().await;
        assert_eq!(h.poller.view_of("mock").unwrap().history.len(), 1);
        // Past midnight: history is wiped before anything else happens.
        h.emitted.lock().unwrap().clear();
        h.clock.set("2026-10-17T00:00:01+00:00[UTC]".parse().unwrap());
        h.poller.tick().await;
        let emitted = h.emitted.lock().unwrap();
        assert!(
            emitted.iter().any(|v| v.history.is_empty() && v.latest.is_none()),
            "a wiped view was emitted"
        );
    }

    #[tokio::test]
    async fn wipe_now_clears_but_keeps_key() {
        let h = harness(Ok(500), true, true).await;
        h.poller.tick().await;
        h.poller.wipe();
        let v = h.poller.view_of("mock").unwrap();
        assert!(v.history.is_empty() && v.latest.is_none() && v.has_key);
    }

    #[tokio::test]
    async fn unauthorized_stops_polling_until_new_key() {
        let h = harness(Err(ProviderError::Unauthorized), true, true).await;
        h.poller.tick().await;
        let v = h.poller.view_of("mock").unwrap();
        assert_eq!(v.error.unwrap().kind, "unauthorized");
        assert_eq!(v.next_poll_ms, None);
        advance(&h.clock, 3600);
        h.poller.tick().await;
        assert_eq!(h.mock.calls.load(Ordering::SeqCst), 1);
        h.poller.key_saved("mock");
        h.poller.tick().await;
        assert_eq!(h.mock.calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn transport_errors_back_off() {
        let h = harness(Err(ProviderError::Transport("timed out".into())), true, true).await;
        h.poller.tick().await;
        let first = h.poller.view_of("mock").unwrap().next_poll_ms.unwrap();
        advance(&h.clock, 300);
        h.poller.tick().await;
        let second = h.poller.view_of("mock").unwrap().next_poll_ms.unwrap();
        let start = "2026-10-16T12:00:00Z".parse::<Timestamp>().unwrap().as_millisecond();
        assert_eq!(first - start, 300_000);
        assert_eq!(second - (start + 300_000), 600_000);
    }

    #[tokio::test]
    async fn a_shorter_interval_takes_effect_from_the_last_poll() {
        let h = harness(Ok(1000), true, true).await;
        h.poller.apply_settings(&Settings {
            poll_interval_secs: 3600,
            enabled: vec!["mock".into()],
            ..Settings::default()
        });
        h.poller.tick().await;
        assert_eq!(h.mock.calls.load(Ordering::SeqCst), 1);
        // Two minutes in, the user drops the interval to one minute.
        advance(&h.clock, 120);
        h.poller.apply_settings(&Settings {
            poll_interval_secs: 60,
            enabled: vec!["mock".into()],
            ..Settings::default()
        });
        let due = h.poller.view_of("mock").unwrap().next_poll_ms.unwrap();
        let start = "2026-10-16T12:00:00Z".parse::<Timestamp>().unwrap().as_millisecond();
        assert_eq!(due - start, 60_000, "due one minute after the last poll, not an hour");
        h.poller.tick().await;
        assert_eq!(h.mock.calls.load(Ordering::SeqCst), 2);
        // A longer interval pushes the next poll out from the last one.
        h.poller.apply_settings(&Settings {
            poll_interval_secs: 3600,
            enabled: vec!["mock".into()],
            ..Settings::default()
        });
        advance(&h.clock, 120);
        h.poller.tick().await;
        assert_eq!(h.mock.calls.load(Ordering::SeqCst), 2);
        advance(&h.clock, 3600);
        h.poller.tick().await;
        assert_eq!(h.mock.calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn a_shorter_interval_does_not_cut_backoff_short() {
        let h = harness(Err(ProviderError::Transport("timed out".into())), true, true).await;
        h.poller.tick().await;
        advance(&h.clock, 300);
        h.poller.tick().await;
        // Two failures: the next attempt is 600 s out. Shortening the interval leaves it.
        let before = h.poller.view_of("mock").unwrap().next_poll_ms;
        h.poller.apply_settings(&Settings {
            poll_interval_secs: 60,
            enabled: vec!["mock".into()],
            ..Settings::default()
        });
        assert_eq!(h.poller.view_of("mock").unwrap().next_poll_ms, before);
    }

    /// Two providers that each block until released. If `tick` polled them
    /// one after the other, the first would have to finish before the second
    /// started, and the test would deadlock waiting for both to enter.
    #[tokio::test]
    async fn due_providers_are_polled_at_the_same_time() {
        struct Slow {
            id: &'static str,
            entered: tokio::sync::Notify,
            release: tokio::sync::Notify,
        }

        #[async_trait]
        impl Provider for Slow {
            fn id(&self) -> &'static str {
                self.id
            }
            fn display_name(&self) -> &'static str {
                "Slow"
            }
            fn docs_url(&self) -> &'static str {
                "https://example.com/docs"
            }
            fn endpoint(&self) -> &'static str {
                "https://example.com/usage"
            }
            fn allowed_host(&self) -> &'static str {
                "example.com"
            }
            fn unit(&self) -> Unit {
                Unit::Credits
            }
            fn category(&self) -> Category {
                Category::Data
            }
            fn min_poll_interval(&self) -> Duration {
                Duration::from_secs(60)
            }
            async fn fetch(&self, _key: &SecretString, _config: Option<&str>, _http: &Http) -> Result<Snapshot, ProviderError> {
                self.entered.notify_one();
                self.release.notified().await;
                Ok(Snapshot {
                    taken_at: Timestamp::UNIX_EPOCH,
                    remaining: Some(1),
                    limit: None,
                    period_end: None,
                    spent: None,
                    spend_per_hour: None,
                    extra: vec![],
                })
            }
        }

        let slow = |id| {
            Arc::new(Slow {
                id,
                entered: tokio::sync::Notify::new(),
                release: tokio::sync::Notify::new(),
            })
        };
        let (a, b) = (slow("a"), slow("b"));
        let store = Arc::new(MemoryKeyStore::default());
        for id in ["a", "b"] {
            store.set(id, &SecretString::from("k".to_owned())).unwrap();
        }
        let settings = Settings {
            enabled: vec!["a".into(), "b".into()],
            ..Settings::default()
        };
        let poller = Poller::new(
            vec![a.clone() as Arc<dyn Provider>, b.clone() as Arc<dyn Provider>],
            store,
            Http::new().unwrap(),
            Arc::new(FakeClock::new("2026-10-16T12:00:00+00:00[UTC]".parse::<Zoned>().unwrap())),
            Arc::new(|_: &ProviderView| {}),
            &settings,
        );
        poller.load_key_presence().await;
        poller.apply_settings(&settings);

        let ticking = tokio::spawn({
            let poller = Arc::clone(&poller);
            async move { poller.tick().await }
        });
        let both_entered = async {
            a.entered.notified().await;
            b.entered.notified().await;
        };
        tokio::time::timeout(Duration::from_secs(5), both_entered)
            .await
            .expect("both providers should be in flight before either is released");
        assert_eq!(poller.refresh("a").await, Err(PollerError::Busy));
        a.release.notify_one();
        b.release.notify_one();
        tokio::time::timeout(Duration::from_secs(5), ticking)
            .await
            .expect("tick finishes")
            .unwrap();
        for id in ["a", "b"] {
            let v = poller.view_of(id).unwrap();
            assert_eq!(v.latest.as_ref().map(|s| s.remaining), Some(Some(1)), "{id}");
            assert!(v.next_poll_ms.is_some(), "{id} is scheduled again");
        }
    }

    #[tokio::test]
    async fn manual_refresh_has_cooldown() {
        let h = harness(Ok(50), true, true).await;
        h.poller.refresh("mock").await.unwrap();
        assert_eq!(h.poller.refresh("mock").await, Err(PollerError::TooSoon));
        advance(&h.clock, 61);
        assert!(h.poller.refresh("mock").await.is_ok());
        assert_eq!(h.poller.refresh("nope").await, Err(PollerError::UnknownProvider));
    }

    /// Blocks in `fetch` until released, then rejects the key.
    struct Gated {
        entered: tokio::sync::Notify,
        release: tokio::sync::Notify,
    }

    #[async_trait]
    impl Provider for Gated {
        fn id(&self) -> &'static str {
            "mock"
        }
        fn display_name(&self) -> &'static str {
            "Mock"
        }
        fn docs_url(&self) -> &'static str {
            "https://example.com/docs"
        }
        fn endpoint(&self) -> &'static str {
            "https://example.com/usage"
        }
        fn allowed_host(&self) -> &'static str {
            "example.com"
        }
        fn unit(&self) -> Unit {
            Unit::Credits
        }
        fn category(&self) -> Category {
            Category::Data
        }
        fn min_poll_interval(&self) -> Duration {
            Duration::from_secs(60)
        }
        async fn fetch(&self, _key: &SecretString, _config: Option<&str>, _http: &Http) -> Result<Snapshot, ProviderError> {
            self.entered.notify_one();
            self.release.notified().await;
            Err(ProviderError::Unauthorized)
        }
    }

    #[tokio::test]
    async fn result_for_a_replaced_key_is_discarded() {
        let gate = Arc::new(Gated {
            entered: tokio::sync::Notify::new(),
            release: tokio::sync::Notify::new(),
        });
        let store = Arc::new(MemoryKeyStore::default());
        store.set("mock", &SecretString::from("old-key".to_owned())).unwrap();
        let settings = Settings {
            enabled: vec!["mock".into()],
            ..Settings::default()
        };
        let poller = Poller::new(
            vec![gate.clone() as Arc<dyn Provider>],
            store,
            Http::new().unwrap(),
            Arc::new(FakeClock::new("2026-10-16T12:00:00+00:00[UTC]".parse::<Zoned>().unwrap())),
            Arc::new(|_: &ProviderView| {}),
            &settings,
        );
        poller.load_key_presence().await;
        let p = Arc::clone(&poller);
        let task = tokio::spawn(async move { p.refresh("mock").await });
        gate.entered.notified().await;
        // The user saves a new key while the old key's request is out.
        poller.key_saved("mock");
        gate.release.notify_one();
        task.await.unwrap().unwrap();
        let v = poller.view_of("mock").unwrap();
        assert!(v.error.is_none(), "old key's rejection leaked onto the new key: {:?}", v.error);
        assert!(v.next_poll_ms.is_some(), "the new key must still be polled");
    }

    #[tokio::test]
    async fn views_never_contain_the_key() {
        let h = harness(Ok(50), true, true).await;
        h.poller.tick().await;
        let json = serde_json::to_string(&h.poller.views()).unwrap();
        assert!(!json.contains("mock-key-123"));
    }
}
