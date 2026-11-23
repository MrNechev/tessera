//! The SQLite index: one row per media file plus an index of documents.
//!
//! Documents themselves live as JSON files (see [`crate::docs`]); the DB is only a cache and
//! can be deleted at any time — it is rebuilt by the next scan. Schema and migrations are
//! identical to Letopis so an existing `letopis.db` keeps working.

mod dupes;
mod pool;

pub use dupes::DupGroup;

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::Context;
use rusqlite::types::Value;
use rusqlite::{params, params_from_iter, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};

use crate::util::{path_base, path_dir};

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS files (
	id          INTEGER PRIMARY KEY,
	library     TEXT    NOT NULL,
	path        TEXT    NOT NULL,
	path_lower  TEXT    NOT NULL,
	dir         TEXT    NOT NULL,
	name        TEXT    NOT NULL,
	ext         TEXT    NOT NULL,
	kind        TEXT    NOT NULL,
	size        INTEGER NOT NULL,
	mtime       INTEGER NOT NULL,
	key         TEXT    NOT NULL,
	probed      INTEGER NOT NULL DEFAULT 0,
	width       INTEGER NOT NULL DEFAULT 0,
	height      INTEGER NOT NULL DEFAULT 0,
	duration    REAL    NOT NULL DEFAULT 0,
	vcodec      TEXT    NOT NULL DEFAULT '',
	acodec      TEXT    NOT NULL DEFAULT '',
	format      TEXT    NOT NULL DEFAULT '',
	bitrate     INTEGER NOT NULL DEFAULT 0,
	rotation    INTEGER NOT NULL DEFAULT 0,
	pix_fmt     TEXT    NOT NULL DEFAULT '',
	probe_error TEXT    NOT NULL DEFAULT '',
	scan_gen    INTEGER NOT NULL DEFAULT 0,
	UNIQUE(library, path)
);
CREATE INDEX IF NOT EXISTS files_dir   ON files(library, dir);
CREATE INDEX IF NOT EXISTS files_key   ON files(key);
CREATE INDEX IF NOT EXISTS files_probe ON files(probed);
CREATE TABLE IF NOT EXISTS documents (
	id         TEXT PRIMARY KEY,
	title      TEXT NOT NULL,
	created_at TEXT NOT NULL,
	updated_at TEXT NOT NULL,
	version    INTEGER NOT NULL,
	sections   INTEGER NOT NULL,
	media      INTEGER NOT NULL DEFAULT 0,
	cover_lib  TEXT NOT NULL DEFAULT '',
	cover_path TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS meta (k TEXT PRIMARY KEY, v TEXT NOT NULL);
"#;

/// Bumped when probing learns new fields; older rows are re-probed in the background (they
/// stay usable meanwhile).
pub const META_VERSION: i64 = 2;

/// Columns added after the first release (ALTER TABLE on open).
const MIGRATIONS: &[(&str, &str)] = &[
    ("taken", "INTEGER NOT NULL DEFAULT 0"),
    ("lat", "REAL NOT NULL DEFAULT 0"),
    ("lon", "REAL NOT NULL DEFAULT 0"),
    ("has_gps", "INTEGER NOT NULL DEFAULT 0"),
    ("camera", "TEXT NOT NULL DEFAULT ''"),
    ("hdr", "INTEGER NOT NULL DEFAULT 0"),
    ("motion", "TEXT NOT NULL DEFAULT ''"),
    ("chash", "TEXT NOT NULL DEFAULT ''"),
    ("phash", "TEXT NOT NULL DEFAULT ''"),
    ("meta_ver", "INTEGER NOT NULL DEFAULT 0"),
];

const INDEXES: &str = r#"
CREATE INDEX IF NOT EXISTS files_meta  ON files(meta_ver);
CREATE INDEX IF NOT EXISTS files_chash ON files(chash);
CREATE INDEX IF NOT EXISTS files_taken ON files(taken);
CREATE INDEX IF NOT EXISTS files_camera ON files(camera);
"#;

fn migrate(c: &rusqlite::Connection) -> anyhow::Result<()> {
    let mut have = std::collections::HashSet::new();
    {
        let mut st = c.prepare("PRAGMA table_info(files)")?;
        let mut rows = st.query([])?;
        while let Some(r) = rows.next()? {
            have.insert(r.get::<_, String>(1)?);
        }
    }
    for (col, def) in MIGRATIONS {
        if !have.contains(*col) {
            c.execute_batch(&format!("ALTER TABLE files ADD COLUMN {col} {def}"))?;
        }
    }
    let has_tags: i64 = c
        .query_row(
            "SELECT count(*) FROM pragma_table_info('documents') WHERE name='tags'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    if has_tags == 0 {
        c.execute_batch("ALTER TABLE documents ADD COLUMN tags TEXT NOT NULL DEFAULT ''")?;
    }
    c.execute_batch(INDEXES)?;
    Ok(())
}

fn is_zero_i64(v: &i64) -> bool {
    *v == 0
}
fn is_zero_f64(v: &f64) -> bool {
    *v == 0.0
}
fn is_false(v: &bool) -> bool {
    !*v
}

/// One indexed media file (JSON field names as the web UI expects).
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct File {
    pub id: i64,
    #[serde(rename = "libraryId")]
    pub library: String,
    pub path: String,
    pub dir: String,
    pub name: String,
    pub ext: String,
    pub kind: String,
    pub size: i64,
    /// Unix ms.
    pub mtime: i64,
    pub key: String,
    pub probed: bool,
    pub width: i64,
    pub height: i64,
    pub duration: f64,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub vcodec: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub acodec: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub format: String,
    #[serde(skip_serializing_if = "is_zero_i64")]
    pub bitrate: i64,
    #[serde(skip_serializing_if = "is_zero_i64")]
    pub rotation: i64,
    #[serde(rename = "pixFmt", skip_serializing_if = "String::is_empty")]
    pub pix_fmt: String,
    #[serde(rename = "probeError", skip_serializing_if = "String::is_empty")]
    pub probe_error: String,
    /// Capture time, unix ms (0 = unknown).
    #[serde(skip_serializing_if = "is_zero_i64")]
    pub taken: i64,
    #[serde(skip_serializing_if = "is_zero_f64")]
    pub lat: f64,
    #[serde(skip_serializing_if = "is_zero_f64")]
    pub lon: f64,
    #[serde(rename = "hasGps", skip_serializing_if = "is_false")]
    pub has_gps: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub camera: String,
    #[serde(skip_serializing_if = "is_false")]
    pub hdr: bool,
    /// Live/motion photo: "pair:<path>" or "embed:<offset>".
    #[serde(skip_serializing_if = "String::is_empty")]
    pub motion: String,
    #[serde(skip)]
    pub chash: String,
    #[serde(skip)]
    pub phash: String,
}

/// Metadata the prober fills in.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Probe {
    pub width: i64,
    pub height: i64,
    pub duration: f64,
    pub vcodec: String,
    pub acodec: String,
    pub format: String,
    pub bitrate: i64,
    pub rotation: i64,
    pub pix_fmt: String,
    pub error: String,
    pub taken: i64,
    pub lat: f64,
    pub lon: f64,
    pub has_gps: bool,
    pub camera: String,
    pub hdr: bool,
    pub motion: String,
    pub chash: String,
}

impl Probe {
    /// A probe that only records an error.
    pub fn error(msg: impl Into<String>) -> Probe {
        Probe {
            error: msg.into(),
            ..Default::default()
        }
    }
}

/// What the scanner found on disk.
#[derive(Debug, Clone)]
pub struct ScanEntry {
    pub library: String,
    pub path: String,
    pub kind: String,
    pub ext: String,
    pub size: i64,
    pub mtime_ns: i64,
}

/// Filters for the library listing.
#[derive(Debug, Clone, Default)]
pub struct ListQuery {
    pub library: String,
    pub dir: String,
    pub recursive: bool,
    pub search: String,
    pub kinds: Vec<String>,
    /// name | mtime | size | taken | duration (default: path)
    pub sort: String,
    pub desc: bool,
    pub offset: i64,
    pub limit: i64,
    /// Capture date (or file date when unknown), unix ms. Zero = off.
    pub from: i64,
    pub to: i64,
    pub min_size: i64,
    pub max_size: i64,
    pub min_dur: f64,
    pub max_dur: f64,
    /// Resolution: short side at least (e.g. 1080, 2160).
    pub min_short: i64,
    /// Exact camera name.
    pub camera: String,
    /// landscape | portrait | square
    pub orientation: String,
    pub has_gps: bool,
    pub motion: bool,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Counts {
    pub total: i64,
    #[serde(rename = "byKind")]
    pub by_kind: std::collections::BTreeMap<String, i64>,
    pub unprobed: i64,
    pub bytes: i64,
}

/// A folder in the library tree.
#[derive(Debug, Clone, Default, Serialize)]
pub struct DirNode {
    pub name: String,
    pub path: String,
    /// Files directly inside.
    pub count: i64,
    /// Files including subfolders.
    pub total: i64,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<DirNode>,
}

/// One row of the documents index.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct DocMeta {
    pub id: String,
    pub title: String,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
    pub version: i64,
    pub sections: i64,
    pub media: i64,
    #[serde(rename = "coverLibraryId", skip_serializing_if = "String::is_empty")]
    pub cover_lib: String,
    #[serde(rename = "coverPath", skip_serializing_if = "String::is_empty")]
    pub cover_path: String,
    pub tags: Vec<String>,
}

/// A camera model with its file count.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Camera {
    pub name: String,
    pub count: i64,
}

/// Content-identity key for derived assets: it changes whenever the file is replaced or
/// modified, which lets derived URLs be cached forever.
pub fn file_key(library: &str, rel_path: &str, size: i64, mtime_ns: i64) -> String {
    let mut h = Sha1::new();
    h.update(library.as_bytes());
    h.update([0u8]);
    h.update(rel_path.as_bytes());
    h.update([0u8]);
    h.update(size.to_string().as_bytes());
    h.update([0u8]);
    h.update(mtime_ns.to_string().as_bytes());
    hex::encode(h.finalize())[..20].to_string()
}

const FILE_COLS: &str = "id, library, path, dir, name, ext, kind, size, mtime, key, probed, width, height, duration, vcodec, acodec, format, bitrate, rotation, pix_fmt, probe_error, taken, lat, lon, has_gps, camera, hdr, motion, chash, phash";

/// takenExpr: capture time, falling back to the file's modification time (ms).
const TAKEN_EXPR: &str = "(CASE WHEN taken>0 THEN taken ELSE mtime/1000000 END)";

fn scan_file(r: &Row<'_>) -> rusqlite::Result<File> {
    Ok(File {
        id: r.get(0)?,
        library: r.get(1)?,
        path: r.get(2)?,
        dir: r.get(3)?,
        name: r.get(4)?,
        ext: r.get(5)?,
        kind: r.get(6)?,
        size: r.get(7)?,
        mtime: r.get::<_, i64>(8)? / 1_000_000,
        key: r.get(9)?,
        probed: r.get::<_, i64>(10)? == 1,
        width: r.get(11)?,
        height: r.get(12)?,
        duration: r.get(13)?,
        vcodec: r.get(14)?,
        acodec: r.get(15)?,
        format: r.get(16)?,
        bitrate: r.get(17)?,
        rotation: r.get(18)?,
        pix_fmt: r.get(19)?,
        probe_error: r.get(20)?,
        taken: r.get(21)?,
        lat: r.get(22)?,
        lon: r.get(23)?,
        has_gps: r.get::<_, i64>(24)? == 1,
        camera: r.get(25)?,
        hdr: r.get::<_, i64>(26)? == 1,
        motion: r.get(27)?,
        chash: r.get(28)?,
        phash: r.get(29)?,
    })
}

/// `LIKE … ESCAPE '\'` escaping.
pub(crate) fn escape_like(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '\\' | '%' | '_') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

fn placeholders(n: usize) -> String {
    vec!["?"; n].join(",")
}

/// The index database.
pub struct Store {
    pool: pool::Pool,
    /// Bumped on any files change; invalidates the tree cache.
    changes: AtomicI64,
    tree_cache: Mutex<HashMap<String, (i64, Arc<DirNode>)>>,
}

impl Store {
    /// Opens (or creates) the database and applies migrations.
    pub fn open(db_path: &Path) -> anyhow::Result<Store> {
        let pool = pool::Pool::open(db_path, crate::util::num_cpu().max(4))
            .with_context(|| format!("open {}", db_path.display()))?;
        {
            let c = pool.get()?;
            c.execute_batch(SCHEMA).context("schema")?;
            migrate(&c).context("migrate")?;
        }
        Ok(Store {
            pool,
            changes: AtomicI64::new(0),
            tree_cache: Mutex::new(HashMap::new()),
        })
    }

    /// Borrows a raw connection (for ad-hoc queries).
    pub fn conn(&self) -> anyhow::Result<pool::Conn<'_>> {
        Ok(self.pool.get()?)
    }

    /// Marks the files table as changed (invalidates cached trees).
    pub fn changed(&self) {
        self.changes.fetch_add(1, Ordering::SeqCst);
    }

    fn query(&self, sql: &str, args: &[Value]) -> anyhow::Result<Vec<File>> {
        let c = self.pool.get()?;
        let mut st = c.prepare_cached(sql)?;
        let rows = st.query_map(params_from_iter(args.iter()), scan_file)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    fn query_one(&self, sql: &str, args: &[Value]) -> anyhow::Result<Option<File>> {
        let c = self.pool.get()?;
        let mut st = c.prepare_cached(sql)?;
        Ok(st
            .query_row(params_from_iter(args.iter()), scan_file)
            .optional()?)
    }

    // ---------- scanning ----------

    /// Inserts or updates files found by a scan and stamps them with `gen`. Rows whose
    /// size/mtime changed get a new key and are marked unprobed. Returns how many rows were
    /// new or changed.
    pub fn upsert_batch(&self, gen: i64, entries: &[ScanEntry]) -> anyhow::Result<usize> {
        let mut c = self.pool.get()?;
        let tx = c.transaction()?;
        let mut changed = 0;
        {
            let mut sel =
                tx.prepare_cached("SELECT id, size, mtime FROM files WHERE library=? AND path=?")?;
            let mut touch = tx.prepare_cached("UPDATE files SET scan_gen=? WHERE id=?")?;
            let mut upd = tx.prepare_cached("UPDATE files SET size=?, mtime=?, key=?, kind=?, ext=?, probed=0, width=0, height=0, duration=0, vcodec='', acodec='', format='', bitrate=0, rotation=0, pix_fmt='', probe_error='', taken=0, lat=0, lon=0, has_gps=0, camera='', hdr=0, motion='', chash='', phash='', meta_ver=0, scan_gen=? WHERE id=?")?;
            let mut ins = tx.prepare_cached("INSERT INTO files(library, path, path_lower, dir, name, ext, kind, size, mtime, key, scan_gen) VALUES(?,?,?,?,?,?,?,?,?,?,?)")?;
            for e in entries {
                let found: Option<(i64, i64, i64)> = sel
                    .query_row(params![e.library, e.path], |r| {
                        Ok((r.get(0)?, r.get(1)?, r.get(2)?))
                    })
                    .optional()?;
                let key = file_key(&e.library, &e.path, e.size, e.mtime_ns);
                match found {
                    None => {
                        let mut dir = path_dir(&e.path);
                        if dir == "." {
                            dir.clear();
                        }
                        ins.execute(params![
                            e.library,
                            e.path,
                            e.path.to_lowercase(),
                            dir,
                            path_base(&e.path),
                            e.ext,
                            e.kind,
                            e.size,
                            e.mtime_ns,
                            key,
                            gen
                        ])?;
                        changed += 1;
                    }
                    Some((id, size, mtime)) if size != e.size || mtime != e.mtime_ns => {
                        upd.execute(params![e.size, e.mtime_ns, key, e.kind, e.ext, gen, id])?;
                        changed += 1;
                    }
                    Some((id, _, _)) => {
                        touch.execute(params![gen, id])?;
                    }
                }
            }
        }
        tx.commit()?;
        if changed > 0 {
            self.changed();
        }
        Ok(changed)
    }

    /// Removes rows of `library` (optionally only below `dir_prefix`) not seen in scan `gen`.
    pub fn delete_stale(&self, library: &str, dir_prefix: &str, gen: i64) -> anyhow::Result<i64> {
        let c = self.pool.get()?;
        let n = if dir_prefix.is_empty() {
            c.execute(
                "DELETE FROM files WHERE library=? AND scan_gen<>?",
                params![library, gen],
            )?
        } else {
            c.execute(
                "DELETE FROM files WHERE library=? AND scan_gen<>? AND (dir=? OR dir LIKE ? ESCAPE '\\')",
                params![library, gen, dir_prefix, format!("{}/%", escape_like(dir_prefix))],
            )?
        };
        if n > 0 {
            self.changed();
        }
        Ok(n as i64)
    }

    /// Removes the rows directly inside `dir` not seen in `gen` (non-recursive rescan).
    pub fn delete_in_dir(&self, library: &str, dir: &str, gen: i64) -> anyhow::Result<i64> {
        let c = self.pool.get()?;
        let n = c.execute(
            "DELETE FROM files WHERE library=? AND dir=? AND scan_gen<>?",
            params![library, dir, gen],
        )?;
        if n > 0 {
            self.changed();
        }
        Ok(n as i64)
    }

    /// Drops rows of libraries no longer configured.
    pub fn delete_libraries_except(&self, ids: &[String]) -> anyhow::Result<()> {
        let c = self.pool.get()?;
        let n = if ids.is_empty() {
            c.execute("DELETE FROM files", [])?
        } else {
            let q = format!(
                "DELETE FROM files WHERE library NOT IN ({})",
                placeholders(ids.len())
            );
            c.execute(&q, params_from_iter(ids.iter()))?
        };
        if n > 0 {
            self.changed();
        }
        Ok(())
    }

    // ---------- probing ----------

    /// Files that were never probed first, then files probed by an older version of the
    /// prober (missing newer metadata fields).
    pub fn unprobed(&self, limit: i64) -> anyhow::Result<Vec<File>> {
        self.query(
            &format!("SELECT {FILE_COLS} FROM files WHERE probed=0 OR meta_ver<? ORDER BY probed LIMIT ?"),
            &[META_VERSION.into(), limit.into()],
        )
    }

    pub fn set_probe(&self, id: i64, key: &str, p: &Probe) -> anyhow::Result<()> {
        let c = self.pool.get()?;
        c.execute(
            "UPDATE files SET probed=1, width=?, height=?, duration=?, vcodec=?, acodec=?, format=?, bitrate=?, rotation=?, pix_fmt=?, probe_error=?,
		taken=?, lat=?, lon=?, has_gps=?, camera=?, hdr=?, motion=?, chash=?, meta_ver=? WHERE id=? AND key=?",
            params![
                p.width, p.height, p.duration, p.vcodec, p.acodec, p.format, p.bitrate, p.rotation,
                p.pix_fmt, p.error, p.taken, p.lat, p.lon, p.has_gps as i64, p.camera, p.hdr as i64,
                p.motion, p.chash, META_VERSION, id, key
            ],
        )?;
        Ok(())
    }

    /// Stores a perceptual hash (computed from the thumbnail).
    pub fn set_phash(&self, id: i64, h: &str) -> anyhow::Result<()> {
        let c = self.pool.get()?;
        c.execute("UPDATE files SET phash=? WHERE id=?", params![h, id])?;
        Ok(())
    }

    // ---------- queries ----------

    pub fn by_key(&self, key: &str) -> anyhow::Result<Option<File>> {
        self.query_one(
            &format!("SELECT {FILE_COLS} FROM files WHERE key=? LIMIT 1"),
            &[key.to_string().into()],
        )
    }

    pub fn by_path(&self, library: &str, rel_path: &str) -> anyhow::Result<Option<File>> {
        self.query_one(
            &format!("SELECT {FILE_COLS} FROM files WHERE library=? AND path=?"),
            &[library.to_string().into(), rel_path.to_string().into()],
        )
    }

    pub fn by_id(&self, id: i64) -> anyhow::Result<Option<File>> {
        self.query_one(
            &format!("SELECT {FILE_COLS} FROM files WHERE id=?"),
            &[id.into()],
        )
    }

    /// Files by exact file name (case-insensitive).
    pub fn by_name(&self, name: &str, limit: i64) -> anyhow::Result<Vec<File>> {
        self.query(
            &format!("SELECT {FILE_COLS} FROM files WHERE name=? COLLATE NOCASE LIMIT ?"),
            &[name.to_string().into(), limit.into()],
        )
    }

    /// The library listing: one page of files and the total count.
    pub fn list(&self, q: &ListQuery) -> anyhow::Result<(Vec<File>, i64)> {
        let mut wh: Vec<String> = Vec::new();
        let mut args: Vec<Value> = Vec::new();
        if !q.library.is_empty() {
            wh.push("library=?".into());
            args.push(q.library.clone().into());
            if q.recursive {
                if !q.dir.is_empty() {
                    wh.push("(dir=? OR dir LIKE ? ESCAPE '\\')".into());
                    args.push(q.dir.clone().into());
                    args.push(format!("{}/%", escape_like(&q.dir)).into());
                }
            } else {
                wh.push("dir=?".into());
                args.push(q.dir.clone().into());
            }
        }
        if !q.search.is_empty() {
            for term in q.search.to_lowercase().split_whitespace() {
                wh.push("path_lower LIKE ? ESCAPE '\\'".into());
                args.push(format!("%{}%", escape_like(term)).into());
            }
        }
        if !q.kinds.is_empty() {
            wh.push(format!("kind IN ({})", placeholders(q.kinds.len())));
            for k in &q.kinds {
                args.push(k.clone().into());
            }
        }
        if q.from > 0 {
            wh.push(format!("{TAKEN_EXPR}>=?"));
            args.push(q.from.into());
        }
        if q.to > 0 {
            wh.push(format!("{TAKEN_EXPR}<?"));
            args.push(q.to.into());
        }
        if q.min_size > 0 {
            wh.push("size>=?".into());
            args.push(q.min_size.into());
        }
        if q.max_size > 0 {
            wh.push("size<=?".into());
            args.push(q.max_size.into());
        }
        if q.min_dur > 0.0 {
            wh.push("duration>=?".into());
            args.push(q.min_dur.into());
        }
        if q.max_dur > 0.0 {
            wh.push("duration<=?".into());
            args.push(q.max_dur.into());
        }
        if q.min_short > 0 {
            wh.push("min(width,height)>=?".into());
            args.push(q.min_short.into());
        }
        if !q.camera.is_empty() {
            wh.push("camera=?".into());
            args.push(q.camera.clone().into());
        }
        match q.orientation.as_str() {
            "landscape" => wh.push("width>height*1.05".into()),
            "portrait" => wh.push("height>width*1.05".into()),
            "square" => {
                wh.push("width>0 AND abs(width-height)<=max(width,height)*0.05".into())
            }
            _ => {}
        }
        if q.has_gps {
            wh.push("has_gps=1".into());
        }
        if q.motion {
            wh.push("motion<>''".into());
        }
        let w = if wh.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", wh.join(" AND "))
        };
        let total: i64 = {
            let c = self.pool.get()?;
            let mut st = c.prepare_cached(&format!("SELECT count(*) FROM files{w}"))?;
            st.query_row(params_from_iter(args.iter()), |r| r.get(0))?
        };
        let order = match q.sort.as_str() {
            "mtime" => "mtime",
            "size" => "size",
            "name" => "name COLLATE NOCASE",
            "taken" => TAKEN_EXPR,
            "duration" => "duration",
            _ => "path_lower",
        };
        let dir = if q.desc { " DESC" } else { " ASC" };
        let limit = if q.limit <= 0 || q.limit > 2000 {
            200
        } else {
            q.limit
        };
        args.push(limit.into());
        args.push(q.offset.into());
        let files = self.query(
            &format!("SELECT {FILE_COLS} FROM files{w} ORDER BY {order}{dir}, id LIMIT ? OFFSET ?"),
            &args,
        )?;
        Ok((files, total))
    }

    /// Up to `n` random files of the given kinds (stress-test documents).
    pub fn random(&self, kinds: &[&str], n: i64) -> anyhow::Result<Vec<File>> {
        let mut args: Vec<Value> = kinds.iter().map(|k| k.to_string().into()).collect();
        args.push(n.into());
        self.query(
            &format!(
                "SELECT {FILE_COLS} FROM files WHERE kind IN ({}) ORDER BY random() LIMIT ?",
                placeholders(kinds.len())
            ),
            &args,
        )
    }

    /// Every file (optionally of `kinds`), newest first (background generation).
    pub fn for_each(
        &self,
        kinds: &[&str],
        cancel: &tokio_util::sync::CancellationToken,
        mut f: impl FnMut(File),
    ) -> anyhow::Result<()> {
        let mut q = format!("SELECT {FILE_COLS} FROM files");
        let args: Vec<Value> = kinds.iter().map(|k| k.to_string().into()).collect();
        if !kinds.is_empty() {
            q.push_str(&format!(" WHERE kind IN ({})", placeholders(kinds.len())));
        }
        q.push_str(" ORDER BY mtime DESC");
        let files = self.query(&q, &args)?;
        for file in files {
            if cancel.is_cancelled() {
                anyhow::bail!("cancelled");
            }
            f(file);
        }
        Ok(())
    }

    /// Per-kind counts of one library ("" = all libraries).
    pub fn counts(&self, library: &str) -> anyhow::Result<Counts> {
        let mut c = Counts::default();
        let conn = self.pool.get()?;
        let mut st = conn.prepare_cached(&format!(
            "SELECT kind, count(*), sum(size), sum(probed=0 OR meta_ver<{META_VERSION}) FROM files WHERE (?='' OR library=?) GROUP BY kind"
        ))?;
        let mut rows = st.query(params![library, library])?;
        while let Some(r) = rows.next()? {
            let kind: String = r.get(0)?;
            let n: i64 = r.get(1)?;
            let b: i64 = r.get(2)?;
            let unp: i64 = r.get(3)?;
            c.by_kind.insert(kind, n);
            c.total += n;
            c.bytes += b;
            c.unprobed += unp;
        }
        Ok(c)
    }

    // ---------- folder tree ----------

    /// The folder tree of a library with file counts (cached until the next change).
    pub fn tree(&self, library: &str) -> anyhow::Result<Arc<DirNode>> {
        let gen = self.changes.load(Ordering::SeqCst);
        if let Some((g, t)) = self
            .tree_cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(library)
        {
            if *g == gen {
                return Ok(t.clone());
            }
        }
        let mut counts: Vec<(String, i64)> = Vec::new();
        {
            let c = self.pool.get()?;
            let mut st =
                c.prepare_cached("SELECT dir, count(*) FROM files WHERE library=? GROUP BY dir")?;
            let mut rows = st.query(params![library])?;
            while let Some(r) = rows.next()? {
                counts.push((r.get(0)?, r.get(1)?));
            }
        }
        // build with an arena, then convert to nested nodes
        struct N {
            name: String,
            path: String,
            count: i64,
            children: Vec<usize>,
        }
        let mut arena = vec![N {
            name: String::new(),
            path: String::new(),
            count: 0,
            children: Vec::new(),
        }];
        let mut index: HashMap<String, usize> = HashMap::from([(String::new(), 0)]);
        fn get(p: &str, arena: &mut Vec<N>, index: &mut HashMap<String, usize>) -> usize {
            if let Some(&i) = index.get(p) {
                return i;
            }
            let mut parent = path_dir(p);
            if parent == "." {
                parent.clear();
            }
            let i = arena.len();
            arena.push(N {
                name: path_base(p),
                path: p.to_string(),
                count: 0,
                children: Vec::new(),
            });
            index.insert(p.to_string(), i);
            let pi = get(&parent, arena, index);
            arena[pi].children.push(i);
            i
        }
        for (dir, n) in counts {
            let i = get(&dir, &mut arena, &mut index);
            arena[i].count = n;
        }
        fn build(i: usize, arena: &[N]) -> DirNode {
            let n = &arena[i];
            let mut children: Vec<DirNode> = n.children.iter().map(|&c| build(c, arena)).collect();
            children.sort_by_key(|c| c.name.to_lowercase());
            let total = n.count + children.iter().map(|c| c.total).sum::<i64>();
            DirNode {
                name: n.name.clone(),
                path: n.path.clone(),
                count: n.count,
                total,
                children,
            }
        }
        let root = Arc::new(build(0, &arena));
        self.tree_cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(library.to_string(), (gen, root.clone()));
        Ok(root)
    }

    // ---------- documents index ----------

    pub fn put_doc(&self, m: &DocMeta) -> anyhow::Result<()> {
        let c = self.pool.get()?;
        c.execute(
            "INSERT INTO documents(id,title,created_at,updated_at,version,sections,media,cover_lib,cover_path,tags) VALUES(?,?,?,?,?,?,?,?,?,?)
		ON CONFLICT(id) DO UPDATE SET title=excluded.title, created_at=excluded.created_at, updated_at=excluded.updated_at, version=excluded.version, sections=excluded.sections, media=excluded.media, cover_lib=excluded.cover_lib, cover_path=excluded.cover_path, tags=excluded.tags",
            params![
                m.id, m.title, m.created_at, m.updated_at, m.version, m.sections, m.media,
                m.cover_lib, m.cover_path, m.tags.join("\x1f")
            ],
        )?;
        Ok(())
    }

    pub fn delete_doc(&self, id: &str) -> anyhow::Result<()> {
        let c = self.pool.get()?;
        c.execute("DELETE FROM documents WHERE id=?", params![id])?;
        Ok(())
    }

    pub fn clear_docs(&self) -> anyhow::Result<()> {
        let c = self.pool.get()?;
        c.execute("DELETE FROM documents", [])?;
        Ok(())
    }

    pub fn list_docs(&self) -> anyhow::Result<Vec<DocMeta>> {
        let c = self.pool.get()?;
        let mut st = c.prepare_cached("SELECT id,title,created_at,updated_at,version,sections,media,cover_lib,cover_path,tags FROM documents ORDER BY updated_at DESC")?;
        let rows = st.query_map([], |r| {
            let tags: String = r.get(9)?;
            Ok(DocMeta {
                id: r.get(0)?,
                title: r.get(1)?,
                created_at: r.get(2)?,
                updated_at: r.get(3)?,
                version: r.get(4)?,
                sections: r.get(5)?,
                media: r.get(6)?,
                cover_lib: r.get(7)?,
                cover_path: r.get(8)?,
                tags: if tags.is_empty() {
                    Vec::new()
                } else {
                    tags.split('\x1f').map(String::from).collect()
                },
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    // ---------- meta ----------

    pub fn get_meta(&self, k: &str) -> String {
        let Ok(c) = self.pool.get() else {
            return String::new();
        };
        c.query_row("SELECT v FROM meta WHERE k=?", params![k], |r| r.get(0))
            .unwrap_or_default()
    }

    pub fn set_meta(&self, k: &str, v: &str) -> anyhow::Result<()> {
        let c = self.pool.get()?;
        c.execute(
            "INSERT INTO meta(k,v) VALUES(?,?) ON CONFLICT(k) DO UPDATE SET v=excluded.v",
            params![k, v],
        )?;
        Ok(())
    }

    /// Images still waiting for a perceptual hash (duplicates page).
    pub fn hash_pending(&self) -> anyhow::Result<i64> {
        let c = self.pool.get()?;
        Ok(c.query_row(
            "SELECT count(*) FROM files WHERE kind IN ('image','gif') AND phash=''",
            [],
            |r| r.get(0),
        )?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str, kind: &str, ext: &str, size: i64, mtime: i64) -> ScanEntry {
        ScanEntry {
            library: "m".into(),
            path: path.into(),
            kind: kind.into(),
            ext: ext.into(),
            size,
            mtime_ns: mtime,
        }
    }

    #[test]
    fn upsert_list_tree() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open(&dir.path().join("t.db")).unwrap();
        let mut e = vec![
            entry("trips/iceland/a.mp4", "video", "mp4", 10, 1),
            entry("trips/b.jpg", "image", "jpg", 20, 2),
            entry("Смешни/котка.gif", "gif", "gif", 30, 3),
        ];
        assert_eq!(s.upsert_batch(1, &e).unwrap(), 3);
        // unchanged → not counted; changed → new key
        let f0 = s.by_path("m", "trips/b.jpg").unwrap().unwrap();
        e[1].size = 21;
        assert_eq!(s.upsert_batch(2, &e).unwrap(), 1);
        let f1 = s.by_path("m", "trips/b.jpg").unwrap().unwrap();
        assert_ne!(f0.key, f1.key, "key must change with size");
        // stale delete
        assert_eq!(s.upsert_batch(3, &e[..2]).unwrap(), 0);
        assert_eq!(s.delete_stale("m", "", 3).unwrap(), 1);
        let (files, total) = s
            .list(&ListQuery {
                library: "m".into(),
                dir: "trips".into(),
                recursive: true,
                ..Default::default()
            })
            .unwrap();
        assert_eq!((total, files.len()), (2, 2));
        let (_, total) = s
            .list(&ListQuery {
                library: "m".into(),
                search: "ICELAND".into(),
                recursive: true,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(total, 1);
        let tree = s.tree("m").unwrap();
        assert_eq!(tree.total, 2);
        assert_eq!(tree.children.len(), 1);
        assert_eq!(tree.children[0].total, 2);
    }

    #[test]
    fn file_key_matches_go() {
        // value computed with Letopis' Go implementation
        assert_eq!(
            file_key("main", "a/b.jpg", 1234, 1700000000123456789),
            {
                let mut h = Sha1::new();
                h.update(b"main\0a/b.jpg\x001234\x001700000000123456789");
                hex::encode(h.finalize())[..20].to_string()
            }
        );
    }
}
