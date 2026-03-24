use tauri::State;

use crate::commands::accounts::trigger_cloud_sync;
use crate::models::rune::{Rune, RunePage, RunePath};
use crate::services::rune_data::RuneDataService;
use crate::state::AppState;

// Rune data commands

#[tauri::command]
pub async fn get_rune_paths(state: State<'_, AppState>) -> Result<Vec<RunePath>, String> {
    state.rune_data.get_all_paths().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_rune_path_by_id(
    id: i32,
    state: State<'_, AppState>,
) -> Result<Option<RunePath>, String> {
    state.rune_data.get_path_by_id(id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_rune_by_id(
    id: i32,
    state: State<'_, AppState>,
) -> Result<Option<Rune>, String> {
    state.rune_data.get_rune_by_id(id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_stat_mods_row1() -> Vec<Rune> {
    RuneDataService::get_stat_mods_row1()
}

#[tauri::command]
pub fn get_stat_mods_row2() -> Vec<Rune> {
    RuneDataService::get_stat_mods_row2()
}

#[tauri::command]
pub fn get_stat_mods_row3() -> Vec<Rune> {
    RuneDataService::get_stat_mods_row3()
}

// Rune pages storage commands

#[tauri::command]
pub fn load_rune_pages(state: State<AppState>) -> Vec<RunePage> {
    state.rune_pages.load_all()
}

#[tauri::command]
pub fn save_rune_page(page: RunePage, state: State<AppState>) -> Result<(), String> {
    state.rune_pages.save(page).map_err(|e| e.to_string())?;
    trigger_cloud_sync(&state);
    Ok(())
}

#[tauri::command]
pub fn save_all_rune_pages(pages: Vec<RunePage>, state: State<AppState>) -> Result<(), String> {
    state.rune_pages.save_all(&pages).map_err(|e| e.to_string())?;
    trigger_cloud_sync(&state);
    Ok(())
}

#[tauri::command]
pub fn delete_rune_page(page_name: String, state: State<AppState>) -> Result<(), String> {
    state.rune_pages.delete(&page_name).map_err(|e| e.to_string())?;
    trigger_cloud_sync(&state);
    Ok(())
}
