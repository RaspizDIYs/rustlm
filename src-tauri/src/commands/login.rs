use tauri::State;

use crate::state::AppState;

#[tauri::command]
pub async fn login_to_account(
    username: String,
    password: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    // Decrypt password
    let plain_password = state
        .accounts
        .unprotect(&password)
        .map_err(|e| e.to_string())?;

    // Start Riot Client if not running
    if !crate::services::riot_client::RiotClientService::is_riot_client_running() {
        crate::services::riot_client::RiotClientService::start_riot_client()
            .map_err(|e| e.to_string())?;
        // Wait for RC to start
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    }

    // Run UIA login on a blocking thread (COM requires STA)
    let user = username.clone();
    let pass = plain_password.clone();
    tokio::task::spawn_blocking(move || {
        #[cfg(windows)]
        {
            crate::services::uia_login::login_to_riot_client(&user, &pass, 30)
                .map_err(|e| e.to_string())
        }
        #[cfg(not(windows))]
        {
            Err("UIA login is only supported on Windows".to_string())
        }
    })
    .await
    .map_err(|e| e.to_string())?
}
