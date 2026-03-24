use tauri::State;

use crate::state::AppState;

#[tauri::command]
pub async fn set_profile_status(
    status: String,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    state.customization.set_profile_status(&status).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_profile_availability(
    availability: String,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    state.customization.set_profile_availability(&availability).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_profile_icon(
    icon_id: i32,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    state.customization.set_profile_icon(icon_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_profile_background(
    background_skin_id: i32,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    state.customization.set_profile_background(background_skin_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_challenges(
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    state.customization.get_challenges().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_challenge_tokens(
    challenge_ids: Vec<i64>,
    title_id: i64,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    state.customization.set_challenge_tokens(&challenge_ids, title_id).await.map_err(|e| e.to_string())
}
