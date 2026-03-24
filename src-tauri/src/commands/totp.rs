use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};

use crate::error::AppError;
use crate::models::account::TotpSetupInfo;
use crate::services::cloud_sync::CloudSyncService;
use crate::state::AppState;

#[tauri::command]
pub async fn totp_get_status(state: State<'_, AppState>) -> Result<bool, AppError> {
    state.goodluck.totp_status().await
}

#[tauri::command]
pub async fn totp_setup(state: State<'_, AppState>) -> Result<TotpSetupInfo, AppError> {
    state.goodluck.totp_setup().await
}

#[tauri::command]
pub async fn totp_confirm_setup(
    code: String,
    state: State<'_, AppState>,
) -> Result<Vec<String>, AppError> {
    let resp = state.goodluck.totp_confirm(&code).await?;
    Ok(resp.recovery_codes)
}

#[tauri::command]
pub async fn totp_disable(code: String, state: State<'_, AppState>) -> Result<(), AppError> {
    state.goodluck.totp_disable(&code).await?;
    state.cloud_sync.clear_cloud_totp_session();
    Ok(())
}

#[tauri::command]
pub async fn totp_validate(
    code: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    let session = state.goodluck.totp_validate(&code).await?;
    state.cloud_sync.set_totp_session(session);
    CloudSyncService::schedule_totp_expiry_notification(Arc::clone(&state.cloud_sync));
    let _ = app.emit("cloud-sync-complete", ());
    Ok(())
}
