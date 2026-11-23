//! Preferences, duplicates, cameras, document search/usage, matching dropped files and
//! prewarming the next presentation slides.

use std::collections::HashMap;

use axum::body::Body;
use axum::extract::State;
use axum::response::Response;
use serde::{Deserialize, Serialize};

use super::api_media::{FileOut, Ref};
use super::http::{blocking, err, json, read_json, Query};
use super::St;
use crate::engine::Core;
use crate::jobs::Priority;
use crate::library::clean_rel;
use crate::media::{KIND_GIF, KIND_IMAGE, KIND_VIDEO};
use crate::prefs::{Item, Prefs};
use crate::store::File;
use crate::util::{path_base, path_unescape};

/// The hidden folders in prefs in the library manager's format.
pub fn hidden_map(p: &Prefs) -> HashMap<String, Vec<String>> {
    let mut m: HashMap<String, Vec<String>> = HashMap::new();
    for h in &p.hidden {
        if let Ok(c) = clean_rel(&h.path) {
            m.entry(h.library_id.clone()).or_default().push(c);
        }
    }
    m
}

// ---------- prefs: favourites, recent, hidden folders ----------

pub async fn prefs_get(State(c): St) -> Response {
    json(200, &c.prefs.get())
}

fn clean_items(items: &[Item], max: usize) -> Vec<Item> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for it in items {
        let Ok(c) = clean_rel(&it.path) else { continue };
        if it.library_id.is_empty() {
            continue;
        }
        if !seen.insert(format!("{}\0{c}", it.library_id)) {
            continue;
        }
        let mut it = it.clone();
        it.path = c;
        out.push(it);
        if out.len() >= max {
            break;
        }
    }
    out
}

#[derive(Deserialize)]
struct PrefsBody {
    #[serde(default)]
    favorites: Option<Vec<Item>>,
    #[serde(default)]
    hidden: Option<Vec<Item>>,
    /// replace (e.g. clear)
    #[serde(default)]
    recent: Option<Vec<Item>>,
    /// prepend
    #[serde(rename = "addRecent", default)]
    add_recent: Option<Vec<Item>>,
}

pub async fn prefs_put(State(c): St, body: Body) -> Response {
    let Ok(b) = read_json::<PrefsBody>(body, 1 << 20).await else {
        return err(400, "bad request");
    };
    blocking(move || {
        let hidden_changed = b.hidden.is_some();
        let res = c.prefs.update(|p| {
            if let Some(f) = &b.favorites {
                p.favorites = clean_items(f, 2000);
            }
            if let Some(h) = &b.hidden {
                let mut h = clean_items(h, 500);
                for it in &mut h {
                    it.dir = true;
                }
                p.hidden = h;
            }
            if let Some(r) = &b.recent {
                p.recent = clean_items(r, 200);
            }
            if let Some(a) = &b.add_recent {
                if !a.is_empty() {
                    p.add_recent(clean_items(a, 200), crate::util::now_ms());
                }
            }
        });
        match res {
            Ok(p) => {
                if hidden_changed {
                    c.lib.set_hidden(hidden_map(&p));
                    c.lib.trigger_scan();
                }
                json(200, &p)
            }
            Err(e) => err(500, &e.to_string()),
        }
    })
    .await
}

// ---------- duplicates, cameras ----------

#[derive(Serialize)]
struct GroupOut {
    kind: &'static str,
    files: Vec<FileOut>,
}

pub async fn duplicates(State(c): St, q: Query) -> Response {
    blocking(move || {
        let mut dist = q.i64("dist");
        if dist <= 0 || dist > 12 {
            dist = 5;
        }
        let groups = match c.db.duplicates(dist as u32) {
            Ok(g) => g,
            Err(e) => return err(500, &e.to_string()),
        };
        let out: Vec<GroupOut> = groups
            .into_iter()
            .take(1000)
            .map(|g| GroupOut {
                kind: g.kind,
                files: g.files.into_iter().map(|f| c.file_out(f)).collect(),
            })
            .collect();
        let pending = c.db.hash_pending().unwrap_or(0);
        json(200, &serde_json::json!({"groups": out, "hashPending": pending}))
    })
    .await
}

pub async fn cameras(State(c): St) -> Response {
    blocking(move || match c.db.cameras() {
        Ok(cams) => json(200, &cams),
        Err(e) => err(500, &e.to_string()),
    })
    .await
}

// ---------- where is this used / search ----------

pub async fn usage(State(c): St, q: Query) -> Response {
    blocking(move || {
        let p = q.get("path").trim_matches('/').to_string();
        match c.docs.usage(q.get("library"), &p, q.get("dir") == "1") {
            Ok(u) => json(200, &u),
            Err(e) => err(500, &e.to_string()),
        }
    })
    .await
}

pub async fn search(State(c): St, q: Query) -> Response {
    blocking(move || match c.docs.search(q.get("q"), 200) {
        Ok(h) => json(200, &h),
        Err(e) => err(500, &e.to_string()),
    })
    .await
}

// ---------- match: files dropped or pasted from outside ----------

#[derive(Deserialize, Default)]
struct MatchItem {
    #[serde(default)]
    name: String,
    #[serde(default)]
    size: i64,
    #[serde(default)]
    path: String,
}

#[derive(Deserialize)]
struct MatchBody {
    #[serde(default)]
    items: Vec<MatchItem>,
}

#[derive(Serialize)]
struct MatchResult {
    exact: bool,
    candidates: Vec<FileOut>,
}

/// Finds indexed files for a file dropped from the user's computer (by name and size) or
/// for a pasted path ("/media/lib/a/b.jpg", "lib:a/b.jpg", "a/b.jpg" or an absolute host
/// path that ends in a library path).
pub async fn match_files(State(c): St, body: Body) -> Response {
    let b = match read_json::<MatchBody>(body, 1 << 20).await {
        Ok(b) if b.items.len() <= 500 => b,
        _ => return err(400, "bad request"),
    };
    blocking(move || {
        let out: Vec<MatchResult> = b
            .items
            .iter()
            .map(|it| {
                let mut res = MatchResult {
                    exact: false,
                    candidates: Vec::new(),
                };
                if !it.path.is_empty() {
                    res.candidates = match_path(&c, &it.path)
                        .into_iter()
                        .map(|f| c.file_out(f))
                        .collect();
                    res.exact = res.candidates.len() == 1;
                } else if !it.name.is_empty() {
                    if let Ok(mut files) = c.db.by_name(&it.name, 50) {
                        files.sort_by_key(|f| (f.size - it.size).abs());
                        for f in files {
                            if it.size > 0 && f.size == it.size {
                                res.exact = true;
                            }
                            res.candidates.push(c.file_out(f));
                        }
                        if res.exact {
                            // keep only the same-size files
                            res.candidates.retain(|c| c.file.size == it.size);
                        }
                    }
                }
                res
            })
            .collect();
        json(200, &out)
    })
    .await
}

fn match_path(c: &Core, p: &str) -> Vec<File> {
    let mut p = p.trim().to_string();
    if let Some(i) = p.find("/media/") {
        p = p[i + "/media/".len()..].to_string();
        if let Some((lib, rest)) = p.split_once('/') {
            if let Ok(Some(f)) = c.db.by_path(lib, &unescape_path(rest)) {
                return vec![f];
            }
        }
    }
    if let Some((lib, rest)) = p.split_once(':') {
        if !lib.contains('/') {
            if let Ok(Some(f)) = c.db.by_path(lib, rest.trim_start_matches('/')) {
                return vec![f];
            }
        }
    }
    // a path from anywhere: try ever shorter suffixes against each library
    let norm = p.replace('\\', "/");
    let segs: Vec<&str> = norm.trim_matches('/').split('/').collect();
    let libs = c.lib.libraries();
    for i in 0..segs.len() {
        let rel = segs[i..].join("/");
        for l in &libs {
            if let Ok(Some(f)) = c.db.by_path(&l.id, &rel) {
                return vec![f];
            }
        }
    }
    let name = path_base(&p);
    if !name.is_empty() {
        return c.db.by_name(&name, 20).unwrap_or_default();
    }
    Vec::new()
}

fn unescape_path(p: &str) -> String {
    p.split('/')
        .map(|s| path_unescape(s).unwrap_or_else(|| s.to_string()))
        .collect::<Vec<_>>()
        .join("/")
}

// ---------- prewarm: get the next slides ready ----------

#[derive(Deserialize)]
struct RefsBody {
    #[serde(default)]
    refs: Vec<Ref>,
}

pub async fn prewarm(State(c): St, body: Body) -> Response {
    let b = match read_json::<RefsBody>(body, 1 << 20).await {
        Ok(b) if b.refs.len() <= 200 => b,
        _ => return err(400, "bad request"),
    };
    blocking(move || {
        let mut queued = 0;
        for r in &b.refs {
            let Ok(clean) = clean_rel(&r.path) else { continue };
            let Ok(Some(f)) = c.db.by_path(&r.library_id, &clean) else {
                continue;
            };
            let Ok(abs) = c.lib.resolve(&f.library, &f.path) else { continue };
            match f.kind.as_str() {
                KIND_IMAGE => {
                    if c.media.thumb_queue(&f, &abs, 2560).is_ok() {
                        c.media.thumbs.promote(&format!("{}-2560", f.key));
                        queued += 1;
                    }
                }
                KIND_GIF => {
                    if c.media.gif_queue(&f, &abs).is_ok() {
                        c.media.gifs.promote(&f.key);
                        queued += 1;
                    }
                }
                KIND_VIDEO => {
                    let v = c.media.present_variant(&f);
                    if !v.is_empty() {
                        c.media.request_proxy(&f, &abs, v, Priority::High, false);
                        queued += 1;
                    }
                    let _ = c.media.thumb_queue(&f, &abs, 2560);
                }
                _ => {}
            }
        }
        json(200, &serde_json::json!({"queued": queued}))
    })
    .await
}
