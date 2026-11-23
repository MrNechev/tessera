//! User settings, stored as JSON in the config directory.

use std::fs;

use super::{err, paths, CmdResult, Mode, Tools};
use serde::{Deserialize, Serialize};

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
    /// Web app: download results to the browser's computer after saving them.
    pub download_results: bool,
    /// Desktop: the folders shown in the Library (media libraries).
    pub libraries: Vec<LibrarySetting>,
    /// Desktop: the first-start setup (choose libraries and the save folder) is done.
    pub setup_done: bool,
}

/// A media library folder chosen in the desktop app.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LibrarySetting {
    /// Stored in documents with every media reference; never changes.
    pub id: String,
    pub name: String,
    pub path: String,
    /// Tessera may write here (e.g. "replace originals"); read-only by default.
    #[serde(default)]
    pub writable: bool,
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
            download_results: true,
            libraries: Vec::new(),
            setup_done: false,
        }
    }
}

impl Settings {
    /// Defaults for this mode: the server saves to its Outputs folder and
    /// can't know the browser's pictures folder.
    pub fn defaults_for(t: &Tools) -> Settings {
        let mut s = Settings::default();
        if t.cfg.mode == Mode::Server {
            s.save_directory = t.cfg.outputs_dir.to_string_lossy().into_owned();
            s.recent_import_dir = None;
            s.check_updates = false;
        }
        s
    }
}

fn file(t: &Tools) -> std::path::PathBuf {
    t.config_dir().join("settings.json")
}

pub fn load_settings(t: &Tools) -> Settings {
    let defaults = Settings::defaults_for(t);
    let mut s: Settings = fs::read_to_string(file(t))
        .ok()
        .and_then(|text| {
            // Missing fields take this mode's defaults.
            let mut v = serde_json::to_value(&defaults).ok()?;
            let saved: serde_json::Value = serde_json::from_str(&text).ok()?;
            if let (Some(base), Some(saved)) = (v.as_object_mut(), saved.as_object()) {
                for (k, val) in saved {
                    base.insert(k.clone(), val.clone());
                }
            }
            serde_json::from_value(v).ok()
        })
        .unwrap_or_else(|| defaults.clone());
    if s.save_directory.trim().is_empty() {
        s.save_directory = defaults.save_directory;
    }
    s
}

pub fn save_settings(t: &Tools, settings: Settings) -> CmdResult<()> {
    let json = serde_json::to_vec_pretty(&settings).map_err(err)?;
    paths::write_atomic(&file(t), &json).map_err(|e| format!("Could not save settings: {e}"))?;
    t.settings_saved(&settings);
    Ok(())
}
