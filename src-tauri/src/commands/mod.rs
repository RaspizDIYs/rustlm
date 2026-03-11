#[tauri::command]
pub fn greet(name: &str) -> String {
    format!("Привет, {}! Добро пожаловать в RustLM!", name)
}
