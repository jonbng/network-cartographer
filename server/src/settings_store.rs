//! Persist settings to the user config dir.

use std::fs;
use std::path::PathBuf;

use crate::dto::SettingsDto;

fn path() -> Option<PathBuf> {
    let dir = dirs::config_dir()?.join("network-cartographer");
    let _ = fs::create_dir_all(&dir);
    Some(dir.join("settings.json"))
}

pub fn load() -> Option<SettingsDto> {
    let p = path()?;
    let raw = fs::read_to_string(p).ok()?;
    let (settings, migrated) = decode(&raw)?;
    if migrated {
        let _ = save(&settings);
    }
    Some(settings)
}

fn decode(raw: &str) -> Option<(SettingsDto, bool)> {
    let value: serde_json::Value = serde_json::from_str(raw).ok()?;
    let old_version = value
        .get("settingsVersion")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(1);
    let mut settings: SettingsDto = serde_json::from_value(value).ok()?;
    let migrated = old_version < 2;
    if migrated {
        // includeUdp existed as a dormant, forcibly-false placeholder. Enable
        // the feature once now that the setting has real behavior.
        settings.include_udp = true;
        settings.settings_version = 2;
    }
    Some((settings, migrated))
}

pub fn save(settings: &SettingsDto) -> Result<(), String> {
    let p = path().ok_or_else(|| "no config dir".to_string())?;
    let raw = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    fs::write(p, raw).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    #[test]
    fn legacy_placeholder_migrates_to_enabled_udp_once() {
        let (settings, migrated) = super::decode(r#"{"includeUdp":false}"#).unwrap();
        assert!(migrated);
        assert!(settings.include_udp);
        assert_eq!(settings.settings_version, 2);
    }

    #[test]
    fn versioned_udp_opt_out_is_preserved() {
        let (settings, migrated) =
            super::decode(r#"{"settingsVersion":2,"includeUdp":false}"#).unwrap();
        assert!(!migrated);
        assert!(!settings.include_udp);
    }
}
