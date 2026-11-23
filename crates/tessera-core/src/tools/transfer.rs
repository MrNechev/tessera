//! Getting files in and out of Tessera from a browser (server mode), and file
//! access for the UI in both modes: serving cached previews, uploads (plain or
//! chunked/resumable), downloads, zips of many results, and a folder browser
//! over the areas the guard allows.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use super::{err, paths, AreaKind, CmdResult, Tools};

/// One entry of the folder browser.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub name: String,
    pub path: String,
    pub dir: bool,
    pub size: u64,
    /// Unix milliseconds.
    pub modified: u64,
    pub image: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Listing {
    /// The folder listed ("" = the list of areas).
    pub path: String,
    pub parent: Option<String>,
    /// Whether results may be saved here.
    pub writable: bool,
    pub entries: Vec<Entry>,
}

fn millis(t: SystemTime) -> u64 {
    t.duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// List a folder (or, with an empty path, the areas: libraries, Outputs, Inbox).
pub fn browse(t: &Tools, path: &str) -> CmdResult<Listing> {
    let image_exts: Vec<&str> = tessera_imaging::input_extensions();
    if path.is_empty() {
        let entries = t
            .guard
            .areas()
            .into_iter()
            .map(|a| Entry {
                name: a.name,
                path: a.root.to_string_lossy().into_owned(),
                dir: true,
                size: 0,
                modified: 0,
                image: false,
            })
            .collect();
        return Ok(Listing {
            path: String::new(),
            parent: None,
            writable: false,
            entries,
        });
    }
    let dir = t.guard.readable(Path::new(path))?;
    let mut entries: Vec<Entry> = fs::read_dir(&dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .flatten()
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            let name = e.file_name().to_string_lossy().into_owned();
            let ext = Path::new(&name)
                .extension()
                .and_then(|x| x.to_str())
                .map(|x| x.to_ascii_lowercase())
                .unwrap_or_default();
            Some(Entry {
                path: e.path().to_string_lossy().into_owned(),
                dir: meta.is_dir(),
                size: if meta.is_dir() { 0 } else { meta.len() },
                modified: meta.modified().map(millis).unwrap_or(0),
                image: !meta.is_dir() && image_exts.contains(&ext.as_str()),
                name,
            })
        })
        .collect();
    entries.sort_by(|a, b| {
        b.dir
            .cmp(&a.dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    // Parent: stop at the area root (server mode can't go above it).
    let area_root = t.guard.area_of(&dir).map(|a| a.root);
    let parent = match &area_root {
        Some(root) if dir.starts_with(root) && dir != *root => dir.parent().map(Path::to_path_buf),
        Some(_) => Some(PathBuf::new()),
        None => dir.parent().map(Path::to_path_buf),
    }
    .map(|p| p.to_string_lossy().into_owned());
    Ok(Listing {
        path: dir.to_string_lossy().into_owned(),
        parent,
        writable: t.guard.writable(&dir).is_ok(),
        entries,
    })
}

/// Where an upload goes.
#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum UploadDest {
    /// Kept for a day, then deleted ("just for this job").
    #[default]
    Temp,
    /// Kept in the Inbox library.
    Inbox,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct UploadParams {
    pub name: String,
    /// Groups the files of one drop/pick; becomes a folder name.
    pub batch: String,
    #[serde(default)]
    pub dest: UploadDest,
    /// Byte offset of this chunk (chunked uploads); 0 starts the file.
    #[serde(default)]
    pub offset: u64,
    /// True on the last chunk.
    #[serde(default = "yes")]
    pub done: bool,
}

fn yes() -> bool {
    true
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadResult {
    /// Absolute server path of the uploaded file (complete when `done`).
    pub path: String,
    /// Bytes stored so far (resume from here after a failed chunk).
    pub received: u64,
}

fn safe_name(name: &str) -> CmdResult<String> {
    let base = Path::new(name)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let clean: String = base
        .chars()
        .map(|c| {
            if c.is_control() || c == '/' || c == '\\' {
                '_'
            } else {
                c
            }
        })
        .collect();
    let clean = clean.trim().trim_start_matches('.').to_string();
    if clean.is_empty() || clean.len() > 240 {
        return Err("invalid file name".into());
    }
    Ok(clean)
}

fn safe_batch(batch: &str) -> CmdResult<String> {
    if batch.is_empty()
        || batch.len() > 64
        || !batch
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err("invalid upload batch".into());
    }
    Ok(batch.to_string())
}

/// Folder that receives a batch of uploads.
fn upload_dir(t: &Tools, p: &UploadParams) -> CmdResult<PathBuf> {
    let batch = safe_batch(&p.batch)?;
    match p.dest {
        UploadDest::Temp => Ok(t.cfg.uploads_dir.join(batch)),
        UploadDest::Inbox => {
            let inbox = t
                .cfg
                .inbox_dir
                .as_ref()
                .ok_or("This server has no Inbox folder")?;
            let day = chrono_like_today();
            Ok(inbox.join(day))
        }
    }
}

/// `YYYY-MM-DD` (local time not needed; UTC is fine for a folder name).
fn chrono_like_today() -> String {
    let secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0) as i64;
    let days = secs.div_euclid(86_400);
    // Civil-from-days (Howard Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    format!("{:04}-{:02}-{:02}", if m <= 2 { y + 1 } else { y }, m, d)
}

/// Store one chunk of an upload. Chunks are appended to `<name>.part`; the
/// last one renames it to a free name in the target folder.
pub fn upload_chunk(t: &Tools, p: &UploadParams, data: &[u8]) -> CmdResult<UploadResult> {
    let name = safe_name(&p.name)?;
    let dir = upload_dir(t, p)?;
    fs::create_dir_all(&dir).map_err(err)?;
    let staging = t
        .cfg
        .uploads_dir
        .join(".partial")
        .join(safe_batch(&p.batch)?);
    fs::create_dir_all(&staging).map_err(err)?;
    let part = staging.join(format!("{name}.part"));
    let have = fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
    if p.offset == 0 {
        let _ = fs::remove_file(&part);
    } else if p.offset != have {
        // Out of order: tell the client where to resume.
        return Ok(UploadResult {
            path: String::new(),
            received: have,
        });
    }
    let mut f = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&part)
        .map_err(err)?;
    f.write_all(data).map_err(err)?;
    drop(f);
    let received = p.offset + data.len() as u64;
    if !p.done {
        return Ok(UploadResult {
            path: String::new(),
            received,
        });
    }
    let target = paths::unique_path(&dir.join(&name));
    t.guard.writable(&target)?;
    fs::rename(&part, &target)
        .or_else(|_| {
            fs::copy(&part, &target)
                .map(|_| ())
                .and_then(|_| fs::remove_file(&part))
        })
        .map_err(err)?;
    let _ = fs::remove_dir(&staging);
    Ok(UploadResult {
        path: target.to_string_lossy().into_owned(),
        received,
    })
}

/// Delete temporary uploads (and abandoned partial ones) older than `max_age`.
pub fn clean_uploads(t: &Tools, max_age: Duration) -> usize {
    let now = SystemTime::now();
    let mut removed = 0;
    let old = |p: &Path| {
        fs::metadata(p)
            .and_then(|m| m.modified())
            .map(|m| now.duration_since(m).unwrap_or_default() > max_age)
            .unwrap_or(false)
    };
    for root in [
        t.cfg.uploads_dir.clone(),
        t.cfg.uploads_dir.join(".partial"),
    ] {
        let Ok(rd) = fs::read_dir(&root) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.file_name().is_some_and(|n| n == ".partial") {
                continue;
            }
            if old(&p)
                && fs::remove_dir_all(&p)
                    .or_else(|_| fs::remove_file(&p))
                    .is_ok()
            {
                removed += 1;
            }
        }
    }
    removed
}

/// Build a zip of `files` (stored, not recompressed: photos are compressed
/// already) in the cache and return its path.
pub fn make_zip(t: &Tools, files: &[String]) -> CmdResult<PathBuf> {
    let dir = t.cache_dir().join("downloads");
    fs::create_dir_all(&dir).map_err(err)?;
    // Old zips are useless after a download; keep the folder small.
    let _ = clean_dir(&dir, Duration::from_secs(3600));
    let path = dir.join(format!(
        "tessera-{}-{}.zip",
        std::process::id(),
        millis(SystemTime::now())
    ));
    let file = fs::File::create(&path).map_err(err)?;
    let mut zip = zip::ZipWriter::new(std::io::BufWriter::new(file));
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Stored)
        .large_file(true);
    let mut used = std::collections::HashSet::new();
    for f in files {
        let src = t.guard.readable(Path::new(f))?;
        let name = src
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "file".into());
        // Same names from different folders: "a.jpg", "a (1).jpg".
        let mut entry = name.clone();
        let mut i = 1;
        while !used.insert(entry.clone()) {
            let p = Path::new(&name);
            let stem = p
                .file_stem()
                .map(|s| s.to_string_lossy())
                .unwrap_or_default();
            entry = match p.extension() {
                Some(ext) => format!("{stem} ({i}).{}", ext.to_string_lossy()),
                None => format!("{stem} ({i})"),
            };
            i += 1;
        }
        zip.start_file(entry, opts).map_err(err)?;
        let mut input = fs::File::open(&src).map_err(err)?;
        std::io::copy(&mut input, &mut zip).map_err(err)?;
    }
    zip.finish().map_err(err)?;
    Ok(path)
}

fn clean_dir(dir: &Path, max_age: Duration) -> std::io::Result<()> {
    let now = SystemTime::now();
    for e in fs::read_dir(dir)?.flatten() {
        if let Ok(m) = e.metadata().and_then(|m| m.modified()) {
            if now.duration_since(m).unwrap_or_default() > max_age {
                let _ = fs::remove_file(e.path());
            }
        }
    }
    Ok(())
}

/// Absolute path of `rel` inside the area (library, Outputs, Inbox) `id`.
pub fn resolve_ref(t: &Tools, id: &str, rel: &str) -> CmdResult<String> {
    let area = t
        .guard
        .areas()
        .into_iter()
        .find(|a| a.id == id)
        .ok_or_else(|| format!("unknown library {id}"))?;
    let rel = rel.trim_start_matches('/');
    let p = t.guard.readable(&area.root.join(rel))?;
    if !p.starts_with(area.root.canonicalize().unwrap_or(area.root)) {
        return Err("invalid path".into());
    }
    Ok(p.to_string_lossy().into_owned())
}

/// Which area kinds exist (for the UI's save destinations).
pub fn has_kind(t: &Tools, kind: AreaKind) -> bool {
    t.guard.areas().iter().any(|a| a.kind == kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_are_civil() {
        let d = chrono_like_today();
        assert_eq!(d.len(), 10);
        assert!(d.starts_with("20"));
    }

    #[test]
    fn names_are_sanitised() {
        assert_eq!(safe_name("../../etc/passwd").unwrap(), "passwd");
        assert_eq!(
            safe_name("a/b\\c.jpg").unwrap(),
            "b\\c.jpg".replace('\\', "_")
        );
        assert!(safe_name("..").is_err());
        assert!(safe_batch("abc-1_2").is_ok());
        assert!(safe_batch("../x").is_err());
    }
}
