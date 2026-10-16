//! The only file the app writes. It must never hold a key, an account email
//! or a usage number; `tests::serialized_settings_hold_no_secrets` enforces it.

use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};

pub const DEFAULT_POLL_SECS: u64 = 5 * 60;
pub const MIN_POLL_SECS: u64 = 60;
pub const MAX_POLL_SECS: u64 = 24 * 60 * 60;
const MAX_FILE_BYTES: u64 = 16 * 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Settings {
    /// Global poll interval. Each provider is polled no faster than its own floor.
    pub poll_interval_secs: u64,
    /// Provider ids with polling enabled.
    pub enabled: Vec<String>,
    pub theme: Theme,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            poll_interval_secs: DEFAULT_POLL_SECS,
            enabled: Vec::new(),
            theme: Theme::System,
        }
    }
}

impl Settings {
    pub fn poll_interval(&self) -> Duration {
        Duration::from_secs(self.poll_interval_secs)
    }

    /// Clamp values and drop unknown or duplicate provider ids.
    pub fn sanitized(mut self, known_ids: &[&str]) -> Self {
        self.poll_interval_secs = self.poll_interval_secs.clamp(MIN_POLL_SECS, MAX_POLL_SECS);
        let mut seen = Vec::new();
        self.enabled.retain(|id| {
            let keep = known_ids.contains(&id.as_str()) && !seen.contains(id);
            if keep {
                seen.push(id.clone());
            }
            keep
        });
        self
    }

    /// Load from disk. A missing, oversized or malformed file yields defaults.
    pub fn load(path: &Path, known_ids: &[&str]) -> Self {
        let parsed = std::fs::metadata(path)
            .ok()
            .filter(|m| m.is_file() && m.len() <= MAX_FILE_BYTES)
            .and_then(|_| std::fs::read(path).ok())
            .and_then(|bytes| serde_json::from_slice::<Settings>(&bytes).ok());
        match parsed {
            Some(s) => s.sanitized(known_ids),
            None => Self::default(),
        }
    }

    /// Write atomically: a temp file in the same directory, then rename.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let dir = path.parent().ok_or_else(|| std::io::Error::other("settings path has no parent"))?;
        std::fs::create_dir_all(dir)?;
        let json = serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const IDS: &[&str] = &["firecrawl", "serpapi", "elicit"];

    #[test]
    fn serialized_settings_hold_no_secrets() {
        let s = Settings {
            poll_interval_secs: 900,
            enabled: IDS.iter().map(|s| (*s).to_owned()).collect(),
            theme: Theme::Dark,
        };
        let json = serde_json::to_string(&s).unwrap().to_ascii_lowercase();
        for forbidden in [
            "key",
            "token",
            "secret",
            "password",
            "auth",
            "email",
            "account",
            "remaining",
            "credit",
            "usage",
            "limit",
            "searches",
        ] {
            assert!(!json.contains(forbidden), "settings JSON contains `{forbidden}`: {json}");
        }
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let tampered = br#"{"pollIntervalSecs":300,"enabled":[],"theme":"system","apiKey":"x"}"#;
        assert!(serde_json::from_slice::<Settings>(tampered).is_err());
    }

    #[test]
    fn sanitize_clamps_and_filters() {
        let s = Settings {
            poll_interval_secs: 1,
            enabled: vec!["serpapi".into(), "nope".into(), "serpapi".into()],
            theme: Theme::Light,
        }
        .sanitized(IDS);
        assert_eq!(s.poll_interval_secs, MIN_POLL_SECS);
        assert_eq!(s.enabled, vec!["serpapi".to_owned()]);
    }

    #[test]
    fn save_and_load_round_trip() {
        let dir = std::env::temp_dir().join(format!("mr-usage-settings-{}", std::process::id()));
        let path = dir.join("settings.json");
        let s = Settings {
            poll_interval_secs: 600,
            enabled: vec!["elicit".into()],
            theme: Theme::Dark,
        };
        s.save(&path).unwrap();
        assert_eq!(Settings::load(&path, IDS), s);
        std::fs::write(&path, b"not json").unwrap();
        assert_eq!(Settings::load(&path, IDS), Settings::default());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
