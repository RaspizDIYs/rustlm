use crate::{
    models::unglitch::{UnglitchAction, UnglitchResult, UnglitchStatus},
    state::AppState,
};
use tauri::State;
static ACTION: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

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
    let _guard = ACTION
        .try_lock()
        .map_err(|_| "Другая операция Unglitch ещё выполняется.")?;
    state
        .logger
        .ui_event("Unglitch", &format!("{action:?}"), Some("requested"));
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(25),
        crate::services::unglitch::execute(state.riot_client.clone(), action, confirmed),
    )
    .await
    .map_err(|_| {
        "Не удалось подтвердить результат за 25 секунд. Обновите состояние перед повтором."
            .to_string()
    })
    .and_then(|result| result);
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
