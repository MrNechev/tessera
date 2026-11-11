//! Batch conversion and compression with progress and cancellation.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::SystemTime;

use rayon::prelude::*;
use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use tessera_imaging::{CompressOptions, ConvertOptions, ImageFormat, JobResult, JobStatus};

use crate::images::{asset_id, expand_paths};
use crate::{emit_progress, err, paths, AppState, CmdResult};

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

/// Compression results kept in the previews folder. Older ones are deleted;
/// the newest few survive so concurrent requests never delete each other's
/// fresh file (the UI ignores stale responses).
const KEEP_RESULTS: usize = 8;
/// Cached full-size renderings of originals the webview can't show directly.
const KEEP_ORIGINALS: usize = 8;

static PREVIEW_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Delete all but the `keep` newest files named `<prefix>…` in `dir`, never
/// touching `current`.
fn prune(dir: &Path, prefix: &str, keep: usize, current: &Path) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    let mut files: Vec<(SystemTime, PathBuf)> = rd
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with(prefix))
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    files.sort_by_key(|f| std::cmp::Reverse(f.0));
    for (_, p) in files.into_iter().skip(keep) {
        if p != current {
            let _ = fs::remove_file(p);
        }
    }
}

/// Unique (per request) result file name.
fn result_name(path: &str, ext: &str) -> String {
    let n = PREVIEW_COUNTER.fetch_add(1, Ordering::Relaxed);
    let tag = blake3::hash(format!("{path}{:?}{n}", SystemTime::now()).as_bytes()).to_hex();
    format!("result-{}-{n}.{ext}", &tag[..12])
}

/// Cached rendering of `src` (keyed by path, size and mtime), made on the
/// first request only.
fn original_preview(dir: &Path, src: &Path, max_side: u32) -> CmdResult<PathBuf> {
    let key = asset_id(src);
    let hit = ["jpg", "png"]
        .iter()
        .map(|e| dir.join(format!("original-{key}.{e}")))
        .find(|p| fs::metadata(p).is_ok_and(|m| m.is_file() && m.len() > 0));
    let path = match hit {
        Some(p) => {
            // Mark as recently used so pruning keeps it.
            let _ = filetime::set_file_mtime(&p, filetime::FileTime::now());
            p
        }
        None => tessera_imaging::make_preview(
            src,
            &dir.join(format!("original-{key}.png")),
            max_side,
            95,
        )
        .map_err(err)?
        .path
        .into(),
    };
    prune(dir, "original-", KEEP_ORIGINALS, &path);
    Ok(path)
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
        let out = dir.join(result_name(&path, res.format.extension()));
        fs::write(&out, &res.bytes).map_err(err)?;
        prune(&dir, "result-", KEEP_RESULTS, &out);
        let probe = tessera_imaging::probe(src).map_err(err)?;
        let original = if displayable(probe.format) && probe.orientation <= 1 {
            // Let the webview read the original file directly.
            app.asset_protocol_scope().allow_file(src).map_err(err)?;
            src.to_path_buf()
        } else {
            original_preview(&dir, src, probe.width.max(probe.height))?
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prune_keeps_the_newest_and_the_current_file() {
        let dir = std::env::temp_dir().join(format!("tessera-prune-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let base = filetime::FileTime::from_unix_time(1_700_000_000, 0);
        let mut files = Vec::new();
        for i in 0..5i64 {
            let p = dir.join(format!("result-{i}.jpg"));
            fs::write(&p, b"x").unwrap();
            let t = filetime::FileTime::from_unix_time(base.unix_seconds() + i, 0);
            filetime::set_file_mtime(&p, t).unwrap();
            files.push(p);
        }
        fs::write(dir.join("original-a.png"), b"x").unwrap();
        // The oldest file is the one this request just wrote: it survives.
        prune(&dir, "result-", 2, &files[0]);
        let mut left: Vec<String> = fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        assert_eq!(
            left,
            vec![
                "original-a.png",
                "result-0.jpg",
                "result-3.jpg",
                "result-4.jpg"
            ]
        );
    }

    #[test]
    fn result_names_are_unique() {
        let a = result_name("/x.jpg", "jpg");
        let b = result_name("/x.jpg", "jpg");
        assert_ne!(a, b);
        assert!(a.starts_with("result-") && a.ends_with(".jpg"));
    }
}
