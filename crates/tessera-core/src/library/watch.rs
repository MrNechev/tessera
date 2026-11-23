//! Filesystem watcher: one inotify watch per directory (up to `watch_max_dirs`); the
//! affected folder is rescanned shortly after changes settle. It is a fast path only:
//! periodic rescans still run, because network/array filesystems can miss events and watch
//! limits exist.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

use notify::{EventKind, RecursiveMode, Watcher as _};

use super::{skip_dir, spawn, Manager};

struct State {
    w: notify::RecommendedWatcher,
    count: usize,
    full: bool,
    /// (library, rel dir) → last event
    dirty: HashMap<(String, String), Instant>,
    /// abs dir → library id
    by_watch: HashMap<PathBuf, String>,
}

/// The running watcher; dropping it stops the event thread.
pub(super) struct Watcher {
    st: Arc<Mutex<State>>,
    max: usize,
    stop: Arc<std::sync::atomic::AtomicBool>,
}

impl Drop for Watcher {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

impl Watcher {
    pub(super) fn new(m: &Arc<Manager>) -> anyhow::Result<Watcher> {
        let (tx, rx) = mpsc::channel();
        let w = notify::recommended_watcher(tx)?;
        let st = Arc::new(Mutex::new(State {
            w,
            count: 0,
            full: false,
            dirty: HashMap::new(),
            by_watch: HashMap::new(),
        }));
        let watcher = Watcher {
            st: st.clone(),
            max: m.cfg.watch_max_dirs,
            stop: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        };
        for l in m.libraries() {
            watcher.add_tree(&l.id, &l.root);
        }
        {
            let g = st.lock().unwrap_or_else(|e| e.into_inner());
            tracing::info!(dirs = g.count, limit_reached = g.full, "filesystem watcher started");
        }
        let weak: Weak<Manager> = Arc::downgrade(m);
        let stop = watcher.stop.clone();
        let max = watcher.max;
        spawn("watcher", move || {
            let mut last_flush = Instant::now();
            loop {
                if stop.load(std::sync::atomic::Ordering::SeqCst) {
                    return;
                }
                let Some(m) = weak.upgrade() else { return };
                if m.cancel.is_cancelled() {
                    return;
                }
                match rx.recv_timeout(Duration::from_millis(250)) {
                    Ok(Ok(ev)) => handle(&m, &st, max, ev),
                    Ok(Err(_)) => {}
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => return,
                }
                if last_flush.elapsed() >= Duration::from_secs(1) {
                    last_flush = Instant::now();
                    flush(&m, &st);
                }
            }
        });
        Ok(watcher)
    }

    /// Adds a watch for every directory below `root`.
    pub(super) fn add_tree(&self, lib_id: &str, root: &Path) {
        add_tree(&self.st, self.max, lib_id, root);
    }

    /// Drops the watches of a removed library.
    pub(super) fn remove_lib(&self, lib_id: &str) {
        let mut g = self.st.lock().unwrap_or_else(|e| e.into_inner());
        let dirs: Vec<PathBuf> = g
            .by_watch
            .iter()
            .filter(|(_, id)| *id == lib_id)
            .map(|(p, _)| p.clone())
            .collect();
        for d in dirs {
            let _ = g.w.unwatch(&d);
            g.by_watch.remove(&d);
            g.count = g.count.saturating_sub(1);
        }
        g.dirty.retain(|(id, _), _| id != lib_id);
    }
}

fn add_tree(st: &Mutex<State>, max: usize, lib_id: &str, root: &Path) {
    fn walk(st: &Mutex<State>, max: usize, lib_id: &str, dir: &Path, root: &Path) -> bool {
        if dir != root {
            if let Some(name) = dir.file_name().and_then(|n| n.to_str()) {
                if skip_dir(name) {
                    return true;
                }
            }
        }
        if !add(st, max, lib_id, dir) {
            return false;
        }
        let Ok(rd) = std::fs::read_dir(dir) else { return true };
        let mut subdirs: Vec<PathBuf> = rd
            .flatten()
            .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
            .map(|e| e.path())
            .collect();
        subdirs.sort();
        for d in subdirs {
            if !walk(st, max, lib_id, &d, root) {
                return false;
            }
        }
        true
    }
    walk(st, max, lib_id, root, root);
}

fn add(st: &Mutex<State>, max: usize, lib_id: &str, dir: &Path) -> bool {
    let mut g = st.lock().unwrap_or_else(|e| e.into_inner());
    if g.by_watch.contains_key(dir) {
        return true;
    }
    if g.count >= max {
        g.full = true;
        return false;
    }
    if g.w.watch(dir, RecursiveMode::NonRecursive).is_err() {
        // typically ENOSPC: inotify watch limit reached
        g.full = true;
        return false;
    }
    g.by_watch.insert(dir.to_path_buf(), lib_id.to_string());
    g.count += 1;
    true
}

/// (library id, rel path) for an absolute path inside a library root.
fn lib_for(m: &Manager, abs: &Path) -> Option<(String, String)> {
    for l in m.libraries() {
        if abs == l.root {
            return Some((l.id, String::new()));
        }
        if let Ok(rel) = abs.strip_prefix(&l.root) {
            return Some((l.id, rel.to_string_lossy().replace('\\', "/")));
        }
    }
    None
}

fn handle(m: &Manager, st: &Mutex<State>, max: usize, ev: notify::Event) {
    if matches!(ev.kind, EventKind::Access(notify::event::AccessKind::Open(_)) | EventKind::Access(notify::event::AccessKind::Read)) {
        return;
    }
    for path in &ev.paths {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if name.starts_with('.') {
            continue;
        }
        let mut dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
        let created = matches!(ev.kind, EventKind::Create(_))
            || matches!(ev.kind, EventKind::Modify(notify::event::ModifyKind::Name(notify::event::RenameMode::To)));
        if created {
            if let Ok(md) = std::fs::metadata(path) {
                if md.is_dir() && !skip_dir(&name) {
                    if let Some((id, _)) = lib_for(m, path) {
                        add_tree(st, max, &id, path);
                    }
                    dir = path.clone();
                }
            }
        }
        let removed = matches!(ev.kind, EventKind::Remove(_))
            || matches!(ev.kind, EventKind::Modify(notify::event::ModifyKind::Name(notify::event::RenameMode::From)));
        if removed {
            let mut g = st.lock().unwrap_or_else(|e| e.into_inner());
            if g.by_watch.remove(path).is_some() {
                g.count = g.count.saturating_sub(1);
            }
        }
        let Some((id, rel)) = lib_for(m, &dir) else { continue };
        st.lock()
            .unwrap_or_else(|e| e.into_inner())
            .dirty
            .insert((id, rel), Instant::now());
    }
}

/// Rescans folders that have been quiet for 2 seconds (copies in progress settle).
fn flush(m: &Manager, st: &Mutex<State>) {
    let ready: Vec<(String, String)> = {
        let mut g = st.lock().unwrap_or_else(|e| e.into_inner());
        let ready: Vec<_> = g
            .dirty
            .iter()
            .filter(|(_, t)| t.elapsed() > Duration::from_secs(2))
            .map(|(k, _)| k.clone())
            .collect();
        for k in &ready {
            g.dirty.remove(k);
        }
        ready
    };
    for (id, rel) in ready {
        m.rescan_dir(&id, &rel);
    }
}
