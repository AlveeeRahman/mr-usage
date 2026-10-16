/// The complete set of commands the frontend may call. `AppManifest::commands`
/// makes Tauri deny any registered command not listed here, and each one must
/// also be granted by name in `capabilities/main.json`.
/// See https://v2.tauri.app/security/capabilities/ (read 2026-10-16).
const COMMANDS: &[&str] = &[
    "list_providers",
    "save_key",
    "forget_key",
    "key_hint",
    "refresh",
    "get_settings",
    "update_settings",
    "wipe_now",
];

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)))?;
    Ok(())
}
