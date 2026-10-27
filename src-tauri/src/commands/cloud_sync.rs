use tauri::{Emitter, State};

use crate::error::AppError;
use crate::models::account::SyncStatus;
use crate::state::AppState;

#[tauri::command]
pub async fn cloud_sync(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<(), AppError> {
    state.cloud_sync.sync(&*state).await?;
    let _ = app.emit("cloud-sync-complete", ());
    Ok(())
}

#[tauri::command]
pub async fn cloud_push(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<(), AppError> {
    state.cloud_sync.push().await?;
    let _ = app.emit("cloud-sync-complete", ());
    Ok(())
}

#[tauri::command]
pub async fn cloud_pull(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<usize, AppError> {
    let count = state.cloud_sync.pull(&*state).await?;
    let _ = app.emit("cloud-sync-complete", ());
    Ok(count)
}

#[tauri::command]
pub async fn cloud_get_status(state: State<'_, AppState>) -> Result<SyncStatus, AppError> {
    Ok(state.cloud_sync.get_status())
}

#[tauri::command]
pub fn cloud_totp_session_active(state: State<'_, AppState>) -> bool {
    state.cloud_sync.get_valid_totp_session_public().is_some()
}

#[tauri::command]
pub fn cloud_notify_change(state: State<'_, AppState>) -> Result<(), AppError> {
    if state.goodluck.is_connected() {
        state.cloud_sync.notify_change();
    }
    Ok(())
}

#[tauri::command]
pub async fn cloud_delete_data(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    let session = state.cloud_sync.get_valid_totp_session_public();
    state
        .cloud_sync
        .delete_cloud_data(session.as_deref())
        .await?;
    let _ = app.emit("cloud-sync-complete", ());
    Ok(())
}

#[tauri::command]
pub fn vault_connected(state: State<'_, AppState>) -> bool {
    state.cloud_sync.vault_connected()
}
#[tauri::command]
pub async fn vault_connect(
    code: Option<String>,
    state: State<'_, AppState>,
) -> Result<Option<String>, AppError> {
    state.cloud_sync.connect_vault(code).await
}
#[tauri::command]
pub async fn vault_resolve(mode: String, state: State<'_, AppState>) -> Result<(), AppError> {
    if !matches!(mode.as_str(), "merge" | "local" | "remote") {
        return Err(AppError::Custom("Неизвестное действие".into()));
    }
    state.cloud_sync.resolve_vault(&mode).await
}

#[tauri::command]
pub fn vault_recovery_code(state: State<'_, AppState>) -> Result<String, AppError> {
    state.cloud_sync.vault_recovery_code()
}
#[tauri::command]
pub async fn vault_rotate(state: State<'_, AppState>) -> Result<String, AppError> {
    state.cloud_sync.rotate_vault().await
}

#[tauri::command]
pub async fn vault_disconnect(state: State<'_,AppState>) -> Result<(),AppError> {state.cloud_sync.disconnect_vault().await}
