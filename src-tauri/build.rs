fn main() {
    // The window loads the UI from the engine's own http://127.0.0.1 origin, so
    // the app's commands need explicit permissions (see capabilities/default.json).
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "reveal_path",
            "open_path",
            "open_url",
            "startup_files",
            "autotest_script",
            "autotest_log",
            "print_native",
        ]),
    ))
    .expect("failed to run tauri-build");
}
