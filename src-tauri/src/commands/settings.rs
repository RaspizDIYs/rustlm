use tauri::State;

use crate::state::AppState;

#[cfg(windows)]
const APP_NAME: &str = "RustLM";
#[cfg(windows)]
const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const AUTOSTART_BG_KEY: &str = "AutostartBackground";

// ── Registry helpers ─────────────────────────────────────────────────────────

#[cfg(windows)]
fn autostart_registry_value(background: bool) -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let path = exe.to_string_lossy();
    if background {
        Ok(format!("\"{}\" --minimized", path))
    } else {
        Ok(format!("\"{}\"", path))
    }
}

#[cfg(windows)]
fn write_autostart(enabled: bool, background: bool) -> Result<(), String> {
    use winreg::{
        enums::{HKEY_CURRENT_USER, KEY_SET_VALUE},
        RegKey,
    };
    let key = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(RUN_KEY, KEY_SET_VALUE)
        .map_err(|e| e.to_string())?;
    if enabled {
        let value = autostart_registry_value(background)?;
        key.set_value(APP_NAME, &value.as_str())
            .map_err(|e| e.to_string())?;
    } else {
        key.delete_value(APP_NAME).ok();
    }
    Ok(())
}

#[cfg(windows)]
fn read_autostart() -> bool {
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(RUN_KEY)
        .and_then(|key| key.get_value::<String, _>(APP_NAME))
        .is_ok()
}

// ── Commands ──────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_autostart_enabled() -> bool {
    #[cfg(windows)]
    return read_autostart();
    #[cfg(not(windows))]
    false
}

#[tauri::command]
pub fn set_autostart_enabled(enabled: bool, state: State<AppState>) -> Result<(), String> {
    let background: bool = state.settings.load_setting(AUTOSTART_BG_KEY, false);
    #[cfg(windows)]
    write_autostart(enabled, background)?;
    Ok(())
}

#[tauri::command]
pub fn get_autostart_background(state: State<AppState>) -> bool {
    state.settings.load_setting(AUTOSTART_BG_KEY, false)
}

#[tauri::command]
pub fn set_autostart_background(enabled: bool, state: State<AppState>) -> Result<(), String> {
    state
        .settings
        .save_setting(AUTOSTART_BG_KEY, &enabled)
        .map_err(|e| e.to_string())?;
    // Re-write registry entry if autostart is currently on
    #[cfg(windows)]
    if read_autostart() {
        write_autostart(true, enabled)?;
    }
    Ok(())
}

/// Returns true when the process was launched with --minimized (autostart + background mode).
#[tauri::command]
pub fn should_start_minimized() -> bool {
    std::env::args().any(|a| a == "--minimized")
}

// ── Generic settings ──────────────────────────────────────────────────────────

#[tauri::command]
pub fn load_setting(
    key: &str,
    default_value: serde_json::Value,
    state: State<AppState>,
) -> serde_json::Value {
    state
        .settings
        .load_setting::<serde_json::Value>(key, default_value)
}

#[tauri::command]
pub fn save_setting(
    key: &str,
    value: serde_json::Value,
    state: State<AppState>,
) -> Result<(), String> {
    state
        .settings
        .save_setting(key, &value)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn load_update_settings(
    state: State<AppState>,
) -> crate::models::settings::UpdateSettings {
    state.settings.load_update_settings()
}

#[tauri::command]
pub fn save_update_settings(
    settings: crate::models::settings::UpdateSettings,
    state: State<AppState>,
) -> Result<(), String> {
    state
        .settings
        .save_update_settings(&settings)
        .map_err(|e| e.to_string())
}
