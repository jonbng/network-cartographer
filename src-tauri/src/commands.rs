use std::net::IpAddr;
use std::sync::Arc;

use tauri::State;

use crate::dto::{SettingsDto, SnapshotDto};
use crate::history;
use crate::monitor::Monitor;

#[tauri::command]
pub fn get_snapshot(monitor: State<'_, Arc<Monitor>>) -> SnapshotDto {
    monitor.snapshot()
}

#[tauri::command]
pub fn refresh_now(monitor: State<'_, Arc<Monitor>>) -> Result<SnapshotDto, String> {
    monitor.tick()
}

#[tauri::command]
pub fn get_settings(monitor: State<'_, Arc<Monitor>>) -> SettingsDto {
    monitor.settings.lock().clone()
}

#[tauri::command]
pub fn set_settings(monitor: State<'_, Arc<Monitor>>, settings: SettingsDto) -> SettingsDto {
    monitor.apply_settings(settings);
    monitor.settings.lock().clone()
}

#[tauri::command]
pub fn reset_monitor(monitor: State<'_, Arc<Monitor>>) {
    monitor.reset();
}

#[tauri::command]
pub fn force_trace(monitor: State<'_, Arc<Monitor>>, ip: String) -> Result<(), String> {
    let addr: IpAddr = ip.parse().map_err(|e| format!("invalid ip: {e}"))?;
    monitor.traces.lock().force(addr);
    Ok(())
}

#[tauri::command]
pub fn force_trace_all(monitor: State<'_, Arc<Monitor>>) {
    let ips = monitor.state.lock().unique_remote_ips();
    monitor.traces.lock().force_many(ips);
}

#[tauri::command]
pub fn list_sessions() -> Result<Vec<String>, String> {
    history::list_sessions()
}

#[tauri::command]
pub fn record_sni(monitor: State<'_, Arc<Monitor>>, ip: String, sni: String) -> Result<(), String> {
    let addr: IpAddr = ip.parse().map_err(|e| format!("invalid ip: {e}"))?;
    // Cache + apply (hook for future pcap / external feed)
    monitor.sni.insert(addr, sni.clone());
    monitor.state.lock().apply_sni(addr, &sni);
    Ok(())
}
