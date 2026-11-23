//! Native-only commands (Tauri IPC): things a browser can't do.

use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;
use tessera_core::tools::paths;

#[tauri::command]
pub fn reveal_path(app: AppHandle, path: String) -> Result<(), String> {
    app.opener()
        .reveal_item_in_dir(paths::expand(&path))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn open_path(app: AppHandle, path: String) -> Result<(), String> {
    app.opener()
        .open_path(paths::expand(&path).to_string_lossy(), None::<&str>)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn open_url(app: AppHandle, url: String) -> Result<(), String> {
    if !url.starts_with("https://") {
        return Err("only https links can be opened".into());
    }
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| e.to_string())
}

/// Files given on the command line ("Open with Tessera").
#[tauri::command]
pub fn startup_files() -> Vec<String> {
    std::env::args_os()
        .skip(1)
        .map(std::path::PathBuf::from)
        .filter(|p| p.exists())
        .filter_map(|p| p.canonicalize().ok())
        .map(|p| p.to_string_lossy().into_owned())
        .collect()
}

/// Development hook: when TESSERA_AUTOTEST points at a JSON script, the UI
/// runs it on start-up (used for end-to-end smoke tests).
#[tauri::command]
pub fn autotest_script() -> Option<String> {
    let path = std::env::var_os("TESSERA_AUTOTEST")?;
    std::fs::read_to_string(path).ok()
}

/// Development hook: print a line from the smoke-test driver to stdout.
#[tauri::command]
pub fn autotest_log(msg: String) {
    if std::env::var_os("TESSERA_AUTOTEST").is_some() {
        println!("[autotest] {msg}");
    }
}
