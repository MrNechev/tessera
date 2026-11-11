//! Native printing. On Linux the WebKitGTK print operation is opened with a
//! page setup that matches the project's paper size (CSS `@page` sizes are
//! ignored by WebKitGTK), so every page lands on exactly one sheet. Other
//! platforms report `false` and the frontend falls back to `window.print()`.

use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PrintResult {
    /// Printing started; a `print-finished` event follows.
    Printing,
    /// The user closed the dialog.
    Cancelled,
    /// No native print path on this platform.
    #[cfg_attr(target_os = "linux", allow(dead_code))]
    Unsupported,
}

/// Opens the print dialog for the current webview content.
#[tauri::command]
pub async fn print_native(
    window: tauri::WebviewWindow,
    width_mm: f64,
    height_mm: f64,
    name: String,
) -> Result<PrintResult, String> {
    imp::print(window, width_mm, height_mm, name).await
}

#[cfg(target_os = "linux")]
mod imp {
    use super::PrintResult;
    use gtk::prelude::*;
    use std::cell::RefCell;
    use tauri::Emitter;
    use webkit2gtk::{PrintOperation, PrintOperationExt, PrintOperationResponse};

    thread_local! {
        /// The last printer/settings chosen, so the dialog remembers them.
        static SETTINGS: RefCell<Option<gtk::PrintSettings>> = const { RefCell::new(None) };
    }

    pub async fn print(
        window: tauri::WebviewWindow,
        width_mm: f64,
        height_mm: f64,
        name: String,
    ) -> Result<PrintResult, String> {
        if !(width_mm > 1.0 && height_mm > 1.0) {
            return Err("invalid page size".into());
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let app = tauri::Manager::app_handle(&window).clone();
        window
            .with_webview(move |wv| {
                let view = wv.inner();
                let op = PrintOperation::new(&view);

                // GTK wants the paper in portrait and the orientation separately.
                let (short, long) = (width_mm.min(height_mm), width_mm.max(height_mm));
                let paper = gtk::PaperSize::new_custom(
                    "tessera",
                    &format!("{name} ({width_mm:.0} × {height_mm:.0} mm)"),
                    short,
                    long,
                    gtk::Unit::Mm,
                );
                let setup = gtk::PageSetup::new();
                setup.set_paper_size(&paper);
                setup.set_orientation(if width_mm > height_mm {
                    gtk::PageOrientation::Landscape
                } else {
                    gtk::PageOrientation::Portrait
                });
                for unit_margin in [
                    gtk::PageSetup::set_top_margin,
                    gtk::PageSetup::set_bottom_margin,
                    gtk::PageSetup::set_left_margin,
                    gtk::PageSetup::set_right_margin,
                ] {
                    unit_margin(&setup, 0.0, gtk::Unit::Mm);
                }
                op.set_page_setup(&setup);
                SETTINGS.with(|s| {
                    if let Some(settings) = s.borrow().as_ref() {
                        op.set_print_settings(settings);
                    }
                });

                let done = app.clone();
                op.connect_finished(move |_| {
                    let _ = done.emit("print-finished", true);
                });
                let failed = app.clone();
                op.connect_failed(move |_, err| {
                    let _ = failed.emit("print-failed", err.to_string());
                });

                let parent = view
                    .toplevel()
                    .and_then(|w| w.downcast::<gtk::Window>().ok());
                let response = op.run_dialog(parent.as_ref());
                let result = match response {
                    PrintOperationResponse::Print => {
                        if let Some(settings) = op.print_settings() {
                            SETTINGS.with(|s| *s.borrow_mut() = Some(settings));
                        }
                        PrintResult::Printing
                    }
                    _ => PrintResult::Cancelled,
                };
                let _ = tx.send(result);
            })
            .map_err(|e| e.to_string())?;
        tauri::async_runtime::spawn_blocking(move || rx.recv())
            .await
            .map_err(|e| e.to_string())?
            .map_err(|_| "the print dialog closed unexpectedly".to_string())
    }
}

#[cfg(not(target_os = "linux"))]
mod imp {
    use super::PrintResult;

    pub async fn print(
        _window: tauri::WebviewWindow,
        _width_mm: f64,
        _height_mm: f64,
        _name: String,
    ) -> Result<PrintResult, String> {
        Ok(PrintResult::Unsupported)
    }
}
