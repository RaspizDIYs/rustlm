use std::sync::atomic::Ordering;
use std::time::Duration;

use tauri::{Manager, State};

use crate::services::riot_client::RiotClientService;
use crate::state::AppState;

/// Check if login was cancelled, return Err if so.
fn check_cancelled(state: &AppState) -> Result<(), String> {
    if state.login_cancelled.load(Ordering::Relaxed) {
        state.logger.login_flow("CANCELLED", None);
        Err("Login cancelled".to_string())
    } else {
        Ok(())
    }
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

    let logger = &state.logger;
    logger.login_flow("START", Some(&format!("user={}", username)));

    let plain_password = state
        .accounts
        .unprotect(&password)
        .map_err(|e| {
            logger.login_flow("ERROR", Some(&format!("unprotect failed: {}", e)));
            e.to_string()
        })?;

    logger.login_flow("DECRYPT", Some("password decrypted OK"));

    let rc = &state.riot_client;
    let rc_running = RiotClientService::is_riot_client_running();
    let league_running = RiotClientService::is_league_running();
    logger.login_flow("STATE", Some(&format!("RC={}, League={}", rc_running, league_running)));

    // 1. Kill League Client only if it's running
    if league_running {
        logger.login_flow("STEP 1", Some("killing League Client..."));
        RiotClientService::kill_league(false);
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    check_cancelled(&state)?;

    // 2. Logout only if RC is running AND someone is authorized
    if rc_running {
        let already_auth = rc.is_rso_authorized().await;
        logger.login_flow("STEP 2", Some(&format!("currently authorized: {}", already_auth)));

        if already_auth {
            logger.login_flow("STEP 2", Some("logging out via RC API..."));
            let _ = rc.logout_via_rc().await;
            rc.invalidate_cache();

            if !rc.wait_for_rso_state(false, Duration::from_secs(6)).await {
                logger.login_flow("STEP 2", Some("RSO still authorized — killing RC"));
                RiotClientService::kill_league(true);
                tokio::time::sleep(Duration::from_millis(800)).await;
            }
        }
    }
    check_cancelled(&state)?;

    // 3. Start RC if not running
    let rc_running_now = RiotClientService::is_riot_client_running();
    if !rc_running_now {
        logger.login_flow("STEP 3", Some("starting Riot Client..."));
        RiotClientService::start_riot_client().map_err(|e| {
            logger.login_flow("ERROR", Some(&format!("start_riot_client failed: {}", e)));
            e.to_string()
        })?;
    }

    // 4. Wait for RC lockfile + API ready
    if !RiotClientService::wait_for_rc_lockfile(Duration::from_secs(15)) {
        return Err("RC lockfile not found after 15s".to_string());
    }

    let api_ready = rc.wait_for_rc_api_ready(Duration::from_secs(10)).await;
    logger.login_flow("STEP 4", Some(&format!("API ready: {}", api_ready)));

    if !api_ready {
        logger.login_flow("STEP 4", Some("RC API stuck — restarting..."));
        RiotClientService::kill_league(true);
        tokio::time::sleep(Duration::from_secs(2)).await;
        check_cancelled(&state)?;

        RiotClientService::start_riot_client().map_err(|e| e.to_string())?;
        if !RiotClientService::wait_for_rc_lockfile(Duration::from_secs(15)) {
            return Err("RC lockfile not found after restart".to_string());
        }
        if !rc.wait_for_rc_api_ready(Duration::from_secs(15)).await {
            return Err("RC API still not ready after restart".to_string());
        }
    }
    check_cancelled(&state)?;

    // 5. Check if already authorized (e.g. "remember me")
    if rc.is_rso_authorized().await {
        logger.login_flow("STEP 5", Some("already authorized — launching League"));
        if !RiotClientService::is_league_running() {
            let _ = rc.launch_league_via_rc().await;
        }
        return Ok(());
    }

    // 6. Login: start UIA in background immediately, try HTTP in parallel
    //    If HTTP succeeds → abort UIA. If HTTP fails → UIA already running.
    logger.login_flow("STEP 6", Some("starting parallel HTTP + UIA login..."));

    // Minimize RustLM window so UIA can access RC
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.minimize();
    }

    // Start UIA in background immediately (it will find the RC window on its own)
    let uia_user = username.clone();
    let uia_pass = plain_password.clone();
    let uia_cancel = state.login_cancelled.clone();
    let uia_handle = tokio::task::spawn_blocking(move || {
        #[cfg(windows)]
        {
            crate::services::uia_login::login_to_riot_client(&uia_user, &uia_pass, 30, Some(&uia_cancel))
                .map_err(|e| e.to_string())
        }
        #[cfg(not(windows))]
        {
            let _ = (&uia_user, &uia_pass, &uia_cancel);
            Err::<(), String>("UIA login is only supported on Windows".to_string())
        }
    });

    // Try HTTP login concurrently (init RSO + credentials)
    let http_ok = {
        let _ = rc.init_rso_session().await; // ignore errors
        match rc.login_via_rc(&username, &plain_password).await {
            Ok(_) => {
                logger.login_flow("STEP 6", Some("HTTP login OK — aborting UIA"));
                uia_handle.abort();
                true
            }
            Err(e) => {
                logger.login_flow("STEP 6", Some(&format!("HTTP failed: {} — waiting for UIA", e)));
                false
            }
        }
    };

    if !http_ok {
        // Wait for UIA to complete (it's already been running ~0.5s by now)
        let uia_result = uia_handle
            .await
            .map_err(|e| {
                logger.login_flow("ERROR", Some(&format!("UIA join error: {}", e)));
                e.to_string()
            })?;

        if let Err(e) = &uia_result {
            logger.login_flow("STEP 6", Some(&format!("UIA error: {}", e)));
        }
        uia_result?;
    }

    // Restore RustLM window
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
    }

    check_cancelled(&state)?;

    // 7. Wait for RSO authorization
    logger.login_flow("STEP 7", Some("waiting for RSO auth..."));
    rc.wait_for_rso_state(true, Duration::from_secs(15)).await;

    // 8. Launch League of Legends
    if !RiotClientService::is_league_running() {
        let _ = rc.launch_league_via_rc().await;
    }

    logger.login_flow("DONE", Some("login complete"));
    Ok(())
}
