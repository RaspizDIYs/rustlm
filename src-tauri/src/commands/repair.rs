use crate::{
    services::{repair, riot_client::RiotClientService},
    state::AppState,
};
use serde::Serialize;
use tauri::{Manager, State};
#[tauri::command]
pub async fn repair_stop_zapret(path: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || repair::stop(&path))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn repair_open_vpn(app: tauri::AppHandle) -> Result<String, String> {
    use tauri_plugin_opener::OpenerExt;
    let path = tokio::task::spawn_blocking(repair::vpn_application)
        .await
        .map_err(|e| e.to_string())??;
    if let Some(path) = path {
        app.opener()
            .open_path(path, None::<&str>)
            .map_err(|e| e.to_string())?;
        Ok("Открыто приложение VPN".into())
    } else {
        app.opener()
            .open_url("ms-settings:network-vpn", None::<&str>)
            .map_err(|e| e.to_string())?;
        Ok("Открыты настройки VPN Windows. Сторонний VPN можно отключить в его приложении или трее.".into())
    }
}
#[tauri::command]
pub fn repair_data_folder() -> Result<String, String> {
    let root = crate::services::repair_paths::root()?;
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    Ok(root.to_string_lossy().into())
}
#[tauri::command]
pub fn repair_open_folder(app: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_path(repair_data_folder()?, None::<&str>)
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn repair_export_report(app: tauri::AppHandle, path: String) -> Result<(), String> {
    if std::path::Path::new(&path)
        .extension()
        .and_then(|s| s.to_str())
        != Some("json")
    {
        return Err("Выберите файл .json".into());
    }
    let reports = super::network::network_history(app)?;
    let logs = crate::services::repair_paths::root()?.join("logs");
    let evidence = tokio::task::spawn_blocking(move || repair::collect(&logs))
        .await
        .map_err(|e| e.to_string())??;
    let bytes = serde_json::to_vec_pretty(&serde_json::json!({"version":env!("CARGO_PKG_VERSION"),"created_at":chrono::Utc::now(),"network_reports":reports,"evidence":evidence,"note":"Отчёт содержит сетевые замеры и категории ошибок. Пароли, аккаунты и сырые клиентские логи не включены."})).map_err(|e|e.to_string())?;
    std::fs::write(path, bytes).map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn repair_readiness(path: String, strategy: String) -> Result<repair::Readiness, String> {
    tokio::task::spawn_blocking(move || repair::readiness(&path, &strategy))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
pub fn repair_save_journal(app: tauri::AppHandle, journal: repair::Journal) -> Result<(), String> {
    repair::save_journal(
        &app.path()
            .app_data_dir()
            .map_err(|e| e.to_string())?
            .join("Repair Tools")
            .join("sessions"),
        journal,
    )
}

#[tauri::command]
pub async fn repair_detect_zapret() -> Result<Option<repair::ActiveZapret>, String> {
    tokio::task::spawn_blocking(repair::detect)
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn repair_restart_zapret(
    app: tauri::AppHandle,
    path: String,
    strategy: String,
) -> Result<String, String> {
    let logs = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("Repair Tools")
        .join("logs");
    tokio::task::spawn_blocking(move || repair::restart(&path, &strategy, &logs))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn repair_collect_evidence(
    app: tauri::AppHandle,
) -> Result<repair::EvidenceReport, String> {
    let logs = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("Repair Tools")
        .join("logs");
    tokio::task::spawn_blocking(move || repair::collect(&logs))
        .await
        .map_err(|e| e.to_string())?
}
#[derive(Serialize)]
pub struct ClientCheck {
    pub status: String,
    pub message: String,
    pub login_ready: bool,
    pub chat_ready: bool,
}
#[tauri::command]
pub async fn repair_verify_client(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<ClientCheck, String> {
    let check = async {
        if RiotClientService::find_lol_install_dir().is_none() {
            return ClientCheck{status:"not_installed".into(),message:"League of Legends не найдена. Сетевая настройка проверена отдельно; запуск игры на этом ПК подтвердить нельзя.".into(),login_ready:false,chat_ready:false};
        }
        if !RiotClientService::is_riot_client_running() {
            if let Err(e) = RiotClientService::start_riot_client() {
                return ClientCheck {
                    status: "failed".into(),
                    message: e.to_string(),
                    login_ready: false,
                    chat_ready: false,
                };
            }
        }
        if !state
            .riot_client
            .wait_for_rc_api_ready(std::time::Duration::from_secs(25))
            .await
        {
            return ClientCheck {
                status: "failed".into(),
                message: "Riot Client не ответил за 25 секунд".into(),
                login_ready: false,
                chat_ready: false,
            };
        }
        if !state.riot_client.is_rso_authorized().await {
            return ClientCheck{status:"login_required".into(),message:"Клиент запущен. Войдите в аккаунт и подтвердите 2FA, затем нажмите «Проверить снова». Это не повод менять сетевые настройки.".into(),login_ready:false,chat_ready:false};
        }
        if !RiotClientService::is_league_running() {
            if let Err(e) = state.riot_client.launch_league_via_rc().await {
                return ClientCheck {
                    status: "failed".into(),
                    message: e.to_string(),
                    login_ready: false,
                    chat_ready: false,
                };
            }
        }
        for _ in 0..20 {
            let login = state
                .riot_client
                .lcu_get("/lol-login/v1/session")
                .await
                .ok()
                .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok());
            let chat = state
                .riot_client
                .lcu_get("/lol-chat/v1/me")
                .await
                .ok()
                .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok());
            let login_ready = login
                .as_ref()
                .is_some_and(|j| j["state"] == "SUCCEEDED" && j.get("errorCode").is_none());
            let chat_ready = chat.as_ref().is_some_and(|j| {
                j.get("errorCode").is_none() && j["id"].as_str().is_some_and(|id| !id.is_empty())
            });
            if login_ready && chat_ready {
                return ClientCheck {
                    status: "ready".into(),
                    message:
                        "Вход League и API чата ответили. Запуск матча и голос не проверялись."
                            .into(),
                    login_ready,
                    chat_ready,
                };
            }
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
        ClientCheck{status:"failed".into(),message:"Не подтверждены успешный вход League и ответ чата. Собираем сетевые ошибки для следующей попытки.".into(),login_ready:false,chat_ready:false}
    };
    let result = tokio::time::timeout(std::time::Duration::from_secs(90), check)
        .await
        .unwrap_or_else(|_| ClientCheck {
            status: "failed".into(),
            message: "Проверка клиента превысила 90 секунд".into(),
            login_ready: false,
            chat_ready: false,
        });
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("Repair Tools")
        .join("logs");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(
        dir.join(format!(
            "client-check-{}.json",
            chrono::Utc::now().format("%Y%m%dT%H%M%S%3f")
        )),
        serde_json::to_vec_pretty(&result).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(result)
}
