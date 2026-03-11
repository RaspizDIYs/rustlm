pub mod accounts;
pub mod auto_accept;
pub mod customization;
pub mod data_dragon;
pub mod login;
pub mod logs;
pub mod reveal;
pub mod riot_client;
pub mod rune_pages;
pub mod settings;

#[tauri::command]
pub fn greet(name: &str) -> String {
    format!("Привет, {}! Добро пожаловать в RustLM!", name)
}
