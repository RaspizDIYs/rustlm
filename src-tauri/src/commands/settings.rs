use tauri::State;

use crate::state::AppState;

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
