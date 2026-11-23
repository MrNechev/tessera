//! Tessera's tools (collage import/export, convert, compress, batch, projects,
//! settings), formerly the desktop app's Tauri commands. They are plain
//! functions taking a [`Tools`] context; [`api`] exposes them over HTTP so the
//! same UI works in the desktop window and in a browser.

pub mod api;
pub mod batch;
pub mod batch_tools;
pub mod events;
pub mod export;
pub mod guard;
pub mod images;
pub mod paths;
pub mod projects;
pub mod settings;
pub mod system;
pub mod transfer;

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Mutex, RwLock};

use serde::Serialize;

pub use events::{EventHub, Progress};
pub use guard::{Area, AreaKind, Guard};

/// Where the tools run: decides defaults (save folder, update checks) and how
/// strictly paths are checked (see [`guard`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// The desktop app: the user's own files, anywhere on disk.
    Desktop,
    /// The self-hosted web app: only configured libraries, Outputs, Inbox and uploads.
    Server,
}

/// Folders the tools use.
#[derive(Clone, Debug)]
pub struct ToolsConfig {
    pub mode: Mode,
    /// Projects (`projects/`) live here.
    pub data_dir: PathBuf,
    /// Settings (`settings.json`).
    pub config_dir: PathBuf,
    /// Proxies, thumbnails, previews, watermarks (safe to delete).
    pub cache_dir: PathBuf,
    /// Default save folder (desktop: `~/Pictures/Tessera`; server: the Outputs volume).
    pub outputs_dir: PathBuf,
    /// Temporary uploads from browsers (server mode).
    pub uploads_dir: PathBuf,
    /// Uploads the user chose to keep (server mode).
    pub inbox_dir: Option<PathBuf>,
    /// Largest single upload in bytes (None = unlimited).
    pub max_upload: Option<u64>,
}

/// Shared state of the tools.
pub struct Tools {
    pub cfg: ToolsConfig,
    pub events: EventHub,
    pub guard: Guard,
    pub exports: Mutex<export::Jobs>,
    pub cancel_batch: AtomicBool,
    on_settings: RwLock<Option<SettingsHook>>,
}

/// Called after settings are saved (the desktop app reconfigures its libraries).
pub type SettingsHook = Box<dyn Fn(&settings::Settings) + Send + Sync>;

impl Tools {
    pub fn new(cfg: ToolsConfig) -> Tools {
        for d in [
            &cfg.data_dir,
            &cfg.config_dir,
            &cfg.cache_dir,
            &cfg.uploads_dir,
        ] {
            let _ = std::fs::create_dir_all(d);
        }
        let guard = Guard::new(cfg.mode);
        let tools = Tools {
            events: EventHub::new(),
            guard,
            exports: Mutex::default(),
            cancel_batch: AtomicBool::new(false),
            on_settings: RwLock::new(None),
            cfg,
        };
        tools.guard.set_internal(vec![
            tools.cfg.cache_dir.clone(),
            tools.cfg.data_dir.join("projects"),
            tools.cfg.uploads_dir.clone(),
        ]);
        tools
    }

    pub fn on_settings_saved(&self, hook: SettingsHook) {
        *self.on_settings.write().unwrap_or_else(|e| e.into_inner()) = Some(hook);
    }

    fn settings_saved(&self, s: &settings::Settings) {
        if let Some(h) = self
            .on_settings
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            h(s);
        }
    }

    pub fn progress(&self, task: &'static str, done: usize, total: usize, current: Option<String>) {
        self.events.progress(Progress {
            task,
            done,
            total,
            current,
        });
    }
}

/// Library id of the Outputs folder (everything Tessera makes).
pub const OUTPUTS_ID: &str = "outputs";
/// Library id of the Inbox folder (uploads the user kept; server mode).
pub const INBOX_ID: &str = "inbox";

/// The guard's areas for the engine's libraries (Outputs and Inbox are libraries too).
pub fn areas_for(libs: &[crate::Library]) -> Vec<Area> {
    libs.iter()
        .map(|l| Area {
            id: l.id.clone(),
            name: l.name.clone(),
            root: l.root.clone(),
            writable: l.writable,
            kind: match l.id.as_str() {
                OUTPUTS_ID => AreaKind::Outputs,
                INBOX_ID => AreaKind::Inbox,
                _ => AreaKind::Library,
            },
        })
        .collect()
}

/// Errors cross the API boundary as user-presentable strings.
pub type CmdResult<T> = Result<T, String>;

pub fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}
