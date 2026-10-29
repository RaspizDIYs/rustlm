use tauri::AppHandle;

/// Сбрасывает кэш WebView2 и перезапускает приложение.
/// Нужен, когда окно пустое/прозрачное и показывает «This page couldn't load».
#[tauri::command]
pub async fn repair_webview_cache(app: AppHandle) -> Result<(), String> {
    crate::services::webview_repair::request_repair_and_restart(&app)
}

/// Фронтенд отрисовался — снимает флаг аварийной починки окна.
#[tauri::command]
pub fn webview_ready() -> bool {
    crate::services::webview_repair::report_ready()
}
