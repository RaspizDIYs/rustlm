use std::sync::atomic::Ordering;
use std::time::Duration;

use tauri::{Emitter, Manager, State};

use crate::services::file_logger::FileLogger;
use crate::services::riot_client::{LoginPhase, RiotClientService};
use crate::state::AppState;

/// Max number of times we'll restart RC if it gets stuck
const MAX_RC_RESTART_ATTEMPTS: u32 = 3;

/// Check if login was cancelled, return Err if so.
fn check_cancelled(state: &AppState) -> Result<(), String> {
    if state.login_cancelled.load(Ordering::Relaxed) {
        state.logger.login_flow("CANCELLED", None);
        Err("Login cancelled".to_string())
    } else {
        Ok(())
    }
}

/// Emit a progress event to the frontend so the user sees what's happening.
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
    password: String,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    // Reset cancellation flag
    state.login_cancelled.store(false, Ordering::Relaxed);

    let logger: &FileLogger = &state.logger;
    logger.login_flow("START", Some(&format!("user={}", username)));

    emit_progress(&app, "Расшифровка пароля...");

    let plain_password = state
        .accounts
        .unprotect(&password)
        .map_err(|e| {
            logger.login_flow("ERROR", Some(&format!("unprotect failed: {}", e)));
            e.to_string()
        })?;

    logger.login_flow("DECRYPT", Some("password decrypted OK"));

    let rc = &state.riot_client;

    // === PHASE DETECTION ===
    let initial_phase = rc.detect_login_phase().await;
    logger.login_flow("PHASE", Some(&format!("{:?}", initial_phase)));

    // === CLEANUP: kill League, logout if authorized ===
    if RiotClientService::is_league_running() {
        emit_progress(&app, "Закрытие League Client...");
        logger.login_flow("CLEANUP", Some("killing League Client..."));
        RiotClientService::kill_league(false);
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    check_cancelled(&state)?;

    if matches!(initial_phase, LoginPhase::Authenticated | LoginPhase::LeagueRunning) {
        emit_progress(&app, "Выход из текущей сессии...");
        logger.login_flow("CLEANUP", Some("logging out current session..."));
        let _ = rc.logout_via_rc().await;
        rc.invalidate_cache();

        if !rc.wait_for_rso_state(false, Duration::from_secs(6)).await {
            logger.login_flow("CLEANUP", Some("RSO still authorized — killing RC"));
            RiotClientService::kill_league(true);
            tokio::time::sleep(Duration::from_millis(800)).await;
        }
    }
    check_cancelled(&state)?;

    // === ENSURE RC IS RUNNING AND API IS READY ===
    emit_progress(&app, "Запуск Riot Client...");
    let rc_ready = ensure_rc_ready(rc, logger, &state, &app, MAX_RC_RESTART_ATTEMPTS).await?;
    if !rc_ready {
        return Err("Riot Client не удалось запустить после нескольких попыток".to_string());
    }
    check_cancelled(&state)?;

    // === CHECK "REMEMBER ME" ===
    if rc.is_rso_authorized().await {
        logger.login_flow("AUTH", Some("already authorized (remember me) — launching League"));
        emit_progress(&app, "Сессия сохранена, запуск League...");
        if !RiotClientService::is_league_running() {
            let _ = rc.launch_league_via_rc().await;
        }
        return Ok(());
    }

    // === LOGIN: try HTTP first (fast, no window manipulation needed) ===
    emit_progress(&app, "Вход в аккаунт...");
    logger.login_flow("LOGIN", Some("trying HTTP login first..."));

    let http_ok = try_http_login(rc, logger, &username, &plain_password).await;
    check_cancelled(&state)?;

    if !http_ok {
        // === UIA FALLBACK: only now minimize and use UI automation ===
        logger.login_flow("LOGIN", Some("HTTP failed — switching to UIA fallback"));
        emit_progress(&app, "Автоматический ввод данных...");

        // Minimize our window so UIA can interact with RC
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.minimize();
        }

        let uia_user = username.clone();
        let uia_pass = plain_password.clone();
        let uia_cancel = state.login_cancelled.clone();
        let uia_result = tokio::task::spawn_blocking(move || {
            #[cfg(windows)]
            {
                crate::services::uia_login::login_to_riot_client(
                    &uia_user, &uia_pass, 30, Some(&uia_cancel),
                )
                .map_err(|e| e.to_string())
            }
            #[cfg(not(windows))]
            {
                let _ = (&uia_user, &uia_pass, &uia_cancel);
                Err::<(), String>("UIA login is only supported on Windows".to_string())
            }
        })
        .await
        .map_err(|e| {
            logger.login_flow("ERROR", Some(&format!("UIA join error: {}", e)));
            e.to_string()
        })?;

        // Restore window immediately after UIA finishes
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.unminimize();
        }

        if let Err(e) = &uia_result {
            logger.login_flow("LOGIN", Some(&format!("UIA error: {}", e)));
        }
        uia_result?;
    }

    check_cancelled(&state)?;

    // === WAIT FOR AUTHORIZATION ===
    emit_progress(&app, "Ожидание авторизации...");
    logger.login_flow("AUTH", Some("waiting for RSO authorization..."));
    if !rc.wait_for_rso_state(true, Duration::from_secs(20)).await {
        return Err("Авторизация не подтверждена в течение 20 секунд".to_string());
    }

    // === LAUNCH LEAGUE ===
    if !RiotClientService::is_league_running() {
        emit_progress(&app, "Запуск League of Legends...");
        logger.login_flow("LAUNCH", Some("launching League of Legends..."));
        let _ = rc.launch_league_via_rc().await;
    }

    logger.login_flow("DONE", Some("login complete"));
    Ok(())
}

/// Try HTTP login: init RSO session then send credentials.
/// Returns true if login succeeded via HTTP.
/// This is the preferred path — fast and no window manipulation needed.
async fn try_http_login(
    rc: &RiotClientService,
    logger: &FileLogger,
    username: &str,
    password: &str,
) -> bool {
    // Init RSO session — required before credentials can be sent.
    // On cold start, RSO subsystem may need a few seconds to initialize.
    // Try up to 3 times with short delays instead of a long blocking wait.
    let mut rso_ready = false;
    for attempt in 1..=3 {
        match rc.init_rso_session().await {
            Ok(_) => {
                rso_ready = true;
                break;
            }
            Err(e) => {
                logger.login_flow("LOGIN", Some(&format!(
                    "RSO session init attempt {}/3 failed: {}", attempt, e
                )));
                if attempt < 3 {
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
            }
        }
    }

    if !rso_ready {
        logger.login_flow("LOGIN", Some("RSO session not ready — HTTP login unavailable"));
        return false;
    }

    // Send credentials
    match rc.login_via_rc(username, password).await {
        Ok(_) => {
            logger.login_flow("LOGIN", Some("HTTP login succeeded"));
            true
        }
        Err(e) => {
            logger.login_flow("LOGIN", Some(&format!("HTTP login failed: {}", e)));
            false
        }
    }
}

/// Ensure Riot Client is fully running with API ready.
/// Uses phase detection to skip already-completed steps.
/// Retries up to `max_attempts` times with full RC restart.
async fn ensure_rc_ready(
    rc: &RiotClientService,
    logger: &FileLogger,
    state: &AppState,
    app: &tauri::AppHandle,
    max_attempts: u32,
) -> Result<bool, String> {
    for attempt in 1..=max_attempts {
        check_cancelled(state)?;

        let phase = rc.detect_login_phase().await;
        logger.login_flow("RC_READY", Some(&format!(
            "attempt {}/{}: phase={:?}", attempt, max_attempts, phase
        )));

        match phase {
            LoginPhase::RcReady | LoginPhase::Authenticated => {
                return Ok(true);
            }
            LoginPhase::Nothing => {
                emit_progress(app, "Запуск Riot Client...");
                logger.login_flow("RC_READY", Some("starting Riot Client..."));
                RiotClientService::start_riot_client().map_err(|e| {
                    logger.login_flow("ERROR", Some(&format!("start_riot_client failed: {}", e)));
                    e.to_string()
                })?;
            }
            LoginPhase::RcStarting => {
                emit_progress(app, "Riot Client загружается...");
                logger.login_flow("RC_READY", Some("RC starting, waiting for lockfile..."));
            }
            LoginPhase::RcWaitingForApi => {
                emit_progress(app, "Ожидание Riot Client API...");
                logger.login_flow("RC_READY", Some("lockfile found, waiting for API..."));
            }
            LoginPhase::LeagueRunning => {
                return Ok(true);
            }
        }

        // Wait for lockfile (skip if already have it)
        if matches!(phase, LoginPhase::Nothing | LoginPhase::RcStarting) {
            let timeout = if attempt == 1 { 20 } else { 15 };
            if !RiotClientService::wait_for_rc_lockfile(Duration::from_secs(timeout)).await {
                logger.login_flow("RC_READY", Some(&format!("lockfile not found after {}s", timeout)));
                if attempt < max_attempts {
                    emit_progress(app, &format!("Перезапуск Riot Client (попытка {})...", attempt + 1));
                    logger.login_flow("RC_READY", Some("killing RC for restart..."));
                    RiotClientService::kill_league(true);
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
                continue;
            }
            logger.login_flow("RC_READY", Some("lockfile found"));
        }

        // Wait for API readiness
        emit_progress(app, "Ожидание Riot Client API...");
        let api_timeout = if attempt == 1 { 15 } else { 20 };
        if !rc.wait_for_rc_api_ready(Duration::from_secs(api_timeout)).await {
            logger.login_flow("RC_READY", Some(&format!("API not ready after {}s", api_timeout)));
            if attempt < max_attempts {
                emit_progress(app, &format!("Перезапуск Riot Client (попытка {})...", attempt + 1));
                logger.login_flow("RC_READY", Some("killing RC for restart..."));
                RiotClientService::kill_league(true);
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
            continue;
        }

        logger.login_flow("RC_READY", Some("RC API is ready"));
        return Ok(true);
    }

    Ok(false)
}
