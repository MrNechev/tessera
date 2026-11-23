//! Scans the media roots into the index and keeps it fresh: periodic rescans, a filesystem
//! watcher and on-demand rescans; probes new files and generates grid thumbnails in the
//! background. Libraries can be replaced at runtime ([`Manager::set_libraries`]).

mod watch;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex, RwLock};
use std::time::{Duration, Instant};

use chrono::{DateTime, Local};
use serde::{Serialize, Serializer};
use tokio_util::sync::CancellationToken;

use crate::config::{Config, Library};
use crate::jobs::{Job, Priority};
use crate::media::{self, kind_of, KIND_AUDIO, KIND_GIF, KIND_IMAGE, KIND_VIDEO};
use crate::store::{File, Probe, ScanEntry, Store};
use crate::util::{mtime_ns, path_clean, path_ext, path_match, rfc3339_nano, GO_ZERO_TIME};

/// Why a library path can't be served.
#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    #[error("invalid path")]
    BadPath,
    #[error("unknown library")]
    NoLibrary,
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

/// A configured library plus its symlink-resolved root.
#[derive(Debug, Clone)]
pub struct Lib {
    pub cfg: Library,
    pub real_root: PathBuf,
}

#[derive(Debug, Default)]
struct LibSet {
    order: Vec<String>,
    map: HashMap<String, Arc<Lib>>,
}

/// Progress of the current (or last) full scan.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ScanState {
    pub running: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub library: String,
    pub seen: i64,
    pub changed: i64,
    pub removed: i64,
    #[serde(rename = "startedAt", serialize_with = "go_time")]
    pub started_at: Option<DateTime<Local>>,
    #[serde(rename = "endedAt", serialize_with = "go_time")]
    pub ended_at: Option<DateTime<Local>>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub error: String,
}

fn go_time<S: Serializer>(t: &Option<DateTime<Local>>, s: S) -> Result<S::Ok, S::Error> {
    match t {
        Some(t) => s.serialize_str(&rfc3339_nano(t)),
        None => s.serialize_str(GO_ZERO_TIME),
    }
}

/// A one-slot wake-up signal (Go: `chan struct{}` with buffer 1).
#[derive(Default)]
struct Kick {
    flag: Mutex<bool>,
    cv: Condvar,
}

impl Kick {
    fn kick(&self) {
        *self.flag.lock().unwrap_or_else(|e| e.into_inner()) = true;
        self.cv.notify_all();
    }

    /// Waits for a kick (true), the timeout (false) or cancellation (false).
    fn wait(&self, timeout: Option<Duration>, cancel: &CancellationToken) -> bool {
        let deadline = timeout.map(|t| Instant::now() + t);
        let mut g = self.flag.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if *g {
                *g = false;
                return true;
            }
            if cancel.is_cancelled() {
                return false;
            }
            let mut slice = Duration::from_millis(500);
            if let Some(d) = deadline {
                let now = Instant::now();
                if now >= d {
                    return false;
                }
                slice = slice.min(d - now);
            }
            g = self
                .cv
                .wait_timeout(g, slice)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
    }
}

/// Sleeps up to `d`, returning early (false) on cancellation.
fn sleep_cancel(d: Duration, cancel: &CancellationToken) -> bool {
    let end = Instant::now() + d;
    while Instant::now() < end {
        if cancel.is_cancelled() {
            return false;
        }
        std::thread::sleep((end - Instant::now()).min(Duration::from_millis(200)));
    }
    !cancel.is_cancelled()
}

/// The library manager.
pub struct Manager {
    cfg: Arc<Config>,
    db: Arc<Store>,
    media: Arc<media::Manager>,
    libs: RwLock<Arc<LibSet>>,

    /// One scan (full or folder) at a time.
    scan_mu: Mutex<()>,
    scanning: AtomicBool,
    gen: AtomicI64,
    state: Mutex<ScanState>,

    probe_kick: Kick,
    bg_kick: Kick,
    watcher: Mutex<Option<watch::Watcher>>,

    /// Hidden folders from the UI (library id → folder paths), on top of `cfg.exclude`.
    hidden: RwLock<HashMap<String, Vec<String>>>,
    cancel: CancellationToken,
}

fn make_lib(l: &Library) -> Lib {
    let real_root = match std::fs::canonicalize(&l.root) {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(library = %l.id, root = %l.root.display(), err = %e, "library root not accessible");
            l.root.clone()
        }
    };
    let writable_on_disk = {
        use std::os::unix::ffi::OsStrExt;
        let c = std::ffi::CString::new(real_root.as_os_str().as_bytes()).unwrap_or_default();
        // SAFETY: access(2) only reads the NUL-terminated path.
        unsafe { libc::access(c.as_ptr(), libc::W_OK) == 0 }
    };
    if l.writable && !writable_on_disk {
        tracing::warn!(library = %l.id, root = %l.root.display(), "library is marked writable but its folder is not writable");
    } else if !l.writable && writable_on_disk {
        tracing::warn!(library = %l.id, root = %l.root.display(), "library is writable — Tessera never writes to read-only libraries; mount it read-only (:ro) to be safe");
    }
    Lib {
        cfg: l.clone(),
        real_root,
    }
}

fn lib_set(libs: &[Library]) -> LibSet {
    let mut s = LibSet::default();
    for l in libs {
        if s.map.contains_key(&l.id) {
            continue;
        }
        s.order.push(l.id.clone());
        s.map.insert(l.id.clone(), Arc::new(make_lib(l)));
    }
    s
}

/// Normalises a library-relative path and rejects traversal.
pub fn clean_rel(rel: &str) -> Result<String, ResolveError> {
    if rel.is_empty() || rel.contains('\0') || rel.contains('\\') || rel.starts_with('/') {
        return Err(ResolveError::BadPath);
    }
    if rel.split('/').any(|s| s == "..") {
        return Err(ResolveError::BadPath);
    }
    let c = path_clean(rel);
    if c == "." || c.is_empty() || c.starts_with("../") || c == ".." {
        return Err(ResolveError::BadPath);
    }
    Ok(c)
}

/// Directories that never contain user media.
fn skip_dir(name: &str) -> bool {
    name.starts_with('.')
        || matches!(
            name,
            "@eaDir"
                | "#recycle"
                | "$RECYCLE.BIN"
                | "System Volume Information"
                | "lost+found"
                | "node_modules"
                | "__MACOSX"
        )
}

fn skip_file(name: &str) -> bool {
    name.starts_with('.')
}

impl Manager {
    pub fn new(cfg: Arc<Config>, db: Arc<Store>, media: Arc<media::Manager>, cancel: CancellationToken) -> Manager {
        let libs = lib_set(&cfg.libraries);
        Manager {
            cfg,
            db,
            media,
            libs: RwLock::new(Arc::new(libs)),
            scan_mu: Mutex::new(()),
            scanning: AtomicBool::new(false),
            gen: AtomicI64::new(chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)),
            state: Mutex::new(ScanState::default()),
            probe_kick: Kick::default(),
            bg_kick: Kick::default(),
            watcher: Mutex::new(None),
            hidden: RwLock::new(HashMap::new()),
            cancel,
        }
    }

    fn set(&self) -> Arc<LibSet> {
        self.libs.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// The configured libraries in order.
    pub fn libraries(&self) -> Vec<Library> {
        let s = self.set();
        s.order.iter().map(|id| s.map[id].cfg.clone()).collect()
    }

    /// One library by id.
    pub fn library(&self, id: &str) -> Option<Library> {
        self.set().map.get(id).map(|l| l.cfg.clone())
    }

    pub fn state(&self) -> ScanState {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn set_state(&self, s: ScanState) {
        *self.state.lock().unwrap_or_else(|e| e.into_inner()) = s;
    }

    /// Replaces the folders hidden from the index; the next scan drops their files.
    pub fn set_hidden(&self, h: HashMap<String, Vec<String>>) {
        *self.hidden.write().unwrap_or_else(|e| e.into_inner()) = h;
    }

    /// Whether a library path (folder or file) is excluded from the index.
    fn excluded(&self, lib_id: &str, rel: &str, name: &str) -> bool {
        for pat in &self.cfg.exclude {
            if path_match(pat, name) {
                return true;
            }
            if pat.contains('/') && path_match(pat, rel) {
                return true;
            }
        }
        let h = self.hidden.read().unwrap_or_else(|e| e.into_inner());
        if let Some(dirs) = h.get(lib_id) {
            for d in dirs {
                if rel == d || rel.starts_with(&format!("{d}/")) {
                    return true;
                }
            }
        }
        false
    }

    /// Maps (library, relative path) to an absolute path, rejecting anything that would
    /// escape the library root (.., absolute paths, symlinks pointing outside).
    pub fn resolve(&self, lib_id: &str, rel: &str) -> Result<PathBuf, ResolveError> {
        let set = self.set();
        let lib = set.map.get(lib_id).ok_or(ResolveError::NoLibrary)?;
        let clean = clean_rel(rel)?;
        let abs = lib.cfg.root.join(&clean);
        let real = std::fs::canonicalize(&abs)?;
        if real != lib.real_root && !real.starts_with(&lib.real_root) {
            return Err(ResolveError::BadPath);
        }
        Ok(real)
    }

    // ---------- lifecycle ----------

    /// Runs the initial scan, the periodic rescans, the watcher and the prober (threads).
    pub fn start(self: &Arc<Self>) {
        let ids = self.set().order.clone();
        if let Err(e) = self.db.delete_libraries_except(&ids) {
            tracing::error!(err = %e, "cleanup removed libraries");
        }
        if self.media.has_ffprobe() {
            if let Ok(n) = self.db.reset_tool_missing_probes() {
                if n > 0 {
                    tracing::info!(files = n, "re-probing files that failed while ffprobe was missing");
                }
            }
        }
        let m = self.clone();
        spawn("prober", move || m.prober());
        let m = self.clone();
        spawn("background", move || m.background());
        let m = self.clone();
        spawn("scanner", move || {
            m.scan_all();
            if m.cfg.watch {
                match watch::Watcher::new(&m) {
                    Ok(w) => *m.watcher.lock().unwrap_or_else(|e| e.into_inner()) = Some(w),
                    Err(e) => tracing::warn!(err = %e, "filesystem watcher unavailable"),
                }
            }
            if m.cfg.scan_interval.is_zero() {
                return;
            }
            while sleep_cancel(m.cfg.scan_interval, &m.cancel) {
                m.scan_all();
            }
        });
    }

    /// Replaces the libraries: rows of removed libraries are dropped, new ones are scanned
    /// and the watcher follows.
    pub fn set_libraries(self: &Arc<Self>, libs: Vec<Library>) {
        let old = self.set();
        let mut new = lib_set(&libs);
        // keep resolved roots of unchanged libraries
        for id in &new.order.clone() {
            if let Some(o) = old.map.get(id) {
                if o.cfg.root == new.map[id].cfg.root {
                    let mut l = (**o).clone();
                    l.cfg = new.map[id].cfg.clone();
                    new.map.insert(id.clone(), Arc::new(l));
                }
            }
        }
        let changed: Vec<String> = new
            .order
            .iter()
            .filter(|id| old.map.get(*id).map(|o| o.cfg.root != new.map[*id].cfg.root).unwrap_or(true))
            .cloned()
            .collect();
        let removed: Vec<String> = old
            .order
            .iter()
            .filter(|id| new.map.get(*id).map(|n| n.cfg.root != old.map[*id].cfg.root).unwrap_or(true))
            .cloned()
            .collect();
        let new = Arc::new(new);
        *self.libs.write().unwrap_or_else(|e| e.into_inner()) = new.clone();
        if !removed.is_empty() {
            let keep: Vec<String> = new.order.iter().filter(|id| !changed.contains(id)).cloned().collect();
            if let Err(e) = self.db.delete_libraries_except(&keep) {
                tracing::error!(err = %e, "drop removed libraries");
            }
            if let Some(w) = self.watcher.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
                for id in &removed {
                    w.remove_lib(id);
                }
            }
        }
        if changed.is_empty() {
            return;
        }
        let m = self.clone();
        spawn("scan-new", move || {
            for id in &changed {
                let Some(lib) = m.set().map.get(id).cloned() else { continue };
                {
                    let _g = m.scan_mu.lock().unwrap_or_else(|e| e.into_inner());
                    match m.scan_tree(&lib, "", None) {
                        Ok((seen, ch, _)) => tracing::info!(library = %id, files = seen, changed = ch, "library scanned"),
                        Err((_, _, e)) => tracing::warn!(library = %id, err = %e, "library scan failed"),
                    }
                }
                if let Some(w) = m.watcher.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
                    w.add_tree(id, &lib.cfg.root);
                }
            }
            m.probe_kick.kick();
            m.bg_kick.kick();
        });
    }

    /// Starts a full rescan in the background unless one is running.
    pub fn trigger_scan(self: &Arc<Self>) -> bool {
        if self.scanning.load(Ordering::SeqCst) {
            return false;
        }
        let m = self.clone();
        spawn("scan", move || m.scan_all());
        true
    }

    /// Scans every library (one full scan at a time).
    pub fn scan_all(&self) {
        if self.scanning.swap(true, Ordering::SeqCst) {
            return;
        }
        struct Reset<'a>(&'a AtomicBool);
        impl Drop for Reset<'_> {
            fn drop(&mut self) {
                self.0.store(false, Ordering::SeqCst);
            }
        }
        let _reset = Reset(&self.scanning);
        // also serialises with watcher rescans (they share the stale-row logic)
        let _g = self.scan_mu.lock().unwrap_or_else(|e| e.into_inner());
        let mut st = ScanState {
            running: true,
            started_at: Some(Local::now()),
            ..Default::default()
        };
        self.set_state(st.clone());
        let mut errs = Vec::new();
        let set = self.set();
        for id in &set.order {
            if self.cancel.is_cancelled() {
                return;
            }
            let mut cur = st.clone();
            cur.library = id.clone();
            self.set_state(cur.clone());
            match self.scan_tree(&set.map[id], "", Some(&cur)) {
                Ok((seen, changed, removed)) => {
                    st.seen += seen;
                    st.changed += changed;
                    st.removed += removed;
                }
                Err((seen, changed, e)) => {
                    st.seen += seen;
                    st.changed += changed;
                    errs.push(format!("{id}: {e}"));
                }
            }
        }
        st.running = false;
        st.library.clear();
        st.ended_at = Some(Local::now());
        st.error = errs.join("; ");
        self.set_state(st.clone());
        let took = st
            .ended_at
            .zip(st.started_at)
            .map(|(e, s)| (e - s).num_milliseconds())
            .unwrap_or(0);
        tracing::info!(files = st.seen, changed = st.changed, removed = st.removed, took_ms = took, "scan finished");
        self.probe_kick.kick();
        self.bg_kick.kick();
    }

    /// Walks `lib` below `rel_dir` (recursively) using parallel directory readers and
    /// upserts what it finds. Files under `rel_dir` not seen are removed from the index.
    /// Returns (seen, changed, removed); on error (seen, changed, error).
    #[allow(clippy::type_complexity)]
    fn scan_tree(
        &self,
        lib: &Lib,
        rel_dir: &str,
        live: Option<&ScanState>,
    ) -> Result<(i64, i64, i64), (i64, i64, anyhow::Error)> {
        let gen = self.gen.fetch_add(1, Ordering::SeqCst) + 1;
        let start_abs = if rel_dir.is_empty() {
            lib.cfg.root.clone()
        } else {
            lib.cfg.root.join(rel_dir)
        };
        if let Err(e) = std::fs::metadata(&start_abs) {
            if !rel_dir.is_empty() && e.kind() == std::io::ErrorKind::NotFound {
                let n = self.db.delete_stale(&lib.cfg.id, rel_dir, gen).unwrap_or(0);
                return Ok((0, 0, n));
            }
            return Err((0, 0, e.into()));
        }

        struct Queue {
            dirs: Vec<String>,
            pending: usize,
        }
        let q = Mutex::new(Queue {
            dirs: vec![rel_dir.to_string()],
            pending: 1,
        });
        let cv = Condvar::new();
        let walk_err: Mutex<Option<std::io::Error>> = Mutex::new(None);
        let (tx, rx) = mpsc::sync_channel::<Vec<ScanEntry>>(64);
        let workers = self.cfg.scan_workers.max(1);
        let (mut seen, mut changed) = (0i64, 0i64);
        let mut fail: Option<anyhow::Error> = None;

        std::thread::scope(|sc| {
            for _ in 0..workers {
                let tx = tx.clone();
                let (q, cv, walk_err) = (&q, &cv, &walk_err);
                sc.spawn(move || loop {
                    let d = {
                        let mut g = q.lock().unwrap_or_else(|e| e.into_inner());
                        loop {
                            if let Some(d) = g.dirs.pop() {
                                break Some(d);
                            }
                            if g.pending == 0 {
                                break None;
                            }
                            g = cv.wait(g).unwrap_or_else(|e| e.into_inner());
                        }
                    };
                    let Some(d) = d else { return };
                    let mut push = |child: String| {
                        let mut g = q.lock().unwrap_or_else(|e| e.into_inner());
                        g.pending += 1;
                        g.dirs.push(child);
                        cv.notify_one();
                    };
                    self.read_dir(lib, &d, &mut push, &tx, walk_err);
                    let mut g = q.lock().unwrap_or_else(|e| e.into_inner());
                    g.pending -= 1;
                    if g.pending == 0 {
                        cv.notify_all();
                    }
                });
            }
            drop(tx);
            let mut batch: Vec<ScanEntry> = Vec::with_capacity(2000);
            for entries in rx {
                if fail.is_some() {
                    continue; // drain so the readers finish
                }
                seen += entries.len() as i64;
                batch.extend(entries);
                if batch.len() >= 2000 {
                    match self.db.upsert_batch(gen, &batch) {
                        Ok(n) => changed += n as i64,
                        Err(e) => fail = Some(e),
                    }
                    batch.clear();
                    if let Some(live) = live {
                        let mut cp = live.clone();
                        cp.seen += seen;
                        cp.changed += changed;
                        self.set_state(cp);
                    }
                }
            }
            if fail.is_none() && !batch.is_empty() {
                match self.db.upsert_batch(gen, &batch) {
                    Ok(n) => changed += n as i64,
                    Err(e) => fail = Some(e),
                }
            }
        });
        if let Some(e) = fail {
            return Err((seen, changed, e));
        }
        if self.cancel.is_cancelled() {
            return Err((seen, changed, anyhow::anyhow!("context canceled")));
        }
        if let Some(e) = walk_err.into_inner().unwrap_or_else(|e| e.into_inner()) {
            if rel_dir.is_empty() && seen == 0 {
                // root unreadable: don't wipe the index
                return Err((seen, changed, e.into()));
            }
        }
        let removed = self
            .db
            .delete_stale(&lib.cfg.id, rel_dir, gen)
            .map_err(|e| (seen, changed, e))?;
        if changed > 0 {
            self.probe_kick.kick();
        }
        Ok((seen, changed, removed))
    }

    fn read_dir(
        &self,
        lib: &Lib,
        rel: &str,
        push: &mut dyn FnMut(String),
        results: &mpsc::SyncSender<Vec<ScanEntry>>,
        walk_err: &Mutex<Option<std::io::Error>>,
    ) {
        if self.cancel.is_cancelled() {
            return;
        }
        let abs = if rel.is_empty() {
            lib.cfg.root.clone()
        } else {
            lib.cfg.root.join(rel)
        };
        let rd = match std::fs::read_dir(&abs) {
            Ok(rd) => rd,
            Err(e) => {
                tracing::debug!(dir = %abs.display(), err = %e, "read dir");
                *walk_err.lock().unwrap_or_else(|e| e.into_inner()) = Some(e);
                return;
            }
        };
        let mut entries: Vec<_> = rd.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        let mut out = Vec::new();
        for e in entries {
            let Some(name) = e.file_name().to_str().map(String::from) else {
                tracing::debug!(dir = %abs.display(), "skipping a file name that is not UTF-8");
                continue;
            };
            let child_rel = if rel.is_empty() {
                name.clone()
            } else {
                format!("{rel}/{name}")
            };
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_dir() {
                if !skip_dir(&name) && !self.excluded(&lib.cfg.id, &child_rel, &name) {
                    push(child_rel);
                }
                continue;
            }
            if skip_file(&name) || (!ft.is_file() && !ft.is_symlink()) {
                continue;
            }
            if self.excluded(&lib.cfg.id, &child_rel, &name) {
                continue;
            }
            let (kind, ext) = kind_of(&name);
            if kind.is_empty() {
                continue;
            }
            let md = if ft.is_symlink() {
                // follow symlinked files only if they stay inside the library
                let Ok(real) = std::fs::canonicalize(abs.join(&name)) else { continue };
                if !real.starts_with(&lib.real_root) || real == lib.real_root {
                    continue;
                }
                match std::fs::metadata(&real) {
                    Ok(md) if md.is_file() => md,
                    _ => continue,
                }
            } else {
                match e.metadata() {
                    Ok(md) => md,
                    Err(_) => continue,
                }
            };
            out.push(ScanEntry {
                library: lib.cfg.id.clone(),
                path: child_rel,
                kind: kind.to_string(),
                ext,
                size: md.len() as i64,
                mtime_ns: mtime_ns(&md),
            });
        }
        if !out.is_empty() {
            let _ = results.send(out);
        }
    }

    /// Rescans one directory subtree (used by the watcher).
    pub fn rescan_dir(&self, lib_id: &str, rel_dir: &str) {
        let Some(lib) = self.set().map.get(lib_id).cloned() else {
            return;
        };
        let res = {
            let _g = self.scan_mu.lock().unwrap_or_else(|e| e.into_inner());
            self.scan_tree(&lib, rel_dir, None)
        };
        match res {
            Ok((_, changed, removed)) => {
                if changed > 0 || removed > 0 {
                    tracing::info!(library = lib_id, dir = rel_dir, changed, removed, "folder changed");
                    self.bg_kick.kick();
                }
            }
            Err((_, _, e)) => tracing::debug!(library = lib_id, dir = rel_dir, err = %e, "rescan dir"),
        }
    }

    // ---------- probing ----------

    /// Fills in metadata for unprobed rows using the probe pool.
    fn prober(self: Arc<Self>) {
        loop {
            self.probe_kick.wait(Some(Duration::from_secs(60)), &self.cancel);
            if self.cancel.is_cancelled() {
                return;
            }
            let mut done: HashSet<i64> = HashSet::new();
            while !self.cancel.is_cancelled() {
                let files = match self.db.unprobed(512) {
                    Ok(f) if !f.is_empty() => f,
                    _ => break,
                };
                let mut pending: Vec<Arc<Job>> = Vec::new();
                let mut progress = false;
                for f in files {
                    if !done.insert(f.id) {
                        continue;
                    }
                    progress = true;
                    let abs = match self.resolve(&f.library, &f.path) {
                        Ok(a) => a,
                        Err(e) => {
                            let _ = self
                                .db
                                .set_probe(f.id, &f.key, &Probe::error(format!("unreadable: {}", go_err(&e))));
                            continue;
                        }
                    };
                    let m = self.clone();
                    let key = format!("p{}", f.key);
                    let (id, fkey) = (f.id, f.key.clone());
                    let res = self.media.probes.submit(&key, Priority::Low, false, move |ctx| {
                        let mut p = m.media.probe(&ctx.cancel, &abs, &f.kind, &f.ext);
                        m.pair_live(&f, &mut p);
                        m.db.set_probe(f.id, &f.key, &p)
                    });
                    match res {
                        Ok(j) => pending.push(j),
                        Err(e) => {
                            let _ = self.db.set_probe(id, &fkey, &Probe::error(e.to_string()));
                        }
                    }
                }
                for j in pending {
                    loop {
                        if j.wait_timeout(Duration::from_millis(500)).is_some() {
                            break;
                        }
                        if self.cancel.is_cancelled() {
                            return;
                        }
                    }
                }
                self.db.changed();
                if !progress {
                    break;
                }
            }
        }
    }

    /// Probes a single file synchronously (e.g. when a document needs it right away).
    pub fn probe_now(&self, f: File) -> anyhow::Result<File> {
        if f.probed {
            return Ok(f);
        }
        let abs = self.resolve(&f.library, &f.path)?;
        let mut p = self.media.probe(&self.cancel, &abs, &f.kind, &f.ext);
        self.pair_live(&f, &mut p);
        self.db.set_probe(f.id, &f.key, &p)?;
        Ok(self.db.by_id(f.id)?.unwrap_or(f))
    }

    /// Links an Apple Live Photo (HEIC/JPEG) to its short .MOV with the same name.
    fn pair_live(&self, f: &File, p: &mut Probe) {
        if f.kind != KIND_IMAGE || !p.motion.is_empty() || !p.error.is_empty() {
            return;
        }
        let stem = &f.name[..f.name.len() - path_ext(&f.name).len()];
        if let Ok(Some(v)) = self.db.sibling_video(&f.library, &f.dir, stem) {
            if !v.is_empty() {
                p.motion = format!("pair:{v}");
            }
        }
    }

    // ---------- background generation ----------

    /// Generates grid thumbnails (and GIF videos) for the whole library after scans.
    fn background(self: Arc<Self>) {
        if !self.cfg.background_gen {
            return;
        }
        loop {
            if !self.bg_kick.wait(None, &self.cancel) {
                return;
            }
            // wait for probing to settle so videos have durations for poster seeking
            for _ in 0..600 {
                if self.cancel.is_cancelled() {
                    return;
                }
                match self.db.counts("") {
                    Ok(c) if c.unprobed > 0 => {
                        if !sleep_cancel(Duration::from_secs(2), &self.cancel) {
                            return;
                        }
                    }
                    _ => break,
                }
            }
            let mut queued = 0;
            let _ = self.db.for_each(&[], &self.cancel, |f| {
                let Ok(abs) = self.resolve(&f.library, &f.path) else { return };
                if (!f.probe_error.is_empty() && f.kind == KIND_VIDEO) || f.kind == KIND_AUDIO {
                    return;
                }
                if !media_exists(&self.media.thumb_path(&f.key, 320))
                    && self.media.thumb_queue(&f, &abs, 320).is_ok()
                {
                    queued += 1;
                }
                if f.kind == KIND_GIF {
                    let _ = self.media.gif_queue(&f, &abs);
                }
            });
            if queued > 0 {
                tracing::info!(count = queued, "background thumbnails queued");
            }
            self.hash_thumbs();
        }
    }

    /// Computes perceptual hashes (duplicate detection) from existing grid thumbnails, a
    /// few hundred at a time, waiting for thumbnails that are still queued.
    fn hash_thumbs(&self) {
        let mut idle = 0;
        while !self.cancel.is_cancelled() && idle < 30 {
            let files = match self.db.missing_phash(500) {
                Ok(f) if !f.is_empty() => f,
                _ => return,
            };
            let mut did = 0;
            for f in files {
                let p = self.media.thumb_path(&f.key, 320);
                if !media_exists(&p) {
                    continue;
                }
                // unusable (tiny/broken): don't retry
                let h = media::phash(&p).unwrap_or_else(|_| "-".into());
                let _ = self.db.set_phash(f.id, &h);
                did += 1;
            }
            if did == 0 {
                idle += 1;
                if !sleep_cancel(Duration::from_secs(20), &self.cancel) {
                    return;
                }
            }
        }
    }

    /// Stops the watcher (background threads stop with the cancellation token).
    pub fn stop(&self) {
        self.watcher.lock().unwrap_or_else(|e| e.into_inner()).take();
        self.probe_kick.kick();
        self.bg_kick.kick();
    }
}

fn media_exists(p: &Path) -> bool {
    std::fs::metadata(p).map(|m| m.len() > 0).unwrap_or(false)
}

/// Go-like error text for path errors.
fn go_err(e: &ResolveError) -> String {
    match e {
        ResolveError::Io(io) if io.kind() == std::io::ErrorKind::NotFound => {
            "no such file or directory".into()
        }
        other => other.to_string(),
    }
}

fn spawn(name: &str, f: impl FnOnce() + Send + 'static) {
    if let Err(e) = std::thread::Builder::new().name(name.into()).spawn(f) {
        tracing::error!(err = %e, thread = name, "could not start thread");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_rel_cases() {
        for (inp, want) in [
            ("a/b.mp4", "a/b.mp4"),
            ("a//b.mp4", "a/b.mp4"),
            ("./a.jpg", "a.jpg"),
            ("Смешни/котка.gif", "Смешни/котка.gif"),
            ("a/./b", "a/b"),
        ] {
            assert_eq!(clean_rel(inp).unwrap(), want, "{inp}");
        }
        for bad in ["", "/etc/passwd", "../x", "a/../../x", "a/..", "..", "a\\..\\b", "a\0b", "."] {
            assert!(clean_rel(bad).is_err(), "{bad:?} should fail");
        }
    }

    #[test]
    fn kick_and_scan_state_json() {
        let k = Kick::default();
        let c = CancellationToken::new();
        assert!(!k.wait(Some(Duration::from_millis(10)), &c));
        k.kick();
        assert!(k.wait(Some(Duration::from_millis(10)), &c));
        let js = serde_json::to_string(&ScanState::default()).unwrap();
        assert_eq!(js, r#"{"running":false,"seen":0,"changed":0,"removed":0,"startedAt":"0001-01-01T00:00:00Z","endedAt":"0001-01-01T00:00:00Z"}"#);
    }
}
