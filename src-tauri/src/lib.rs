mod commands;
mod error;
mod models;
mod services;
mod state;

use state::AppState;
use tauri::Manager;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app_state = AppState::new();

    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
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
            commands::logs::get_log_lines,
            commands::logs::get_log_path,
            commands::logs::clear_logs,
            commands::accounts::load_accounts,
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
            commands::reveal::set_reveal_api_config,
            commands::reveal::test_api_key,
            commands::reveal::get_teams_info,
            commands::reveal::send_chat_message,
        ])
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }

            // Load persisted automation settings and set app handle
            let state = app.state::<AppState>();
            commands::auto_accept::load_persisted_automation_settings(&state);

            let handle = app.handle().clone();
            let auto_accept = state.auto_accept.clone();
            tauri::async_runtime::spawn(async move {
                auto_accept.set_app_handle(handle).await;
            });

            // Set window icon explicitly (for dev mode where EXE resources aren't embedded)
            let app_icon = tauri::image::Image::from_bytes(include_bytes!("../icons/icon.png"))?;
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_icon(app_icon.clone());
            }

            // System tray
            let show_item = MenuItem::with_id(app, "show", "Показать", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Выход", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_item, &quit_item])?;

            let _tray = TrayIconBuilder::new()
                .icon(app_icon)
                .menu(&menu)
                .tooltip("RustLM")
                .on_menu_event(|app, event| {
                    match event.id.as_ref() {
                        "show" => {
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                        "quit" => {
                            app.exit(0);
                        }
                        _ => {}
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(app)?;

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
