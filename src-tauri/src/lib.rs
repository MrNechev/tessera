//! Tessera desktop app: a thin shell around `tessera-core`.
//!
//! At start-up it runs Tessera's engine as a private web server on
//! `127.0.0.1:<random port>` (protected by a per-launch secret token) and
//! opens the window on it. The UI and its media then load exactly as they do
//! in a browser talking to the Docker server. Only things a browser can't do
//! stay native, over Tauri IPC: file dialogs, OS drag-and-drop paths, GTK
//! printing, opening files/folders/links, window state.

mod desktop;
mod print;
mod server;

use std::sync::atomic::{AtomicU32, Ordering};

use tauri::webview::NewWindowResponse;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_opener::OpenerExt;

static WINDOWS: AtomicU32 = AtomicU32::new(0);

/// `window.open` from the UI: our own pages (speaker notes, print layouts,
/// presentations) open as Tessera windows; web links open in the browser.
fn new_window(
    app: &AppHandle,
    url: url::Url,
    features: tauri::webview::NewWindowFeatures,
) -> NewWindowResponse<tauri::Wry> {
    let local = matches!(url.host_str(), Some("127.0.0.1") | Some("localhost"));
    if !local {
        if url.scheme() == "https" || url.scheme() == "http" {
            let _ = app.opener().open_url(url.as_str(), None::<&str>);
        }
        return NewWindowResponse::Deny;
    }
    let label = format!("w{}", WINDOWS.fetch_add(1, Ordering::Relaxed) + 1);
    let built = WebviewWindowBuilder::new(app, label, WebviewUrl::External(url))
        .window_features(features)
        .title("Tessera")
        .on_document_title_changed(|w, title| {
            let _ = w.set_title(&title);
        })
        .build();
    match built {
        Ok(window) => NewWindowResponse::Create { window },
        Err(e) => {
            log::error!("could not open a window: {e}");
            NewWindowResponse::Deny
        }
    }
}

/// AppImages bundle libheif's HEVC decoder plugin (for HEIC photos) inside
/// the image; tell libheif where to find it. Must run before any thread starts.
fn configure_bundled_libheif() {
    if std::env::var_os("LIBHEIF_PLUGIN_PATH").is_some() {
        return;
    }
    if let Some(dir) = std::env::var_os("APPDIR") {
        let plugins = std::path::Path::new(&dir).join("usr/lib/libheif/plugins");
        if plugins.is_dir() {
            std::env::set_var("LIBHEIF_PLUGIN_PATH", plugins);
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    configure_bundled_libheif();
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("warn,rawler=error"),
    )
    .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .setup(|app| {
            let handle = app.handle().clone();
            let started = tauri::async_runtime::block_on(server::start(handle))?;
            let url = started.window_url();
            app.manage(started);
            let h = app.handle().clone();
            WebviewWindowBuilder::new(app, "main", WebviewUrl::External(url))
                .on_new_window(move |url, features| new_window(&h, url, features))
                .title("Tessera")
                .inner_size(1440.0, 900.0)
                .min_inner_size(1100.0, 700.0)
                .center()
                .background_color(tauri::window::Color(0x16, 0x14, 0x1d, 0xff))
                .build()?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            desktop::reveal_path,
            desktop::open_path,
            desktop::open_url,
            desktop::startup_files,
            desktop::autotest_script,
            desktop::autotest_log,
            print::print_native,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Tessera");
}
