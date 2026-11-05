use std::sync::Arc;

use tauri::State;

use crate::error::AppError;
use crate::services::lol_config::{ConfigStatus, PresetMeta};
use crate::state::AppState;

fn join_err(e: tokio::task::JoinError) -> AppError {
    AppError::Custom(format!("background task failed: {}", e))
}

#[tauri::command]
pub fn lol_cfg_get_status(state: State<'_, AppState>) -> ConfigStatus {
    state.lol_config.get_status()
}

#[tauri::command]
pub async fn lol_cfg_set_readonly(
    state: State<'_, AppState>,
    readonly: bool,
) -> Result<(), AppError> {
    let svc = Arc::clone(&state.lol_config);
    tokio::task::spawn_blocking(move || svc.set_readonly(readonly))
        .await
        .map_err(join_err)?
}

#[tauri::command]
pub async fn lol_cfg_list_presets(state: State<'_, AppState>) -> Result<Vec<PresetMeta>, AppError> {
    let svc = Arc::clone(&state.lol_config);
    tokio::task::spawn_blocking(move || svc.list_presets())
        .await
        .map_err(join_err)?
}

#[tauri::command]
pub async fn lol_cfg_create_preset(
    state: State<'_, AppState>,
    name: String,
) -> Result<PresetMeta, AppError> {
    let svc = Arc::clone(&state.lol_config);
    tokio::task::spawn_blocking(move || svc.create_preset(name))
        .await
        .map_err(join_err)?
}

#[tauri::command]
pub async fn lol_cfg_apply_preset(state: State<'_, AppState>, id: String) -> Result<(), AppError> {
    let svc = Arc::clone(&state.lol_config);
    tokio::task::spawn_blocking(move || svc.apply_preset(id))
        .await
        .map_err(join_err)?
}

#[tauri::command]
pub async fn lol_cfg_delete_preset(state: State<'_, AppState>, id: String) -> Result<(), AppError> {
    let svc = Arc::clone(&state.lol_config);
    tokio::task::spawn_blocking(move || svc.delete_preset(id))
        .await
        .map_err(join_err)?
}

#[tauri::command]
pub async fn lol_cfg_export_preset(
    state: State<'_, AppState>,
    id: String,
    path: String,
) -> Result<(), AppError> {
    let svc = Arc::clone(&state.lol_config);
    tokio::task::spawn_blocking(move || svc.export_preset(id, path))
        .await
        .map_err(join_err)?
}

#[tauri::command]
pub async fn lol_cfg_import_preset(
    state: State<'_, AppState>,
    path: String,
) -> Result<PresetMeta, AppError> {
    let svc = Arc::clone(&state.lol_config);
    tokio::task::spawn_blocking(move || svc.import_preset(path))
        .await
        .map_err(join_err)?
}

#[tauri::command]
pub async fn lol_cfg_guest_status(
    state: State<'_, AppState>,
) -> Result<crate::services::lol_config::GuestConfigStatus, AppError> {
    let svc = Arc::clone(&state.lol_config);
    tokio::task::spawn_blocking(move || svc.guest_status())
        .await
        .map_err(join_err)?
}
#[tauri::command]
pub async fn lol_cfg_guest_start(
    state: State<'_, AppState>,
    id: String,
    mode: Option<crate::services::lol_config::GuestConfigMode>,
) -> Result<(), AppError> {
    let svc = Arc::clone(&state.lol_config);
    tokio::task::spawn_blocking(move || svc.guest_start(id, mode.unwrap_or_default()))
        .await
        .map_err(join_err)?
}
#[tauri::command]
pub async fn lol_cfg_guest_restore(
    state: State<'_, AppState>,
    confirm_changes: bool,
    save_name: Option<String>,
) -> Result<(), AppError> {
    let svc = Arc::clone(&state.lol_config);
    tokio::task::spawn_blocking(move || svc.guest_restore(confirm_changes, save_name))
        .await
        .map_err(join_err)?
}

#[tauri::command]
pub async fn lol_cfg_guest_set_default(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), AppError> {
    let svc = Arc::clone(&state.lol_config);
    tokio::task::spawn_blocking(move || svc.set_personal_preset(id))
        .await
        .map_err(join_err)?
}
