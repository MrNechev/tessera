//! Collage export: the UI renders tiles and streams raw RGBA here; we assemble
//! the full image and encode it to the requested format.

use std::collections::HashMap;
use std::time::Instant;

use image::{DynamicImage, RgbaImage};
use serde::Serialize;
use tauri::ipc::{InvokeBody, Request};
use tauri::State;
use tessera_imaging::{Chroma, EncodeOptions, ImageFormat};

use crate::{err, paths, AppState, CmdResult};

/// Largest export we accept (≈ 200 MP, 800 MB of RGBA).
const MAX_PIXELS: u64 = 200_000_000;

pub struct Job {
    image: RgbaImage,
    started: Instant,
}

#[derive(Default)]
pub struct Jobs {
    next: u32,
    jobs: HashMap<u32, Job>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub path: String,
    pub bytes: u64,
    pub width: u32,
    pub height: u32,
}

#[tauri::command]
pub fn export_begin(state: State<'_, AppState>, width: u32, height: u32) -> CmdResult<u32> {
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err(format!("Unsupported export size {width}×{height}"));
    }
    let mut jobs = state.exports.lock().map_err(err)?;
    // Drop abandoned jobs (e.g. the UI was reloaded mid-export).
    jobs.jobs.retain(|_, j| j.started.elapsed().as_secs() < 3600);
    jobs.next = jobs.next.wrapping_add(1);
    let id = jobs.next;
    jobs.jobs.insert(id, Job { image: RgbaImage::new(width, height), started: Instant::now() });
    Ok(id)
}

fn header<'a>(req: &'a Request<'_>, name: &str) -> CmdResult<&'a str> {
    req.headers()
        .get(name)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| format!("missing header {name}"))
}

#[tauri::command]
pub fn export_tile(state: State<'_, AppState>, request: Request<'_>) -> CmdResult<()> {
    let job: u32 = header(&request, "x-job")?.parse().map_err(err)?;
    let rect: Vec<u32> = header(&request, "x-rect")?
        .split(',')
        .map(|v| v.trim().parse::<u32>())
        .collect::<Result<_, _>>()
        .map_err(err)?;
    let [x, y, w, h] = rect[..] else {
        return Err("bad tile rectangle".into());
    };
    let InvokeBody::Raw(data) = request.body() else {
        return Err("tile must be sent as raw bytes".into());
    };
    if data.len() != (w as usize) * (h as usize) * 4 {
        return Err(format!("tile has {} bytes, expected {}", data.len(), w * h * 4));
    }
    let mut jobs = state.exports.lock().map_err(err)?;
    let job = jobs.jobs.get_mut(&job).ok_or("unknown export job")?;
    let (iw, ih) = job.image.dimensions();
    if x + w > iw || y + h > ih {
        return Err("tile outside the image".into());
    }
    let stride = iw as usize * 4;
    let buf: &mut [u8] = &mut job.image;
    for row in 0..h as usize {
        let src = &data[row * w as usize * 4..(row + 1) * w as usize * 4];
        let off = (y as usize + row) * stride + x as usize * 4;
        buf[off..off + w as usize * 4].copy_from_slice(src);
    }
    Ok(())
}

#[tauri::command]
pub fn export_cancel(state: State<'_, AppState>, job: u32) -> CmdResult<()> {
    state.exports.lock().map_err(err)?.jobs.remove(&job);
    Ok(())
}

#[tauri::command]
pub async fn export_finish(
    state: State<'_, AppState>,
    job: u32,
    path: String,
    format: String,
    quality: u8,
    dpi: u32,
    unique: bool,
) -> CmdResult<ExportResult> {
    let job = state.exports.lock().map_err(err)?.jobs.remove(&job).ok_or("unknown export job")?;
    tauri::async_runtime::spawn_blocking(move || {
        let img = job.image;
        let (width, height) = img.dimensions();
        let bytes = if format == "pdf" {
            tessera_imaging::rgba_to_pdf(&img, dpi.max(1), quality.clamp(1, 100)).map_err(err)?
        } else {
            let fmt = ImageFormat::from_extension(&format).ok_or_else(|| format!("Unsupported format {format}"))?;
            let opts = EncodeOptions {
                format: fmt,
                quality: quality.clamp(1, 100),
                lossless: matches!(fmt, ImageFormat::Png | ImageFormat::Tiff),
                progressive: true,
                chroma: Chroma::Auto,
                // Exports can be large; favour reasonable speed.
                effort: if matches!(fmt, ImageFormat::Avif) { 35 } else { 50 },
                dpi: Some(dpi.max(1)),
                background: [255, 255, 255],
            };
            tessera_imaging::encode(&DynamicImage::ImageRgba8(img), &opts).map_err(err)?
        };
        let mut target = paths::expand(&path);
        if unique {
            target = paths::unique_path(&target);
        }
        paths::write_atomic(&target, &bytes).map_err(|e| format!("Could not write {}: {e}", target.display()))?;
        Ok(ExportResult { path: target.to_string_lossy().into_owned(), bytes: bytes.len() as u64, width, height })
    })
    .await
    .map_err(err)?
}

