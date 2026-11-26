//! Paint documents ("drawings"): layered images saved as OpenRaster (`.ora`)
//! files in the app's project library, next to the collage projects:
//!
//! - `projects/drawings/<id>.ora`: the document (a zip: `mimetype`,
//!   `stack.xml`, one PNG per layer, `mergedimage.png`, `Thumbnails/…`),
//!   written by the UI; GIMP and Krita open it as is.
//! - `<id>.json`: name, size and modification time for the Home screen.
//! - `<id>.png`: the thumbnail, copied out of the `.ora` on every save.
//! - `exports.json`: which drawing produced which exported image, so "Edit in
//!   Paint" on an exported PNG/JPEG reopens the layered document.

use std::collections::BTreeMap;
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{err, paths, CmdResult, Tools};

/// Largest drawing accepted (16 MP × a few layers of PNG fits easily).
pub const MAX_DRAWING_BYTES: usize = 400 * 1024 * 1024;
const MAX_THUMB_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DrawingSummary {
    pub id: String,
    pub name: String,
    pub modified: String,
    pub width: u32,
    pub height: u32,
    /// Absolute path of the `.ora` (for "open with GIMP/Krita" on the desktop).
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub thumb: Option<String>,
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn check_id(id: &str) -> CmdResult<()> {
    if valid_id(id) {
        Ok(())
    } else {
        Err("invalid drawing id".into())
    }
}

impl Tools {
    pub fn drawings(&self) -> PathBuf {
        let d = self.projects().join("drawings");
        let _ = fs::create_dir_all(&d);
        d
    }
}

/// What a valid OpenRaster file must contain; returns (width, height, thumbnail PNG).
pub fn inspect_ora(bytes: &[u8]) -> CmdResult<(u32, u32, Option<Vec<u8>>)> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| format!("not an OpenRaster file: {e}"))?;
    let mut mime = String::new();
    zip.by_name("mimetype")
        .map_err(|_| "not an OpenRaster file (no mimetype)".to_string())?
        .take(64)
        .read_to_string(&mut mime)
        .map_err(err)?;
    if mime.trim() != "image/openraster" {
        return Err("not an OpenRaster file".into());
    }
    let mut stack = String::new();
    zip.by_name("stack.xml")
        .map_err(|_| "not an OpenRaster file (no stack.xml)".to_string())?
        .take(8 * 1024 * 1024)
        .read_to_string(&mut stack)
        .map_err(err)?;
    let (w, h) = image_size(&stack).ok_or("stack.xml has no image size")?;
    let thumb = match zip.by_name("Thumbnails/thumbnail.png") {
        Ok(f) => {
            let mut buf = Vec::new();
            f.take(MAX_THUMB_BYTES).read_to_end(&mut buf).map_err(err)?;
            Some(buf)
        }
        Err(_) => None,
    };
    Ok((w, h, thumb))
}

/// `w` and `h` of the `<image>` element of a stack.xml.
fn image_size(stack: &str) -> Option<(u32, u32)> {
    let start = stack.find("<image")?;
    let end = start + stack[start..].find('>')?;
    let tag = &stack[start..end];
    let attr = |name: &str| -> Option<u32> {
        let mut rest = tag;
        loop {
            let i = rest.find(name)?;
            let before = rest[..i].chars().last();
            let after = &rest[i + name.len()..];
            if matches!(before, Some(c) if c.is_whitespace()) && after.trim_start().starts_with('=')
            {
                let v = after.trim_start()[1..].trim_start();
                let q = v.chars().next()?;
                let v = &v[1..];
                return v[..v.find(q)?].trim().parse().ok();
            }
            rest = after;
        }
    };
    let (w, h) = (attr("w")?, attr("h")?);
    (w > 0 && h > 0).then_some((w, h))
}

fn summary_in(dir: &Path, id: &str) -> Option<DrawingSummary> {
    let text = fs::read_to_string(dir.join(format!("{id}.json"))).ok()?;
    let mut s: DrawingSummary = serde_json::from_str(&text).ok()?;
    let ora = dir.join(format!("{id}.ora"));
    if !ora.is_file() {
        return None;
    }
    s.id = id.to_string();
    s.path = ora.to_string_lossy().into_owned();
    let thumb = dir.join(format!("{id}.png"));
    s.thumb = thumb
        .is_file()
        .then(|| thumb.to_string_lossy().into_owned());
    Some(s)
}

pub fn list_drawings(t: &Tools) -> CmdResult<Vec<DrawingSummary>> {
    list_in(&t.drawings())
}

fn list_in(dir: &Path) -> CmdResult<Vec<DrawingSummary>> {
    let mut out: Vec<DrawingSummary> = fs::read_dir(dir)
        .map_err(err)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("ora"))
        .filter_map(|p| {
            let id = p.file_stem()?.to_str()?.to_string();
            summary_in(dir, &id)
        })
        .collect();
    out.sort_by(|a, b| b.modified.cmp(&a.modified));
    Ok(out)
}

pub fn save_drawing(t: &Tools, id: &str, name: &str, bytes: &[u8]) -> CmdResult<DrawingSummary> {
    save_in(&t.drawings(), id, name, bytes)
}

fn save_in(dir: &Path, id: &str, name: &str, bytes: &[u8]) -> CmdResult<DrawingSummary> {
    check_id(id)?;
    if bytes.len() > MAX_DRAWING_BYTES {
        return Err("This drawing is too large to save".into());
    }
    let (width, height, thumb) = inspect_ora(bytes)?;
    let ora = dir.join(format!("{id}.ora"));
    paths::write_atomic(&ora, bytes).map_err(|e| format!("Could not save the drawing: {e}"))?;
    let thumb_path = dir.join(format!("{id}.png"));
    match thumb {
        Some(png) => {
            let _ = paths::write_atomic(&thumb_path, &png);
        }
        None => {
            let _ = fs::remove_file(&thumb_path);
        }
    }
    let name: String = name.trim().chars().take(200).collect();
    let meta = DrawingSummary {
        id: id.to_string(),
        name: if name.is_empty() {
            "Untitled drawing".into()
        } else {
            name
        },
        modified: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        width,
        height,
        path: String::new(),
        thumb: None,
    };
    let json = serde_json::to_vec(&meta).map_err(err)?;
    paths::write_atomic(&dir.join(format!("{id}.json")), &json).map_err(err)?;
    summary_in(dir, id).ok_or_else(|| "could not read back the drawing".into())
}

/// The `.ora` of a drawing (for loading it into the editor).
pub fn drawing_path(t: &Tools, id: &str) -> CmdResult<PathBuf> {
    check_id(id)?;
    let p = t.drawings().join(format!("{id}.ora"));
    if p.is_file() {
        Ok(p)
    } else {
        Err("This drawing no longer exists".into())
    }
}

pub fn rename_drawing(t: &Tools, id: &str, name: &str) -> CmdResult<DrawingSummary> {
    check_id(id)?;
    let dir = t.drawings();
    let mut s = summary_in(&dir, id).ok_or("This drawing no longer exists")?;
    let name: String = name.trim().chars().take(200).collect();
    if !name.is_empty() {
        s.name = name;
    }
    let meta = DrawingSummary {
        path: String::new(),
        thumb: None,
        ..s
    };
    paths::write_atomic(
        &dir.join(format!("{id}.json")),
        &serde_json::to_vec(&meta).map_err(err)?,
    )
    .map_err(err)?;
    summary_in(&dir, id).ok_or_else(|| "could not read back the drawing".into())
}

pub fn delete_drawing(t: &Tools, id: &str) -> CmdResult<()> {
    check_id(id)?;
    let dir = t.drawings();
    for ext in ["png", "json"] {
        let _ = fs::remove_file(dir.join(format!("{id}.{ext}")));
    }
    fs::remove_file(dir.join(format!("{id}.ora"))).map_err(err)
}

type ExportMap = BTreeMap<String, String>;

fn exports_file(dir: &Path) -> PathBuf {
    dir.join("exports.json")
}

fn read_exports(dir: &Path) -> ExportMap {
    fs::read(exports_file(dir))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

/// Remember that the image at `path` (on the engine's machine) was exported from drawing `id`.
pub fn link_export(t: &Tools, id: &str, path: &str) -> CmdResult<()> {
    link_in(&t.drawings(), id, path)
}

fn link_in(dir: &Path, id: &str, path: &str) -> CmdResult<()> {
    check_id(id)?;
    if path.is_empty() {
        return Err("empty path".into());
    }
    let mut map = read_exports(dir);
    // Forget exports of drawings that were deleted.
    map.retain(|_, v| dir.join(format!("{v}.ora")).is_file());
    map.insert(path.to_string(), id.to_string());
    paths::write_atomic(&exports_file(dir), &serde_json::to_vec(&map).map_err(err)?).map_err(err)
}

/// The drawing an exported image came from, if it still exists.
pub fn drawing_for_export(t: &Tools, path: &str) -> CmdResult<Option<String>> {
    Ok(lookup_in(&t.drawings(), path))
}

fn lookup_in(dir: &Path, path: &str) -> Option<String> {
    read_exports(dir)
        .get(path)
        .filter(|id| valid_id(id) && dir.join(format!("{id}.ora")).is_file())
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn ora(w: u32, h: u32, thumb: bool) -> Vec<u8> {
        let mut buf = Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut buf);
            let stored = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            z.start_file("mimetype", stored).unwrap();
            z.write_all(b"image/openraster").unwrap();
            z.start_file("stack.xml", stored).unwrap();
            write!(
                z,
                "<?xml version='1.0' encoding='UTF-8'?>\n<image version=\"0.0.5\" w=\"{w}\" h=\"{h}\"><stack><layer name=\"a\" src=\"data/a.png\"/></stack></image>"
            )
            .unwrap();
            if thumb {
                z.start_file("Thumbnails/thumbnail.png", stored).unwrap();
                z.write_all(b"\x89PNGfake").unwrap();
            }
            z.finish().unwrap();
        }
        buf.into_inner()
    }

    #[test]
    fn reads_image_size() {
        assert_eq!(
            image_size("<image version=\"0.0.5\" w=\"640\" h='480' xres=\"72\">"),
            Some((640, 480))
        );
        assert_eq!(image_size("<image h=\"10\" w = \"20\">"), Some((20, 10)));
        assert_eq!(image_size("<image xw=\"5\" w=\"0\" h=\"3\">"), None);
        assert_eq!(image_size("<stack/>"), None);
    }

    #[test]
    fn save_list_link_delete() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        assert!(save_in(d, "../x", "x", &ora(1, 1, false)).is_err());
        assert!(save_in(d, "a1", "x", b"not a zip").is_err());
        let s = save_in(d, "a1", "  My drawing ", &ora(320, 200, true)).unwrap();
        assert_eq!(
            (s.width, s.height, s.name.as_str()),
            (320, 200, "My drawing")
        );
        assert!(s.thumb.is_some());
        let s2 = save_in(d, "b2", "", &ora(10, 10, false)).unwrap();
        assert_eq!(s2.name, "Untitled drawing");
        assert!(s2.thumb.is_none());
        assert_eq!(list_in(d).unwrap().len(), 2);

        link_in(d, "a1", "/out/Drawings/x.png").unwrap();
        assert_eq!(lookup_in(d, "/out/Drawings/x.png").as_deref(), Some("a1"));
        assert_eq!(lookup_in(d, "/out/other.png"), None);
        fs::remove_file(d.join("a1.ora")).unwrap();
        // A deleted drawing is no longer offered.
        assert_eq!(lookup_in(d, "/out/Drawings/x.png"), None);
        assert_eq!(list_in(d).unwrap().len(), 1);
    }
}
