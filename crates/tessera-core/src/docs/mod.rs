//! Documents stored as JSON files in `<data>/documents/<id>.json`.
//!
//! Saves carry the version the client started from; a mismatch means another tab or
//! computer saved in between, and the save is refused with the current document so the UI
//! can warn instead of silently overwriting. Snapshots of earlier versions are kept in
//! `.history/<id>/` (at most one per `snapshot_every` while editing, plus one before
//! restores). Deleted documents move to `.trash/`. JSON fields the server doesn't model are
//! preserved on every round trip.

mod search;

pub use search::{plain_text, Hit, Use};

use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};

use crate::store::{DocMeta, Store as Index};
use crate::util::{rfc3339_nano_utc, truncate_bytes};
use crate::zipstream::{Method, ZipStream};

/// Errors of the document store (the HTTP layer maps them to 404/409/400/500).
#[derive(Debug, thiserror::Error)]
pub enum DocError {
    #[error("document not found")]
    NotFound,
    /// Another tab or computer saved in between; carries the current document.
    #[error("document was changed elsewhere")]
    Conflict(Box<Document>),
    #[error("invalid document: {0}")]
    Invalid(String),
    #[error("{0}")]
    Other(#[from] anyhow::Error),
}

impl From<std::io::Error> for DocError {
    fn from(e: std::io::Error) -> Self {
        DocError::Other(e.into())
    }
}

pub type Extra = Map<String, Value>;

/// `null` → default (Go leaves the zero value for JSON null).
fn nd<'de, D: Deserializer<'de>, T: Default + Deserialize<'de>>(d: D) -> Result<T, D::Error> {
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

/// A present field keeps its raw JSON value, including `null` (Go `json.RawMessage`).
fn raw<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Value>, D::Error> {
    Ok(Some(Value::deserialize(d)?))
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}
fn is_zero_i(v: &i64) -> bool {
    *v == 0
}
fn is_false(v: &bool) -> bool {
    !*v
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct MediaRef {
    #[serde(rename = "libraryId", default, deserialize_with = "nd")]
    pub library_id: String,
    #[serde(default, deserialize_with = "nd")]
    pub path: String,
    #[serde(
        default,
        deserialize_with = "nd",
        skip_serializing_if = "String::is_empty"
    )]
    pub caption: String,
    #[serde(
        rename = "startAt",
        default,
        deserialize_with = "nd",
        skip_serializing_if = "is_zero"
    )]
    pub start_at: f64,
    #[serde(
        rename = "endAt",
        default,
        deserialize_with = "nd",
        skip_serializing_if = "is_zero"
    )]
    pub end_at: f64,
    #[serde(default, deserialize_with = "nd", skip_serializing_if = "is_false")]
    pub muted: bool,
    #[serde(
        rename = "loop",
        default,
        deserialize_with = "nd",
        skip_serializing_if = "is_false"
    )]
    pub loop_: bool,
    #[serde(default, deserialize_with = "nd", skip_serializing_if = "is_false")]
    pub autoplay: bool,
    /// contain | cover
    #[serde(
        default,
        deserialize_with = "nd",
        skip_serializing_if = "String::is_empty"
    )]
    pub fit: String,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Gallery {
    /// grid | carousel
    #[serde(
        default,
        deserialize_with = "nd",
        skip_serializing_if = "String::is_empty"
    )]
    pub mode: String,
    #[serde(default, deserialize_with = "nd", skip_serializing_if = "is_zero_i")]
    pub columns: i64,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Section {
    #[serde(default, deserialize_with = "nd")]
    pub id: String,
    #[serde(
        default,
        deserialize_with = "nd",
        skip_serializing_if = "String::is_empty"
    )]
    pub title: String,
    #[serde(default, deserialize_with = "nd")]
    pub layout: String,
    #[serde(default, deserialize_with = "nd")]
    pub media: Vec<MediaRef>,
    /// TipTap JSON (owned by the web app).
    #[serde(
        default,
        deserialize_with = "raw",
        skip_serializing_if = "Option::is_none"
    )]
    pub text: Option<Value>,
    #[serde(default, deserialize_with = "nd")]
    pub slide: bool,
    #[serde(
        default,
        deserialize_with = "nd",
        skip_serializing_if = "String::is_empty"
    )]
    pub notes: String,
    #[serde(default, deserialize_with = "nd", skip_serializing_if = "is_false")]
    pub collapsed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gallery: Option<Gallery>,
    #[serde(
        default,
        deserialize_with = "nd",
        skip_serializing_if = "String::is_empty"
    )]
    pub transition: String,
    /// The section card's position on the whiteboard.
    #[serde(
        default,
        deserialize_with = "raw",
        skip_serializing_if = "Option::is_none"
    )]
    pub canvas: Option<Value>,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Settings {
    /// fade | slide | zoom | none
    #[serde(
        default,
        deserialize_with = "nd",
        skip_serializing_if = "String::is_empty"
    )]
    pub transition: String,
    #[serde(
        default,
        deserialize_with = "nd",
        skip_serializing_if = "String::is_empty"
    )]
    pub theme: String,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Document {
    #[serde(default, deserialize_with = "nd")]
    pub id: String,
    #[serde(default, deserialize_with = "nd")]
    pub title: String,
    #[serde(rename = "createdAt", default, deserialize_with = "nd")]
    pub created_at: String,
    #[serde(rename = "updatedAt", default, deserialize_with = "nd")]
    pub updated_at: String,
    #[serde(default, deserialize_with = "nd")]
    pub version: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<Settings>,
    #[serde(default, deserialize_with = "nd")]
    pub sections: Vec<Section>,
    /// Whiteboard items (notes, shapes, arrows, frames). The server only checks that it is a
    /// JSON object; the web app owns the schema.
    #[serde(
        default,
        deserialize_with = "raw",
        skip_serializing_if = "Option::is_none"
    )]
    pub canvas: Option<Value>,
    #[serde(
        default,
        deserialize_with = "nd",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover: Option<MediaRef>,
    #[serde(flatten)]
    pub extra: Extra,
}

/// Caps the whiteboard JSON of one document.
pub const MAX_CANVAS_BYTES: usize = 8 << 20;

const LAYOUTS: &[&str] = &[
    "media-top",
    "media-left",
    "media-right",
    "media-full",
    "text-only",
    "gallery",
    "compare",
];

/// A previous version in the history list.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct HistoryEntry {
    pub version: i64,
    #[serde(rename = "savedAt")]
    pub saved_at: String,
    pub title: String,
    pub sections: i64,
    pub media: i64,
    pub bytes: i64,
}

/// The documents folder plus its SQLite index.
pub struct Store {
    dir: PathBuf,
    index: Arc<Index>,
    keep: usize,
    /// History keeps one snapshot per this interval while editing.
    pub snapshot_every: Duration,
    locks: Mutex<HashMap<String, Arc<Mutex<()>>>>,
}

/// A random UUIDv4-style id.
pub fn new_id() -> String {
    let mut b = [0u8; 16];
    rand::RngCore::fill_bytes(&mut rand::rng(), &mut b);
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    format!(
        "{}-{}-{}-{}-{}",
        hex::encode(&b[0..4]),
        hex::encode(&b[4..6]),
        hex::encode(&b[6..8]),
        hex::encode(&b[8..10]),
        hex::encode(&b[10..])
    )
}

fn valid_id(id: &str) -> bool {
    (8..=64).contains(&id.len())
        && id
            .chars()
            .all(|r| r.is_ascii_lowercase() || r.is_ascii_digit() || r == '-')
}

fn now() -> String {
    rfc3339_nano_utc(chrono::Utc::now())
}

/// Library-relative paths; ".." is rejected as a path segment only, so file names like
/// "wait... what.gif" are fine.
pub fn valid_ref_path(p: &str) -> bool {
    if p.is_empty() || p.starts_with('/') || p.contains('\0') || p.contains('\\') {
        return false;
    }
    !p.split('/').any(|s| s == "..")
}

fn json_object_or_null(v: &Option<Value>) -> bool {
    matches!(v, None | Some(Value::Null) | Some(Value::Object(_)))
}

/// Fixes up a document coming from a client.
pub fn normalize(d: &mut Document) -> Result<(), DocError> {
    d.title = d.title.trim().to_string();
    if d.title.len() > 500 {
        d.title = truncate_bytes(&d.title, 500).to_string();
    }
    // tags: trimmed, unique, at most 50 of at most 60 characters
    let mut seen_tag = HashSet::new();
    let mut tags = Vec::new();
    for t in &d.tags {
        let t = truncate_bytes(t.trim(), 60).to_string();
        if !t.is_empty() && !seen_tag.contains(&t.to_lowercase()) && tags.len() < 50 {
            seen_tag.insert(t.to_lowercase());
            tags.push(t);
        }
    }
    d.tags = tags;
    if let Some(c) = &d.cover {
        if c.library_id.is_empty() || !valid_ref_path(&c.path) {
            d.cover = None;
        }
    }
    if d.sections.len() > 20000 {
        return Err(DocError::Invalid("too many sections".into()));
    }
    let mut seen = HashSet::new();
    for sec in &mut d.sections {
        if sec.id.is_empty() || seen.contains(&sec.id) || sec.id.len() > 64 {
            sec.id = new_id();
        }
        seen.insert(sec.id.clone());
        if !LAYOUTS.contains(&sec.layout.as_str()) {
            sec.layout = "media-top".into();
        }
        for mr in &sec.media {
            if mr.library_id.is_empty() || !valid_ref_path(&mr.path) {
                return Err(DocError::Invalid(format!(
                    "bad media reference {:?}",
                    mr.path
                )));
            }
        }
        if !json_object_or_null(&sec.canvas) {
            return Err(DocError::Invalid("section canvas must be an object".into()));
        }
    }
    if let Some(c) = &d.canvas {
        if serde_json::to_vec(c).map(|b| b.len()).unwrap_or(0) > MAX_CANVAS_BYTES {
            return Err(DocError::Invalid("canvas is too large".into()));
        }
    }
    if !json_object_or_null(&d.canvas) {
        return Err(DocError::Invalid("canvas must be an object".into()));
    }
    Ok(())
}

/// The index row of a document.
pub fn meta(d: &Document) -> DocMeta {
    let mut m = DocMeta {
        id: d.id.clone(),
        title: d.title.clone(),
        created_at: d.created_at.clone(),
        updated_at: d.updated_at.clone(),
        version: d.version,
        sections: d.sections.len() as i64,
        tags: d.tags.clone(),
        ..Default::default()
    };
    if let Some(c) = &d.cover {
        if valid_ref_path(&c.path) {
            m.cover_lib = c.library_id.clone();
            m.cover_path = c.path.clone();
        }
    }
    for sec in &d.sections {
        m.media += sec.media.len() as i64;
        if m.cover_path.is_empty() && !sec.media.is_empty() {
            m.cover_lib = sec.media[0].library_id.clone();
            m.cover_path = sec.media[0].path.clone();
        }
    }
    m
}

fn write_json(path: &Path, v: &impl Serialize) -> anyhow::Result<()> {
    let b = serde_json::to_vec_pretty(v)?;
    let tmp = PathBuf::from(format!("{}.tmp", path.display()));
    {
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&tmp)?;
        f.write_all(&b)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

fn read_doc(path: &Path) -> Result<Document, DocError> {
    let b = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(DocError::NotFound),
        Err(e) => return Err(e.into()),
    };
    serde_json::from_slice(&b).map_err(|e| {
        DocError::Other(anyhow::anyhow!(
            "{}: {e}",
            path.file_name()
                .map(|n| n.to_string_lossy())
                .unwrap_or_default()
        ))
    })
}

/// Every media reference used in a document (deduplicated).
pub fn refs(d: &Document) -> Vec<MediaRef> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for sec in &d.sections {
        for m in &sec.media {
            if seen.insert(format!("{}\0{}", m.library_id, m.path)) {
                out.push(m.clone());
            }
        }
    }
    out
}

impl Store {
    pub fn new(data_dir: &Path, index: Arc<Index>, keep: usize) -> anyhow::Result<Store> {
        let dir = data_dir.join("documents");
        for d in [dir.clone(), dir.join(".history"), dir.join(".trash")] {
            std::fs::create_dir_all(d)?;
        }
        Ok(Store {
            dir,
            index,
            keep: keep.max(1),
            snapshot_every: Duration::from_secs(5 * 60),
            locks: Mutex::new(HashMap::new()),
        })
    }

    fn lock(&self, id: &str) -> Arc<Mutex<()>> {
        self.locks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .entry(id.to_string())
            .or_default()
            .clone()
    }

    fn path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.json"))
    }

    fn hist_dir(&self, id: &str) -> PathBuf {
        self.dir.join(".history").join(id)
    }

    /// Rebuilds the SQLite document index from the JSON files.
    pub fn reindex(&self) -> anyhow::Result<()> {
        self.index.clear_docs()?;
        for e in std::fs::read_dir(&self.dir)?.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if e.file_type().map(|t| t.is_dir()).unwrap_or(true) || !name.ends_with(".json") {
                continue;
            }
            let Ok(d) = read_doc(&e.path()) else { continue };
            self.index.put_doc(&meta(&d))?;
        }
        Ok(())
    }

    pub fn list(&self) -> anyhow::Result<Vec<DocMeta>> {
        self.index.list_docs()
    }

    pub fn get(&self, id: &str) -> Result<Document, DocError> {
        if !valid_id(id) {
            return Err(DocError::NotFound);
        }
        read_doc(&self.path(id))
    }

    pub fn create(&self, mut d: Document) -> Result<Document, DocError> {
        normalize(&mut d)?;
        d.id = new_id();
        if d.title.is_empty() {
            d.title = "Untitled".into();
        }
        d.created_at = now();
        d.updated_at = d.created_at.clone();
        d.version = 1;
        let l = self.lock(&d.id);
        let _g = l.lock().unwrap_or_else(|e| e.into_inner());
        write_json(&self.path(&d.id), &d)?;
        self.index.put_doc(&meta(&d))?;
        Ok(d)
    }

    /// Writes `d` over the stored document if `base_version` still matches (`force` skips
    /// the check).
    pub fn save(
        &self,
        id: &str,
        base_version: i64,
        mut d: Document,
        force: bool,
    ) -> Result<Document, DocError> {
        if !valid_id(id) {
            return Err(DocError::NotFound);
        }
        normalize(&mut d)?;
        let l = self.lock(id);
        let _g = l.lock().unwrap_or_else(|e| e.into_inner());
        let cur = read_doc(&self.path(id))?;
        if !force && cur.version != base_version {
            return Err(DocError::Conflict(Box::new(cur)));
        }
        self.maybe_snapshot(&cur, false)?;
        d.id = id.to_string();
        d.created_at = cur.created_at.clone();
        d.updated_at = now();
        d.version = cur.version + 1;
        if d.title.is_empty() {
            d.title = "Untitled".into();
        }
        write_json(&self.path(id), &d)?;
        self.index.put_doc(&meta(&d))?;
        Ok(d)
    }

    fn history_versions(&self, id: &str) -> Vec<i64> {
        let mut vs: Vec<i64> = std::fs::read_dir(self.hist_dir(id))
            .map(|rd| {
                rd.flatten()
                    .filter_map(|e| {
                        let n = e.file_name().to_string_lossy().into_owned();
                        n.strip_suffix(".json").unwrap_or(&n).parse().ok()
                    })
                    .collect()
            })
            .unwrap_or_default();
        vs.sort_unstable_by(|a, b| b.cmp(a));
        vs
    }

    /// Stores `cur` in history if the latest snapshot is older than `snapshot_every`.
    fn maybe_snapshot(&self, cur: &Document, always: bool) -> anyhow::Result<()> {
        let dir = self.hist_dir(&cur.id);
        let vs = self.history_versions(&cur.id);
        if !always && !vs.is_empty() {
            if vs[0] == cur.version {
                return Ok(());
            }
            if let Ok(md) = std::fs::metadata(dir.join(format!("{}.json", vs[0]))) {
                let age = md
                    .modified()
                    .ok()
                    .and_then(|t| SystemTime::now().duration_since(t).ok())
                    .unwrap_or(Duration::ZERO);
                if age < self.snapshot_every {
                    return Ok(());
                }
            }
        }
        std::fs::create_dir_all(&dir)?;
        write_json(&dir.join(format!("{}.json", cur.version)), cur)?;
        let vs = self.history_versions(&cur.id);
        for v in vs.iter().skip(self.keep) {
            let _ = std::fs::remove_file(dir.join(format!("{v}.json")));
        }
        Ok(())
    }

    /// Forces a history entry for the current version ("save a version now").
    pub fn snapshot(&self, id: &str) -> Result<(), DocError> {
        if !valid_id(id) {
            return Err(DocError::NotFound);
        }
        let l = self.lock(id);
        let _g = l.lock().unwrap_or_else(|e| e.into_inner());
        let cur = read_doc(&self.path(id))?;
        self.maybe_snapshot(&cur, true)?;
        Ok(())
    }

    pub fn history(&self, id: &str) -> Result<Vec<HistoryEntry>, DocError> {
        if !valid_id(id) {
            return Err(DocError::NotFound);
        }
        let mut out = Vec::new();
        for v in self.history_versions(id) {
            let p = self.hist_dir(id).join(format!("{v}.json"));
            let Ok(d) = read_doc(&p) else { continue };
            let bytes = std::fs::metadata(&p).map(|m| m.len() as i64).unwrap_or(0);
            let m = meta(&d);
            out.push(HistoryEntry {
                version: d.version,
                saved_at: d.updated_at.clone(),
                title: d.title.clone(),
                sections: m.sections,
                media: m.media,
                bytes,
            });
        }
        Ok(out)
    }

    pub fn get_version(&self, id: &str, version: i64) -> Result<Document, DocError> {
        if !valid_id(id) {
            return Err(DocError::NotFound);
        }
        read_doc(&self.hist_dir(id).join(format!("{version}.json")))
    }

    /// Makes an old version current again (as a new version; nothing is lost).
    pub fn restore(&self, id: &str, version: i64) -> Result<Document, DocError> {
        let old = self.get_version(id, version)?;
        {
            let l = self.lock(id);
            let _g = l.lock().unwrap_or_else(|e| e.into_inner());
            if let Ok(cur) = read_doc(&self.path(id)) {
                self.maybe_snapshot(&cur, true)?;
            }
        }
        self.save(id, 0, old, true)
    }

    pub fn duplicate(&self, id: &str) -> Result<Document, DocError> {
        let mut d = self.get(id)?;
        d.title = format!("{} (copy)", d.title);
        self.create(d)
    }

    /// Moves a document to `.trash/<id>-<unix>.json`.
    pub fn delete(&self, id: &str) -> Result<(), DocError> {
        if !valid_id(id) {
            return Err(DocError::NotFound);
        }
        let l = self.lock(id);
        let _g = l.lock().unwrap_or_else(|e| e.into_inner());
        let src = self.path(id);
        if std::fs::metadata(&src).is_err() {
            return Err(DocError::NotFound);
        }
        let dst = self
            .dir
            .join(".trash")
            .join(format!("{id}-{}.json", chrono::Utc::now().timestamp()));
        std::fs::rename(&src, &dst)?;
        self.index.delete_doc(id)?;
        Ok(())
    }

    /// Writes a zip with every document and its history (`documents/…`).
    pub fn export_all(&self, w: impl Write) -> anyhow::Result<()> {
        let mut files = Vec::new();
        collect_json(&self.dir, &self.dir, &mut files);
        let mut z = ZipStream::new(w);
        for (rel, p) in files {
            if rel.starts_with(".trash") {
                continue;
            }
            let mut f = std::fs::File::open(&p)?;
            z.add(&format!("documents/{rel}"), Method::Deflate, None, &mut f)?;
        }
        z.finish()?;
        Ok(())
    }
}

/// Every `*.json` file below `dir` in lexical walk order: (slash-relative path, abs path).
fn collect_json(root: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = rd.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let p = e.path();
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_dir() {
            collect_json(root, &p, out);
        } else if p.to_string_lossy().ends_with(".json") {
            if let Ok(rel) = p.strip_prefix(root) {
                out.push((rel.to_string_lossy().replace('\\', "/"), p.clone()));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_store(dir: &Path) -> Store {
        let db = Arc::new(Index::open(&dir.join("x.db")).unwrap());
        Store::new(dir, db, 3).unwrap()
    }

    fn mref(lib: &str, p: &str) -> MediaRef {
        MediaRef {
            library_id: lib.into(),
            path: p.into(),
            ..Default::default()
        }
    }

    #[test]
    fn save_conflict_history() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = new_store(dir.path());
        s.snapshot_every = Duration::ZERO; // snapshot every save for the test
        let d = s
            .create(Document {
                title: "Trip".into(),
                sections: vec![Section {
                    layout: "bogus".into(),
                    media: vec![mref("main", "a/b.mp4")],
                    ..Default::default()
                }],
                ..Default::default()
            })
            .unwrap();
        assert_eq!(d.version, 1);
        assert_eq!(d.sections[0].layout, "media-top");
        assert!(!d.sections[0].id.is_empty());
        // tab A saves on top of v1
        let mut a = d.clone();
        a.title = "Trip A".into();
        s.save(&d.id, 1, a, false).unwrap();
        // tab B still thinks it's v1 → conflict carrying the current doc
        let mut b = d.clone();
        b.title = "Trip B".into();
        match s.save(&d.id, 1, b.clone(), false) {
            Err(DocError::Conflict(cur)) => assert_eq!(cur.title, "Trip A"),
            other => panic!("want conflict, got {other:?}"),
        }
        // force overwrites
        assert_eq!(s.save(&d.id, 1, b, true).unwrap().version, 3);
        for _ in 0..5 {
            let cur = s.get(&d.id).unwrap();
            std::thread::sleep(Duration::from_millis(2));
            let v = cur.version;
            s.save(&d.id, v, cur, false).unwrap();
        }
        let h = s.history(&d.id).unwrap();
        assert_eq!(h.len(), 3, "history keeps 3");
        // restore an old version → becomes a new version
        let old = h.last().unwrap().version;
        let r = s.restore(&d.id, old).unwrap();
        assert!(r.version > old);
        let list = s.list().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].media, 1);
    }

    #[test]
    fn rejects_bad_refs() {
        let dir = tempfile::tempdir().unwrap();
        let s = new_store(dir.path());
        let doc = |p: &str| Document {
            sections: vec![Section {
                media: vec![mref("m", p)],
                ..Default::default()
            }],
            ..Default::default()
        };
        for p in ["wait... what.gif", "a/b..c/d.mp4", "Смешни/..котка.gif"] {
            assert!(s.create(doc(p)).is_ok(), "path {p:?} should be accepted");
        }
        for p in ["/etc/passwd", "../x.mp4", "a/../../x", "a/..", ""] {
            assert!(
                matches!(s.create(doc(p)), Err(DocError::Invalid(_))),
                "path {p:?}: want invalid"
            );
        }
        assert!(matches!(s.get("../../etc/passwd"), Err(DocError::NotFound)));
    }

    #[test]
    fn canvas_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let s = new_store(dir.path());
        let canvas: Value = serde_json::from_str(r#"{"items":[{"id":"n1","type":"note","x":10,"y":20,"w":200,"h":120,"text":"hi","color":"yellow","size":24}]}"#).unwrap();
        let d = s
            .create(Document {
                title: "Board".into(),
                canvas: Some(canvas),
                sections: vec![Section {
                    layout: "text-only".into(),
                    canvas: Some(serde_json::json!({"x":0,"y":0,"w":480,"h":300})),
                    ..Default::default()
                }],
                ..Default::default()
            })
            .unwrap();
        let got = s.get(&d.id).unwrap();
        assert!(got.canvas.is_some() && got.sections[0].canvas.is_some());
        for bad in ["[1,2]", "\"x\""] {
            let mut doc = Document {
                title: "x".into(),
                canvas: Some(serde_json::from_str(bad).unwrap()),
                ..Default::default()
            };
            assert!(
                matches!(normalize(&mut doc), Err(DocError::Invalid(_))),
                "canvas {bad} accepted"
            );
        }
        let mut doc = Document {
            title: "x".into(),
            canvas: Some(Value::Null),
            ..Default::default()
        };
        assert!(normalize(&mut doc).is_ok(), "null canvas rejected");
    }

    #[test]
    fn unknown_fields_survive() {
        let input = r#"{"id":"abcdefgh1","title":"T","createdAt":"","updatedAt":"","version":1,"futureDoc":{"a":1},
		"settings":{"transition":"fade","music":{"libraryId":"m","path":"a.mp3"}},
		"sections":[{"id":"s1","layout":"compare","media":[{"libraryId":"m","path":"x.jpg","crop":{"x":0.1}}],"slide":true,"chapter":true}]}"#;
        let d: Document = serde_json::from_str(input).unwrap();
        let out = serde_json::to_string(&d).unwrap();
        for want in [
            r#""futureDoc":{"a":1}"#,
            r#""music":{"libraryId":"m","path":"a.mp3"}"#,
            r#""crop":{"x":0.1}"#,
            r#""chapter":true"#,
            r#""transition":"fade""#,
        ] {
            assert!(out.contains(want), "lost {want} in {out}");
        }
    }

    #[test]
    fn export_zip_and_trash() {
        let dir = tempfile::tempdir().unwrap();
        let s = new_store(dir.path());
        let d = s.create(Document::default()).unwrap();
        assert_eq!(d.title, "Untitled");
        s.snapshot(&d.id).unwrap();
        let mut buf = Vec::new();
        s.export_all(&mut buf).unwrap();
        assert!(buf.len() > 100);
        s.delete(&d.id).unwrap();
        assert!(matches!(s.get(&d.id), Err(DocError::NotFound)));
        assert_eq!(
            std::fs::read_dir(dir.path().join("documents/.trash"))
                .unwrap()
                .count(),
            1
        );
    }
}
