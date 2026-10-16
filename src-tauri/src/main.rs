#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod cli;
mod commands;
mod hardening;
mod logging;
mod window;

use std::process::ExitCode;
use std::sync::{Arc, Mutex};

use mr_usage_core::keystore::OsKeyStore;
use mr_usage_core::net::Http;
use mr_usage_core::poller::{Poller, ProviderView, Sink};
use mr_usage_core::providers::registry;
use mr_usage_core::settings::Settings;
use mr_usage_core::wipe::SystemClock;
use tauri::{Emitter, Manager, RunEvent};

use crate::commands::AppState;

/// Event carrying one `ProviderView` whenever a provider's state changes.
pub const UPDATE_EVENT: &str = "usage:update";

fn main() -> ExitCode {
    use std::io::Write;

    match cli::parse(std::env::args_os().skip(1)) {
        Ok(cli::Command::Help) => {
            let _ = std::io::stdout().write_all(cli::help_text().as_bytes());
            return ExitCode::SUCCESS;
        }
        Ok(cli::Command::Version) => {
            let _ = std::io::stdout().write_all(cli::version_text().as_bytes());
            return ExitCode::SUCCESS;
        }
        Ok(cli::Command::ForgetKeys) => return forget_keys(),
        Ok(cli::Command::Gui) => {}
        Err(e) => {
            let _ = writeln!(
                std::io::stderr(),
                "mr-usage: {e}\n{}\nTry 'mr-usage --help' for more information.",
                cli::USAGE
            );
            return ExitCode::from(2);
        }
    }
    logging::init();
    hardening::apply();
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!(error = %mr_usage_core::redact::scrub(&e.to_string()), "Mr. Usage failed to start");
            ExitCode::FAILURE
        }
    }
}

/// `mr-usage --forget-keys`: delete every key this app stored in the OS
/// credential store, then exit without opening a window. The install scripts
/// run it before removing the app. Exit codes are documented in `cli.rs`;
/// the failure lines on stderr are built by `keystore::map_error` and never
/// contain key bytes.
fn forget_keys() -> ExitCode {
    use mr_usage_core::keystore::KeyStore;
    use std::io::Write;

    let store = OsKeyStore;
    let mut failed = false;
    for provider in registry() {
        match store.delete(provider.id()) {
            Ok(()) => {}
            // No store at all: every further attempt would say the same thing.
            Err(e @ mr_usage_core::keystore::KeystoreError::Unavailable(_)) => {
                let _ = writeln!(std::io::stderr(), "mr-usage: {e}");
                return ExitCode::FAILURE;
            }
            Err(e) => {
                failed = true;
                let _ = writeln!(
                    std::io::stderr(),
                    "mr-usage: could not remove the {} key: {e}",
                    provider.display_name()
                );
            }
        }
    }
    if failed { ExitCode::FAILURE } else { ExitCode::SUCCESS }
}

/// The webview may only show the bundled app. Production origins are
/// `tauri://localhost` (macOS, Linux) and `http://tauri.localhost` (Windows);
/// debug builds also allow the Vite dev server. TODO(verify): add the Tauri
/// documentation URL for these origins to docs/SOURCES.md.
fn is_app_url(url: &tauri::Url) -> bool {
    let host = url.host_str();
    match url.scheme() {
        "tauri" => host == Some("localhost"),
        "http" | "https" if host == Some("tauri.localhost") => true,
        "http" if cfg!(debug_assertions) => host == Some("localhost") && url.port() == Some(1420),
        _ => false,
    }
}

/// Resolves on SIGTERM or SIGINT, so `kill`, `systemctl --user stop`, a
/// session logout or Ctrl-C shut the app down through Tauri's normal exit
/// path instead of killing it mid-write.
#[cfg(unix)]
async fn shutdown_signal() {
    use tokio::signal::unix::{SignalKind, signal};
    match (signal(SignalKind::terminate()), signal(SignalKind::interrupt())) {
        (Ok(mut term), Ok(mut int)) => {
            tokio::select! {
                _ = term.recv() => {}
                _ = int.recv() => {}
            }
        }
        _ => {
            tracing::warn!("could not install signal handlers");
            std::future::pending::<()>().await;
        }
    }
}

/// Windows GUI apps get WM_CLOSE (for example from `taskkill` without `/F`),
/// which Tauri already turns into a normal exit. Ctrl-C only reaches console
/// builds, which release builds are not. TODO(verify): source for the
/// WM_CLOSE path; .github/scripts/smoke-windows.ps1 exercises it.
#[cfg(windows)]
async fn shutdown_signal() {
    if tokio::signal::ctrl_c().await.is_err() {
        std::future::pending::<()>().await;
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let nav_guard = tauri::plugin::Builder::<tauri::Wry>::new("nav-guard")
        .on_navigation(|_, url| {
            let allowed = is_app_url(url);
            if !allowed {
                tracing::warn!(url = %mr_usage_core::redact::redact_url(url.as_str()), "blocked navigation");
            }
            allowed
        })
        .build();

    let app = tauri::Builder::default()
        .plugin(nav_guard)
        .setup(|app| {
            window::create_main(app)?;
            let settings_path = app.path().app_config_dir()?.join("settings.json");
            let providers = registry();
            let ids: Vec<&str> = providers.iter().map(|p| p.id()).collect();
            let settings = Settings::load(&settings_path, &ids);

            let handle = app.handle().clone();
            let sink: Sink = Arc::new(move |view: &ProviderView| {
                if let Err(e) = handle.emit(UPDATE_EVENT, view) {
                    tracing::warn!(error = %e, "could not emit update");
                }
            });
            let keystore = Arc::new(OsKeyStore);
            let poller = Poller::new(providers, keystore.clone(), Http::new()?, Arc::new(SystemClock), sink, &settings);

            app.manage(AppState {
                poller: Arc::clone(&poller),
                keystore,
                settings: Mutex::new(settings.clone()),
                settings_path,
            });

            let exit_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                shutdown_signal().await;
                tracing::info!("termination signal received; exiting");
                // Same path as closing the window: RunEvent::Exit wipes history.
                exit_handle.exit(0);
            });

            tauri::async_runtime::spawn(async move {
                poller.load_key_presence().await;
                poller.apply_settings(&settings);
                poller.run().await;
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_providers,
            commands::save_key,
            commands::forget_key,
            commands::key_hint,
            commands::refresh,
            commands::get_settings,
            commands::update_settings,
            commands::wipe_now,
        ])
        .build(tauri::generate_context!())?;

    app.run(|handle, event| {
        if let RunEvent::Exit = event {
            if let Some(state) = handle.try_state::<AppState>() {
                state.poller.wipe();
            }
            window::cleanup(handle);
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::is_app_url;

    #[test]
    fn only_app_origins_are_navigable() {
        for ok in ["tauri://localhost/", "http://tauri.localhost/index.html"] {
            assert!(is_app_url(&ok.parse().unwrap()), "{ok}");
        }
        for bad in [
            "https://serpapi.com/",
            "https://evil.example/",
            "file:///etc/passwd",
            "tauri://evil/",
            "http://localhost:8080/",
        ] {
            assert!(!is_app_url(&bad.parse().unwrap()), "{bad}");
        }
    }
}
