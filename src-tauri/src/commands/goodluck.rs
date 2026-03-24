use tauri::Emitter;
use tauri::State;
use tauri_plugin_opener::OpenerExt;

use crate::commands::accounts::trigger_cloud_sync;
use crate::error::AppError;
use crate::models::account::AccountRecord;
use crate::models::goodluck::{GlImportResult, GoodLuckRiotAccount, GoodLuckUser, SyncAccountData, SyncResult};
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
pub async fn goodluck_get_user(state: State<'_, AppState>) -> Result<Option<GoodLuckUser>, AppError> {
    Ok(state.goodluck.get_user())
}

#[tauri::command]
pub async fn goodluck_refresh_profile(state: State<'_, AppState>) -> Result<GoodLuckUser, AppError> {
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

/// Import Riot accounts from GoodLuck profile.
/// Dedup by exact riot_id match only — if riot_id already exists locally, skip.
#[tauri::command]
pub async fn goodluck_import_profile_accounts(
    state: State<'_, AppState>,
    riot_accounts: Vec<GoodLuckRiotAccount>,
) -> Result<GlImportResult, AppError> {
    let existing = state.accounts.load_all();
    let mut existing_riot_ids: std::collections::HashSet<String> = existing
        .iter()
        .filter(|a| !a.riot_id.is_empty())
        .map(|a| a.riot_id.trim().to_lowercase())
        .collect();

    let mut imported = 0u32;
    let mut skipped = 0u32;

    for acc in riot_accounts {
        let gl_rid = acc.riot_id.trim().to_lowercase();
        if existing_riot_ids.contains(&gl_rid) {
            skipped += 1;
            continue;
        }

        let summoner_name = acc.riot_id.split('#').next().unwrap_or(&acc.riot_id).to_string();
        let record = AccountRecord {
            username: format!("gl:{}", acc.riot_id),
            encrypted_password: String::new(),
            note: String::from("Импорт из GoodLuck"),
            riot_id: acc.riot_id,
            summoner_name,
            server: acc.server,
            rank: acc.rank.clone(),
            rank_display: acc.rank,
            ..AccountRecord::default()
        };
        state.accounts.save(record)?;
        existing_riot_ids.insert(gl_rid);
        imported += 1;
    }

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
