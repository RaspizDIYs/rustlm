use crate::{
    models::unglitch::{UnglitchAction, UnglitchResult, UnglitchStatus},
    state::AppState,
};
use tauri::State;

#[tauri::command]
pub async fn unglitch_status(state: State<'_, AppState>) -> Result<UnglitchStatus, String> {
    tokio::time::timeout(
        std::time::Duration::from_secs(12),
        crate::services::unglitch::status(&state.riot_client),
    )
    .await
    .map_err(|_| "Клиент League не ответил вовремя. Повторите проверку.")?
}
#[tauri::command]
pub async fn unglitch_execute(
    state: State<'_, AppState>,
    action: UnglitchAction,
    confirmed: bool,
) -> Result<UnglitchResult, String> {
    state.unglitch.pause_for_manual_action();
    state
        .logger
        .ui_event("Unglitch", &format!("{action:?}"), Some("requested"));
    let result =
        crate::services::unglitch::execute(state.riot_client.clone(), action, confirmed).await;
    state.logger.ui_event(
        "Unglitch",
        &format!("{action:?}"),
        Some(if result.is_ok() {
            "confirmed"
        } else {
            "not confirmed"
        }),
    );
    result
}

#[tauri::command]
pub fn unglitch_watch_status(state: State<'_, AppState>) -> crate::models::unglitch::WatchStatus {
    state.unglitch.snapshot()
}

#[tauri::command]
pub fn unglitch_watch_enabled(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<crate::models::unglitch::WatchStatus, String> {
    state
        .settings
        .save_setting("UnglitchAutoRecovery", &enabled)
        .map_err(|_| "Не удалось сохранить настройку.")?;
    state.unglitch.set_enabled(enabled);
    Ok(state.unglitch.snapshot())
}

#[tauri::command]
pub async fn unglitch_diagnostics(
    state: State<'_, AppState>,
    capture: bool,
) -> Result<crate::services::client_diagnostics::Report, String> {
    if capture {
        state.riot_client.diagnostics.probe().await;
    }
    Ok(state.riot_client.diagnostics.snapshot())
}

#[tauri::command]
pub async fn unglitch_export_diagnostics(state: State<'_, AppState>) -> Result<String, String> {
    let report = state.riot_client.diagnostics.snapshot();
    tokio::task::spawn_blocking(move || {
        let folder = crate::services::repair_paths::root()?.join("logs");
        let path = report.export(&folder)?;
        Ok::<_, String>(path.to_string_lossy().into_owned())
    })
    .await
    .map_err(|_| "Не удалось сохранить отчёт.")?
}
