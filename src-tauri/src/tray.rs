//! System tray (best-effort). Soft-fails if tray unavailable.

use std::sync::Arc;
use std::sync::Mutex;

use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, Runtime,
};

use crate::monitor::Monitor;

pub fn setup<R: Runtime>(app: &AppHandle<R>, monitor: Arc<Monitor>) -> Result<(), String> {
    let show_i = MenuItem::with_id(app, "show", "Show hopglobe", true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let quit_i =
        MenuItem::with_id(app, "quit", "Quit", true, None::<&str>).map_err(|e| e.to_string())?;
    let menu = Menu::with_items(app, &[&show_i, &quit_i]).map_err(|e| e.to_string())?;

    let mon = Arc::clone(&monitor);
    let tray = TrayIconBuilder::new()
        .icon(app.default_window_icon().cloned().ok_or("no icon")?)
        .menu(&menu)
        .tooltip("hopglobe")
        .on_menu_event(move |app, event| match event.id.as_ref() {
            "quit" => app.exit(0),
            "show" => {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }
        })
        .build(app)
        .map_err(|e| e.to_string())?;

    // Keep tray handle for periodic tooltip refresh
    let tray = Arc::new(Mutex::new(tray));
    let tray_h = Arc::clone(&tray);
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_secs(5));
        let snap = mon.snapshot();
        let tip = format!(
            "hopglobe · {} apps · {} dests",
            snap.app_count, snap.dest_count
        );
        if let Ok(t) = tray_h.lock() {
            let _ = t.set_tooltip(Some(&tip));
        }
    });

    Ok(())
}
