mod collect;
mod commands;
mod dto;
mod geo;
mod history;
mod model;
mod monitor;
mod privileges;
mod resolve;
mod settings_store;
mod trace;
mod tray;

use std::sync::Arc;

use monitor::{spawn_poll_loop, Monitor};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let monitor = Arc::new(Monitor::new());

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(monitor.clone())
        .setup(move |app| {
            let _ = tray::setup(app.handle(), Arc::clone(&monitor));
            spawn_poll_loop(app.handle().clone(), monitor);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_snapshot,
            commands::refresh_now,
            commands::get_settings,
            commands::set_settings,
            commands::reset_monitor,
            commands::force_trace,
            commands::force_trace_all,
            commands::list_sessions,
            commands::record_sni,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
