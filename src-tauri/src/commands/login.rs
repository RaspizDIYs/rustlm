use std::sync::atomic::Ordering;
use std::time::Duration;

use tauri::{Emitter, State};

use crate::commands::accounts::{spawn_post_login_lcu_refresh, trigger_cloud_sync};
use crate::services::file_logger::FileLogger;
use crate::services::riot_client::{LoginPhase, RiotClientService};
use crate::state::AppState;

fn check_cancelled(state: &AppState) -> Result<(), String> {
    if state.login_cancelled.load(Ordering::Relaxed) {
        state.logger.login_flow("CANCELLED", None);
        Err("Login cancelled".to_string())
    } else {
        Ok(())
    }
}

fn emit_progress(app: &tauri::AppHandle, message: &str) {
    let _ = app.emit("login-progress", message.to_string());
}

#[tauri::command]
pub fn cancel_login(state: State<'_, AppState>) {
    state.login_cancelled.store(true, Ordering::Relaxed);
    state.logger.login_flow("CANCEL_REQUESTED", None);
}

#[tauri::command]
pub async fn login_to_account(
    username: String,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    state.login_cancelled.store(false, Ordering::Relaxed);

    let logger: &FileLogger = &state.logger;
    logger.login_flow("START", Some(&format!("user={}", username)));

    // 1. Look up encrypted password from storage (never exposed to frontend)
    let account = state
        .accounts
        .load_all()
        .into_iter()
        .find(|a| a.username == username)
        .ok_or_else(|| "Аккаунт не найден".to_string())?;

    if account.encrypted_password.is_empty() {
        return Err("Аккаунт не активирован — введите данные для входа".to_string());
    }

    emit_progress(&app, "Расшифровка пароля...");
    let plain_password = state
        .accounts
        .unprotect(&account.encrypted_password)
        .map_err(|e| {
            logger.login_flow("ERROR", Some(&format!("unprotect failed: {}", e)));
            e.to_string()
        })?;
    logger.login_flow("DECRYPT", Some("password decrypted OK"));

    let rc = &state.riot_client;

    // 2. Detect current phase
    emit_progress(&app, "Определение состояния...");
    let phase = rc.detect_login_phase().await;
    logger.login_flow("PHASE", Some(&format!("{:?}", phase)));

    // 3. React to phase — shortest path to login page
    let uia_timeout = match phase {
        LoginPhase::Nothing => {
            // Delete saved sessions so RC starts on login page, not auto-login
            delete_rc_session_data(logger);
            emit_progress(&app, "Запуск Riot Client...");
            if let Err(e) = RiotClientService::start_riot_client() {
                logger.login_flow("ERROR", Some(&format!("start failed: {}", e)));
                return Err(format!("Не удалось запустить Riot Client: {}", e));
            }
            45
        }

        LoginPhase::RcStarting => {
            emit_progress(&app, "Ожидание запуска Riot Client...");
            45
        }

        LoginPhase::RcWaitingForApi => {
            emit_progress(&app, "Ожидание загрузки Riot Client...");
            30
        }

        LoginPhase::RcReady => {
            emit_progress(&app, "Вход в аккаунт...");
            15
        }

        LoginPhase::Authenticated => {
            prepare_logout(rc, logger, &app, &state, false).await?
        }

        LoginPhase::LeagueRunning => {
            prepare_logout(rc, logger, &app, &state, true).await?
        }
    };
    check_cancelled(&state)?;

    // 4. UIA login with phase-specific timeout
    emit_progress(&app, "Вход в аккаунт...");
    logger.login_flow("UIA", Some(&format!("timeout={}s", uia_timeout)));

    let uia_ok = run_uia_login(&username, &plain_password, uia_timeout, &state).await;
    check_cancelled(&state)?;

    if uia_ok {
        logger.login_flow("UIA", Some("success"));
        return finalize_login(rc, logger, &app, &state).await;
    }

    // 5. Fallback — two stages: soft (retry logout), then hard (kill+restart)
    logger.login_flow("UIA", Some("failed, entering fallback"));

    // Soft fallback: RC UI is alive → retry logout via API + UIA
    if RiotClientService::is_riot_client_ui_running() {
        check_cancelled(&state)?;
        logger.login_flow("FALLBACK_SOFT", Some("RC UI alive, retrying logout + UIA"));

        emit_progress(&app, "Повторная попытка выхода...");
        rc.invalidate_cache();
        let _ = rc.logout_via_rc().await;
        check_cancelled(&state)?;

        // Kill League if still lingering
        if RiotClientService::is_league_running() {
            RiotClientService::kill_league(false);
        }

        emit_progress(&app, "Вход в аккаунт...");
        let uia_ok = run_uia_login(&username, &plain_password, 10, &state).await;
        check_cancelled(&state)?;

        if uia_ok {
            logger.login_flow("FALLBACK_SOFT", Some("succeeded"));
            return finalize_login(rc, logger, &app, &state).await;
        }
        logger.login_flow("FALLBACK_SOFT", Some("failed"));
    }

    // Hard fallback: kill everything + delete sessions + restart
    check_cancelled(&state)?;
    logger.login_flow("FALLBACK_HARD", Some("kill + delete + restart"));

    emit_progress(&app, "Перезапуск Riot Client...");
    rc.invalidate_cache();
    kill_and_wait(logger, &state).await?;
    delete_rc_session_data(logger);
    check_cancelled(&state)?;

    emit_progress(&app, "Запуск Riot Client...");
    RiotClientService::start_riot_client()
        .map_err(|e| format!("Не удалось запустить Riot Client: {}", e))?;
    check_cancelled(&state)?;

    emit_progress(&app, "Вход в аккаунт...");
    let uia_ok = run_uia_login(&username, &plain_password, 45, &state).await;
    check_cancelled(&state)?;

    if uia_ok {
        logger.login_flow("FALLBACK_HARD", Some("succeeded"));
        return finalize_login(rc, logger, &app, &state).await;
    }

    Err("Не удалось войти после нескольких попыток".to_string())
}

/// Handle Authenticated / LeagueRunning phases.
/// Logout FIRST (while RC is stable), then kill League.
/// Returns the UIA timeout to use.
async fn prepare_logout(
    rc: &RiotClientService,
    logger: &FileLogger,
    app: &tauri::AppHandle,
    state: &AppState,
    league_running: bool,
) -> Result<u64, String> {
    // Logout FIRST while RC is in a stable state.
    // If we kill League before logout, RC detects a crash and enters a
    // broken state (crash dialog / hidden UX), making logout unreliable.
    emit_progress(app, "Выход из текущей сессии...");
    rc.invalidate_cache();
    let _ = rc.logout_via_rc().await;
    check_cancelled(state)?;

    // Now kill League — RC already processed the logout, so the crash
    // of League won't confuse it.
    if league_running && RiotClientService::is_league_running() {
        emit_progress(app, "Закрытие League of Legends...");
        RiotClientService::kill_league(false);
    }

    // Verify RC UI is still alive after logout.
    // Logout can sometimes cause RiotClientUx to restart — wait briefly.
    if !wait_for_rc_ui(3, state).await? {
        logger.login_flow("LOGOUT", Some("RC UI died after logout, restarting"));
        kill_and_wait(logger, state).await?;
        delete_rc_session_data(logger);
        emit_progress(app, "Запуск Riot Client...");
        RiotClientService::start_riot_client()
            .map_err(|e| format!("Не удалось запустить Riot Client: {}", e))?;
        return Ok(45);
    }

    logger.login_flow("LOGOUT", Some("RC UI alive, proceeding to UIA"));
    Ok(10)
}

/// Wait for authorization and launch League.
async fn finalize_login(
    rc: &RiotClientService,
    logger: &FileLogger,
    app: &tauri::AppHandle,
    state: &AppState,
) -> Result<(), String> {
    emit_progress(app, "Ожидание авторизации...");
    logger.login_flow("AUTH", Some("waiting for RSO authorization..."));
    if !wait_rso_with_cancel(rc, true, 20, state).await? {
        return Err("Авторизация не подтверждена в течение 20 секунд".to_string());
    }

    check_cancelled(state)?;

    if !RiotClientService::is_league_running() {
        emit_progress(app, "Запуск League of Legends...");
        logger.login_flow("LAUNCH", Some("launching League of Legends..."));
        match rc.launch_league_via_rc().await {
            Ok(_) => {}
            Err(e) => {
                logger.login_flow("LAUNCH", Some(&format!("launch failed: {}", e)));
            }
        }
    }

    logger.login_flow("DONE", Some("login complete"));
    rc.invalidate_cache();
    trigger_cloud_sync(state);
    spawn_post_login_lcu_refresh(app.clone());
    Ok(())
}

/// Run UIA login in a blocking thread. Returns true on success.
async fn run_uia_login(
    username: &str,
    password: &str,
    timeout_secs: u64,
    state: &AppState,
) -> bool {
    let uia_user = username.to_string();
    let uia_pass = password.to_string();
    let uia_cancel = state.login_cancelled.clone();

    let result = tokio::task::spawn_blocking(move || {
        crate::services::uia_login::login_to_riot_client(
            &uia_user, &uia_pass, timeout_secs, Some(&uia_cancel),
        )
    })
    .await;

    matches!(result, Ok(Ok(())))
}

/// Wait for RC UI process (Riot Client.exe / RiotClientUx.exe) to be running.
/// Returns false if not found within timeout.
async fn wait_for_rc_ui(timeout_secs: u64, state: &AppState) -> Result<bool, String> {
    let deadline = std::time::Instant::now() + Duration::from_secs(timeout_secs);
    loop {
        check_cancelled(state)?;
        if RiotClientService::is_riot_client_ui_running() {
            return Ok(true);
        }
        if std::time::Instant::now() > deadline {
            return Ok(false);
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

/// Poll RSO authorization state with cancel support.
async fn wait_rso_with_cancel(
    rc: &RiotClientService,
    authorized: bool,
    timeout_secs: u64,
    state: &AppState,
) -> Result<bool, String> {
    let deadline = std::time::Instant::now() + Duration::from_secs(timeout_secs);
    loop {
        check_cancelled(state)?;
        if rc.is_rso_authorized().await == authorized {
            return Ok(true);
        }
        if std::time::Instant::now() > deadline {
            return Ok(false);
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
}

/// Kill all RC/League processes and wait until they are actually dead.
async fn kill_and_wait(logger: &FileLogger, state: &AppState) -> Result<(), String> {
    RiotClientService::kill_league(true);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while RiotClientService::is_riot_client_running() {
        check_cancelled(state)?;
        if tokio::time::Instant::now() > deadline {
            logger.login_flow("CLEANUP", Some("RC still alive after 10s, proceeding"));
            break;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    Ok(())
}

/// Delete RC session/cookie data so RC doesn't auto-login on next start.
fn delete_rc_session_data(logger: &FileLogger) {
    let local_app_data = match std::env::var("LOCALAPPDATA") {
        Ok(v) => std::path::PathBuf::from(v),
        Err(_) => return,
    };

    let paths_to_delete = [
        local_app_data.join("Riot Games").join("Riot Client").join("Data").join("RiotGamesPrivateSettings.yaml"),
        local_app_data.join("Riot Games").join("Riot Client").join("Data").join("RiotClientPrivateSettings.yaml"),
    ];

    for path in &paths_to_delete {
        if path.exists() {
            match std::fs::remove_file(path) {
                Ok(_) => logger.login_flow("CLEANUP", Some(&format!("deleted {}", path.display()))),
                Err(e) => logger.login_flow("CLEANUP", Some(&format!("failed to delete {}: {}", path.display(), e))),
            }
        }
    }
}
