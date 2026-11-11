//! Batch conversion and compression with progress and cancellation.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use rayon::prelude::*;
use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use tessera_imaging::{CompressOptions, ConvertOptions, ImageFormat, JobResult, JobStatus};

use crate::{emit_progress, err, images::expand_paths, paths, AppState, CmdResult};

/// Heavy codecs (AVIF, smart search) use a lot of memory per image; keep the
/// pool modest so big batches don't exhaust RAM on small machines.
pub(crate) fn pool() -> rayon::ThreadPool {
    let n = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    rayon::ThreadPoolBuilder::new()
        .num_threads((n / 2).clamp(2, 6))
        .build()
        .expect("thread pool")
}

pub(crate) fn cancelled(input: &Path) -> JobResult {
    JobResult {
        input: input.to_string_lossy().into_owned(),
        output: None,
        status: JobStatus::Skipped,
        input_bytes: std::fs::metadata(input).map(|m| m.len()).unwrap_or(0),
        output_bytes: 0,
        width: 0,
        height: 0,
        format: None,
        quality: None,
        ssim: None,
        message: Some("Cancelled".into()),
        warnings: Vec::new(),
    }
}

fn run_batch<F>(app: &AppHandle, task: &'static str, files: Vec<PathBuf>, job: F) -> Vec<JobResult>
where
    F: Fn(&Path) -> JobResult + Sync,
{
    let state = app.state::<AppState>();
    state.cancel_batch.store(false, Ordering::SeqCst);
    let total = files.len();
    let done = AtomicUsize::new(0);
    emit_progress(app, task, 0, total, None);
    pool().install(|| {
        files
            .par_iter()
            .map(|p| {
                let r = if state.cancel_batch.load(Ordering::SeqCst) {
                    cancelled(p)
                } else {
                    job(p)
                };
                let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                emit_progress(app, task, n, total, Some(p.to_string_lossy().into_owned()));
                r
            })
            .collect()
    })
}

#[tauri::command]
pub async fn convert_images(
    app: AppHandle,
    files: Vec<String>,
    options: ConvertOptions,
) -> CmdResult<Vec<JobResult>> {
    tauri::async_runtime::spawn_blocking(move || {
        let files = expand_paths(&files);
        run_batch(&app, "convert", files, |p| {
            tessera_imaging::convert_file(p, &options)
        })
    })
    .await
    .map_err(err)
}

#[tauri::command]
pub async fn compress_images(
    app: AppHandle,
    files: Vec<String>,
    options: CompressOptions,
) -> CmdResult<Vec<JobResult>> {
    tauri::async_runtime::spawn_blocking(move || {
        let files = expand_paths(&files);
        run_batch(&app, "compress", files, |p| {
            tessera_imaging::compress_file(p, &options)
        })
    })
    .await
    .map_err(err)
}

#[tauri::command]
pub fn cancel_batch(state: State<'_, AppState>) {
    state.cancel_batch.store(true, Ordering::SeqCst);
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompressPreviewInfo {
    pub preview_path: String,
    pub original_preview_path: String,
    pub format: ImageFormat,
    pub quality: Option<u8>,
    pub ssim: Option<f64>,
    pub original_bytes: u64,
    pub bytes: u64,
    pub width: u32,
    pub height: u32,
}

/// Formats the webview can show directly.
fn displayable(f: ImageFormat) -> bool {
    matches!(
        f,
        ImageFormat::Jpeg
            | ImageFormat::Png
            | ImageFormat::Webp
            | ImageFormat::Gif
            | ImageFormat::Bmp
            | ImageFormat::Avif
    )
}

#[tauri::command]
pub async fn compress_preview(
    app: AppHandle,
    path: String,
    options: CompressOptions,
) -> CmdResult<CompressPreviewInfo> {
    tauri::async_runtime::spawn_blocking(move || {
        let src = Path::new(&path);
        let res = tessera_imaging::compress_preview(src, &options).map_err(err)?;
        let dir = paths::previews(&app);
        // One preview at a time: clear older ones.
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                let _ = std::fs::remove_file(e.path());
            }
        }
        let tag = blake3::hash(format!("{path}{:?}", std::time::SystemTime::now()).as_bytes())
            .to_hex()[..12]
            .to_string();
        let out = dir.join(format!("result-{tag}.{}", res.format.extension()));
        std::fs::write(&out, &res.bytes).map_err(err)?;
        let probe = tessera_imaging::probe(src).map_err(err)?;
        let original = if displayable(probe.format) && probe.orientation <= 1 {
            // Let the webview read the original file directly.
            app.asset_protocol_scope().allow_file(src).map_err(err)?;
            src.to_path_buf()
        } else {
            let o = tessera_imaging::make_preview(
                src,
                &dir.join(format!("original-{tag}.png")),
                probe.width.max(probe.height),
                95,
            )
            .map_err(err)?;
            o.path.into()
        };
        Ok(CompressPreviewInfo {
            preview_path: out.to_string_lossy().into_owned(),
            original_preview_path: original.to_string_lossy().into_owned(),
            format: res.format,
            quality: res.quality,
            ssim: res.ssim,
            original_bytes: res.original_bytes,
            bytes: res.bytes.len() as u64,
            width: res.width,
            height: res.height,
        })
    })
    .await
    .map_err(err)?
}
