//! The main window is created in code, not in `tauri.conf.json`, so its
//! webview storage can be made ephemeral.
//!
//! `incognito(true)` gives every platform a non-persistent website data store.
//! On Linux, WebKitGTK still creates files in its base data directory
//! (`hsts-storage.sqlite`, cache and storage salts) even in that mode, so the
//! base directory is pointed at a per-launch folder in `$XDG_RUNTIME_DIR` (a
//! user-only tmpfs cleared at logout) and removed when the app exits.
//! (Observed 2026-10-16 during review with WebKitGTK 4.1 on Linux: with
//! incognito on, those three files land in the per-launch folder.)
//! TODO(verify): the non-persistent store on macOS and Windows; WebView2 still
//! creates its user data folder (docs/FOLLOWUPS.md).

use std::path::PathBuf;

use tauri::{App, AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

/// The per-launch webview data folder, if one was created.
pub struct EphemeralDir(pub Option<PathBuf>);

pub fn create_main(app: &mut App) -> tauri::Result<()> {
    let builder = WebviewWindowBuilder::new(app, "main", WebviewUrl::default())
        .title("Mr. Usage")
        .inner_size(460.0, 760.0)
        .min_inner_size(360.0, 480.0)
        .incognito(true);

    #[cfg(target_os = "linux")]
    let builder = {
        let dir = linux::prepare()?;
        app.manage(EphemeralDir(Some(dir.clone())));
        builder.data_directory(dir)
    };
    #[cfg(not(target_os = "linux"))]
    app.manage(EphemeralDir(None));

    builder.build()?;
    Ok(())
}

/// Delete the per-launch folder. Called on exit.
pub fn cleanup(handle: &AppHandle) {
    let Some(state) = handle.try_state::<EphemeralDir>() else { return };
    let Some(dir) = &state.0 else { return };
    match std::fs::remove_dir_all(dir) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => tracing::warn!(error = %e, "could not remove the webview data folder"),
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use std::os::unix::fs::DirBuilderExt;
    use std::path::{Path, PathBuf};

    const PREFIX: &str = "mr-usage-webview-";

    fn base() -> PathBuf {
        std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .filter(|p| p.is_dir())
            .unwrap_or_else(std::env::temp_dir)
    }

    /// Remove folders left by earlier runs that crashed: ours carry the PID
    /// of the process that made them, and a PID with no `/proc` entry is gone.
    fn sweep(base: &Path) {
        let Ok(entries) = std::fs::read_dir(base) else { return };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(pid) = name.to_str().and_then(|n| n.strip_prefix(PREFIX)) else {
                continue;
            };
            if pid.parse::<u32>().is_ok() && !Path::new("/proc").join(pid).exists() {
                let _ = std::fs::remove_dir_all(entry.path());
            }
        }
    }

    pub fn prepare() -> tauri::Result<PathBuf> {
        let base = base();
        sweep(&base);
        let dir = base.join(format!("{PREFIX}{}", std::process::id()));
        // A folder with our PID can only be a leftover from a reused PID.
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::DirBuilder::new().mode(0o700).create(&dir)?;
        Ok(dir)
    }
}
