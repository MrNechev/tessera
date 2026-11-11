//! User settings, stored as JSON in the config directory.

use std::fs;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::{err, paths, CmdResult};

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub save_directory: String,
    pub filename_pattern: String,
    pub export_format: String,
    pub export_quality: u8,
    pub theme: String,
    pub check_updates: bool,
    pub last_update_check: Option<String>,
    pub default_preset: String,
    pub default_dpi: u32,
    pub default_orientation: String,
    pub recent_import_dir: Option<String>,
    pub show_welcome: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            save_directory: paths::default_save_dir().to_string_lossy().into_owned(),
            filename_pattern: "{name} {date} {time}".into(),
            export_format: "jpeg".into(),
            export_quality: 92,
            theme: "system".into(),
            check_updates: true,
            last_update_check: None,
            default_preset: "a4".into(),
            default_dpi: 300,
            default_orientation: "landscape".into(),
            recent_import_dir: dirs::picture_dir().map(|p| p.to_string_lossy().into_owned()),
            show_welcome: true,
        }
    }
}

fn file(app: &AppHandle) -> std::path::PathBuf {
    paths::config_dir(app).join("settings.json")
}

#[tauri::command]
pub fn load_settings(app: AppHandle) -> Settings {
    let mut s: Settings = fs::read_to_string(file(&app))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    if s.save_directory.trim().is_empty() {
        s.save_directory = Settings::default().save_directory;
    }
    s
}

/// Async so the fsync in `write_atomic` doesn't run on the main thread.
#[tauri::command]
pub async fn save_settings(app: AppHandle, settings: Settings) -> CmdResult<()> {
    let json = serde_json::to_vec_pretty(&settings).map_err(err)?;
    tauri::async_runtime::spawn_blocking(move || {
        paths::write_atomic(&file(&app), &json).map_err(|e| format!("Could not save settings: {e}"))
    })
    .await
    .map_err(err)?
}
