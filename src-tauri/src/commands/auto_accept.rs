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
    mut settings: AutomationSettings,
    state: State<'_, AppState>,
) -> Result<(), String> {
    // Resolve champion names → numeric IDs
    resolve_champion_id(&state, &settings.pick_champion1, &mut settings.pick_champion1_id).await;
    resolve_champion_id(&state, &settings.pick_champion2, &mut settings.pick_champion2_id).await;
    resolve_champion_id(&state, &settings.pick_champion3, &mut settings.pick_champion3_id).await;
    resolve_champion_id(&state, &settings.ban_champion, &mut settings.ban_champion_id).await;

    log::info!("[Cmd] set_automation_settings: pick1={:?}→{:?}, ban={:?}→{:?}, spell1={:?}, spell2={:?}",
        settings.pick_champion1, settings.pick_champion1_id,
        settings.ban_champion, settings.ban_champion_id,
        settings.spell1_id, settings.spell2_id,
    );

    // Persist to disk
    state.settings.save_setting(AUTOMATION_SETTINGS_KEY, &settings)
        .map_err(|e| e.to_string())?;
    // Update in-memory
    state.auto_accept.set_settings(settings).await;
    Ok(())
}

async fn resolve_champion_id(
    state: &AppState,
    name: &Option<String>,
    id_out: &mut Option<i32>,
) {
    resolve_champion_id_with_dd(&state.data_dragon, name, id_out).await;
}

#[tauri::command]
pub async fn get_automation_settings(
    state: State<'_, AppState>,
) -> Result<AutomationSettings, String> {
    Ok(state.auto_accept.get_settings().await)
}

/// Load persisted automation settings into AutoAcceptService (called at startup)
pub fn load_persisted_automation_settings(state: &AppState) {
    let mut settings: AutomationSettings = state.settings.load_setting(
        AUTOMATION_SETTINGS_KEY,
        AutomationSettings::default(),
    );
    let auto_accept = state.auto_accept.clone();
    let data_dragon = state.data_dragon.clone();
    tauri::async_runtime::spawn(async move {
        // Resolve champion names → IDs at startup
        resolve_champion_id_with_dd(&data_dragon, &settings.pick_champion1, &mut settings.pick_champion1_id).await;
        resolve_champion_id_with_dd(&data_dragon, &settings.pick_champion2, &mut settings.pick_champion2_id).await;
        resolve_champion_id_with_dd(&data_dragon, &settings.pick_champion3, &mut settings.pick_champion3_id).await;
        resolve_champion_id_with_dd(&data_dragon, &settings.ban_champion, &mut settings.ban_champion_id).await;
        auto_accept.set_settings(settings).await;
    });
}

async fn resolve_champion_id_with_dd(
    data_dragon: &crate::services::data_dragon::DataDragonService,
    name: &Option<String>,
    id_out: &mut Option<i32>,
) {
    if let Some(name) = name {
        if !name.is_empty() {
            // Ensure champions are loaded
            let _ = data_dragon.get_champions().await;
            if let Some(info) = data_dragon.get_champion_info(name).await {
                *id_out = info.id.parse::<i32>().ok();
                return;
            }
        }
    }
    *id_out = None;
}
