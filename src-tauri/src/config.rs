// Persisted app config: last connected device, for auto-reconnect on startup.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LastDevice {
    pub address: u64,
    pub name: String,
}

pub fn config_path() -> PathBuf {
    let base = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(base).join("xiaomibuds-desktop.json")
}

pub fn load_last_device() -> Option<LastDevice> {
    let text = fs::read_to_string(config_path()).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn save_last_device(device: &LastDevice) {
    if let Ok(text) = serde_json::to_string_pretty(device) {
        let _ = fs::write(config_path(), text);
    }
}
