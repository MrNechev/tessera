//! Libraries, listing, file details, resolving document references, proxies, original and
//! derived media streaming, and the status page.

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, Method};
use axum::response::Response;
use serde::{Deserialize, Serialize};

use super::http::{blocking, err, json, read_json, serve_content, set, Query, ReqInfo, Source};
use super::St;
use crate::engine::Core;
use crate::jobs::{JobError, Priority};
use crate::library::{clean_rel, ResolveError};
use crate::media::{self, valid_thumb_size, ProxyState, KIND_AUDIO, KIND_GIF, KIND_VIDEO};
use crate::store::{Counts, File, ListQuery};
use crate::util::{format_go_duration, mtime_ns, path_escape};

/// A file as the UI sees it.
#[derive(Debug, Clone, Serialize)]
pub struct FileOut {
    #[serde(flatten)]
    pub file: File,
    pub playable: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub place: String,
}

impl Core {
    pub(crate) fn file_out(&self, f: File) -> FileOut {
        FileOut {
            playable: self.media.playable(&f),
            place: if f.has_gps {
                crate::geo::place(f.lat, f.lon)
            } else {
                String::new()
            },
            file: f,
        }
    }
}

fn escape_path(p: &str) -> String {
    p.split('/').map(path_escape).collect::<Vec<_>>().join("/")
}

pub(crate) fn original_url(lib: &str, p: &str) -> String {
    format!("/media/{lib}/{}", escape_path(p))
}

// ---------- libraries ----------

#[derive(Serialize)]
struct LibOut {
    id: String,
    name: String,
    writable: bool,
    counts: Counts,
}

fn lib_outs(c: &Core) -> Vec<LibOut> {
    c.lib
        .libraries()
        .into_iter()
        .map(|l| LibOut {
            counts: c.db.counts(&l.id).unwrap_or_default(),
            id: l.id,
            name: l.name,
            writable: l.writable,
        })
        .collect()
}

pub async fn libraries(State(c): St) -> Response {
    blocking(move || json(200, &lib_outs(&c))).await
}

pub async fn tree(State(c): St, Path(lib): Path<String>) -> Response {
    blocking(move || match c.db.tree(&lib) {
        Ok(t) => json(200, &*t),
        Err(e) => err(500, &e.to_string()),
    })
    .await
}

pub async fn files(State(c): St, q: Query) -> Response {
    blocking(move || {
        let truthy = |k: &str| q.get(k) == "1" || q.get(k) == "true";
        let mut lq = ListQuery {
            library: q.get("library").to_string(),
            dir: q.get("dir").trim_matches('/').to_string(),
            recursive: truthy("recursive"),
            search: q.get("q").trim().to_string(),
            sort: q.get("sort").to_string(),
            desc: truthy("desc"),
            offset: q.i64("offset"),
            limit: q.i64("limit"),
            from: q.i64("from"),
            to: q.i64("to"),
            min_size: q.i64("minSize"),
            max_size: q.i64("maxSize"),
            min_dur: q.f64("minDur"),
            max_dur: q.f64("maxDur"),
            min_short: q.i64("minShort"),
            camera: q.get("camera").to_string(),
            orientation: q.get("orientation").to_string(),
            has_gps: q.get("gps") == "1",
            motion: q.get("motion") == "1",
            ..Default::default()
        };
        let k = q.get("kind");
        if !k.is_empty() && k != "all" {
            lq.kinds = k.split(',').map(String::from).collect();
        }
        if !lq.search.is_empty() && lq.dir.is_empty() {
            lq.recursive = true;
        }
        match c.db.list(&lq) {
            Ok((files, total)) => {
                let items: Vec<FileOut> = files.into_iter().map(|f| c.file_out(f)).collect();
                json(
                    200,
                    &serde_json::json!({"total": total, "offset": lq.offset, "items": items}),
                )
            }
            Err(e) => err(500, &e.to_string()),
        }
    })
    .await
}

pub async fn file(State(c): St, Path(id): Path<String>) -> Response {
    blocking(move || {
        let id: i64 = id.parse().unwrap_or(0);
        match c.db.by_id(id) {
            Ok(Some(f)) => json(200, &c.file_out(f)),
            _ => err(404, "not found"),
        }
    })
    .await
}

pub async fn scan(State(c): St) -> Response {
    let started = c.lib.trigger_scan();
    json(202, &serde_json::json!({"started": started, "state": c.lib.state()}))
}

// ---------- resolve: document references → playable sources ----------

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct Ref {
    #[serde(rename = "libraryId", default)]
    pub library_id: String,
    #[serde(default)]
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Source2 {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub url: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub variant: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proxy: Option<ProxyState>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Resolved {
    #[serde(rename = "libraryId")]
    pub library_id: String,
    pub path: String,
    pub found: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<FileOut>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub original: String,
    /// "/d/<key>/" prefix for thumbnails.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub derived: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub gif: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<Source2>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub present: Option<Source2>,
    /// Hover clip of a live/motion photo.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub motion: String,
    /// Video sprite sheet.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub sprite: String,
}

impl Core {
    pub(crate) fn resolve_ref(&self, r: &Ref) -> Resolved {
        let mut res = Resolved {
            library_id: r.library_id.clone(),
            path: r.path.clone(),
            ..Default::default()
        };
        let Ok(clean) = clean_rel(&r.path) else { return res };
        let Ok(Some(mut f)) = self.db.by_path(&r.library_id, &clean) else {
            return res;
        };
        if !f.probed {
            if let Ok(pf) = self.lib.probe_now(f.clone()) {
                f = pf;
            }
        }
        let fo = self.file_out(f.clone());
        let playable = fo.playable;
        res.found = true;
        res.file = Some(fo);
        res.original = original_url(&f.library, &f.path);
        res.derived = format!("/d/{}/", f.key);
        match f.kind.as_str() {
            KIND_GIF => res.gif = format!("{}gif.mp4", res.derived),
            KIND_VIDEO => {
                res.doc = Some(self.source(&f, self.media.doc_variant(&f), playable));
                res.present = Some(self.source(&f, self.media.present_variant(&f), playable));
                if f.duration > 0.0 {
                    res.sprite = format!("{}sprite.jpg", res.derived);
                }
            }
            KIND_AUDIO => {
                let s = Source2 {
                    url: res.original.clone(),
                    ..Default::default()
                };
                res.doc = Some(s.clone());
                res.present = Some(s);
            }
            _ => {}
        }
        if !f.motion.is_empty() {
            res.motion = format!("{}motion.mp4", res.derived);
        }
        res
    }

    fn source(&self, f: &File, variant: &str, playable: bool) -> Source2 {
        if variant.is_empty() {
            if !playable {
                return Source2::default(); // unprobed/broken: nothing to play
            }
            return Source2 {
                url: original_url(&f.library, &f.path),
                ..Default::default()
            };
        }
        let st = self.media.proxy_status(f, variant);
        let mut src = Source2 {
            variant: variant.to_string(),
            ..Default::default()
        };
        if st.status == "ready" {
            src.url = format!("/d/{}/{variant}.mp4", f.key);
        } else if playable {
            src.url = original_url(&f.library, &f.path); // play the original until the proxy exists
        }
        src.proxy = Some(st);
        src
    }

    /// Prepares proxies for every video a document uses (background priority).
    pub(crate) fn queue_doc_proxies(self: &Arc<Self>, refs: &[Ref]) {
        if !self.cfg.proxy_doc_videos {
            return;
        }
        for r in refs {
            let Ok(Some(mut f)) = self.db.by_path(&r.library_id, &r.path) else {
                continue;
            };
            if f.kind == KIND_GIF {
                if let Ok(abs) = self.lib.resolve(&f.library, &f.path) {
                    let _ = self.media.gif_queue(&f, &abs);
                }
                continue;
            }
            if f.kind != KIND_VIDEO {
                continue;
            }
            if !f.probed {
                match self.lib.probe_now(f) {
                    Ok(pf) => f = pf,
                    Err(_) => continue,
                }
            }
            let Ok(abs) = self.lib.resolve(&f.library, &f.path) else { continue };
            for v in [self.media.doc_variant(&f), self.media.present_variant(&f)] {
                if !v.is_empty() {
                    self.media.request_proxy(&f, &abs, v, Priority::Low, false);
                }
            }
        }
    }
}

#[derive(Deserialize)]
struct ResolveBody {
    #[serde(default)]
    refs: Vec<Ref>,
}

pub async fn resolve(State(c): St, body: Body) -> Response {
    let Ok(b) = read_json::<ResolveBody>(body, 8 << 20).await else {
        return err(400, "bad request");
    };
    if b.refs.len() > 5000 {
        return err(400, "too many references");
    }
    blocking(move || {
        let out: Vec<Resolved> = b.refs.iter().map(|r| c.resolve_ref(r)).collect();
        json(200, &out)
    })
    .await
}

#[derive(Deserialize)]
struct ProxyBody {
    #[serde(default)]
    key: String,
    #[serde(default)]
    variant: String,
    #[serde(default)]
    retry: bool,
}

pub async fn proxy(State(c): St, body: Body) -> Response {
    let Ok(b) = read_json::<ProxyBody>(body, 4096).await else {
        return err(400, "bad request");
    };
    blocking(move || {
        let Ok(Some(f)) = c.db.by_key(&b.key) else {
            return err(404, "file not found (it may have changed; reload)");
        };
        let Ok(abs) = c.lib.resolve(&f.library, &f.path) else {
            return err(404, "file not readable");
        };
        json(200, &c.media.request_proxy(&f, &abs, &b.variant, Priority::High, b.retry))
    })
    .await
}

// ---------- streaming ----------

/// Streams an original file with Range support (seeking in multi-GB videos without
/// loading them) and ETag/Last-Modified revalidation. Only indexed media is served.
pub async fn original(
    State(c): St,
    Path((lib, rel)): Path<(String, String)>,
    q: Query,
    method: Method,
    req_headers: HeaderMap,
) -> Response {
    let c2 = c.clone();
    let lib2 = lib.clone();
    let rel2 = rel.clone();
    let found = blocking(move || -> Result<(std::path::PathBuf, std::fs::Metadata), Response> {
        let clean = clean_rel(&rel2).map_err(|_| err(403, "forbidden"))?;
        match c2.db.by_path(&lib2, &clean) {
            Ok(Some(_)) => {}
            _ => return Err(err(404, "not found")),
        }
        let abs = c2.lib.resolve(&lib2, &rel2).map_err(|e| match e {
            ResolveError::BadPath => err(403, "forbidden"),
            _ => err(404, "not found"),
        })?;
        let md = std::fs::metadata(&abs).map_err(|_| err(404, "not found"))?;
        if !md.is_file() {
            return Err(err(404, "not found"));
        }
        Ok((abs, md))
    })
    .await;
    let (abs, md) = match found {
        Ok(v) => v,
        Err(r) => return r,
    };
    let (_, ext) = media::kind_of(&abs.to_string_lossy());
    let mut h = HeaderMap::new();
    set(&mut h, header::CONTENT_TYPE, media::mime(&ext));
    set(&mut h, header::ETAG, &format!("\"{:x}-{:x}\"", md.len(), mtime_ns(&md)));
    set(&mut h, header::CACHE_CONTROL, "private, no-cache");
    set(&mut h, header::ACCEPT_RANGES, "bytes");
    if q.get("download") == "1" {
        let name = abs
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        set(
            &mut h,
            header::CONTENT_DISPOSITION,
            &format!("attachment; filename*=UTF-8''{}", path_escape(&name)),
        );
    }
    serve_content(&method, &req_headers, h, "", md.modified().ok(), Source::File(abs)).await
}

fn base36(mut n: u64) -> String {
    if n == 0 {
        return "0".into();
    }
    let digits = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut out = Vec::new();
    while n > 0 {
        out.push(digits[(n % 36) as usize]);
        n /= 36;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

pub(crate) async fn serve_immutable(req: &(Method, HeaderMap), p: std::path::PathBuf, ctype: &str) -> Response {
    let Ok(md) = tokio::fs::metadata(&p).await else {
        return err(404, "not found");
    };
    let mut h = HeaderMap::new();
    set(&mut h, header::CONTENT_TYPE, ctype);
    set(&mut h, header::CACHE_CONTROL, "private, max-age=31536000, immutable");
    set(&mut h, header::ETAG, &format!("\"{}\"", base36(md.len())));
    serve_content(&req.0, &req.1, h, "", md.modified().ok(), Source::File(p)).await
}

fn no_store(mut r: Response) -> Response {
    set(r.headers_mut(), header::CACHE_CONTROL, "no-store");
    r
}

/// The client went away (or nobody wanted the job anymore): nothing to send.
fn abandoned() -> Response {
    no_store(Response::new(Body::empty()))
}

/// Serves `/d/<key>/t320.webp`, `gif.mp4`, `preview.mp4`, `full.mp4`, `sprite.jpg`,
/// `motion.mp4`. Thumbnails and GIF videos are generated on demand at high priority.
pub async fn derived(
    State(c): St,
    Path((key, name)): Path<(String, String)>,
    method: Method,
    headers: HeaderMap,
) -> Response {
    let req = (method, headers);
    if key.len() != 20 {
        return err(404, "not found");
    }
    // fast path: proxies only need the file to exist
    if name == "preview.mp4" || name == "full.mp4" {
        let variant = name.trim_end_matches(".mp4");
        if let Some(p) = c.media.proxy_path_if_ready(&key, variant) {
            c.media.proxy_stats.hit();
            return serve_immutable(&req, p, "video/mp4").await;
        }
        c.media.proxy_stats.miss();
        return err(404, "proxy not ready");
    }
    let c2 = c.clone();
    let k2 = key.clone();
    let looked = blocking(move || -> Result<(File, std::path::PathBuf), Response> {
        let f = match c2.db.by_key(&k2) {
            Ok(Some(f)) => f,
            _ => return Err(no_store(err(404, "unknown file"))),
        };
        let abs = c2.lib.resolve(&f.library, &f.path).map_err(|_| err(404, "not found"))?;
        Ok((f, abs))
    })
    .await;
    let (mut f, abs) = match looked {
        Ok(v) => v,
        Err(r) => return r,
    };
    let limit = Duration::from_secs(120);
    let m = c.media.clone();
    if let Some(size) = name
        .strip_prefix('t')
        .and_then(|s| s.strip_suffix(".webp"))
    {
        let size: i64 = size.parse().unwrap_or(0);
        if !valid_thumb_size(size) {
            return err(404, "not found");
        }
        if f.kind == KIND_VIDEO && !f.probed {
            let c3 = c.clone();
            let f2 = f.clone();
            if let Ok(pf) = blocking(move || c3.lib.probe_now(f2)).await {
                f = pf;
            }
        }
        return match tokio::time::timeout(limit, m.thumb(&f, &abs, size)).await {
            Ok(Ok(p)) => serve_immutable(&req, p, "image/webp").await,
            Ok(Err(JobError::Cancelled | JobError::Dropped)) => abandoned(),
            _ if f.kind == KIND_AUDIO => no_store(err(404, "no cover art")),
            _ => no_store(err(500, "thumbnail failed")),
        };
    }
    match name.as_str() {
        "sprite.jpg" if f.kind == KIND_VIDEO => {
            match tokio::time::timeout(limit, m.sprite(&f, &abs)).await {
                Ok(Ok(p)) => serve_immutable(&req, p, "image/jpeg").await,
                _ => no_store(err(500, "sprite failed")),
            }
        }
        "motion.mp4" if !f.motion.is_empty() => {
            let mut pair = None;
            if let Some(rel) = f.motion.strip_prefix("pair:") {
                match c.lib.resolve(&f.library, rel) {
                    Ok(p) => pair = Some(p),
                    Err(_) => return err(404, "clip missing"),
                }
            }
            match tokio::time::timeout(limit, m.motion(&f, &abs, pair)).await {
                Ok(Ok(p)) => serve_immutable(&req, p, "video/mp4").await,
                _ => no_store(err(500, "clip failed")),
            }
        }
        "gif.mp4" if f.kind == KIND_GIF => match tokio::time::timeout(limit, m.gif(&f, &abs)).await {
            Ok(Ok(p)) => serve_immutable(&req, p, "video/mp4").await,
            _ => no_store(err(500, "conversion failed")),
        },
        _ => err(404, "not found"),
    }
}

// ---------- status ----------

fn proc_status() -> (i64, i64) {
    let s = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    let field = |k: &str| -> i64 {
        s.lines()
            .find(|l| l.starts_with(k))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|v| v.parse().ok())
            .unwrap_or(0)
    };
    (field("Threads:"), field("VmRSS:") / 1024)
}

#[derive(Serialize)]
struct ActiveOut {
    key: String,
    progress: f64,
    seconds: f64,
    #[serde(skip_serializing_if = "String::is_empty")]
    file: String,
}

pub async fn status(State(c): St, ri: ReqInfo) -> Response {
    blocking(move || {
        let (hw, hw_note) = c.media.hw();
        let counts = c.db.counts("").unwrap_or_default();
        let active: Vec<ActiveOut> = c
            .media
            .transcode
            .active()
            .into_iter()
            .map(|a| {
                let mut file = String::new();
                if a.key.len() > 20 {
                    if let Ok(Some(f)) = c.db.by_key(&a.key[..20]) {
                        file = format!("{}:{}", f.library, f.path);
                    }
                }
                ActiveOut {
                    key: a.key,
                    progress: a.progress,
                    seconds: a.seconds,
                    file,
                }
            })
            .collect();
        let (threads, rss_mb) = proc_status();
        let uptime = format_go_duration(Duration::from_secs(c.started.elapsed().as_secs_f64().round() as u64));
        json(
            200,
            &serde_json::json!({
                "version": c.cfg.version,
                "uptime": uptime,
                "libraries": lib_outs(&c),
                "counts": counts,
                "scan": c.lib.state(),
                "pools": [c.media.probes.stats(), c.media.thumbs.stats(), c.media.gifs.stats(), c.media.transcode.stats()],
                "transcodes": if active.is_empty() { None } else { Some(active) },
                "cache": {
                    "dir": c.cfg.cache_dir.to_string_lossy(),
                    "bytes": c.media.cache_sizes(),
                    "thumbs": c.media.thumb_stats.stats(),
                    "gif": c.media.gif_stats.stats(),
                    "proxy": c.media.proxy_stats.stats(),
                },
                "hw": {"mode": hw, "note": hw_note},
                "vips": c.media.has_vips(),
                "ffmpeg": c.media.has_ffmpeg(),
                "tools": {"ffmpeg": c.media.has_ffmpeg(), "ffprobe": c.media.has_ffprobe(), "vips": c.media.has_vips()},
                "runtime": {"cpus": crate::util::num_cpu(), "goroutines": threads, "heapMB": rss_mb, "sysMB": rss_mb},
                "config": {
                    "scanInterval": format_go_duration(c.cfg.scan_interval),
                    "watch": c.cfg.watch,
                    "playableVideo": c.cfg.playable_vcodecs,
                    "playableAudio": c.cfg.playable_acodecs,
                    "previewHeight": c.cfg.preview_height,
                    "httpsAddr": c.cfg.https_addr.clone().unwrap_or_default(),
                    "httpAddr": c.cfg.http_addr.clone().unwrap_or_default(),
                    "authEnabled": c.auth.password_enabled(),
                    "backgroundThumb": c.cfg.background_gen,
                },
                "request": {"proto": ri.proto, "tls": ri.tls},
            }),
        )
    })
    .await
}
