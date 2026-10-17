//! The main window is created in code, not in `tauri.conf.json`, so its
//! webview storage can be made ephemeral.
//!
//! On macOS and Windows the webview runs in incognito mode, which gives it a
//! non-persistent website data store. TODO(verify): the non-persistent store
//! on macOS and Windows; WebView2 still creates its user data folder
//! (docs/FOLLOWUPS.md).
//!
//! On Linux, WebKitGTK writes `hsts-storage.sqlite`, cache and storage salts
//! into its base data directory whether or not incognito is on (observed
//! 2026-10-16 with WebKitGTK 4.1), so the whole base directory is a
//! per-launch folder in `$XDG_RUNTIME_DIR` (a user-only tmpfs cleared at
//! logout), removed when the app exits. Incognito is left off there: it adds
//! nothing on top of a throwaway directory, and wry 0.57 disables WebKit
//! automation for incognito contexts, which would make the WebDriver
//! end-to-end test impossible.

use std::path::PathBuf;

use mr_usage_core::settings::{MIN_WINDOW_HEIGHT, MIN_WINDOW_WIDTH, WindowGeometry};
use tauri::{App, AppHandle, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder, Window};

use crate::commands::AppState;

/// The per-launch webview data folder, if one was created.
pub struct EphemeralDir(pub Option<PathBuf>);

/// Create the window where it was last closed. A saved position is used only
/// when it still falls on a connected monitor, so a window last seen on an
/// unplugged display does not open off screen; otherwise the system places
/// it. Full screen is never restored: the user leaves it with the same button
/// they entered with, and reopening into it is a surprise. Maximized is.
pub fn create_main(app: &mut App, saved: Option<WindowGeometry>) -> tauri::Result<()> {
    let mut builder = WebviewWindowBuilder::new(app, "main", WebviewUrl::default())
        .title("Mr. Usage")
        .inner_size(460.0, 760.0)
        .min_inner_size(f64::from(MIN_WINDOW_WIDTH), f64::from(MIN_WINDOW_HEIGHT));
    if let Some(g) = saved {
        builder = builder.inner_size(f64::from(g.width), f64::from(g.height));
        if on_a_monitor(app, g) {
            builder = builder.position(f64::from(g.x), f64::from(g.y));
        }
        if g.maximized {
            builder = builder.maximized(true);
        }
    }

    #[cfg(target_os = "linux")]
    let builder = {
        let dir = linux::prepare()?;
        app.manage(EphemeralDir(Some(dir.clone())));
        builder.data_directory(dir)
    };
    #[cfg(not(target_os = "linux"))]
    let builder = {
        app.manage(EphemeralDir(None));
        builder.incognito(true)
    };

    builder.build()?;
    Ok(())
}

/// True when the top-left corner, in logical pixels, lies inside one of the
/// monitors connected now. A failed monitor query counts as "no".
fn on_a_monitor(app: &App, g: WindowGeometry) -> bool {
    let Ok(monitors) = app.available_monitors() else { return false };
    monitors.iter().any(|m| {
        let scale = m.scale_factor();
        let pos = m.position();
        let size = m.size();
        let (left, top) = (f64::from(pos.x) / scale, f64::from(pos.y) / scale);
        let (right, bottom) = (left + f64::from(size.width) / scale, top + f64::from(size.height) / scale);
        let (x, y) = (f64::from(g.x), f64::from(g.y));
        x >= left && x < right - 40.0 && y >= top && y < bottom - 40.0
    })
}

/// Record the window's size and position after a resize or move, unless it
/// is minimized (some systems report a far off-screen position then) or
/// full screen (the size would be the whole display). A maximized window
/// keeps the last normal geometry and only sets the flag, so unmaximizing
/// after a restart gives back the earlier size.
pub fn remember_geometry(window: &Window) {
    let Some(state) = window.try_state::<AppState>() else { return };
    let (Ok(minimized), Ok(fullscreen), Ok(maximized)) = (window.is_minimized(), window.is_fullscreen(), window.is_maximized()) else {
        return;
    };
    if minimized || fullscreen {
        return;
    }
    let mut slot = match state.window_geometry.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    if maximized {
        if let Some(g) = slot.as_mut() {
            g.maximized = true;
        }
        return;
    }
    let (Ok(size), Ok(position), Ok(scale)) = (window.inner_size(), window.outer_position(), window.scale_factor()) else {
        return;
    };
    let size = size.to_logical::<u32>(scale);
    let position: tauri::LogicalPosition<i32> = PhysicalPosition::new(position.x, position.y).to_logical(scale);
    *slot = Some(WindowGeometry {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
        maximized: false,
    });
}

/// Persist the remembered geometry with the rest of the settings. Called on
/// exit, after the window is gone, so it reads the recorded value rather
/// than the window.
pub fn save_geometry(state: &AppState) {
    let geometry = match state.window_geometry.lock() {
        Ok(g) => *g,
        Err(poisoned) => *poisoned.into_inner(),
    };
    let Some(geometry) = geometry else { return };
    if let Err(e) = state.update(|s| s.window = Some(geometry)) {
        tracing::warn!(error = %e, "could not save the window geometry");
    }
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
