use tauri::State;

use crate::models::league_client::ClientConnectivityStatus;
use crate::services::riot_client::{AccountInfo, RiotClientService};
use crate::state::AppState;

#[tauri::command]
pub fn is_riot_client_running() -> bool {
    RiotClientService::is_riot_client_running()
}

#[tauri::command]
pub fn is_league_running() -> bool {
    RiotClientService::is_league_running()
}

#[tauri::command]
pub fn kill_league(include_riot_client: bool) {
    RiotClientService::kill_league(include_riot_client);
}

#[tauri::command]
pub fn start_riot_client() -> Result<(), String> {
    RiotClientService::start_riot_client().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn restart_league(state: State<'_, AppState>) -> Result<(), String> {
    RiotClientService::kill_league(false);
    // Wait a bit for processes to die
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    state.riot_client.launch_league_via_rc().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn probe_connectivity(state: State<'_, AppState>) -> Result<ClientConnectivityStatus, String> {
    Ok(state.riot_client.probe_connectivity().await)
}

#[tauri::command]
pub async fn get_account_info(state: State<'_, AppState>) -> Result<Option<AccountInfo>, String> {
    state
        .riot_client
        .get_account_info()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn lcu_get(endpoint: String, state: State<'_, AppState>) -> Result<String, String> {
    state
        .riot_client
        .lcu_get(&endpoint)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn lcu_post(
    endpoint: String,
    body: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    state
        .riot_client
        .lcu_post(&endpoint, &body)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn invalidate_lcu_cache(state: State<AppState>) {
    state.riot_client.invalidate_cache();
}

#[tauri::command]
pub async fn detect_server(state: State<'_, AppState>) -> Result<String, String> {
    state
        .riot_client
        .detect_server()
        .await
        .map_err(|e| e.to_string())
}
