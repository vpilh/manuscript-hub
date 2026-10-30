//! The tray icon (FR21): the app keeps checking mail while its window is
//! closed, and is opened, paused or quit from here.

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};

use crate::mail;

/// The pause entry, whose label follows the pause state.
pub struct PauseItem(MenuItem<Wry>);

pub fn pause_label(paused: bool) -> &'static str {
    if paused {
        "Resume automatic checks"
    } else {
        "Pause automatic checks"
    }
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Manuscript Hub", true, None::<&str>)?;
    let check = MenuItem::with_id(app, "check", "Check mailboxes now", true, None::<&str>)?;
    let pause = MenuItem::with_id(app, "pause", pause_label(false), true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Manuscript Hub", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &check, &pause, &quit])?;
    app.manage(PauseItem(pause));

    let mut tray = TrayIconBuilder::with_id("main")
        .tooltip("Manuscript Hub")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_window(app),
            "check" => mail::check_all_now(app.clone()),
            "pause" => mail::set_paused(app, !mail::is_paused(app)),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_window(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

/// Updates the tray's pause entry after the state changed elsewhere.
pub fn show_paused(app: &AppHandle, paused: bool) {
    if let Some(item) = app.try_state::<PauseItem>() {
        let _ = item.0.set_text(pause_label(paused));
    }
}

pub fn show_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}
