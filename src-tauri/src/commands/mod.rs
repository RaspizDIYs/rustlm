pub mod accounts;
pub mod auto_accept;
pub mod cloud_sync;
pub mod customization;
pub mod data_dragon;
pub mod goodluck;
pub mod lol_config;
pub mod login;
pub mod logs;
pub mod migration;
pub mod reveal;
pub mod riot_client;
pub mod rune_pages;
pub mod settings;
pub mod totp;

#[tauri::command]
pub fn greet(name: &str) -> String {
    format!("Привет, {}! Добро пожаловать в RustLM!", name)
}

#[tauri::command]
pub fn refresh_tray(app: tauri::AppHandle) -> Result<(), String> {
    crate::tray::rebuild_tray_menu(&app).map_err(|e| e.to_string())
}
