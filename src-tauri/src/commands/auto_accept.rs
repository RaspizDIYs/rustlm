use tauri::State;

use crate::commands::accounts::trigger_cloud_sync;
use crate::models::automation::AutomationSettings;
use crate::state::AppState;

const AUTOMATION_SETTINGS_KEY: &str = "AutomationSettings";

pub async fn apply_auto_accept_enabled(state: &AppState, enabled: bool) -> Result<(), String> {
    state.auto_accept.set_enabled_arc(enabled).await;
    let mut s: AutomationSettings = state
        .settings
        .load_setting(AUTOMATION_SETTINGS_KEY, AutomationSettings::default());
    s.is_enabled = enabled;
    state
        .settings
        .save_setting(AUTOMATION_SETTINGS_KEY, &s)
        .map_err(|e| e.to_string())?;
    trigger_cloud_sync(state);
    Ok(())
}

#[tauri::command]
pub async fn set_auto_accept_enabled(
    enabled: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    apply_auto_accept_enabled(&state, enabled).await
}

#[tauri::command]
pub fn is_auto_accept_enabled(state: State<AppState>) -> bool {
    state.auto_accept.is_enabled()
}

fn normalize_empty_champion_names(settings: &mut AutomationSettings) {
    for opt in [
        &mut settings.pick_champion1,
        &mut settings.pick_champion2,
        &mut settings.pick_champion3,
        &mut settings.ban_champion,
    ] {
        if matches!(opt.as_deref(), Some("")) {
            *opt = None;
        }
    }
}

#[tauri::command]
pub async fn set_automation_settings(
    mut settings: AutomationSettings,
    state: State<'_, AppState>,
) -> Result<(), String> {
    if !matches!(
        settings.auto_accept_method.as_str(),
        "WebSocket" | "Polling" | "UIA"
    ) {
        return Err("Неизвестный метод автопринятия".into());
    }
    if settings.profiles.len() > 9
        || settings.profiles.iter().any(|(key, profile)| {
            !matches!(
                key.as_str(),
                "classic"
                    | "classic:top"
                    | "classic:jungle"
                    | "classic:middle"
                    | "classic:bottom"
                    | "classic:utility"
                    | "aram"
                    | "mayhem"
                    | "arena"
            ) || profile.picks.len() > (if matches!(key.as_str(), "aram" | "mayhem") {30} else {3})
                || profile.pick_count.is_some_and(|count| count == 0 || count > (if matches!(key.as_str(), "aram" | "mayhem") {30} else {3}))
                || !matches!(
                    profile.arena_pick.as_str(),
                    "" | "preferred" | "crowd" | "manual"
                )
                || profile.picks.iter().any(|name| name.len() > 100)
                || profile.ban.len() > 100
                || profile.rune_page.len() > 200
        })
    {
        return Err("Некорректный профиль автоматизации".into());
    }
    resolve_profiles(&state.data_dragon, &mut settings).await;
    normalize_empty_champion_names(&mut settings);
    // Resolve champion names → numeric IDs
    resolve_champion_id(
        &state,
        &settings.pick_champion1,
        &mut settings.pick_champion1_id,
    )
    .await;
    resolve_champion_id(
        &state,
        &settings.pick_champion2,
        &mut settings.pick_champion2_id,
    )
    .await;
    resolve_champion_id(
        &state,
        &settings.pick_champion3,
        &mut settings.pick_champion3_id,
    )
    .await;
    resolve_champion_id(
        &state,
        &settings.ban_champion,
        &mut settings.ban_champion_id,
    )
    .await;

    log::info!(
        "[Cmd] set_automation_settings: pick1={:?}→{:?}, ban={:?}→{:?}, spell1={:?}, spell2={:?}",
        settings.pick_champion1,
        settings.pick_champion1_id,
        settings.ban_champion,
        settings.ban_champion_id,
        settings.spell1_id,
        settings.spell2_id,
    );

    // Persist to disk
    state
        .settings
        .save_setting(AUTOMATION_SETTINGS_KEY, &settings)
        .map_err(|e| e.to_string())?;
    // Update in-memory
    state.auto_accept.set_settings(settings).await;
    trigger_cloud_sync(&state);
    Ok(())
}

async fn resolve_champion_id(state: &AppState, name: &Option<String>, id_out: &mut Option<i32>) {
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
    let mut settings: AutomationSettings = state
        .settings
        .load_setting(AUTOMATION_SETTINGS_KEY, AutomationSettings::default());
    settings.auto_accept_method = state
        .settings
        .load_setting("AutoAcceptMethod", settings.auto_accept_method);
    state.auto_accept.set_enabled_flag_only(settings.is_enabled);
    let auto_accept = state.auto_accept.clone();
    let data_dragon = state.data_dragon.clone();
    tauri::async_runtime::spawn(async move {
        resolve_profiles(&data_dragon, &mut settings).await;
        // Resolve champion names → IDs at startup
        resolve_champion_id_with_dd(
            &data_dragon,
            &settings.pick_champion1,
            &mut settings.pick_champion1_id,
        )
        .await;
        resolve_champion_id_with_dd(
            &data_dragon,
            &settings.pick_champion2,
            &mut settings.pick_champion2_id,
        )
        .await;
        resolve_champion_id_with_dd(
            &data_dragon,
            &settings.pick_champion3,
            &mut settings.pick_champion3_id,
        )
        .await;
        resolve_champion_id_with_dd(
            &data_dragon,
            &settings.ban_champion,
            &mut settings.ban_champion_id,
        )
        .await;
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

            if let Some(info) = data_dragon.get_champion_info(name).await {
                *id_out = info.id.parse::<i32>().ok();
                return;
            }
        }
    }
    *id_out = None;
}

async fn resolve_profiles(
    data_dragon: &crate::services::data_dragon::DataDragonService,
    settings: &mut AutomationSettings,
) {
    let _ = data_dragon.get_champions().await;
    for profile in settings.profiles.values_mut() {
        profile.pick_ids.clear();
        for name in &profile.picks {
            let mut id = None;
            resolve_champion_id_with_dd(data_dragon, &Some(name.clone()), &mut id).await;
            profile.pick_ids.push(id.unwrap_or(0));
        }
        resolve_champion_id_with_dd(data_dragon, &Some(profile.ban.clone()), &mut profile.ban_id)
            .await;
    }
}
