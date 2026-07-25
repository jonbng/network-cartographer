//! Optional session history (JSONL under app data dir).

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

use crate::dto::SnapshotDto;

fn dir() -> Option<PathBuf> {
    let d = dirs::data_local_dir()?.join("hopglobe").join("history");
    let _ = fs::create_dir_all(&d);
    Some(d)
}

fn today_file() -> Option<PathBuf> {
    let d = dir()?;
    let day = chrono_lite_date();
    Some(d.join(format!("{day}.jsonl")))
}

fn chrono_lite_date() -> String {
    // Avoid extra chrono dep: use system time via format
    use std::time::SystemTime;
    let secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // rough UTC date
    let days = secs / 86400;
    let (y, m, d) = civil_from_days(days as i64);
    format!("{y:04}-{m:02}-{d:02}")
}

// Howard Hinnant civil_from_days
fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m as u32, d as u32)
}

pub fn append_snapshot(snap: &SnapshotDto) -> Result<(), String> {
    let p = today_file().ok_or_else(|| "no data dir".to_string())?;
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(p)
        .map_err(|e| e.to_string())?;
    let line = serde_json::to_string(snap).map_err(|e| e.to_string())?;
    writeln!(f, "{line}").map_err(|e| e.to_string())
}

pub fn list_sessions() -> Result<Vec<String>, String> {
    let d = dir().ok_or_else(|| "no data dir".to_string())?;
    let mut names = Vec::new();
    if let Ok(rd) = fs::read_dir(d) {
        for e in rd.flatten() {
            if let Some(n) = e.file_name().to_str() {
                if n.ends_with(".jsonl") {
                    names.push(n.to_string());
                }
            }
        }
    }
    names.sort();
    names.reverse();
    Ok(names)
}
