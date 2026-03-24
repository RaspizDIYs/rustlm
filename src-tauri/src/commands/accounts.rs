use tauri::Manager;
use tauri::State;

use crate::models::account::AccountRecord;
use crate::services::riot_client::AccountInfo;
use crate::state::AppState;

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LcuProfileRefreshResult {
    pub updated: bool,
    pub matched_username: Option<String>,
    pub message: String,
}

fn pick_account_index_for_lcu(
    accounts: &[AccountRecord],
    info: &AccountInfo,
    login: &str,
) -> Option<usize> {
    let login = login.trim();
    if !login.is_empty() {
        if let Some(i) = accounts
            .iter()
            .position(|a| a.username.eq_ignore_ascii_case(login))
        {
            return Some(i);
        }
    }
    let p = info.puuid.trim();
    if !p.is_empty() {
        if let Some(i) = accounts
            .iter()
            .position(|a| !a.puuid.is_empty() && a.puuid == p)
        {
            return Some(i);
        }
    }
    let r = info.riot_id.trim();
    if !r.is_empty() {
        if let Some(i) = accounts.iter().position(|a| {
            !a.riot_id.trim().is_empty() && a.riot_id.eq_ignore_ascii_case(r)
        }) {
            return Some(i);
        }
    }
    let sn = info.summoner_name.trim();
    if !sn.is_empty() {
        if let Some(i) = accounts.iter().position(|a| {
            !a.summoner_name.trim().is_empty() && a.summoner_name.eq_ignore_ascii_case(sn)
        }) {
            return Some(i);
        }
    }
    None
}

fn merge_account_from_lcu_info(mut acc: AccountRecord, info: &AccountInfo) -> AccountRecord {
    if !info.server.is_empty() {
        acc.server = info.server.clone();
    }
    if !info.summoner_name.is_empty() {
        acc.summoner_name = info.summoner_name.clone();
    }
    if !info.riot_id.is_empty() {
        acc.riot_id = info.riot_id.clone();
    }
    if !info.puuid.is_empty() {
        acc.puuid = info.puuid.clone();
    }
    if !info.rank.is_empty() {
        acc.rank = info.rank.clone();
    }
    if !info.rank_display.is_empty() {
        acc.rank_display = info.rank_display.clone();
    }
    if !info.avatar_url.is_empty() {
        acc.avatar_url = info.avatar_url.clone();
    }
    acc
}

fn account_profile_fields_changed(before: &AccountRecord, after: &AccountRecord) -> bool {
    before.server != after.server
        || before.summoner_name != after.summoner_name
        || before.riot_id != after.riot_id
        || before.puuid != after.puuid
        || before.rank != after.rank
        || before.rank_display != after.rank_display
        || before.avatar_url != after.avatar_url
}

pub(crate) fn trigger_cloud_sync(state: &AppState) {
    if state.goodluck.is_connected() {
        state.cloud_sync.notify_change();
    }
}

pub async fn refresh_account_profile_from_lcu_inner(
    state: &AppState,
) -> Result<LcuProfileRefreshResult, String> {
    state.riot_client.invalidate_cache();
    let info = state
        .riot_client
        .get_account_info()
        .await
        .map_err(|e| e.to_string())?;
    let Some(info) = info else {
        return Ok(LcuProfileRefreshResult {
            updated: false,
            matched_username: None,
            message: "LCU не отвечает или нет данных призывателя (запусти клиент и войди в аккаунт)"
                .to_string(),
        });
    };

    let login = state
        .riot_client
        .get_authorized_riot_login_username()
        .await
        .unwrap_or_default();

    let has_identity = !info.puuid.trim().is_empty()
        || !info.riot_id.trim().is_empty()
        || !info.summoner_name.trim().is_empty();
    if !has_identity {
        return Ok(LcuProfileRefreshResult {
            updated: false,
            matched_username: None,
            message: "LCU ещё не отдал ник / puuid — подожди загрузки клиента и повтори"
                .to_string(),
        });
    }

    let accounts = state.accounts.load_all();
    let Some(idx) = pick_account_index_for_lcu(&accounts, &info, &login) else {
        return Ok(LcuProfileRefreshResult {
            updated: false,
            matched_username: None,
            message: "Нет строки аккаунта, совпадающей с текущей сессией Riot (логин LCU, puuid или riot id)"
                .to_string(),
        });
    };

    let before = accounts[idx].clone();
    let merged = merge_account_from_lcu_info(before.clone(), &info);
    let uname = merged.username.clone();
    if !account_profile_fields_changed(&before, &merged) {
        return Ok(LcuProfileRefreshResult {
            updated: false,
            matched_username: Some(uname),
            message: "Данные уже совпадают с LCU".to_string(),
        });
    }

    state.accounts.save(merged).map_err(|e| e.to_string())?;
    trigger_cloud_sync(state);
    Ok(LcuProfileRefreshResult {
        updated: true,
        matched_username: Some(uname),
        message: "Профиль обновлён из League клиента".to_string(),
    })
}

pub fn spawn_post_login_lcu_refresh(app: tauri::AppHandle) {
    let st = app.state::<AppState>();
    let mut slot = st
        .post_login_lcu_task
        .lock()
        .expect("post_login_lcu_task mutex poisoned");
    if let Some(old) = slot.take() {
        old.abort();
    }
    let h = app.clone();
    let join = tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        for attempt in 0..28u32 {
            let res = {
                let st = h.state::<AppState>();
                refresh_account_profile_from_lcu_inner(&*st).await
            };
            match &res {
                Ok(r) if r.updated => break,
                Ok(r) if r.message.contains("уже совпадают") => break,
                Ok(r)
                    if r.message.contains("LCU не отвечает")
                        || r.message.contains("ещё не отдал")
                        || r.message.contains("Нет строки аккаунта") =>
                {
                    if attempt >= 27 {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                }
                Ok(_) => {
                    if attempt >= 27 {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                }
                Err(_) => {
                    if attempt >= 27 {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                }
            }
        }
        let st = h.state::<AppState>();
        trigger_cloud_sync(&*st);
    });
    *slot = Some(join);
}

#[tauri::command]
pub async fn refresh_account_profile_from_lcu(
    state: State<'_, AppState>,
) -> Result<LcuProfileRefreshResult, String> {
    refresh_account_profile_from_lcu_inner(&*state).await
}

#[tauri::command]
pub fn load_accounts(state: State<AppState>) -> Vec<AccountRecord> {
    state
        .accounts
        .load_all()
        .into_iter()
        .map(|mut a| {
            a.encrypted_password = if a.encrypted_password.is_empty() {
                String::new()
            } else {
                "***".to_string()
            };
            a
        })
        .collect()
}

#[tauri::command]
pub fn save_account(account: AccountRecord, state: State<AppState>) -> Result<(), String> {
    let mut to_save = account;

    if to_save.encrypted_password == "***" || to_save.encrypted_password.is_empty() {
        if let Some(existing) = state
            .accounts
            .load_all()
            .into_iter()
            .find(|a| a.username == to_save.username)
        {
            to_save.encrypted_password = existing.encrypted_password;
        }
    }

    state.accounts.save(to_save).map_err(|e| e.to_string())?;
    trigger_cloud_sync(&*state);
    Ok(())
}

#[tauri::command]
pub fn save_accounts_order(
    accounts: Vec<AccountRecord>,
    state: State<AppState>,
) -> Result<(), String> {
    // Frontend sends accounts with masked passwords ("***").
    // Restore real encrypted passwords from current storage before writing.
    let current = state.accounts.load_all();
    let restored: Vec<AccountRecord> = accounts
        .into_iter()
        .map(|mut a| {
            if a.encrypted_password == "***" || a.encrypted_password.is_empty() {
                if let Some(existing) = current.iter().find(|c| c.username == a.username) {
                    a.encrypted_password = existing.encrypted_password.clone();
                }
            }
            a
        })
        .collect();

    state
        .accounts
        .save_accounts(restored)
        .map_err(|e| e.to_string())?;
    trigger_cloud_sync(&*state);
    Ok(())
}

#[tauri::command]
pub fn delete_account(username: &str, state: State<AppState>) -> Result<(), String> {
    state.accounts.delete(username).map_err(|e| e.to_string())?;
    trigger_cloud_sync(&*state);
    Ok(())
}

#[tauri::command]
pub fn protect_password(plain: &str, state: State<AppState>) -> Result<String, String> {
    state.accounts.protect(plain).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn export_accounts(
    path: &str,
    password: Option<&str>,
    selected_usernames: Option<Vec<String>>,
    state: State<AppState>,
) -> Result<(), String> {
    state
        .accounts
        .export_accounts(
            path,
            password,
            selected_usernames.as_deref(),
        )
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn import_accounts(
    path: &str,
    password: Option<&str>,
    state: State<AppState>,
) -> Result<usize, String> {
    let count = state
        .accounts
        .import_accounts(path, password)
        .map_err(|e| e.to_string())?;
    trigger_cloud_sync(&*state);
    Ok(count)
}
