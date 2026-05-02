mod cloud_profile_apply;
mod commands;
mod error;
mod models;
mod services;
mod state;
mod tray;

use state::AppState;
use tauri::Emitter;
use tauri::Manager;
use tauri_plugin_deep_link::DeepLinkExt;

/// Parse deep link URL and handle GoodLuck OAuth callback
fn handle_deep_link_url(handle: &tauri::AppHandle, url_str: &str) {
    if !url_str.starts_with("rustlm://auth/callback") {
        return;
    }
    // Parse query parameters from URL
    let query = match url_str.split_once('?') {
        Some((_, q)) => q,
        None => return,
    };
    let params: std::collections::HashMap<String, String> = query
        .split('&')
        .filter_map(|pair| {
            let mut parts = pair.splitn(2, '=');
            let k = parts.next()?.to_string();
            let v_raw = parts.next()?.to_string();
            let v = urlencoding::decode(&v_raw)
                .map(|c| c.into_owned())
                .unwrap_or(v_raw);
            Some((k, v))
        })
        .collect();

    if let (Some(code), Some(state)) = (params.get("code"), params.get("state")) {
        let code = code.clone();
        let state = state.clone();
        let handle = handle.clone();
        tauri::async_runtime::spawn(async move {
            let app_state = handle.state::<AppState>();
            match app_state.goodluck.handle_callback(&code, &state).await {
                Ok(oauth_user) => {
                    let user = commands::goodluck::run_goodluck_post_login(
                        &handle,
                        &*app_state,
                        oauth_user,
                    )
                    .await;
                    let _ = handle.emit("goodluck-auth-success", &user);
                }
                Err(e) => {
                    let _ = handle.emit("goodluck-auth-error", e.to_string());
                }
            }
        });
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app_state = AppState::new();

    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            // On Windows, deep link URLs arrive as command-line args of the second instance
            for arg in args {
                if arg.starts_with("rustlm://") {
                    handle_deep_link_url(app, &arg);
                }
            }
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_focus();
                let _ = window.show();
            }
        }))
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            commands::greet,
            commands::settings::load_setting,
            commands::settings::save_setting,
            commands::settings::load_update_settings,
            commands::settings::save_update_settings,
            commands::settings::get_autostart_enabled,
            commands::settings::set_autostart_enabled,
            commands::settings::get_autostart_background,
            commands::settings::set_autostart_background,
            commands::settings::should_start_minimized,
            commands::logs::get_log_lines,
            commands::logs::get_log_path,
            commands::logs::clear_logs,
            commands::accounts::load_accounts,
            commands::accounts::refresh_account_profile_from_lcu,
            commands::accounts::save_account,
            commands::accounts::save_accounts_order,
            commands::accounts::delete_account,
            commands::accounts::protect_password,
            commands::accounts::export_accounts,
            commands::accounts::import_accounts,
            commands::riot_client::is_riot_client_running,
            commands::riot_client::is_league_running,
            commands::riot_client::kill_league,
            commands::riot_client::start_riot_client,
            commands::riot_client::restart_league,
            commands::riot_client::probe_connectivity,
            commands::riot_client::get_account_info,
            commands::riot_client::lcu_get,
            commands::riot_client::lcu_post,
            commands::riot_client::invalidate_lcu_cache,
            commands::riot_client::detect_server,
            commands::riot_client::get_authorized_riot_login_username,
            commands::data_dragon::get_ddragon_version,
            commands::data_dragon::get_champions,
            commands::data_dragon::get_champion_info,
            commands::data_dragon::get_summoner_spells,
            commands::data_dragon::get_champion_image_url,
            commands::rune_pages::get_rune_paths,
            commands::rune_pages::get_rune_path_by_id,
            commands::rune_pages::get_rune_by_id,
            commands::rune_pages::get_stat_mods_row1,
            commands::rune_pages::get_stat_mods_row2,
            commands::rune_pages::get_stat_mods_row3,
            commands::rune_pages::load_rune_pages,
            commands::rune_pages::save_rune_page,
            commands::rune_pages::save_all_rune_pages,
            commands::rune_pages::delete_rune_page,
            commands::auto_accept::set_auto_accept_enabled,
            commands::auto_accept::is_auto_accept_enabled,
            commands::auto_accept::set_automation_settings,
            commands::auto_accept::get_automation_settings,
            commands::login::login_to_account,
            commands::login::cancel_login,
            commands::customization::set_profile_status,
            commands::customization::set_profile_availability,
            commands::customization::set_profile_icon,
            commands::customization::set_profile_background,
            commands::customization::get_challenges,
            commands::customization::set_challenge_tokens,
            commands::reveal::get_reveal_api_config,
            commands::reveal::set_reveal_api_config,
            commands::reveal::test_api_key,
            commands::reveal::get_teams_info,
            commands::reveal::send_chat_message,
            commands::migration::check_lolmanager_installed,
            commands::migration::uninstall_lolmanager,
            commands::goodluck::goodluck_login,
            commands::goodluck::goodluck_handle_callback,
            commands::goodluck::goodluck_get_user,
            commands::goodluck::goodluck_is_connected,
            commands::goodluck::goodluck_logout,
            commands::goodluck::goodluck_sync_accounts,
            commands::goodluck::goodluck_delete_server_data,
            commands::goodluck::goodluck_get_synced_accounts,
            commands::goodluck::goodluck_get_profile_accounts,
            commands::goodluck::goodluck_import_profile_accounts,
            commands::goodluck::goodluck_refresh_profile,
            commands::cloud_sync::cloud_sync,
            commands::cloud_sync::cloud_push,
            commands::cloud_sync::cloud_pull,
            commands::cloud_sync::cloud_get_status,
            commands::cloud_sync::cloud_totp_session_active,
            commands::cloud_sync::cloud_notify_change,
            commands::cloud_sync::cloud_delete_data,
            commands::totp::totp_get_status,
            commands::totp::totp_setup,
            commands::totp::totp_confirm_setup,
            commands::totp::totp_disable,
            commands::totp::totp_validate,
            commands::lol_config::lol_cfg_get_status,
            commands::lol_config::lol_cfg_set_readonly,
            commands::lol_config::lol_cfg_list_presets,
            commands::lol_config::lol_cfg_create_preset,
            commands::lol_config::lol_cfg_apply_preset,
            commands::lol_config::lol_cfg_delete_preset,
            commands::lol_config::lol_cfg_export_preset,
            commands::lol_config::lol_cfg_import_preset,
            commands::refresh_tray,
        ])
        .setup(|app| {
            app.handle().plugin(
                tauri_plugin_log::Builder::default()
                    .level(log::LevelFilter::Info)
                    .target(tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::LogDir { file_name: Some("rustlm".into()) }))
                    .build(),
            )?;

            // Load persisted automation settings and set app handle
            let state = app.state::<AppState>();
            commands::auto_accept::load_persisted_automation_settings(&state);

            let handle = app.handle().clone();
            let auto_accept = state.auto_accept.clone();
            tauri::async_runtime::spawn(async move {
                auto_accept.set_app_handle(handle).await;
                // Start background listeners immediately (champ-select automation is always active)
                auto_accept.ensure_listeners_started().await;
            });

            state.cloud_sync.set_app_handle(app.handle().clone());
            state
                .cloud_sync
                .start_debounce_loop(&app.handle().clone());

            let gl_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let st = gl_handle.state::<AppState>();
                let has_auth = st.goodluck.get_user().is_some();
                if has_auth {
                    if let Ok(user) = st.goodluck.refresh_profile_from_server().await {
                        let _ = gl_handle.emit("goodluck-profile-updated", &user);
                    }

                    let auto_sync: bool = st.settings.load_setting("GoodLuckAutoSync", false);
                    if auto_sync {
                        let accounts = st.accounts.load_all();
                        let sync_data: Vec<crate::models::goodluck::SyncAccountData> = accounts
                            .into_iter()
                            .filter(|a| !a.riot_id.trim().is_empty())
                            .map(|a| crate::models::goodluck::SyncAccountData {
                                riot_id: a.riot_id,
                                server: commands::goodluck::map_lm_server_for_goodluck_platform(&a.server),
                                rank: a.rank,
                                summoner_name: a.summoner_name,
                            })
                            .collect();
                        if !sync_data.is_empty() {
                            let _ = st.goodluck.sync_accounts(sync_data).await;
                        }
                    }

                    let _ = st.cloud_sync.sync(&*st).await;
                    let _ = gl_handle.emit("cloud-sync-complete", ());
                }
            });

            // Set window icon explicitly (for dev mode where EXE resources aren't embedded)
            let app_icon = tauri::image::Image::from_bytes(include_bytes!("../icons/icon.png"))?;
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_icon(app_icon.clone());
            }

            // Deep link handler for GoodLuck OAuth callback (macOS/Linux; on Windows handled via single-instance)
            let handle_for_deeplink = app.handle().clone();
            app.deep_link().on_open_url(move |event| {
                for url in event.urls() {
                    handle_deep_link_url(&handle_for_deeplink, url.as_str());
                }
            });

            // System tray with auto-accept checkbox and accounts submenu
            tray::setup_tray(app)?;

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
