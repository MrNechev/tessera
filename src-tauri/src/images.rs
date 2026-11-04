//! Importing photos: folder expansion, probing, cached proxies & thumbnails,
//! and full-quality pixel access for export.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use rayon::prelude::*;
use serde::Serialize;
use tauri::ipc::Response;
use tauri::AppHandle;
use tessera_imaging::{ImageFormat, MetadataSummary, ProbeInfo};

use crate::{emit_progress, err, paths, CmdResult};

/// Long edge of the editor proxy and the library thumbnail.
const PROXY_SIDE: u32 = 2048;
const THUMB_SIDE: u32 = 384;
/// Folders are scanned at most this deep.
const MAX_DEPTH: usize = 8;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ImportedImage {
    pub id: String,
    pub path: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub format: ImageFormat,
    pub file_size: u64,
    pub taken_at: Option<String>,
    pub proxy: String,
    pub thumb: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Failure {
    pub path: String,
    pub error: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub images: Vec<ImportedImage>,
    pub failed: Vec<Failure>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeResult {
    pub files: Vec<ProbeInfo>,
    pub failed: Vec<Failure>,
}

fn is_image(path: &Path, exts: &HashSet<String>) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| exts.contains(&e.to_ascii_lowercase()))
        .unwrap_or(false)
}

fn walk(dir: &Path, depth: usize, exts: &HashSet<String>, out: &mut Vec<PathBuf>) {
    if depth > MAX_DEPTH {
        return;
    }
    let Ok(rd) = fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = rd.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let name = e.file_name();
        if name.to_string_lossy().starts_with('.') {
            continue;
        }
        let p = e.path();
        match e.file_type() {
            Ok(t) if t.is_dir() => walk(&p, depth + 1, exts, out),
            Ok(_) if is_image(&p, exts) => out.push(p),
            _ => {}
        }
    }
}

/// Expand folders into the image files they contain (files are kept as given).
pub fn expand_paths(paths: &[String]) -> Vec<PathBuf> {
    let exts: HashSet<String> = tessera_imaging::input_extensions()
        .into_iter()
        .map(str::to_string)
        .collect();
    let mut out = Vec::new();
    for p in paths {
        let p = paths::expand(p);
        if p.is_dir() {
            walk(&p, 0, &exts, &mut out);
        } else {
            out.push(p);
        }
    }
    let mut seen = HashSet::new();
    out.retain(|p| seen.insert(p.clone()));
    out
}

/// Stable id for a file version: changes when the file is modified.
fn asset_id(path: &Path) -> String {
    let meta = fs::metadata(path).ok();
    let mut h = blake3::Hasher::new();
    h.update(path.to_string_lossy().as_bytes());
    if let Some(m) = &meta {
        h.update(&m.len().to_le_bytes());
        if let Ok(t) = m.modified() {
            if let Ok(d) = t.duration_since(std::time::UNIX_EPOCH) {
                h.update(&d.as_nanos().to_le_bytes());
            }
        }
    }
    h.finalize().to_hex()[..16].to_string()
}

fn cached(dir: &Path, id: &str) -> Option<PathBuf> {
    ["jpg", "png"]
        .iter()
        .map(|e| dir.join(format!("{id}.{e}")))
        .find(|p| p.exists())
}

fn import_one(app: &AppHandle, path: &Path) -> Result<ImportedImage, String> {
    let info = tessera_imaging::probe(path).map_err(err)?;
    let id = asset_id(path);
    let proxy = match cached(&paths::proxies(app), &id) {
        Some(p) => p,
        None => {
            let res = tessera_imaging::make_preview(
                path,
                &paths::proxies(app).join(format!("{id}.jpg")),
                PROXY_SIDE,
                88,
            )
            .map_err(err)?;
            PathBuf::from(res.path)
        }
    };
    let thumb = match cached(&paths::thumbs(app), &id) {
        Some(p) => p,
        // Thumbnails come from the proxy: much faster than decoding the original twice.
        None => {
            let res = tessera_imaging::make_preview(
                &proxy,
                &paths::thumbs(app).join(format!("{id}.jpg")),
                THUMB_SIDE,
                82,
            )
            .map_err(err)?;
            PathBuf::from(res.path)
        }
    };
    Ok(ImportedImage {
        id,
        path: path.to_string_lossy().into_owned(),
        name: path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        width: info.width,
        height: info.height,
        format: info.format,
        file_size: info.file_size,
        taken_at: info.taken_at,
        proxy: proxy.to_string_lossy().into_owned(),
        thumb: thumb.to_string_lossy().into_owned(),
    })
}

#[tauri::command]
pub async fn import_images(app: AppHandle, paths: Vec<String>) -> CmdResult<ImportResult> {
    tauri::async_runtime::spawn_blocking(move || {
        let files = expand_paths(&paths);
        let total = files.len();
        let done = AtomicUsize::new(0);
        emit_progress(&app, "import", 0, total, None);
        let results: Vec<(PathBuf, Result<ImportedImage, String>)> = files
            .par_iter()
            .map(|p| {
                let r = import_one(&app, p);
                let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                emit_progress(
                    &app,
                    "import",
                    n,
                    total,
                    Some(p.to_string_lossy().into_owned()),
                );
                (p.clone(), r)
            })
            .collect();
        let mut out = ImportResult {
            images: Vec::new(),
            failed: Vec::new(),
        };
        for (p, r) in results {
            match r {
                Ok(i) => out.images.push(i),
                Err(e) => out.failed.push(Failure {
                    path: p.to_string_lossy().into_owned(),
                    error: e,
                }),
            }
        }
        out
    })
    .await
    .map_err(err)
}

#[tauri::command]
pub async fn probe_files(paths: Vec<String>) -> CmdResult<ProbeResult> {
    tauri::async_runtime::spawn_blocking(move || {
        let files = expand_paths(&paths);
        let results: Vec<_> = files
            .par_iter()
            .map(|p| (p.clone(), tessera_imaging::probe(p)))
            .collect();
        let mut out = ProbeResult {
            files: Vec::new(),
            failed: Vec::new(),
        };
        for (p, r) in results {
            match r {
                Ok(i) => out.files.push(i),
                Err(e) => out.failed.push(Failure {
                    path: p.to_string_lossy().into_owned(),
                    error: e.to_string(),
                }),
            }
        }
        out
    })
    .await
    .map_err(err)
}

/// Upright RGBA pixels fitted inside max_width × max_height, prefixed by
/// width and height as little-endian u32s. Used by the exporter.
#[tauri::command]
pub async fn load_image_rgba(path: String, max_width: u32, max_height: u32) -> CmdResult<Response> {
    tauri::async_runtime::spawn_blocking(move || {
        let img = tessera_imaging::load_scaled_rgba(
            Path::new(&path),
            max_width.max(1),
            max_height.max(1),
        )
        .map_err(err)?;
        let (w, h) = img.dimensions();
        let mut buf = Vec::with_capacity(8 + (w as usize) * (h as usize) * 4);
        buf.extend_from_slice(&w.to_le_bytes());
        buf.extend_from_slice(&h.to_le_bytes());
        buf.extend_from_slice(img.as_raw());
        Ok(Response::new(buf))
    })
    .await
    .map_err(err)?
}

#[tauri::command]
pub async fn read_metadata(path: String) -> CmdResult<MetadataSummary> {
    tauri::async_runtime::spawn_blocking(move || {
        tessera_imaging::read_summary(Path::new(&path)).map_err(err)
    })
    .await
    .map_err(err)?
}
