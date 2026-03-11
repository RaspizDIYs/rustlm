use tauri::State;

use crate::models::automation::AutomationSettings;
use crate::state::AppState;

const AUTOMATION_SETTINGS_KEY: &str = "AutomationSettings";

#[tauri::command]
pub async fn set_auto_accept_enabled(
    enabled: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.auto_accept.set_enabled_arc(enabled).await;
    Ok(())
}

#[tauri::command]
pub fn is_auto_accept_enabled(state: State<AppState>) -> bool {
    state.auto_accept.is_enabled()
}

#[tauri::command]
pub async fn set_automation_settings(
    settings: AutomationSettings,
    state: State<'_, AppState>,
) -> Result<(), String> {
    // Persist to disk
    state.settings.save_setting(AUTOMATION_SETTINGS_KEY, &settings)
        .map_err(|e| e.to_string())?;
    // Update in-memory
    state.auto_accept.set_settings(settings).await;
    Ok(())
}

#[tauri::command]
pub async fn get_automation_settings(
    state: State<'_, AppState>,
) -> Result<AutomationSettings, String> {
    Ok(state.auto_accept.get_settings().await)
}

/// Load persisted automation settings into AutoAcceptService (called at startup)
pub fn load_persisted_automation_settings(state: &AppState) {
    let settings: AutomationSettings = state.settings.load_setting(
        AUTOMATION_SETTINGS_KEY,
        AutomationSettings::default(),
    );
    let auto_accept = state.auto_accept.clone();
    tauri::async_runtime::spawn(async move {
        auto_accept.set_settings(settings).await;
    });
}
