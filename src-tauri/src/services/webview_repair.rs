// Починка окна: если WebView2-профиль (EBWebView) повреждён, страница не
// загружается, окно остаётся прозрачным и показывает «This page couldn't load».
// Лечится полным удалением профиля — но только когда вебвью ещё не создан,
// поэтому заявка на починку сохраняется в файл-маркер и применяется при старте.

use std::path::PathBuf;
use std::time::Duration;

const IDENTIFIER: &str = "com.rustlm.app";
const MARKER_FILE: &str = ".webview-repair-requested";
const WEBVIEW_DIR: &str = "EBWebView";

fn app_local_dir() -> Option<PathBuf> {
    dirs::data_local_dir().map(|dir| dir.join(IDENTIFIER))
}

/// Вызывается в самом начале запуска, до создания вебвью.
pub fn apply_pending_repair() -> bool {
    let Some(dir) = app_local_dir() else {
        return false;
    };
    let marker = dir.join(MARKER_FILE);
    if !marker.exists() {
        return false;
    }

    let webview_dir = dir.join(WEBVIEW_DIR);
    // Прошлый процесс мог ещё держать локи — пробуем несколько раз.
    for _ in 0..6 {
        if !webview_dir.exists() {
            break;
        }
        if std::fs::remove_dir_all(&webview_dir).is_ok() {
            break;
        }
        std::thread::sleep(Duration::from_millis(500));
    }

    let _ = std::fs::remove_file(&marker);
    true
}

/// Ставит заявку на починку и перезапускает приложение.
pub fn request_repair_and_restart(app: &tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;

    let dir = app
        .path()
        .app_local_data_dir()
        .map_err(|e| format!("Не удалось определить папку данных: {}", e))?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("Не удалось создать папку данных: {}", e))?;
    std::fs::write(dir.join(MARKER_FILE), b"1")
        .map_err(|e| format!("Не удалось записать заявку на починку: {}", e))?;

    // Запускаем новый процесс и выходим: удалять профиль изнутри живого
    // процесса нельзя — WebView2 держит файлы открытыми.
    let exe = std::env::current_exe().map_err(|e| format!("Не удалось найти exe: {}", e))?;
    std::process::Command::new(exe)
        .args(std::env::args().skip(1))
        .spawn()
        .map_err(|e| format!("Не удалось перезапустить приложение: {}", e))?;

    app.exit(0);
    Ok(())
}

/// Версия WebView2 из реестра — в лог, чтобы поддержка видела окружение.
#[cfg(windows)]
pub fn webview2_version() -> Option<String> {
    use winreg::enums::HKEY_LOCAL_MACHINE;
    use winreg::RegKey;

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let key = hklm
        .open_subkey(
            r"SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}",
        )
        .ok()?;
    key.get_value::<String, _>("pv").ok()
}

#[cfg(not(windows))]
pub fn webview2_version() -> Option<String> {
    None
}
