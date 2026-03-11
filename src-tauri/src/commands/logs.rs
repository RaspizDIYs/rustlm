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
    std::fs::write(state.logger.log_path(), "").map_err(|e| e.to_string())
}
