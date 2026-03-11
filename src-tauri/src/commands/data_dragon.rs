use std::collections::HashMap;
use tauri::State;

use crate::models::champion::ChampionInfo;
use crate::state::AppState;

#[tauri::command]
pub async fn get_ddragon_version(state: State<'_, AppState>) -> Result<String, String> {
    state.data_dragon.get_latest_version().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_champions(state: State<'_, AppState>) -> Result<HashMap<String, String>, String> {
    state.data_dragon.get_champions().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_champion_info(
    display_name: String,
    state: State<'_, AppState>,
) -> Result<Option<ChampionInfo>, String> {
    // Ensure champions are loaded
    let _ = state.data_dragon.get_champions().await;
    Ok(state.data_dragon.get_champion_info(&display_name).await)
}

#[tauri::command]
pub async fn get_summoner_spells(
    state: State<'_, AppState>,
) -> Result<HashMap<String, String>, String> {
    state.data_dragon.get_summoner_spells().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_champion_image_url(
    champion_name: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    Ok(state.data_dragon.get_champion_image_url(&champion_name).await)
}
