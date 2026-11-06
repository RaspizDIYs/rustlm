use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;

use tauri::{Emitter, State};

use crate::commands::accounts::{spawn_post_login_lcu_refresh, trigger_cloud_sync};
use crate::services::file_logger::FileLogger;
use crate::services::riot_client::{LoginPhase, RiotClientService};
use crate::services::riot_game::RiotGame;
use crate::state::AppState;

use crate::services::{client_action as login_lock, ClientAction, ClientActionGuard};

fn begin_login(cancelled: &std::sync::atomic::AtomicBool) -> Result<ClientAction, String> {
    let guard = login_lock()
        .clone()
        .try_lock_owned()
        .map_err(|_| "Вход или восстановление клиента уже выполняется".to_string())?;
    cancelled.store(false, Ordering::Relaxed);
    Ok(Arc::new(ClientActionGuard::new(guard)))
}

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
    login_local_for_party(username, &state, app)
        .await
        .map(|_| ())
}

#[tauri::command]
pub async fn launch_riot_account(
    username: String,
    game: RiotGame,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    login_local_for_game(username, game, &state, app)
        .await
        .map(|_| ())
}

pub(crate) async fn login_local_for_party(
    username: String,
    state: &AppState,
    app: tauri::AppHandle,
) -> Result<ClientAction, String> {
    login_local_for_game(username, RiotGame::League, state, app).await
}

async fn login_local_for_game(
    username: String,
    game: RiotGame,
    state: &AppState,
    app: tauri::AppHandle,
) -> Result<ClientAction, String> {
    let guard = begin_login(&state.login_cancelled)?;
    // Local accounts use DPAPI; shared accounts never enter this storage.
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
        .map_err(|e| e.to_string())?;
    login_with_credentials_for_game(
        username,
        plain_password,
        false,
        game,
        &state,
        app,
        guard.clone(),
    )
    .await?;
    // The party caller keeps other logins excluded until lobby membership is confirmed.
    Ok(guard)
}

// Restore the config after a completed attempt, including UI cancellation and launch failures.
async fn login_with_credentials(
    username: String,
    plain_password: String,
    shared_account: bool,
    state: &AppState,
    app: tauri::AppHandle,
    action: ClientAction,
) -> Result<(), String> {
    login_with_credentials_for_game(
        username,
        plain_password,
        shared_account,
        RiotGame::League,
        state,
        app,
        action,
    )
    .await
}

async fn login_with_credentials_for_game(
    username: String,
    plain_password: String,
    shared_account: bool,
    game: RiotGame,
    state: &AppState,
    app: tauri::AppHandle,
    action: ClientAction,
) -> Result<(), String> {
    #[cfg(debug_assertions)]
    if std::env::var_os("RUSTLM_TEST_LCU").is_some() {
        return Err("В режиме воспроизведения вход в Riot отключён".into());
    }
    ensure_safe_switch(state).await?;
    if game.uses_league() {
        // Register before prepare: an aborted prepare worker may set the flag later.
        action.retain_config_login(state.lol_config.login_lifecycle_flag());
    }
    let result = login_with_credentials_inner(
        username,
        plain_password,
        shared_account,
        game,
        state,
        app,
        action.clone(),
    )
    .await;
    if !game.uses_league() {
        return result;
    }
    let config = state.lol_config.clone();
    let success = result.is_ok();
    let recovery = spawn_client_blocking(action.clone(), move || config.finish_login(success))
        .await
        .map_err(|_| "Не завершено восстановление настроек".to_string())?
        .map_err(|error| error.to_string());
    match (result, recovery) {
        (Err(login), Err(recovery)) => Err(format!(
            "{login}. Настройки ожидают восстановления: {recovery}"
        )),
        (Err(login), _) => Err(login),
        (Ok(()), recovery) => recovery,
    }
}

async fn ensure_safe_switch(state: &AppState) -> Result<(), String> {
    let probe = RiotClientService::unglitch_probe();
    if RiotClientService::is_valorant_running() {
        return Err("Сначала закройте Valorant перед сменой Riot-аккаунта".into());
    }
    if probe.game_running {
        return Err("Сначала завершите игру; LM не будет закрывать её ради смены аккаунта".into());
    }
    if probe.backend_running || probe.ui_running {
        let raw = state
            .riot_client
            .lcu_get("/lol-gameflow/v1/gameflow-phase")
            .await
            .map_err(|_| {
                "Не удалось безопасно проверить состояние League. Закройте клиент вручную"
                    .to_string()
            })?;
        let phase: String =
            serde_json::from_str(&raw).map_err(|_| "Неизвестное состояние League".to_string())?;
        if !matches!(phase.as_str(), "None" | "Lobby") {
            return Err("Сначала выйдите из поиска, выбора чемпионов или игры".into());
        }
    }
    Ok(())
}

async fn wait_league_closed(state: &AppState) -> Result<(), String> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        check_cancelled(state)?;
        let probe = RiotClientService::unglitch_probe();
        if !probe.backend_running && !probe.ui_running && !probe.game_running {
            return Ok(());
        }
        if probe.game_running || tokio::time::Instant::now() >= deadline {
            return Err("League ещё работает. Закройте клиент и повторите вход".into());
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

async fn login_with_credentials_inner(
    username: String,
    plain_password: String,
    shared_account: bool,
    game: RiotGame,
    state: &AppState,
    app: tauri::AppHandle,
    action: ClientAction,
) -> Result<(), String> {
    check_cancelled(state)?;
    let logger: &FileLogger = &state.logger;
    logger.login_flow("START", Some(&format!("user={}", username)));
    logger.login_flow("DECRYPT", Some("password available in Rust memory only"));

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

        LoginPhase::Authenticated => prepare_logout(rc, logger, &app, &state, false).await?,

        LoginPhase::LeagueRunning => prepare_logout(rc, logger, &app, &state, true).await?,
    };
    check_cancelled(&state)?;

    wait_league_closed(state).await?;
    if game.uses_league() {
        let config = state.lol_config.clone();
        spawn_client_blocking(action.clone(), move || config.prepare_login(shared_account))
            .await
            .map_err(|_| "Не удалось подготовить личные настройки".to_string())?
            .map_err(|error| error.to_string())?;
        check_cancelled(state)?;
    }
    // 4. UIA login with phase-specific timeout
    emit_progress(&app, "Вход в аккаунт...");
    logger.login_flow("UIA", Some(&format!("timeout={}s", uia_timeout)));

    let uia_ok = run_uia_login(
        &username,
        &plain_password,
        uia_timeout,
        &state,
        action.clone(),
    )
    .await;
    check_cancelled(&state)?;

    if uia_ok {
        logger.login_flow("UIA", Some("success"));
        return finalize_login(
            rc,
            logger,
            &app,
            &state,
            shared_account,
            &username,
            game,
            action.clone(),
        )
        .await;
    }

    // 5. Fallback — two stages: soft (retry logout), then hard (kill+restart)
    logger.login_flow("UIA", Some("failed, entering fallback"));

    // Soft fallback: RC UI is alive → retry logout via API + UIA
    if RiotClientService::is_riot_client_ui_running() {
        check_cancelled(&state)?;
        logger.login_flow("FALLBACK_SOFT", Some("RC UI alive, retrying logout + UIA"));

        emit_progress(&app, "Повторная попытка выхода...");
        ensure_safe_switch(state).await?;
        rc.invalidate_cache();
        let _ = rc.logout_via_rc().await;
        check_cancelled(&state)?;

        // Kill League if still lingering
        if RiotClientService::is_league_running() {
            if RiotClientService::unglitch_probe().game_running {
                return Err("Игра запущена; автоматическое закрытие отменено".into());
            }
            RiotClientService::kill_league(false);
        }

        emit_progress(&app, "Вход в аккаунт...");
        let uia_ok = run_uia_login(&username, &plain_password, 10, &state, action.clone()).await;
        check_cancelled(&state)?;

        if uia_ok {
            logger.login_flow("FALLBACK_SOFT", Some("succeeded"));
            return finalize_login(
                rc,
                logger,
                &app,
                &state,
                shared_account,
                &username,
                game,
                action.clone(),
            )
            .await;
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
    let uia_ok = run_uia_login(&username, &plain_password, 45, &state, action.clone()).await;
    check_cancelled(&state)?;

    if uia_ok {
        logger.login_flow("FALLBACK_HARD", Some("succeeded"));
        return finalize_login(
            rc,
            logger,
            &app,
            &state,
            shared_account,
            &username,
            game,
            action.clone(),
        )
        .await;
    }

    Err("Не удалось войти после нескольких попыток".to_string())
}

#[tauri::command]
pub async fn league_login(
    id: String,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let action = begin_login(&state.login_cancelled)?;
    let (username, password) = crate::commands::league_sharing::credentials(&id, &state)
        .await
        .map_err(|e| e.to_string())?;
    login_with_credentials(username, password, true, &state, app, action).await
}

#[tauri::command]
pub async fn login_to_shared_account(
    id: String,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let action = begin_login(&state.login_cancelled)?;
    emit_progress(&app, "Получение разрешения GoodLuck...");
    let session = state.cloud_sync.get_valid_totp_session_public();
    let credentials = state
        .goodluck
        .get_shared_credentials(&id, session.as_deref())
        .await
        .map_err(|e| e.to_string())?;
    login_with_credentials(
        credentials.username,
        credentials.password,
        true,
        &state,
        app,
        action,
    )
    .await
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
    ensure_safe_switch(state).await?;
    emit_progress(app, "Выход из текущей сессии...");
    rc.invalidate_cache();
    let _ = rc.logout_via_rc().await;
    check_cancelled(state)?;

    // Now kill League — RC already processed the logout, so the crash
    // of League won't confuse it.
    if league_running && RiotClientService::is_league_running() {
        emit_progress(app, "Закрытие League of Legends...");
        if RiotClientService::unglitch_probe().game_running {
            return Err("Игра запущена; автоматическое закрытие отменено".into());
        }
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
    shared_account: bool,
    username: &str,
    game: RiotGame,
    action: ClientAction,
) -> Result<(), String> {
    emit_progress(app, "Ожидание авторизации...");
    logger.login_flow("AUTH", Some("waiting for RSO authorization..."));
    if !wait_rso_with_cancel(rc, true, 20, state).await? {
        return Err("Авторизация не подтверждена в течение 20 секунд".to_string());
    }

    check_cancelled(state)?;

    cancellable_login_step(rc.verify_authorized_login(username), &state.login_cancelled).await??;
    check_cancelled(state)?;

    if shared_account && game.uses_league() {
        let config = state.lol_config.clone();
        let username = username.to_owned();
        spawn_client_blocking(action.clone(), move || {
            config.apply_for_shared_login(&username)
        })
        .await
        .map_err(|_| "Не удалось применить личные клавиши".to_string())?
        .map_err(|error| error.to_string())?;
        check_cancelled(state)?;
    }

    if game == RiotGame::Valorant {
        emit_progress(app, "Запуск Valorant...");
        RiotClientService::launch_product(game).map_err(|e| e.to_string())?;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(120);
        loop {
            check_cancelled(state)?;
            if RiotClientService::is_valorant_running() {
                break;
            }
            if tokio::time::Instant::now() >= deadline {
                return Err("Valorant не запустился за 2 минуты. Проверьте обновление, Vanguard и запросы Riot Client".into());
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    } else if !RiotClientService::is_league_running() {
        emit_progress(app, "Запуск League of Legends...");
        logger.login_flow("LAUNCH", Some("launching League of Legends..."));
        match rc.launch_league_via_rc().await {
            Ok(_) => {}
            Err(e) => {
                logger.login_flow("LAUNCH", Some(&format!("launch failed: {}", e)));
                return Err(format!("Не удалось запустить League of Legends: {}", e));
            }
        }
    }

    if game.uses_league() {
        emit_progress(app, "Ожидание League Client...");
        let deadline = tokio::time::Instant::now() + Duration::from_secs(120);
        loop {
            check_cancelled(state)?;
            if rc
                .lcu_get("/lol-summoner/v1/current-summoner")
                .await
                .is_ok()
            {
                break;
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(
                    "Клиент ещё не готов. Проверьте обновление League в Riot Client".into(),
                );
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        if game == RiotGame::Tft {
            cancellable_login_step(rc.verify_authorized_login(username), &state.login_cancelled)
                .await??;
            check_cancelled(state)?;
            emit_progress(app, "Открытие TFT...");
            rc.open_tft_lobby().await?;
        }
    }
    cancellable_login_step(rc.verify_authorized_login(username), &state.login_cancelled).await??;
    check_cancelled(state)?;
    logger.login_flow("DONE", Some(game.name()));
    rc.invalidate_cache();
    if !shared_account && game.uses_league() {
        trigger_cloud_sync(state);
        spawn_post_login_lcu_refresh(app.clone());
    }
    Ok(())
}

fn spawn_client_blocking<T: Send + 'static>(
    action: ClientAction,
    work: impl FnOnce() -> T + Send + 'static,
) -> tokio::task::JoinHandle<T> {
    tokio::task::spawn_blocking(move || {
        // Aborting the awaiting future must not admit another client mutation.
        let _action = action;
        work()
    })
}

struct CancelWorkerOnDrop(Arc<AtomicBool>);

impl Drop for CancelWorkerOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

async fn run_login_worker<T: Send + 'static>(
    action: ClientAction,
    ui_cancel: Arc<AtomicBool>,
    work: impl FnOnce(Arc<AtomicBool>) -> T + Send + 'static,
) -> Result<T, tokio::task::JoinError> {
    let cancel = Arc::new(AtomicBool::new(ui_cancel.load(Ordering::Relaxed)));
    let _cancel_on_drop = CancelWorkerOnDrop(cancel.clone());
    let worker_cancel = cancel.clone();
    let mut worker = spawn_client_blocking(action, move || work(worker_cancel));
    loop {
        if ui_cancel.load(Ordering::Relaxed) {
            cancel.store(true, Ordering::Relaxed);
        }
        tokio::select! {
            result = &mut worker => return result,
            _ = tokio::time::sleep(Duration::from_millis(50)) => {}
        }
    }
}

/// Run UIA login in a blocking thread. Returns true on success.
///
/// UI Automation есть только на Windows. Заглушка для остальных платформ нужна,
/// чтобы проект можно было собрать и проверить на любой машине разработчика.
async fn run_uia_login(
    username: &str,
    password: &str,
    timeout_secs: u64,
    state: &AppState,
    action: ClientAction,
) -> bool {
    #[cfg(not(windows))]
    {
        let _ = (username, password, timeout_secs, state, action);
        false
    }

    #[cfg(windows)]
    {
        let uia_user = username.to_string();
        let uia_pass = password.to_string();
        let result = run_login_worker(action, state.login_cancelled.clone(), move |cancel| {
            crate::services::uia_login::login_to_riot_client(
                &uia_user,
                &uia_pass,
                timeout_secs,
                Some(&cancel),
            )
        })
        .await;
        matches!(result, Ok(Ok(())))
    }
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
    let result = cancellable_login_step(
        rc.wait_for_rso_state(authorized, Duration::from_secs(timeout_secs)),
        &state.login_cancelled,
    )
    .await;
    if result.is_err() {
        state.logger.login_flow("CANCELLED", None);
    }
    result
}

async fn cancellable_login_step<T>(
    wait: impl std::future::Future<Output = T>,
    cancelled: &AtomicBool,
) -> Result<T, String> {
    let cancellation = async {
        while !cancelled.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    };
    tokio::select! {
        biased;
        _ = cancellation => Err("Login cancelled".into()),
        result = wait => {
            if cancelled.load(Ordering::SeqCst) { Err("Login cancelled".into()) } else { Ok(result) }
        }
    }
}

/// Kill all RC/League processes and wait until they are actually dead.
async fn kill_and_wait(logger: &FileLogger, state: &AppState) -> Result<(), String> {
    ensure_safe_switch(state).await?;
    RiotClientService::kill_league(true);
    let result = wait_for_client_exit(Duration::from_secs(10), &state.login_cancelled, || {
        let (backend, ui, _) = RiotClientService::unglitch_process_state();
        RiotClientService::is_riot_client_running() || backend || ui
    })
    .await;
    if result.is_err() {
        logger.login_flow(
            "CLEANUP",
            Some("client exit not confirmed; session cleanup cancelled"),
        );
    }
    result
}

async fn wait_for_client_exit(
    timeout: Duration,
    cancelled: &AtomicBool,
    running: impl Fn() -> bool,
) -> Result<(), String> {
    let stopped = cancellable_login_step(
        async {
            tokio::time::timeout(timeout, async {
                while running() {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                true
            })
            .await
            .unwrap_or(false)
        },
        cancelled,
    )
    .await?;
    if stopped {
        Ok(())
    } else {
        Err("Riot Client не закрылся за отведённое время. Закройте его вручную и повторите вход. Новая очистка сессии отменена".into())
    }
}

/// Delete RC session/cookie data so RC doesn't auto-login on next start.
fn delete_rc_session_data(logger: &FileLogger) {
    let local_app_data = match std::env::var("LOCALAPPDATA") {
        Ok(v) => std::path::PathBuf::from(v),
        Err(_) => return,
    };

    let paths_to_delete = [
        local_app_data
            .join("Riot Games")
            .join("Riot Client")
            .join("Data")
            .join("RiotGamesPrivateSettings.yaml"),
        local_app_data
            .join("Riot Games")
            .join("Riot Client")
            .join("Data")
            .join("RiotClientPrivateSettings.yaml"),
    ];

    for path in &paths_to_delete {
        if path.exists() {
            match std::fs::remove_file(path) {
                Ok(_) => logger.login_flow("CLEANUP", Some(&format!("deleted {}", path.display()))),
                Err(e) => logger.login_flow(
                    "CLEANUP",
                    Some(&format!("failed to delete {}: {}", path.display(), e)),
                ),
            }
        }
    }
}

#[cfg(test)]
mod login_guard_tests {
    use super::*;
    #[tokio::test]
    async fn unconfirmed_process_exit_never_allows_session_cleanup_to_continue() {
        let cancelled = AtomicBool::new(false);
        assert!(wait_for_client_exit(Duration::ZERO, &cancelled, || false)
            .await
            .is_ok());
        assert!(wait_for_client_exit(Duration::ZERO, &cancelled, || true)
            .await
            .unwrap_err()
            .contains("очистка сессии отменена"));
        assert_eq!(
            wait_for_client_exit(Duration::from_secs(1), &AtomicBool::new(true), || false).await,
            Err("Login cancelled".into())
        );
    }
    #[tokio::test]
    async fn rso_wait_cancellation_does_not_wait_for_a_stalled_http_response() {
        assert_eq!(
            cancellable_login_step(async { true }, &AtomicBool::new(true)).await,
            Err("Login cancelled".into())
        );
        let cancelled = Arc::new(AtomicBool::new(false));
        let flag = cancelled.clone();
        let (started, ready) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            cancellable_login_step(
                async {
                    let _ = started.send(());
                    std::future::pending::<bool>().await
                },
                &flag,
            )
            .await
        });
        ready.await.unwrap();
        cancelled.store(true, Ordering::SeqCst);
        let result = tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(result, Err("Login cancelled".into()));
    }

    #[tokio::test]
    async fn aborted_login_cancels_worker_and_keeps_client_lock_until_worker_exits() {
        let lock = Arc::new(tokio::sync::Mutex::new(()));
        let action = Arc::new(ClientActionGuard::new(
            lock.clone().try_lock_owned().unwrap(),
        ));
        let ui_cancel = Arc::new(AtomicBool::new(false));
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let task = tokio::spawn(run_login_worker(action, ui_cancel.clone(), move |cancel| {
            started_tx.send(cancel.clone()).unwrap();
            release_rx.recv().unwrap();
            cancel.load(Ordering::Relaxed)
        }));
        let worker_cancel = started_rx.await.unwrap();
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        let cancelled = worker_cancel.load(Ordering::Relaxed);
        let still_locked = lock.try_lock().is_err();
        // A reset of the UI flag cannot revive this worker's private cancellation.
        ui_cancel.store(false, Ordering::Relaxed);
        let stays_cancelled = worker_cancel.load(Ordering::Relaxed);
        release_tx.send(()).unwrap();
        let _next = lock.lock().await;
        assert!(cancelled);
        assert!(stays_cancelled);
        assert!(still_locked);
    }

    #[tokio::test]
    async fn blocking_config_work_retains_client_lock_after_awaiting_task_is_aborted() {
        let lock = Arc::new(tokio::sync::Mutex::new(()));
        let action = Arc::new(ClientActionGuard::new(
            lock.clone().try_lock_owned().unwrap(),
        ));
        let login_in_progress = Arc::new(AtomicBool::new(false));
        action.retain_config_login(login_in_progress.clone());
        let worker_flag = login_in_progress.clone();
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let task = tokio::spawn(async move {
            spawn_client_blocking(action, move || {
                worker_flag.store(true, Ordering::SeqCst);
                started_tx.send(()).unwrap();
                release_rx.recv().unwrap();
            })
            .await
            .unwrap();
        });
        started_rx.await.unwrap();
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        let still_locked = lock.try_lock().is_err();
        let still_in_progress = login_in_progress.load(Ordering::SeqCst);
        release_tx.send(()).unwrap();
        let _next = lock.lock().await;
        assert!(still_locked);
        assert!(still_in_progress);
        assert!(!login_in_progress.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn config_worker_return_does_not_end_the_outer_login_lifecycle() {
        let lock = Arc::new(tokio::sync::Mutex::new(()));
        let action = Arc::new(ClientActionGuard::new(
            lock.clone().try_lock_owned().unwrap(),
        ));
        let flag = Arc::new(AtomicBool::new(false));
        action.retain_config_login(flag.clone());
        let worker_flag = flag.clone();
        spawn_client_blocking(action.clone(), move || {
            worker_flag.store(true, Ordering::SeqCst);
        })
        .await
        .unwrap();
        assert!(flag.load(Ordering::SeqCst));
        assert!(lock.try_lock().is_err());
        drop(action);
        assert!(!flag.load(Ordering::SeqCst));
        assert!(lock.try_lock().is_ok());
    }

    #[tokio::test]
    async fn ui_cancellation_is_copied_to_the_private_worker_flag() {
        let lock = Arc::new(tokio::sync::Mutex::new(()));
        let action = Arc::new(ClientActionGuard::new(
            lock.clone().try_lock_owned().unwrap(),
        ));
        let ui_cancel = Arc::new(AtomicBool::new(true));
        assert!(run_login_worker(action, ui_cancel, |cancel| {
            cancel.load(Ordering::Relaxed)
        })
        .await
        .unwrap());
        assert!(lock.try_lock().is_ok());
    }

    #[test]
    fn duplicate_login_cannot_reset_cancellation_and_guard_releases_after_failure() {
        let cancelled = std::sync::atomic::AtomicBool::new(true);
        let guard = begin_login(&cancelled).unwrap();
        assert!(!cancelled.load(Ordering::Relaxed));
        cancelled.store(true, Ordering::Relaxed);
        assert!(crate::services::unglitch::action_lock().try_lock().is_err());
        assert!(begin_login(&cancelled).is_err());
        assert!(cancelled.load(Ordering::Relaxed));
        drop(guard);
        let next = begin_login(&cancelled).unwrap();
        assert!(!cancelled.load(Ordering::Relaxed));
        drop(next);
        let _unglitch = crate::services::unglitch::action_lock().try_lock().unwrap();
        cancelled.store(true, Ordering::Relaxed);
        assert!(begin_login(&cancelled).is_err());
        assert!(cancelled.load(Ordering::Relaxed));
    }
}
