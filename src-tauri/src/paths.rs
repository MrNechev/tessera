//! Well-known directories.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

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

static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// A temporary name next to `path`, unique within this process and (with
/// the process id) across concurrent Tessera instances.
fn temp_path(path: &Path) -> PathBuf {
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".into());
    // Stay well below the usual 255-byte file name limit.
    while name.len() > 200 {
        name.pop();
    }
    dir.join(format!(
        ".{name}.{}-{}.part",
        std::process::id(),
        TMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ))
}

/// Write a file atomically (unique temp file, fsync, rename) so a crash never
/// leaves half a file and concurrent writers never share a temp file. The
/// temp file is removed when anything fails.
pub fn write_atomic(path: &Path, data: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    let tmp = temp_path(path);
    let res = (|| {
        let mut f = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
        drop(f);
        fs::rename(&tmp, path)
    })();
    if res.is_err() {
        let _ = fs::remove_file(&tmp);
        return res;
    }
    // Persist the rename itself (best effort; not possible on every platform).
    #[cfg(unix)]
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        if let Ok(d) = fs::File::open(parent) {
            let _ = d.sync_all();
        }
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("tessera-paths-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    }

    #[test]
    fn write_atomic_replaces_and_leaves_no_temp_files() {
        let dir = tmpdir("atomic");
        let p = dir.join("a.json");
        write_atomic(&p, b"one").unwrap();
        write_atomic(&p, b"two").unwrap();
        assert_eq!(fs::read(&p).unwrap(), b"two");
        assert_eq!(names(&dir), vec!["a.json"]);
    }

    #[test]
    fn concurrent_writers_do_not_share_a_temp_file() {
        let dir = tmpdir("concurrent");
        let p = dir.join("same.bin");
        std::thread::scope(|s| {
            for i in 0..8u8 {
                let p = &p;
                s.spawn(move || write_atomic(p, &vec![i; 100_000]).unwrap());
            }
        });
        let data = fs::read(&p).unwrap();
        assert_eq!(data.len(), 100_000);
        assert!(data.iter().all(|&b| b == data[0]));
        assert_eq!(names(&dir), vec!["same.bin"]);
    }

    #[test]
    fn failed_write_removes_the_temp_file() {
        let dir = tmpdir("fail");
        // Renaming a file over a non-empty directory fails.
        let target = dir.join("busy");
        fs::create_dir_all(target.join("child")).unwrap();
        assert!(write_atomic(&target, b"x").is_err());
        assert_eq!(names(&dir), vec!["busy"]);
    }
}
