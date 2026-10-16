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

    /// Replace the file without ever leaving a half-written one in its place.
    ///
    /// The new contents go into a temp file next to the target, created
    /// exclusively (so a symlink or leftover planted at that name is refused
    /// rather than followed) and flushed to disk before the rename, so a
    /// crash leaves either the old file or the complete new one. The name
    /// carries the process id and a counter, so two processes cannot stage
    /// onto the same path. On any failure the temp file is removed and the
    /// old file stays as it was.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        use std::io::Write;
        use std::sync::atomic::{AtomicU32, Ordering};

        static STAGED: AtomicU32 = AtomicU32::new(0);

        let dir = path.parent().ok_or_else(|| std::io::Error::other("settings path has no parent"))?;
        let name = path
            .file_name()
            .ok_or_else(|| std::io::Error::other("settings path has no file name"))?;
        std::fs::create_dir_all(dir)?;
        let json = serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?;

        let mut staged = name.to_os_string();
        staged.push(format!(".{}.{}.tmp", std::process::id(), STAGED.fetch_add(1, Ordering::Relaxed)));
        let tmp = dir.join(staged);
        let written = (|| {
            let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&tmp)?;
            file.write_all(&json)?;
            file.sync_all()?;
            std::fs::rename(&tmp, path)
        })();
        if written.is_err() {
            // Best effort: the error being reported is the one that matters.
            let _ = std::fs::remove_file(&tmp);
        }
        written
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

    /// A fresh directory per test, removed when the guard drops.
    struct TempDir(std::path::PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("mr-usage-settings-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn entries(&self) -> Vec<String> {
            let mut names: Vec<String> = std::fs::read_dir(&self.0)
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
            names.sort();
            names
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn save_and_load_round_trip() {
        let dir = TempDir::new("round-trip");
        let path = dir.0.join("settings.json");
        let s = Settings {
            poll_interval_secs: 600,
            enabled: vec!["elicit".into()],
            theme: Theme::Dark,
        };
        s.save(&path).unwrap();
        assert_eq!(Settings::load(&path, IDS), s);
        std::fs::write(&path, b"not json").unwrap();
        assert_eq!(Settings::load(&path, IDS), Settings::default());
    }

    #[test]
    fn save_replaces_the_file_and_leaves_no_staging_file_behind() {
        let dir = TempDir::new("replace");
        let path = dir.0.join("settings.json");
        let first = Settings {
            poll_interval_secs: 600,
            ..Settings::default()
        };
        let second = Settings {
            poll_interval_secs: 900,
            ..Settings::default()
        };
        first.save(&path).unwrap();
        second.save(&path).unwrap();
        assert_eq!(Settings::load(&path, IDS), second);
        assert_eq!(dir.entries(), vec!["settings.json".to_owned()]);
    }

    #[test]
    fn a_failed_save_keeps_the_old_file_and_cleans_up() {
        let dir = TempDir::new("failed");
        let path = dir.0.join("settings.json");
        let kept = Settings {
            poll_interval_secs: 600,
            ..Settings::default()
        };
        kept.save(&path).unwrap();
        // A non-empty directory at the target name lets the staging file be
        // written in full and then makes the final rename fail.
        let occupied = dir.0.join("occupied.json");
        std::fs::create_dir(&occupied).unwrap();
        std::fs::write(occupied.join("child"), b"").unwrap();
        assert!(Settings::default().save(&occupied).is_err());
        // The earlier file is untouched and the staging file was removed.
        assert_eq!(Settings::load(&path, IDS), kept);
        assert_eq!(dir.entries(), vec!["occupied.json".to_owned(), "settings.json".to_owned()]);
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_planted_at_the_staging_name_is_not_followed() {
        let dir = TempDir::new("symlink");
        let path = dir.0.join("settings.json");
        let victim = dir.0.join("victim");
        std::fs::write(&victim, b"untouched").unwrap();
        // The staging name is predictable for a given process and counter, so
        // plant links at the next few candidates.
        for n in 0..8 {
            let link = dir.0.join(format!("settings.json.{}.{n}.tmp", std::process::id()));
            std::os::unix::fs::symlink(&victim, &link).unwrap();
        }
        assert!(
            Settings::default().save(&path).is_err(),
            "an exclusive create must refuse an existing link"
        );
        assert_eq!(std::fs::read(&victim).unwrap(), b"untouched");
        assert!(!path.exists());
    }
}
