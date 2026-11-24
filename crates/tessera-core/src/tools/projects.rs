//! Project storage: the app's own library (home-screen "Recent") plus
//! user-chosen `.tessera` files. A project is the collage document as JSON;
//! photos are referenced by path.

use std::fs;
use std::path::Path;

use super::{err, paths, CmdResult, Tools};
use base64::Engine;
use serde::Serialize;

const MAX_PROJECT_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    pub id: String,
    pub name: String,
    pub modified: String,
    pub path: String,
    pub thumb: Option<String>,
    pub width: u32,
    pub height: u32,
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn summarize(path: &Path) -> Option<ProjectSummary> {
    let text = fs::read_to_string(path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    let id = v.get("id")?.as_str()?.to_string();
    let thumb = path.with_extension("jpg");
    Some(ProjectSummary {
        name: v
            .get("name")
            .and_then(|n| n.as_str())
            .unwrap_or("Untitled")
            .to_string(),
        modified: v
            .get("modified")
            .and_then(|n| n.as_str())
            .unwrap_or("")
            .to_string(),
        width: v
            .pointer("/page/width")
            .and_then(|n| n.as_f64())
            .unwrap_or(0.0) as u32,
        height: v
            .pointer("/page/height")
            .and_then(|n| n.as_f64())
            .unwrap_or(0.0) as u32,
        path: path.to_string_lossy().into_owned(),
        thumb: thumb.exists().then(|| thumb.to_string_lossy().into_owned()),
        id,
    })
}

pub fn list_projects(t: &Tools) -> CmdResult<Vec<ProjectSummary>> {
    let dir = t.projects();
    let mut out: Vec<ProjectSummary> = fs::read_dir(&dir)
        .map_err(err)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("tessera"))
        .filter_map(|p| summarize(&p))
        .collect();
    out.sort_by(|a, b| b.modified.cmp(&a.modified));
    Ok(out)
}

pub fn load_project(t: &Tools, path: String) -> CmdResult<String> {
    let p = t.guard.readable(&paths::expand(&path))?;
    let meta = fs::metadata(&p).map_err(|e| format!("{}: {e}", p.display()))?;
    if meta.len() > MAX_PROJECT_BYTES {
        return Err("Project file is too large".into());
    }
    fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))
}

/// Load a project of the app's library by its id (stories embed live collages
/// by id, so they keep working when the projects folder moves).
pub fn load_project_by_id(t: &Tools, id: String) -> CmdResult<String> {
    load_project_in(&t.projects(), &id)
}

fn load_project_in(dir: &Path, id: &str) -> CmdResult<String> {
    if !valid_id(id) {
        return Err("invalid project id".into());
    }
    let p = dir.join(format!("{id}.tessera"));
    match fs::metadata(&p) {
        Ok(m) if m.len() > MAX_PROJECT_BYTES => Err("Project file is too large".into()),
        Ok(_) => fs::read_to_string(&p).map_err(err),
        Err(_) => Err("This collage no longer exists".into()),
    }
}

pub fn save_project(
    t: &Tools,
    id: String,
    json: String,
    thumbnail: Option<String>,
) -> CmdResult<ProjectSummary> {
    if !valid_id(&id) {
        return Err("invalid project id".into());
    }
    // Make sure it's a JSON document before writing anything.
    serde_json::from_str::<serde_json::Value>(&json)
        .map_err(|e| format!("invalid project data: {e}"))?;
    let path = t.projects().join(format!("{id}.tessera"));
    paths::write_atomic(&path, json.as_bytes()).map_err(err)?;
    if let Some(data_url) = thumbnail {
        if let Some((_, b64)) = data_url.split_once(";base64,") {
            if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(b64) {
                let _ = paths::write_atomic(&path.with_extension("jpg"), &bytes);
            }
        }
    }
    summarize(&path).ok_or_else(|| "could not read back project".into())
}

pub fn save_project_file(t: &Tools, path: String, json: String) -> CmdResult<()> {
    serde_json::from_str::<serde_json::Value>(&json)
        .map_err(|e| format!("invalid project data: {e}"))?;
    let p = t.guard.writable(&paths::expand(&path))?;
    paths::write_atomic(&p, json.as_bytes())
        .map_err(|e| format!("Could not save {}: {e}", p.display()))
}

pub fn delete_project(t: &Tools, id: String) -> CmdResult<()> {
    if !valid_id(&id) {
        return Err("invalid project id".into());
    }
    let base = t.projects().join(format!("{id}.tessera"));
    let _ = fs::remove_file(base.with_extension("jpg"));
    fs::remove_file(&base).map_err(err)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_by_id() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("abc-1.tessera"), r#"{"id":"abc-1"}"#).unwrap();
        assert_eq!(
            load_project_in(dir.path(), "abc-1").unwrap(),
            r#"{"id":"abc-1"}"#
        );
        assert!(load_project_in(dir.path(), "missing").is_err());
        // Ids are file names: no way out of the projects folder.
        assert!(load_project_in(dir.path(), "../abc-1").is_err());
        assert!(load_project_in(dir.path(), "").is_err());
    }
}
