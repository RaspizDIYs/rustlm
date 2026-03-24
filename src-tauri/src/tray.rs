use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};

use crate::state::AppState;

/// Build the tray context menu with auto-accept checkbox and accounts submenu.
fn build_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let state = app.state::<AppState>();
    let is_auto_accept_on = state.auto_accept.is_enabled();
    let accounts = state.accounts.load_all();

    let auto_accept_item = CheckMenuItem::with_id(
        app,
        "auto_accept",
        "Автопринятие",
        true,
        is_auto_accept_on,
        None::<&str>,
    )?;

    let mut login_items: Vec<MenuItem<tauri::Wry>> = Vec::new();
    if accounts.is_empty() {
        login_items.push(MenuItem::with_id(
            app,
            "no_accounts",
            "Нет аккаунтов",
            false,
            None::<&str>,
        )?);
    } else {
        for account in &accounts {
            let id = format!("login_{}", account.username);
            let label = if !account.riot_id.is_empty() {
                format!("{} ({})", account.username, account.riot_id)
            } else {
                account.username.clone()
            };
            login_items.push(MenuItem::with_id(app, id, label, true, None::<&str>)?);
        }
    }

    let login_refs: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> =
        login_items.iter().map(|i| i as &dyn tauri::menu::IsMenuItem<tauri::Wry>).collect();
    let login_submenu =
        Submenu::with_id_and_items(app, "login_submenu", "Войти", true, &login_refs)?;

    let separator = PredefinedMenuItem::separator(app)?;
    let show_item = MenuItem::with_id(app, "show", "Показать", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Выход", true, None::<&str>)?;

    Menu::with_items(app, &[
        &auto_accept_item,
        &login_submenu,
        &separator,
        &show_item,
        &quit_item,
    ])
}

/// Create the system tray icon with menu. Called once during app setup.
pub fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    let handle = app.handle();
    let menu = build_menu(handle)?;
    let app_icon = tauri::image::Image::from_bytes(include_bytes!("../icons/icon.png"))?;

    TrayIconBuilder::with_id("main_tray")
        .icon(app_icon)
        .menu(&menu)
        .tooltip("RustLM")
        .on_menu_event(|app, event| {
            handle_menu_event(app, event.id.as_ref());
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
}

/// Rebuild the tray menu to reflect current state (accounts, auto-accept).
pub fn rebuild_tray_menu(app: &AppHandle) -> tauri::Result<()> {
    if let Some(tray) = app.tray_by_id("main_tray") {
        // Remove old menu first to free menu item IDs
        tray.set_menu(None::<Menu<tauri::Wry>>)?;
        let menu = build_menu(app)?;
        tray.set_menu(Some(menu))?;
    }
    Ok(())
}

fn handle_menu_event(app: &AppHandle, id: &str) {
    match id {
        "auto_accept" => {
            let state = app.state::<AppState>();
            let new_enabled = !state.auto_accept.is_enabled();
            let app_handle = app.clone();
            tauri::async_runtime::spawn(async move {
                let state = app_handle.state::<AppState>();
                if let Err(e) =
                    crate::commands::auto_accept::apply_auto_accept_enabled(&state, new_enabled).await
                {
                    log::error!("[Tray] persist auto-accept: {}", e);
                }
            });
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.emit("auto-accept-changed", new_enabled);
            }
        }
        "show" => {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }
        "quit" => {
            app.exit(0);
        }
        id if id.starts_with("login_") => {
            let username = id.strip_prefix("login_").unwrap();
            // Show window and tell frontend to trigger login for this account
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
                let _ = window.emit("tray-login", username);
            }
        }
        _ => {}
    }
}
