//! Offline presentation export: a zip with `index.html` (a self-contained viewer), the
//! slides as `data.js` and the media (full-size images, GIF videos, browser-playable
//! videos). Rich text arrives pre-rendered from the browser (the editor owns the format).
//!
//! The viewer page is compiled into the binary (it is not part of the web UI build).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::{Path, Request, State};
use axum::http::header;
use axum::response::Response;
use serde::{Deserialize, Serialize};

use super::api_docs::ChannelWriter;
use super::http::{blocking, err, set};
use super::St;
use crate::engine::Core;
use crate::jobs::Priority;
use crate::library::clean_rel;
use crate::media::{KIND_GIF, KIND_IMAGE, KIND_VIDEO, VARIANT_FULL, VARIANT_PREVIEW};
use crate::store::File;
use crate::util::{path_escape, truncate_bytes};
use crate::zipstream::{Method, ZipStream};

static VIEWER: &[u8] = include_bytes!("offline_viewer/viewer.html");

fn safe_file_name(s: &str) -> String {
    let mut out = String::new();
    let mut in_run = false;
    for ch in s.chars() {
        let bad = matches!(ch, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
            || (ch as u32) < 0x20;
        if bad {
            if !in_run {
                out.push('_');
            }
            in_run = true;
        } else {
            out.push(ch);
            in_run = false;
        }
    }
    let mut s = truncate_bytes(out.trim(), 80).to_string();
    if s.is_empty() {
        s = "presentation".into();
    }
    s
}

#[derive(Deserialize, Default)]
struct Payload {
    #[serde(default)]
    html: HashMap<String, String>,
    /// light (default) | original
    #[serde(default)]
    quality: String,
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}
fn is_false(v: &bool) -> bool {
    !*v
}
fn is_zero_i(v: &i64) -> bool {
    *v == 0
}

#[derive(Serialize)]
struct MediaOut {
    kind: String,
    src: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    poster: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    caption: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    fit: String,
    #[serde(rename = "startAt", skip_serializing_if = "is_zero")]
    start_at: f64,
    #[serde(skip_serializing_if = "is_false")]
    muted: bool,
    #[serde(rename = "loop", skip_serializing_if = "is_false")]
    loop_: bool,
}

#[derive(Serialize)]
struct SlideOut {
    title: String,
    layout: String,
    #[serde(skip_serializing_if = "is_false")]
    chapter: bool,
    #[serde(skip_serializing_if = "is_zero_i")]
    columns: i64,
    #[serde(skip_serializing_if = "String::is_empty")]
    html: String,
    media: Vec<MediaOut>,
}

/// Picks a file the browser can play: the original when it is playable (and either asked
/// for or light enough), otherwise a proxy — made now if needed.
async fn offline_video(
    c: &Arc<Core>,
    f: &File,
    abs: &std::path::Path,
    original: bool,
) -> Result<PathBuf, String> {
    let playable = c.media.playable(f);
    if playable && (original || !c.media.needs_preview(f)) {
        return Ok(abs.to_path_buf());
    }
    let variant = if playable || (!original && f.width.min(f.height) > c.cfg.preview_height) {
        VARIANT_PREVIEW
    } else {
        VARIANT_FULL
    };
    if let Some(p) = c.media.proxy_path_if_ready(&f.key, variant) {
        return Ok(p);
    }
    c.media
        .request_proxy(f, abs, variant, Priority::High, false);
    let deadline = Instant::now() + Duration::from_secs(20 * 60);
    while Instant::now() < deadline {
        tokio::time::sleep(Duration::from_secs(1)).await;
        if let Some(p) = c.media.proxy_path_if_ready(&f.key, variant) {
            return Ok(p);
        }
        let st = c.media.proxy_status(f, variant);
        if st.status == "failed" {
            if playable {
                return Ok(abs.to_path_buf());
            }
            return Err(st.error);
        }
    }
    if playable {
        return Ok(abs.to_path_buf());
    }
    Err("conversion took too long".into())
}

pub async fn offline(State(c): St, Path(id): Path<String>, req: Request) -> Response {
    let c2 = c.clone();
    let id2 = id.clone();
    let Ok(d) = blocking(move || c2.docs.get(&id2)).await else {
        return err(404, "document not found");
    };
    // form fields: query string and urlencoded body (Go's FormValue)
    let query = req.uri().query().unwrap_or("").to_string();
    let Ok(body) = axum::body::to_bytes(req.into_body(), 10 << 20).await else {
        return err(400, "bad request");
    };
    let mut payload = String::new();
    for (k, v) in form_urlencoded::parse(&body).chain(form_urlencoded::parse(query.as_bytes())) {
        if k == "payload" {
            payload = v.into_owned();
            break;
        }
    }
    let input: Payload = serde_json::from_str(&payload).unwrap_or_default();

    let mut files: Vec<(String, PathBuf)> = Vec::new();
    let mut added: HashMap<PathBuf, String> = HashMap::new();
    let mut add = |abs: PathBuf, ext: &str, files: &mut Vec<(String, PathBuf)>| -> String {
        if let Some(p) = added.get(&abs) {
            return p.clone();
        }
        let p = format!("media/{:03}{ext}", added.len() + 1);
        added.insert(abs.clone(), p.clone());
        files.push((p.clone(), abs));
        p
    };

    let mut slides = Vec::new();
    for sec in &d.sections {
        if !sec.slide {
            continue;
        }
        let mut so = SlideOut {
            title: sec.title.clone(),
            layout: sec.layout.clone(),
            chapter: sec.extra.get("chapter") == Some(&serde_json::Value::Bool(true)),
            columns: sec.gallery.as_ref().map(|g| g.columns).unwrap_or(0),
            html: input.html.get(&sec.id).cloned().unwrap_or_default(),
            media: Vec::new(),
        };
        let mut refs = sec.media.clone();
        if sec.layout == "compare" && refs.len() > 2 {
            refs.truncate(2);
        }
        for mr in refs {
            let Ok(clean) = clean_rel(&mr.path) else {
                continue;
            };
            let c2 = c.clone();
            let lib = mr.library_id.clone();
            let found = blocking(move || {
                let f = c2.db.by_path(&lib, &clean).ok().flatten()?;
                let abs = c2.lib.resolve(&f.library, &f.path).ok()?;
                Some((f, abs))
            })
            .await;
            let Some((f, abs)) = found else { continue };
            let mut mo = MediaOut {
                kind: f.kind.clone(),
                src: String::new(),
                poster: String::new(),
                caption: mr.caption.clone(),
                fit: mr.fit.clone(),
                start_at: mr.start_at,
                muted: mr.muted,
                loop_: mr.loop_,
            };
            match f.kind.as_str() {
                KIND_IMAGE => {
                    let Ok(p) = c.media.thumb(&f, &abs, 2560).await else {
                        continue;
                    };
                    mo.src = add(p, ".webp", &mut files);
                }
                KIND_GIF => {
                    let Ok(p) = c.media.gif(&f, &abs).await else {
                        continue;
                    };
                    mo.src = add(p, ".mp4", &mut files);
                }
                KIND_VIDEO => {
                    if let Ok(p) = c.media.thumb(&f, &abs, 1280).await {
                        mo.poster = add(p, ".webp", &mut files);
                    }
                    let Ok(src) = offline_video(&c, &f, &abs, input.quality == "original").await
                    else {
                        continue;
                    };
                    let ext = if src == abs {
                        format!(".{}", f.ext)
                    } else {
                        ".mp4".to_string()
                    };
                    mo.src = add(src, &ext, &mut files);
                }
                _ => continue,
            }
            so.media.push(mo);
        }
        slides.push(so);
    }

    let data = serde_json::to_vec(&serde_json::json!({
        "title": d.title,
        "slides": if slides.is_empty() { None } else { Some(slides) },
        "exported": crate::util::rfc3339_nano(&chrono::Local::now().with_nanosecond_zero()),
    }))
    .unwrap_or_default();
    let name = safe_file_name(&d.title);
    let (w, body) = ChannelWriter::new();
    let zname = name.clone();
    tokio::task::spawn_blocking(move || {
        let res = (|| -> std::io::Result<()> {
            let mut z = ZipStream::new(w);
            z.add_bytes(&format!("{zname}/index.html"), Method::Deflate, VIEWER)?;
            let mut js = b"window.LETOPIS = ".to_vec();
            js.extend_from_slice(&data);
            js.extend_from_slice(b";\n");
            z.add_bytes(&format!("{zname}/data.js"), Method::Deflate, &js)?;
            z.add_bytes(
                &format!("{zname}/README.txt"),
                Method::Deflate,
                b"Open index.html in a browser (double-click). Arrow keys or clicks move between slides, F toggles full screen.\nMade with Tessera.\n",
            )?;
            for (zp, abs) in files {
                let Ok(mut f) = std::fs::File::open(&abs) else {
                    continue;
                };
                let modified = f.metadata().and_then(|m| m.modified()).ok();
                z.add(&format!("{zname}/{zp}"), Method::Store, modified, &mut f)?;
            }
            z.finish()?;
            Ok(())
        })();
        if let Err(e) = res {
            tracing::debug!(err = %e, "offline export aborted");
        }
    });
    let mut r = Response::new(body);
    set(r.headers_mut(), header::CONTENT_TYPE, "application/zip");
    set(
        r.headers_mut(),
        header::CONTENT_DISPOSITION,
        &format!(
            "attachment; filename=\"{}.zip\"; filename*=UTF-8''{}",
            name.replace('"', ""),
            path_escape(&format!("{name}.zip"))
        ),
    );
    r
}

trait NanoZero {
    fn with_nanosecond_zero(self) -> Self;
}

impl NanoZero for chrono::DateTime<chrono::Local> {
    fn with_nanosecond_zero(self) -> Self {
        use chrono::Timelike;
        self.with_nanosecond(0).unwrap_or(self)
    }
}
