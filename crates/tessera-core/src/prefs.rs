//! The user's library preferences in `<data>/prefs.json`: favourite files and folders,
//! recently inserted media and folders hidden from the library. They live on the server so
//! they follow you to every computer.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

fn is_false(v: &bool) -> bool {
    !*v
}
fn is_zero(v: &i64) -> bool {
    *v == 0
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Item {
    #[serde(rename = "libraryId", default)]
    pub library_id: String,
    #[serde(default)]
    pub path: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub dir: bool,
    /// Unix ms.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub at: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Prefs {
    #[serde(default, deserialize_with = "null_vec")]
    pub favorites: Vec<Item>,
    #[serde(default, deserialize_with = "null_vec")]
    pub recent: Vec<Item>,
    /// Hidden folders are excluded from the index (like TESSERA_EXCLUDE).
    #[serde(default, deserialize_with = "null_vec")]
    pub hidden: Vec<Item>,
}

fn null_vec<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<Item>, D::Error> {
    Ok(Option::<Vec<Item>>::deserialize(d)?.unwrap_or_default())
}

const MAX_RECENT: usize = 200;

/// The preferences file.
pub struct Store {
    path: PathBuf,
    p: Mutex<Prefs>,
}

impl Store {
    pub fn open(data_dir: &Path) -> anyhow::Result<Store> {
        let path = data_dir.join("prefs.json");
        let mut p = Prefs::default();
        match std::fs::read(&path) {
            Ok(b) if !b.is_empty() => p = serde_json::from_slice(&b)?,
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        norm(&mut p);
        Ok(Store {
            path,
            p: Mutex::new(p),
        })
    }

    pub fn get(&self) -> Prefs {
        self.p.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Applies `f` to a copy and saves it atomically.
    pub fn update(&self, f: impl FnOnce(&mut Prefs)) -> anyhow::Result<Prefs> {
        let mut g = self.p.lock().unwrap_or_else(|e| e.into_inner());
        let mut p = g.clone();
        f(&mut p);
        norm(&mut p);
        *g = p.clone();
        let b = serde_json::to_vec_pretty(&p)?;
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, b)?;
        std::fs::rename(&tmp, &self.path)?;
        Ok(p)
    }
}

fn norm(p: &mut Prefs) {
    p.recent.truncate(MAX_RECENT);
}

impl Prefs {
    /// Moves items to the front of the recent list.
    pub fn add_recent(&mut self, items: Vec<Item>, now: i64) {
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for mut it in items {
            let k = format!("{}\0{}", it.library_id, it.path);
            if !seen.insert(k) {
                continue;
            }
            it.at = now;
            out.push(it);
        }
        for it in &self.recent {
            if !seen.contains(&format!("{}\0{}", it.library_id, it.path)) {
                out.push(it.clone());
            }
        }
        out.truncate(MAX_RECENT);
        self.recent = out;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_recent() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open(dir.path()).unwrap();
        assert_eq!(serde_json::to_string(&s.get()).unwrap(), r#"{"favorites":[],"recent":[],"hidden":[]}"#);
        let item = |p: &str| Item { library_id: "m".into(), path: p.into(), dir: false, at: 0 };
        s.update(|p| p.add_recent(vec![item("a"), item("b"), item("a")], 5)).unwrap();
        s.update(|p| p.add_recent(vec![item("b")], 7)).unwrap();
        let p = Store::open(dir.path()).unwrap().get();
        let names: Vec<_> = p.recent.iter().map(|i| (i.path.as_str(), i.at)).collect();
        assert_eq!(names, [("b", 7), ("a", 5)]);
    }
}
