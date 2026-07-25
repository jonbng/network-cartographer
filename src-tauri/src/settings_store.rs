//! Persist settings to the user config dir.

use std::fs;
use std::path::PathBuf;

use crate::dto::SettingsDto;

fn path() -> Option<PathBuf> {
    let dir = dirs::config_dir()?.join("hopglobe");
    let _ = fs::create_dir_all(&dir);
    Some(dir.join("settings.json"))
}

pub fn load() -> Option<SettingsDto> {
    let p = path()?;
    let raw = fs::read_to_string(p).ok()?;
    serde_json::from_str(&raw).ok()
}

pub fn save(settings: &SettingsDto) -> Result<(), String> {
    let p = path().ok_or_else(|| "no config dir".to_string())?;
    let raw = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    fs::write(p, raw).map_err(|e| e.to_string())
}
