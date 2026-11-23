//! The write guard: one place that decides whether a path may be read or
//! written by the tools.
//!
//! Every folder Tessera knows is an [`Area`]: a media library (read-only unless
//! the user made it writable), Outputs, Inbox, or an internal folder (cache,
//! projects, uploads). Rules:
//! - Writing inside a read-only area is always refused, in both modes.
//! - Desktop: paths outside every area are the user's "loose files" and behave
//!   as before (readable and writable).
//! - Server: paths outside every area are refused, so a browser can never reach
//!   the rest of the server's disk.

use std::path::{Component, Path, PathBuf};
use std::sync::RwLock;

use serde::Serialize;

use super::Mode;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AreaKind {
    Library,
    Outputs,
    Inbox,
}

/// A folder the user (or the server's configuration) gave Tessera.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Area {
    pub id: String,
    pub name: String,
    pub root: PathBuf,
    pub writable: bool,
    pub kind: AreaKind,
}

pub struct Guard {
    mode: Mode,
    areas: RwLock<Vec<(Area, PathBuf)>>,
    internal: RwLock<Vec<PathBuf>>,
}

/// Resolve symlinks and `.`/`..` so a path can't escape its area. Paths that
/// don't exist yet are resolved through their nearest existing ancestor.
pub fn resolve(p: &Path) -> Result<PathBuf, String> {
    if !p.is_absolute() {
        return Err(format!("not an absolute path: {}", p.display()));
    }
    if let Ok(c) = p.canonicalize() {
        return Ok(c);
    }
    let mut rest = Vec::new();
    let mut cur = p.to_path_buf();
    loop {
        if let Ok(c) = cur.canonicalize() {
            let mut out = c;
            for part in rest.iter().rev() {
                out.push(part);
            }
            return Ok(out);
        }
        match (cur.file_name(), cur.parent()) {
            (Some(name), Some(parent)) => {
                rest.push(name.to_os_string());
                cur = parent.to_path_buf();
            }
            _ => break,
        }
    }
    // Nothing exists (unusual): normalise lexically, refusing `..`.
    if p.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(format!("invalid path: {}", p.display()));
    }
    Ok(p.to_path_buf())
}

impl Guard {
    pub fn new(mode: Mode) -> Guard {
        Guard {
            mode,
            areas: RwLock::new(Vec::new()),
            internal: RwLock::new(Vec::new()),
        }
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// Replace the known areas (libraries changed in Settings, server start-up).
    pub fn set_areas(&self, areas: Vec<Area>) {
        let resolved = areas
            .into_iter()
            .map(|a| {
                let r = a.root.canonicalize().unwrap_or_else(|_| a.root.clone());
                (a, r)
            })
            .collect();
        *self.areas.write().unwrap_or_else(|e| e.into_inner()) = resolved;
    }

    pub fn areas(&self) -> Vec<Area> {
        self.areas
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .map(|(a, _)| a.clone())
            .collect()
    }

    /// Folders Tessera itself manages (cache, projects, uploads): always allowed.
    pub fn set_internal(&self, dirs: Vec<PathBuf>) {
        let dirs = dirs
            .into_iter()
            .map(|d| {
                let _ = std::fs::create_dir_all(&d);
                d.canonicalize().unwrap_or(d)
            })
            .collect();
        *self.internal.write().unwrap_or_else(|e| e.into_inner()) = dirs;
    }

    /// The innermost area containing `p` (already resolved).
    pub fn area_of(&self, p: &Path) -> Option<Area> {
        self.areas
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .filter(|(_, root)| p.starts_with(root))
            .max_by_key(|(_, root)| root.components().count())
            .map(|(a, _)| a.clone())
    }

    fn internal(&self, p: &Path) -> bool {
        self.internal
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .any(|d| p.starts_with(d))
    }

    /// Check that `p` may be read; returns the resolved path.
    pub fn readable(&self, p: &Path) -> Result<PathBuf, String> {
        let r = resolve(p)?;
        if self.mode == Mode::Desktop || self.internal(&r) || self.area_of(&r).is_some() {
            return Ok(r);
        }
        Err(format!(
            "{} is outside Tessera's libraries, Outputs and Inbox",
            p.display()
        ))
    }

    /// Check that `p` (a file or folder to create or change) may be written.
    pub fn writable(&self, p: &Path) -> Result<PathBuf, String> {
        let r = resolve(p)?;
        if self.internal(&r) {
            return Ok(r);
        }
        match self.area_of(&r) {
            Some(a) if a.writable => Ok(r),
            Some(a) => Err(match self.mode {
                Mode::Desktop => format!(
                    "The library “{}” is read-only in Tessera. Make it writable in Settings → Libraries to save there.",
                    a.name
                ),
                Mode::Server => format!(
                    "The library “{}” is read-only on this server (add “{}” to TESSERA_WRITABLE_LIBRARIES and mount it without :ro to allow writing). Save to Outputs instead.",
                    a.name, a.id
                ),
            }),
            None if self.mode == Mode::Desktop => Ok(r),
            None => Err(format!(
                "{} is outside Tessera's writable folders; save to Outputs instead",
                p.display()
            )),
        }
    }

    /// Whether outputs placed "next to the originals" of `input` are allowed.
    pub fn writable_next_to(&self, input: &Path) -> Result<PathBuf, String> {
        let dir = input.parent().unwrap_or(Path::new("/"));
        self.writable(dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("tessera-guard-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d.canonicalize().unwrap()
    }

    fn area(id: &str, root: &Path, writable: bool) -> Area {
        Area {
            id: id.into(),
            name: id.into(),
            root: root.into(),
            writable,
            kind: AreaKind::Library,
        }
    }

    #[test]
    fn read_only_libraries_refuse_writes_in_both_modes() {
        let root = dir("ro");
        for mode in [Mode::Desktop, Mode::Server] {
            let g = Guard::new(mode);
            g.set_areas(vec![area("lib", &root, false)]);
            assert!(g.readable(&root.join("a.jpg")).is_ok());
            assert!(g.writable(&root.join("a.jpg")).is_err());
            assert!(g.writable(&root.join("new/sub/b.jpg")).is_err());
        }
    }

    #[test]
    fn server_refuses_paths_outside_areas() {
        let root = dir("srv");
        let other = dir("srv-other");
        let g = Guard::new(Mode::Server);
        g.set_areas(vec![area("out", &root, true)]);
        assert!(g.writable(&root.join("x.png")).is_ok());
        assert!(g.readable(&other.join("x.png")).is_err());
        assert!(g.writable(&other.join("x.png")).is_err());
        // `..` can't escape the area.
        assert!(g.readable(&root.join("../srv-other/x.png")).is_err() || !other.exists());
        assert!(g.writable(&root.join("..").join("escape.png")).is_err());
    }

    #[test]
    fn desktop_allows_loose_files() {
        let other = dir("loose");
        let g = Guard::new(Mode::Desktop);
        assert!(g.readable(&other.join("x.png")).is_ok());
        assert!(g.writable(&other.join("x.png")).is_ok());
    }

    #[test]
    fn innermost_area_wins() {
        let root = dir("nest");
        let inner = root.join("out");
        std::fs::create_dir_all(&inner).unwrap();
        let g = Guard::new(Mode::Server);
        g.set_areas(vec![area("lib", &root, false), area("out", &inner, true)]);
        assert!(g.writable(&inner.join("a.png")).is_ok());
        assert!(g.writable(&root.join("a.png")).is_err());
    }
}
