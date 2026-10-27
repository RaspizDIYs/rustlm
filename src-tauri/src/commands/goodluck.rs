use tauri::Emitter;
use tauri::State;
use tauri_plugin_opener::OpenerExt;

use crate::commands::accounts::trigger_cloud_sync;
use crate::error::AppError;
use crate::models::goodluck::{
    GlImportResult, GoodLuckRiotAccount, GoodLuckUser, SyncAccountData, SyncResult,
};
use crate::state::AppState;

pub async fn run_goodluck_post_login(
    app: &tauri::AppHandle,
    state: &AppState,
    oauth_user: GoodLuckUser,
) -> GoodLuckUser {
    let user = match state.goodluck.refresh_profile_from_server().await {
        Ok(u) => u,
        Err(e) => {
            log::warn!(
                "GoodLuck post-login: profile refresh failed (using cached user): {}",
                e
            );
            state.goodluck.get_user().unwrap_or(oauth_user)
        }
    };

    let auto_sync: bool = state.settings.load_setting("GoodLuckAutoSync", false);
    if auto_sync {
        let accounts = state.accounts.load_all();
        let sync_data: Vec<SyncAccountData> = accounts
            .into_iter()
            .filter(|a| !a.riot_id.trim().is_empty())
            .map(|a| SyncAccountData {
                riot_id: a.riot_id,
                server: map_lm_server_for_goodluck_platform(&a.server),
                rank: a.rank,
                summoner_name: a.summoner_name,
            })
            .collect();
        if !sync_data.is_empty() {
            if let Err(e) = state.goodluck.sync_accounts(sync_data).await {
                log::warn!("GoodLuck post-login: platform sync failed: {}", e);
            }
        }
    }

    match state.cloud_sync.sync(state).await {
        Ok(()) => {
            let _ = app.emit("cloud-sync-complete", ());
        }
        Err(e) => {
            log::warn!("GoodLuck post-login: cloud sync failed: {}", e);
        }
    }

    user
}

pub fn map_lm_server_for_goodluck_platform(server: &str) -> String {
    match server.trim().to_uppercase().as_str() {
        "" => "RU".to_string(),
        "EUW" | "EUW1" => "EUW1".to_string(),
        "EUNE" | "EUN1" => "EUN1".to_string(),
        "NA" | "NA1" => "NA1".to_string(),
        "KR" | "KR1" => "KR".to_string(),
        "RU" | "RU1" => "RU".to_string(),
        "TR" | "TR1" => "TR1".to_string(),
        "BR" | "BR1" => "BR1".to_string(),
        "JP" | "JP1" => "JP1".to_string(),
        "LAN" | "LA1" => "LA1".to_string(),
        "LAS" | "LA2" => "LA2".to_string(),
        "OCE" | "OC1" => "OC1".to_string(),
        "ME" | "ME1" => "ME1".to_string(),
        "PBE" => "PBE".to_string(),
        "SEA" => "SEA".to_string(),
        "SG2" | "SG" => "SG2".to_string(),
        "TW2" | "TW" => "TW2".to_string(),
        "VN2" | "VN" => "VN2".to_string(),
        s => s.to_string(),
    }
}

#[tauri::command]
pub async fn goodluck_login(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    let url = state.goodluck.start_auth_flow()?;
    app.opener()
        .open_url(&url, None::<&str>)
        .map_err(|e| AppError::Custom(format!("Failed to open browser: {}", e)))?;
    Ok(())
}

#[tauri::command]
pub async fn goodluck_handle_callback(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    code: String,
    callback_state: String,
) -> Result<GoodLuckUser, AppError> {
    let oauth_user = state
        .goodluck
        .handle_callback(&code, &callback_state)
        .await?;
    let user = run_goodluck_post_login(&app, &*state, oauth_user).await;
    let _ = app.emit("goodluck-auth-success", &user);
    Ok(user)
}

#[tauri::command]
pub async fn goodluck_get_user(
    state: State<'_, AppState>,
) -> Result<Option<GoodLuckUser>, AppError> {
    Ok(state.goodluck.get_user())
}

#[tauri::command]
pub async fn goodluck_refresh_profile(
    state: State<'_, AppState>,
) -> Result<GoodLuckUser, AppError> {
    state.goodluck.refresh_profile_from_server().await
}

#[tauri::command]
pub async fn goodluck_is_connected(state: State<'_, AppState>) -> Result<bool, AppError> {
    Ok(state.goodluck.is_connected())
}

#[tauri::command]
pub async fn goodluck_logout(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    if state.goodluck.is_connected() {
        if state.cloud_sync.sync(&*state).await.is_ok() {
            let _ = app.emit("cloud-sync-complete", ());
        }
    }
    state.goodluck.logout().await?;
    state.cloud_sync.on_goodluck_logout();
    let _ = app.emit("goodluck-logged-out", ());
    Ok(())
}

#[tauri::command]
pub async fn goodluck_sync_accounts(state: State<'_, AppState>) -> Result<SyncResult, AppError> {
    let accounts = state.accounts.load_all();
    let sync_data: Vec<SyncAccountData> = accounts
        .into_iter()
        .filter(|a| !a.riot_id.trim().is_empty())
        .map(|a| SyncAccountData {
            riot_id: a.riot_id,
            server: map_lm_server_for_goodluck_platform(&a.server),
            rank: a.rank,
            summoner_name: a.summoner_name,
        })
        .collect();
    state.goodluck.sync_accounts(sync_data).await
}

#[tauri::command]
pub async fn goodluck_delete_server_data(state: State<'_, AppState>) -> Result<(), AppError> {
    state.goodluck.delete_server_data().await
}

#[tauri::command]
pub async fn goodluck_get_synced_accounts(
    state: State<'_, AppState>,
) -> Result<Vec<SyncAccountData>, AppError> {
    state.goodluck.get_synced_accounts().await
}

#[tauri::command]
pub async fn goodluck_get_profile_accounts(
    state: State<'_, AppState>,
) -> Result<Vec<GoodLuckRiotAccount>, AppError> {
    let user = state
        .goodluck
        .get_user()
        .ok_or_else(|| AppError::Custom("Not connected to GoodLuck".to_string()))?;
    Ok(user.riot_accounts)
}

/// Automatic imports respect local removals; explicit selection can restore an account.
#[tauri::command]
pub async fn goodluck_import_profile_accounts(
    state: State<'_, AppState>,
    riot_accounts: Vec<GoodLuckRiotAccount>,
    restore_removed: Option<bool>,
) -> Result<GlImportResult, AppError> {
    let (imported, skipped) = state
        .accounts
        .import_profiles(riot_accounts, restore_removed.unwrap_or(false))?;
    if imported > 0 {
        trigger_cloud_sync(&*state);
    }
    Ok(GlImportResult {
        imported,
        updated: 0,
        skipped,
        updated_pairs: Vec::new(),
    })
}

#[tauri::command]
pub async fn goodluck_get_friends(
    state: State<'_, AppState>,
) -> Result<Vec<crate::models::goodluck::GoodLuckFriend>, AppError> {
    state.goodluck.get_friends().await
}

#[tauri::command]
pub async fn goodluck_search_users(
    query: String,
    state: State<'_, AppState>,
) -> Result<Vec<crate::models::goodluck::GoodLuckFriend>, AppError> {
    let query = query.trim();
    if query.chars().count() < 2 {
        return Ok(Vec::new());
    }
    state.goodluck.search_users(query).await
}

#[tauri::command]
pub async fn goodluck_send_friend_request(
    addressee_user_id: String,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    if addressee_user_id.trim().is_empty() {
        return Err(AppError::Custom(
            "Не выбран пользователь GoodLuck".to_string(),
        ));
    }
    state
        .goodluck
        .send_friend_request(addressee_user_id.trim())
        .await
}

#[tauri::command]
pub async fn goodluck_get_shared_accounts(
    state: State<'_, AppState>,
) -> Result<crate::models::goodluck::SharedAccountsResponse, AppError> {
    let response = state.goodluck.get_shared_accounts().await?;
    if let Err(error) = process_shared_account_cleanups(&state).await {
        log::warn!("Shared account cleanup failed: {}", error);
    }
    Ok(response)
}

#[tauri::command]
pub async fn goodluck_share_account(
    mut request: crate::models::goodluck::ShareAccountRequest,
    state: State<'_, AppState>,
) -> Result<crate::models::goodluck::SharedAccount, AppError> {
    request.server = map_lm_server_for_goodluck_platform(&request.server);
    request.puuid = request.puuid.trim().to_string();
    if !crate::models::account::is_valid_puuid(&request.puuid) {
        // PUUID is optional for sharing; Riot ID remains the cleanup fallback.
        request.puuid.clear();
    }
    let session = state.cloud_sync.get_valid_totp_session_public();
    state
        .goodluck
        .share_account(&request, session.as_deref())
        .await
}

#[tauri::command]
pub async fn goodluck_revoke_shared_account(
    id: String,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    state.goodluck.revoke_shared_account(&id).await
}

#[tauri::command]
pub async fn goodluck_process_shared_cleanups(state: State<'_, AppState>) -> Result<u32, AppError> {
    process_shared_account_cleanups(&state).await
}

/// Returns None when the current identity is not reliable enough to acknowledge cleanup.
/// PUUID is authoritative; Riot ID is a fallback for older shares without a PUUID.
fn shared_identity_matches(
    task_puuid: &str,
    task_riot_id: &str,
    current_puuid: &str,
    game_name: &str,
    tag_line: &str,
) -> Option<bool> {
    let task_puuid = task_puuid.trim();
    let current_puuid = current_puuid.trim();
    if !task_puuid.is_empty() && !current_puuid.is_empty() {
        return Some(task_puuid == current_puuid);
    }
    if !task_riot_id.trim().is_empty() && !game_name.is_empty() && !tag_line.is_empty() {
        let current_riot_id = format!("{}#{}", game_name, tag_line);
        return Some(task_riot_id.trim().to_lowercase() == current_riot_id.to_lowercase());
    }
    None
}

/// Process durable revocation/expiration tasks. A task is acknowledged only after
/// the current Riot identity is known: matching accounts are logged out; a verified
/// different account is left untouched. Offline/unknown sessions remain queued.
pub async fn process_shared_account_cleanups(state: &AppState) -> Result<u32, AppError> {
    static CLEANUP_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _guard = CLEANUP_LOCK.lock().await;
    let tasks = state.goodluck.get_shared_cleanups().await?;
    let mut completed = 0u32;

    for task in tasks {
        let current = match state
            .riot_client
            .lcu_get("/lol-summoner/v1/current-summoner")
            .await
        {
            Ok(raw) => serde_json::from_str::<serde_json::Value>(&raw).unwrap_or_default(),
            Err(_) => continue,
        };
        if current.get("errorCode").is_some() {
            continue;
        }

        let current_puuid = current.get("puuid").and_then(|v| v.as_str()).unwrap_or("");
        let game_name = current
            .get("gameName")
            .and_then(|v| v.as_str())
            .or_else(|| current.get("displayName").and_then(|v| v.as_str()))
            .unwrap_or("");
        let tag_line = current
            .get("tagLine")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let Some(same_account) = shared_identity_matches(
            &task.puuid,
            &task.riot_id,
            current_puuid,
            game_name,
            tag_line,
        ) else {
            continue;
        };

        if same_account {
            state.riot_client.invalidate_cache();
            if let Err(error) = state.riot_client.logout_via_rc().await {
                log::warn!(
                    "Shared account {} cleanup logout failed: {}",
                    task.id,
                    error
                );
                continue;
            }
            if crate::services::riot_client::RiotClientService::is_league_running() {
                crate::services::riot_client::RiotClientService::kill_league(false);
            }
            if state.riot_client.is_rso_authorized().await {
                log::warn!(
                    "Shared account {} cleanup: RSO is still authorized",
                    task.id
                );
                continue;
            }
        }

        if let Err(error) = state.goodluck.complete_shared_cleanup(&task.id).await {
            log::warn!(
                "Shared account {} cleanup acknowledgement failed: {}",
                task.id,
                error
            );
            continue;
        }
        completed += 1;
        log::info!(
            "Shared account {} cleanup completed (matching={})",
            task.id,
            same_account
        );
    }
    Ok(completed)
}

#[cfg(test)]
mod tests {
    use super::shared_identity_matches;

    #[test]
    fn puuid_is_authoritative_when_available() {
        assert_eq!(
            shared_identity_matches("puuid-a", "Old Name#EUW", "puuid-a", "New Name", "EUW"),
            Some(true)
        );
        assert_eq!(
            shared_identity_matches("puuid-a", "Same#EUW", "puuid-b", "Same", "EUW"),
            Some(false)
        );
    }

    #[test]
    fn riot_id_is_case_insensitive_fallback() {
        assert_eq!(
            shared_identity_matches("", "Shared Account#EUW", "", "shared account", "euw"),
            Some(true)
        );
        assert_eq!(
            shared_identity_matches("", "Shared Account#EUW", "", "Personal", "EUW"),
            Some(false)
        );
    }

    #[test]
    fn unknown_identity_stays_queued() {
        assert_eq!(shared_identity_matches("puuid-a", "", "", "", ""), None);
        assert_eq!(shared_identity_matches("", "", "", "Personal", "EUW"), None);
    }
}

#[tauri::command]
pub async fn lm_friends(state: State<'_, AppState>) -> Result<serde_json::Value, AppError> {
    state.goodluck.lm_friends().await
}
#[tauri::command]
pub async fn lm_set_visibility(enabled: bool, state: State<'_, AppState>) -> Result<(), AppError> {
    if !enabled {
        state.goodluck.lm_delete_presence().await?;
    }
    state.settings.save_setting("LmFriendsVisible", &enabled)?;
    Ok(())
}
pub async fn publish_lm_friends(state: &AppState) -> Result<serde_json::Value, AppError> {
    if !state.settings.load_setting("LmFriendsVisible", false) {
        return Err(AppError::Custom("Включите обнаружение друзей LM".into()));
    }
    let current: serde_json::Value = serde_json::from_str(
        &state
            .riot_client
            .lcu_get("/lol-summoner/v1/current-summoner")
            .await?,
    )?;
    let puuid = current
        .get("puuid")
        .and_then(|v| v.as_str())
        .filter(|p| crate::models::account::is_valid_puuid(p))
        .ok_or_else(|| AppError::Custom("Войдите в League, чтобы обновить Riot-друзей".into()))?;
    let friends: serde_json::Value =
        serde_json::from_str(&state.riot_client.lcu_get("/lol-chat/v1/friends").await?)?;
    let list = friends
        .as_array()
        .ok_or_else(|| AppError::Custom("Клиент Riot вернул неизвестный формат друзей".into()))?;
    let puuids: Vec<_> = list
        .iter()
        .filter_map(|f| f.get("puuid").and_then(|p| p.as_str()))
        .filter(|p| crate::models::account::is_valid_puuid(p))
        .collect();
    state
        .goodluck
        .lm_presence(serde_json::json!({"puuid":puuid,"friends":puuids}))
        .await
}
#[tauri::command]
pub async fn lm_publish_friends(state: State<'_, AppState>) -> Result<serde_json::Value, AppError> {
    publish_lm_friends(&state).await
}
