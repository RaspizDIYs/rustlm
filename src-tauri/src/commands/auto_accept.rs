use tauri::State;

use crate::models::automation::AutomationSettings;
use crate::state::AppState;

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
    state.auto_accept.set_settings(settings).await;
    Ok(())
}

#[tauri::command]
pub async fn get_automation_settings(
    state: State<'_, AppState>,
) -> Result<AutomationSettings, String> {
    Ok(state.auto_accept.get_settings().await)
}
