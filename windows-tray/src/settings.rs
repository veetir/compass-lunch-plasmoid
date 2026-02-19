use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub restaurant_code: String,
    pub language: String,
    pub refresh_minutes: u32,
    pub show_prices: bool,
    pub dark_mode: bool,
    pub hide_allergens: bool,
    pub last_updated_epoch_ms: i64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            restaurant_code: "0437".to_string(),
            language: "fi".to_string(),
            refresh_minutes: 1440,
            show_prices: false,
            dark_mode: true,
            hide_allergens: true,
            last_updated_epoch_ms: 0,
        }
    }
}

pub fn settings_dir() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".to_string());
    Path::new(&base).join("compass-lunch")
}

pub fn settings_path() -> PathBuf {
    settings_dir().join("settings.json")
}

pub fn load_settings() -> Settings {
    let path = settings_path();
    match fs::read_to_string(&path) {
        Ok(data) => serde_json::from_str(&data).unwrap_or_default(),
        Err(_) => Settings::default(),
    }
}

pub fn save_settings(settings: &Settings) -> anyhow::Result<()> {
    let dir = settings_dir();
    fs::create_dir_all(&dir)?;
    let data = serde_json::to_string_pretty(settings)?;
    fs::write(dir.join("settings.json"), data)?;
    Ok(())
}
