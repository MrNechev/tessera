//! Documents: CRUD, history, export/import, backup zip and the stress-test generator.

use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::header;
use axum::response::Response;
use bytes::Bytes;
use serde::Deserialize;

use super::api_media::Ref;
use super::http::{blocking, err, json, read_json, set};
use super::St;
use crate::docs::{self, DocError, Document, MediaRef, Section};
use crate::engine::Core;
use crate::media::{KIND_GIF, KIND_IMAGE, KIND_VIDEO};
use crate::util::path_escape;

const MAX_DOC_BYTES: usize = 64 << 20;

fn doc_err(e: DocError) -> Response {
    match e {
        DocError::Conflict(cur) => json(
            409,
            &serde_json::json!({"error": "document was changed elsewhere", "current": cur}),
        ),
        DocError::NotFound => err(404, "document not found"),
        e @ DocError::Invalid(_) => err(400, &e.to_string()),
        DocError::Other(e) => {
            tracing::error!(err = %e, "document store");
            err(500, &e.to_string())
        }
    }
}

fn refs_of(d: &Document) -> Vec<Ref> {
    docs::refs(d)
        .into_iter()
        .map(|m| Ref {
            library_id: m.library_id,
            path: m.path,
        })
        .collect()
}

fn after_save(c: &Arc<Core>, d: &Document) {
    let refs = refs_of(d);
    let c = c.clone();
    tokio::task::spawn_blocking(move || c.queue_doc_proxies(&refs));
}

pub async fn list(State(c): St) -> Response {
    blocking(move || match c.docs.list() {
        Ok(l) => json(200, &l),
        Err(e) => doc_err(DocError::Other(e)),
    })
    .await
}

#[derive(Deserialize, Default)]
struct CreateBody {
    #[serde(default)]
    title: String,
}

pub async fn create(State(c): St, body: Body) -> Response {
    let b = read_json::<CreateBody>(body, 1 << 16)
        .await
        .unwrap_or_default();
    blocking(move || {
        let d = Document {
            title: b.title,
            sections: vec![Section {
                layout: "text-only".into(),
                slide: true,
                ..Default::default()
            }],
            ..Default::default()
        };
        match c.docs.create(d) {
            Ok(d) => json(201, &d),
            Err(e) => doc_err(e),
        }
    })
    .await
}

pub async fn import(State(c): St, body: Body) -> Response {
    let d = match read_json::<Document>(body, MAX_DOC_BYTES).await {
        Ok(d) => d,
        Err(e) => return err(400, &format!("not a Tessera document: {e}")),
    };
    let c2 = c.clone();
    match blocking(move || c2.docs.create(d)).await {
        Ok(nd) => {
            after_save(&c, &nd);
            json(201, &nd)
        }
        Err(e) => doc_err(e),
    }
}

pub async fn get_doc(State(c): St, Path(id): Path<String>) -> Response {
    blocking(move || match c.docs.get(&id) {
        Ok(d) => json(200, &d),
        Err(e) => doc_err(e),
    })
    .await
}

#[derive(Deserialize)]
struct SaveBody {
    #[serde(rename = "baseVersion", default)]
    base_version: i64,
    #[serde(default)]
    force: bool,
    #[serde(default)]
    document: Option<Document>,
}

pub async fn save(State(c): St, Path(id): Path<String>, body: Body) -> Response {
    let b = match read_json::<SaveBody>(body, MAX_DOC_BYTES).await {
        Ok(SaveBody {
            document: Some(d),
            base_version,
            force,
        }) => (d, base_version, force),
        _ => return err(400, "bad request"),
    };
    let c2 = c.clone();
    match blocking(move || c2.docs.save(&id, b.1, b.0, b.2)).await {
        Ok(d) => {
            after_save(&c, &d);
            json(
                200,
                &serde_json::json!({"version": d.version, "updatedAt": d.updated_at}),
            )
        }
        Err(e) => doc_err(e),
    }
}

pub async fn delete_doc(State(c): St, Path(id): Path<String>) -> Response {
    blocking(move || match c.docs.delete(&id) {
        Ok(()) => json(200, &serde_json::json!({"ok": true})),
        Err(e) => doc_err(e),
    })
    .await
}

pub async fn duplicate(State(c): St, Path(id): Path<String>) -> Response {
    blocking(move || match c.docs.duplicate(&id) {
        Ok(d) => json(201, &d),
        Err(e) => doc_err(e),
    })
    .await
}

pub async fn snapshot(State(c): St, Path(id): Path<String>) -> Response {
    blocking(move || match c.docs.snapshot(&id) {
        Ok(()) => json(200, &serde_json::json!({"ok": true})),
        Err(e) => doc_err(e),
    })
    .await
}

pub async fn history(State(c): St, Path(id): Path<String>) -> Response {
    blocking(move || match c.docs.history(&id) {
        Ok(h) => json(200, &h),
        Err(e) => doc_err(e),
    })
    .await
}

pub async fn version(State(c): St, Path((id, v)): Path<(String, String)>) -> Response {
    blocking(
        move || match c.docs.get_version(&id, v.parse().unwrap_or(0)) {
            Ok(d) => json(200, &d),
            Err(e) => doc_err(e),
        },
    )
    .await
}

pub async fn restore(State(c): St, Path((id, v)): Path<(String, String)>) -> Response {
    blocking(move || match c.docs.restore(&id, v.parse().unwrap_or(0)) {
        Ok(d) => json(200, &d),
        Err(e) => doc_err(e),
    })
    .await
}

pub async fn export(State(c): St, Path(id): Path<String>) -> Response {
    blocking(move || {
        let d = match c.docs.get(&id) {
            Ok(d) => d,
            Err(e) => return doc_err(e),
        };
        let b = serde_json::to_vec_pretty(&d).unwrap_or_default();
        let mut r = Response::new(Body::from(b));
        set(r.headers_mut(), header::CONTENT_TYPE, "application/json");
        set(
            r.headers_mut(),
            header::CONTENT_DISPOSITION,
            &format!(
                "attachment; filename*=UTF-8''{}",
                path_escape(&format!("{}.tessera.json", d.title))
            ),
        );
        r
    })
    .await
}

/// A `Write` that feeds a streaming response body (used from blocking threads).
pub(crate) struct ChannelWriter {
    tx: tokio::sync::mpsc::Sender<Result<Bytes, std::io::Error>>,
    buf: Vec<u8>,
}

impl ChannelWriter {
    pub(crate) fn new() -> (ChannelWriter, Body) {
        let (tx, rx) = tokio::sync::mpsc::channel::<Result<Bytes, std::io::Error>>(8);
        let body = Body::from_stream(futures_util::stream::unfold(rx, |mut rx| async move {
            rx.recv().await.map(|v| (v, rx))
        }));
        (
            ChannelWriter {
                tx,
                buf: Vec::with_capacity(256 << 10),
            },
            body,
        )
    }

    fn send(&mut self) -> std::io::Result<()> {
        if self.buf.is_empty() {
            return Ok(());
        }
        let chunk = Bytes::from(std::mem::take(&mut self.buf));
        self.tx
            .blocking_send(Ok(chunk))
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::BrokenPipe, "client went away"))
    }
}

impl std::io::Write for ChannelWriter {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.buf.extend_from_slice(b);
        if self.buf.len() >= 256 << 10 {
            self.send()?;
        }
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.send()
    }
}

impl Drop for ChannelWriter {
    fn drop(&mut self) {
        let _ = self.send();
    }
}

pub async fn export_all(State(c): St) -> Response {
    let (w, body) = ChannelWriter::new();
    tokio::task::spawn_blocking(move || {
        if let Err(e) = c.docs.export_all(w) {
            tracing::error!(err = %e, "export");
        }
    });
    let mut r = Response::new(body);
    set(r.headers_mut(), header::CONTENT_TYPE, "application/zip");
    set(
        r.headers_mut(),
        header::CONTENT_DISPOSITION,
        &format!(
            "attachment; filename=\"tessera-backup-{}.zip\"",
            chrono::Local::now().format("%Y-%m-%d")
        ),
    );
    r
}

#[derive(Deserialize, Default)]
struct StressBody {
    #[serde(default)]
    videos: i64,
    #[serde(default)]
    gifs: i64,
    #[serde(default)]
    images: i64,
}

/// Builds a document from random library media (many videos + GIFs) to check that
/// scrolling stays smooth.
pub async fn stress_test(State(c): St, body: Body) -> Response {
    let mut b = read_json::<StressBody>(body, 4096)
        .await
        .unwrap_or_default();
    if b.videos == 0 && b.gifs == 0 && b.images == 0 {
        (b.videos, b.gifs, b.images) = (40, 25, 40);
    }
    let c2 = c.clone();
    let res = blocking(move || {
        let videos = c2
            .db
            .random(&[KIND_VIDEO], b.videos.min(500))
            .unwrap_or_default();
        let gifs = c2
            .db
            .random(&[KIND_GIF], b.gifs.min(500))
            .unwrap_or_default();
        let images = c2
            .db
            .random(&[KIND_IMAGE], b.images.min(2000))
            .unwrap_or_default();
        let layouts = ["media-top", "media-left", "media-right", "media-full"];
        let mut d = Document {
            title: format!(
                "Stress test {}",
                chrono::Local::now().format("%Y-%m-%d %H:%M")
            ),
            ..Default::default()
        };
        let text = |s: &str| {
            serde_json::json!({"type": "doc", "content": [
                {"type": "paragraph", "content": [{"type": "text", "text": s}]}
            ]})
        };
        let mut i = 0usize;
        let mut add = |d: &mut Document, title: &str, refs: Vec<MediaRef>| {
            let layout = if refs.len() > 1 {
                "gallery"
            } else {
                layouts[i % layouts.len()]
            };
            d.sections.push(Section {
                title: title.into(),
                layout: layout.into(),
                text: Some(text(&format!("Section {}: {}", i + 1, refs[0].path))),
                media: refs,
                slide: true,
                ..Default::default()
            });
            i += 1;
        };
        let mref = |f: &crate::store::File| MediaRef {
            library_id: f.library.clone(),
            path: f.path.clone(),
            ..Default::default()
        };
        let (mut vi, mut gi, mut ii) = (0, 0, 0);
        while vi < videos.len() || gi < gifs.len() || ii < images.len() {
            if vi < videos.len() {
                add(&mut d, "Video", vec![mref(&videos[vi])]);
                vi += 1;
            }
            if gi < gifs.len() {
                add(&mut d, "GIF", vec![mref(&gifs[gi])]);
                gi += 1;
            }
            if ii < images.len() {
                let n = 4.min(images.len() - ii);
                let refs = images[ii..ii + n].iter().map(mref).collect();
                ii += n;
                add(&mut d, "Images", refs);
            }
        }
        c2.docs.create(d)
    })
    .await;
    match res {
        Ok(nd) => {
            after_save(&c, &nd);
            json(201, &nd)
        }
        Err(e) => doc_err(e),
    }
}
