//! The commands the frontend may call. Each is listed in `build.rs` and granted
//! by name in `capabilities/main.json`. Errors cross the bridge as plain,
//! already-scrubbed strings.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use mr_usage_core::keystore::{self, KeyStore};
use mr_usage_core::poller::{Poller, ProviderView};
use mr_usage_core::settings::{MAX_POLL_SECS, MIN_POLL_SECS, Settings, Theme};
use secrecy::SecretString;
use serde::Serialize;
use tauri::State;

pub struct AppState {
    pub poller: Arc<Poller>,
    pub keystore: Arc<dyn KeyStore>,
    pub settings: Mutex<Settings>,
    pub settings_path: PathBuf,
}

impl AppState {
    fn settings(&self) -> MutexGuard<'_, Settings> {
        match self.settings.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    fn known_provider(&self, id: &str) -> Result<&'static str, String> {
        self.poller
            .provider_ids()
            .into_iter()
            .find(|known| *known == id)
            .ok_or_else(|| "Unknown provider.".to_owned())
    }

    /// Change settings, persist them, and tell the poller.
    fn update(&self, change: impl FnOnce(&mut Settings)) -> Result<Settings, String> {
        let ids = self.poller.provider_ids();
        let next = {
            let mut guard = self.settings();
            let mut next = guard.clone();
            change(&mut next);
            let next = next.sanitized(&ids);
            next.save(&self.settings_path).map_err(|e| {
                tracing::warn!(error = %e, "could not save settings");
                "Could not save settings.".to_owned()
            })?;
            *guard = next.clone();
            next
        };
        self.poller.apply_settings(&next);
        Ok(next)
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    pub settings: Settings,
    pub store_label: &'static str,
    pub min_poll_secs: u64,
    pub max_poll_secs: u64,
}

#[tauri::command]
pub fn list_providers(state: State<'_, AppState>) -> Vec<ProviderView> {
    state.poller.views()
}

/// Store a key. The frontend clears its input as soon as this returns.
#[tauri::command]
pub async fn save_key(state: State<'_, AppState>, provider_id: String, key: SecretString) -> Result<ProviderView, String> {
    let id = state.known_provider(&provider_id)?;
    let key = keystore::normalize_key(key).map_err(|e| e.to_string())?;
    let store = Arc::clone(&state.keystore);
    tokio::task::spawn_blocking(move || store.set(id, &key))
        .await
        .map_err(|_| "The credential store task failed.".to_owned())?
        .map_err(|e| e.to_string())?;
    state.poller.key_saved(id);
    state.update(|s| {
        if !s.enabled.iter().any(|e| e == id) {
            s.enabled.push(id.to_owned());
        }
    })?;
    state.poller.view_of(id).ok_or_else(|| "Unknown provider.".to_owned())
}

#[tauri::command]
pub async fn forget_key(state: State<'_, AppState>, provider_id: String) -> Result<ProviderView, String> {
    let id = state.known_provider(&provider_id)?;
    let store = Arc::clone(&state.keystore);
    tokio::task::spawn_blocking(move || store.delete(id))
        .await
        .map_err(|_| "The credential store task failed.".to_owned())?
        .map_err(|e| e.to_string())?;
    state.update(|s| s.enabled.retain(|e| e != id))?;
    state.poller.key_forgotten(id);
    state.poller.view_of(id).ok_or_else(|| "Unknown provider.".to_owned())
}

/// A masked hint (`••••abcd`) of the stored key, or `None` if there is none.
#[tauri::command]
pub async fn key_hint(state: State<'_, AppState>, provider_id: String) -> Result<Option<String>, String> {
    let id = state.known_provider(&provider_id)?;
    let store = Arc::clone(&state.keystore);
    tokio::task::spawn_blocking(move || store.get(id).map(|k| k.as_ref().map(keystore::hint)))
        .await
        .map_err(|_| "The credential store task failed.".to_owned())?
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn refresh(state: State<'_, AppState>, provider_id: String) -> Result<ProviderView, String> {
    let id = state.known_provider(&provider_id)?;
    state.poller.refresh(id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> SettingsView {
    SettingsView {
        settings: state.settings().clone(),
        store_label: keystore::store_label(),
        min_poll_secs: MIN_POLL_SECS,
        max_poll_secs: MAX_POLL_SECS,
    }
}

#[tauri::command]
pub fn update_settings(state: State<'_, AppState>, poll_interval_secs: u64, theme: Theme) -> Result<SettingsView, String> {
    state.update(|s| {
        s.poll_interval_secs = poll_interval_secs;
        s.theme = theme;
    })?;
    Ok(get_settings(state))
}

#[tauri::command]
pub fn wipe_now(state: State<'_, AppState>) {
    state.poller.wipe();
}
