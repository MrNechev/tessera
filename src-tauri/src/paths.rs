//! Well-known directories.

use std::fs;
use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager};

fn ensure(p: PathBuf) -> PathBuf {
    let _ = fs::create_dir_all(&p);
    p
}

pub fn cache_dir(app: &AppHandle) -> PathBuf {
    ensure(
        app.path()
            .app_cache_dir()
            .unwrap_or_else(|_| std::env::temp_dir().join("tessera")),
    )
}

pub fn data_dir(app: &AppHandle) -> PathBuf {
    ensure(
        app.path()
            .app_data_dir()
            .unwrap_or_else(|_| std::env::temp_dir().join("tessera-data")),
    )
}

pub fn config_dir(app: &AppHandle) -> PathBuf {
    ensure(
        app.path()
            .app_config_dir()
            .unwrap_or_else(|_| data_dir(app)),
    )
}

pub fn proxies(app: &AppHandle) -> PathBuf {
    ensure(cache_dir(app).join("proxies"))
}

pub fn thumbs(app: &AppHandle) -> PathBuf {
    ensure(cache_dir(app).join("thumbs"))
}

pub fn previews(app: &AppHandle) -> PathBuf {
    ensure(cache_dir(app).join("previews"))
}

pub fn projects(app: &AppHandle) -> PathBuf {
    ensure(data_dir(app).join("projects"))
}

/// `~/Pictures/Tessera` (or `~/Tessera` when there is no pictures folder).
pub fn default_save_dir() -> PathBuf {
    dirs::picture_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join("Pictures")))
        .unwrap_or_else(std::env::temp_dir)
        .join("Tessera")
}

/// Expand a leading `~/`.
pub fn expand(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(path)
}

/// Total size of all files below `dir`.
pub fn dir_size(dir: &Path) -> u64 {
    let Ok(rd) = fs::read_dir(dir) else { return 0 };
    rd.flatten()
        .map(|e| match e.file_type() {
            Ok(t) if t.is_dir() => dir_size(&e.path()),
            Ok(_) => e.metadata().map(|m| m.len()).unwrap_or(0),
            Err(_) => 0,
        })
        .sum()
}

/// Write a file atomically (temp file + rename) so a crash never leaves half a file.
pub fn write_atomic(path: &Path, data: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension(format!(
        "{}.part",
        path.extension().and_then(|e| e.to_str()).unwrap_or("tmp")
    ));
    fs::write(&tmp, data)?;
    fs::rename(&tmp, path)
}

/// `name.ext` → `name (1).ext`, `name (2).ext`, … until the path is free.
pub fn unique_path(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("file")
        .to_string();
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|e| format!(".{e}"))
        .unwrap_or_default();
    let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
    for i in 1..10_000 {
        let candidate = dir.join(format!("{stem} ({i}){ext}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    path.to_path_buf()
}
