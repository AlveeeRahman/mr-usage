//! The commands the frontend may call. Each is listed in `build.rs` and granted
//! by name in `capabilities/main.json`. Errors cross the bridge as plain,
//! already-scrubbed strings.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use mr_usage_core::keystore::{self, KeyStore};
use mr_usage_core::poller::{Poller, ProviderView};
use mr_usage_core::settings::{MAX_POLL_SECS, MIN_POLL_SECS, Settings, Theme, View, WindowGeometry};
use secrecy::SecretString;
use serde::Serialize;
use tauri::State;

pub struct AppState {
    pub poller: Arc<Poller>,
    pub keystore: Arc<dyn KeyStore>,
    pub settings: Mutex<Settings>,
    pub settings_path: PathBuf,
    /// The last size and position the window reported while it was neither
    /// minimized nor full screen. Written to the settings file on exit.
    pub window_geometry: Mutex<Option<WindowGeometry>>,
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

    /// Change settings, tell the poller, then persist them. The poller is
    /// updated while the settings lock is held, so two concurrent updates
    /// (for example two `save_key` calls) cannot reach it in the opposite
    /// order from the one they were saved in.
    ///
    /// The change is applied to this session before the file is written, so
    /// a disk that refuses the write (full, read-only, permissions) does not
    /// leave the window, the poller and the saved key disagreeing with each
    /// other. The error says that only the persistence failed.
    pub fn update(&self, change: impl FnOnce(&mut Settings)) -> Result<Settings, String> {
        let ids = self.poller.provider_ids();
        let mut guard = self.settings();
        let mut next = guard.clone();
        change(&mut next);
        let next = next.sanitized(&ids);
        *guard = next.clone();
        self.poller.apply_settings(&next);
        next.save(&self.settings_path).map_err(|e| {
            tracing::warn!(error = %e, "could not save settings");
            "Applied for this session, but the settings file could not be written.".to_owned()
        })?;
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

/// Which view is open, which cards are pinned and which are folded. Pure
/// layout; the poller is not involved.
#[tauri::command]
pub fn update_layout(state: State<'_, AppState>, view: View, pinned: Vec<String>, collapsed: Vec<String>) -> Result<SettingsView, String> {
    state.update(|s| {
        s.view = view;
        s.pinned = pinned;
        s.collapsed = collapsed;
    })?;
    Ok(get_settings(state))
}

/// Set or clear a provider's extra identifier (project id, team id, workspace
/// name). Not a key: it is stored in the settings file. The provider is
/// polled again so the new value takes effect at once.
#[tauri::command]
pub fn set_provider_config(state: State<'_, AppState>, provider_id: String, value: Option<String>) -> Result<ProviderView, String> {
    let id = state.known_provider(&provider_id)?;
    state.update(|s| match value.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
        Some(v) => {
            s.provider_config.insert(id.to_owned(), v.to_owned());
        }
        None => {
            s.provider_config.remove(id);
        }
    })?;
    state.poller.config_changed(id);
    state.poller.view_of(id).ok_or_else(|| "Unknown provider.".to_owned())
}

#[tauri::command]
pub fn wipe_now(state: State<'_, AppState>) {
    state.poller.wipe();
}
