use tauri::State;

use crate::models::account::AccountRecord;
use crate::state::AppState;

#[tauri::command]
pub fn load_accounts(state: State<AppState>) -> Vec<AccountRecord> {
    state.accounts.load_all()
}

#[tauri::command]
pub fn save_account(account: AccountRecord, state: State<AppState>) -> Result<(), String> {
    state.accounts.save(account).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_accounts_order(
    accounts: Vec<AccountRecord>,
    state: State<AppState>,
) -> Result<(), String> {
    state
        .accounts
        .save_accounts(accounts)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_account(username: &str, state: State<AppState>) -> Result<(), String> {
    state.accounts.delete(username).map_err(|e| e.to_string())
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
    state
        .accounts
        .import_accounts(path, password)
        .map_err(|e| e.to_string())
}
