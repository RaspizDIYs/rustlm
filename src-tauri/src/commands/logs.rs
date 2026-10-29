use tauri::State;

use crate::state::AppState;

#[tauri::command]
pub fn get_log_lines(state: State<AppState>) -> Vec<String> {
    state.logger.get_log_lines()
}

#[tauri::command]
pub fn get_log_path(state: State<AppState>) -> String {
    state.logger.log_path().to_string_lossy().to_string()
}

#[tauri::command]
pub fn clear_logs(state: State<AppState>) -> Result<(), String> {
    state.logger.clear().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_log_chunk(state: State<'_, AppState>, offset: u64, generation: u64) -> Result<crate::services::file_logger::LogChunk, String> {
    let logger = state.logger.clone();
    tokio::task::spawn_blocking(move || logger.chunk(offset, generation)).await.map_err(|e| e.to_string())?
}
