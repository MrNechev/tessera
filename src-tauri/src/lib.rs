//! Tessera desktop application: Tauri shell and command layer.
//!
//! All pixel work lives in the `tessera-imaging` crate; this crate wires it to
//! the UI (commands, progress events, file locations, settings, projects).

mod batch;
mod export;
mod images;
mod paths;
mod projects;
mod settings;
mod system;

use std::sync::atomic::AtomicBool;
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter};

/// Progress notification sent to the UI on `tessera://progress`.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub task: &'static str,
    pub done: usize,
    pub total: usize,
    pub current: Option<String>,
}

pub fn emit_progress(
    app: &AppHandle,
    task: &'static str,
    done: usize,
    total: usize,
    current: Option<String>,
) {
    let _ = app.emit(
        "tessera://progress",
        Progress {
            task,
            done,
            total,
            current,
        },
    );
}

/// Shared, mutable application state.
#[derive(Default)]
pub struct AppState {
    pub exports: Mutex<export::Jobs>,
    pub cancel_batch: AtomicBool,
}

/// Errors cross the IPC boundary as user-presentable strings.
pub type CmdResult<T> = Result<T, String>;

pub fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            system::app_info,
            system::reveal_path,
            system::open_path,
            system::open_url,
            system::ensure_dir,
            system::check_updates,
            system::cache_size,
            system::clear_cache,
            system::autotest_script,
            system::autotest_log,
            system::startup_files,
            settings::load_settings,
            settings::save_settings,
            images::import_images,
            images::probe_files,
            images::load_image_rgba,
            images::read_metadata,
            export::export_begin,
            export::export_tile,
            export::export_finish,
            export::export_cancel,
            batch::convert_images,
            batch::compress_images,
            batch::compress_preview,
            batch::cancel_batch,
            projects::list_projects,
            projects::load_project,
            projects::save_project,
            projects::save_project_file,
            projects::delete_project,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Tessera");
}
