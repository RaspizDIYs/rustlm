// Починка окна: если WebView2-профиль (EBWebView) повреждён, страница не
// загружается, окно остаётся прозрачным и показывает «This page couldn't load».
// Лечится полным удалением профиля — но только когда вебвью ещё не создан,
// поэтому заявка на починку сохраняется в файл-маркер и применяется при старте.
//
// Два пути:
//  1) вручную — кнопка «Починить отображение» в настройках;
//  2) автоматически — если фронтенд не отметился за 12 секунд, приложение само
//     чистит профиль и перезапускается; если не помогло, второй раз пробует
//     без GPU-ускорения WebView2. Больше двух попыток не делаем, чтобы не
//     попасть в цикл перезапусков.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

const IDENTIFIER: &str = "com.rustlm.app";
const MARKER_FILE: &str = ".webview-repair-requested";
const RETRY_FILE: &str = ".webview-repair-tried";
const GPU_FILE: &str = ".webview-repair-disable-gpu";
const WEBVIEW_DIR: &str = "EBWebView";
/// Сколько ждём отметку от фронтенда, прежде чем считать, что окно не поднялось.
const READY_TIMEOUT: Duration = Duration::from_secs(12);

static READY: AtomicBool = AtomicBool::new(false);

fn app_local_dir() -> Option<PathBuf> {
    dirs::data_local_dir().map(|dir| dir.join(IDENTIFIER))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingRepair {
    None,
    CacheCleared,
    CacheClearedWithoutGpu,
}

/// Вызывается в самом начале запуска, до создания вебвью.
pub fn apply_pending_repair() -> PendingRepair {
    let Some(dir) = app_local_dir() else {
        return PendingRepair::None;
    };
    let marker = dir.join(MARKER_FILE);
    if !marker.exists() {
        return PendingRepair::None;
    }

    let without_gpu = dir.join(GPU_FILE).exists();
    if without_gpu {
        // WebView2 читает переменную при создании окружения — успеть до вебвью.
        std::env::set_var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS", "--disable-gpu");
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
    if without_gpu {
        PendingRepair::CacheClearedWithoutGpu
    } else {
        PendingRepair::CacheCleared
    }
}

fn restart(app: &tauri::AppHandle, reason: &str) -> Result<(), String> {
    log::warn!("[webview] перезапуск для починки окна: {}", reason);
    let exe = std::env::current_exe().map_err(|e| format!("Не удалось найти exe: {}", e))?;
    std::process::Command::new(exe)
        .args(std::env::args().skip(1))
        .spawn()
        .map_err(|e| format!("Не удалось перезапустить приложение: {}", e))?;
    app.exit(0);
    Ok(())
}

/// Ставит заявку на починку и перезапускает приложение (кнопка в настройках).
pub fn request_repair_and_restart(app: &tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;

    let dir = app
        .path()
        .app_local_data_dir()
        .map_err(|e| format!("Не удалось определить папку данных: {}", e))?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("Не удалось создать папку данных: {}", e))?;
    std::fs::write(dir.join(MARKER_FILE), b"1")
        .map_err(|e| format!("Не удалось записать заявку на починку: {}", e))?;
    restart(app, "запрошено пользователем")
}

/// Фронтенд отрисовался: снимаем флаги аварийной починки.
pub fn report_ready() -> bool {
    if READY.swap(true, Ordering::SeqCst) {
        return true;
    }
    if let Some(dir) = app_local_dir() {
        let _ = std::fs::remove_file(dir.join(RETRY_FILE));
        let _ = std::fs::remove_file(dir.join(GPU_FILE));
    }
    log::info!("[webview] frontend ready");
    true
}

/// Сторож: если через READY_TIMEOUT фронтенд не поднялся — чистим кэш и
/// перезапускаемся (не более двух попыток за запуск).
pub fn arm_recovery(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(READY_TIMEOUT).await;
        if READY.load(Ordering::SeqCst) {
            return;
        }

        let Some(dir) = app_local_dir() else {
            log::error!("[webview] окно не поднялось, папка данных не определена");
            return;
        };
        let _ = std::fs::create_dir_all(&dir);
        let tried = dir.join(RETRY_FILE);
        let gpu = dir.join(GPU_FILE);
        let marker = dir.join(MARKER_FILE);

        if !tried.exists() {
            log::warn!("[webview] окно не поднялось за {:?}: пробую сброс кэша", READY_TIMEOUT);
            let _ = std::fs::write(&tried, b"1");
            let _ = std::fs::write(&marker, b"1");
            let _ = restart(&app, "авто-починка: сброс EBWebView");
        } else if !gpu.exists() {
            log::warn!("[webview] окно не поднялось после сброса кэша: пробую без GPU");
            let _ = std::fs::write(&gpu, b"1");
            let _ = std::fs::write(&marker, b"1");
            let _ = restart(&app, "авто-починка: сброс кэша + отключение GPU");
        } else {
            log::error!(
                "[webview] окно не поднялось даже без GPU — оставляю как есть, нужны логи и версия WebView2"
            );
        }
    });
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
