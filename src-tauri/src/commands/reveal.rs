use tauri::State;

use crate::commands::accounts::trigger_cloud_sync;
use crate::models::player::PlayerInfo;
use crate::state::AppState;

#[tauri::command]
pub async fn get_reveal_api_config(
    state: State<'_, AppState>,
) -> Result<(String, String), String> {
    Ok(state.reveal.get_api_config().await)
}

#[tauri::command]
pub async fn set_reveal_api_config(
    api_key: String,
    region: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.reveal.set_api_configuration(&api_key, &region).await;
    trigger_cloud_sync(&state);
    Ok(())
}

#[tauri::command]
pub async fn test_api_key(
    state: State<'_, AppState>,
) -> Result<(bool, String), String> {
    state.reveal.test_api_key().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_teams_info(
    state: State<'_, AppState>,
) -> Result<(Vec<PlayerInfo>, Vec<PlayerInfo>), String> {
    state.reveal.get_teams_info().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn send_chat_message(
    message: String,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    state.reveal.send_message_to_chat(&message).await.map_err(|e| e.to_string())
}
