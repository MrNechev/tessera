//! HTTP routes for the tools (mounted inside the authenticated API):
//!
//! - `POST /api/t/{command}`: JSON arguments in, JSON result out (the former
//!   Tauri commands, same names and argument names).
//! - Binary endpoints: export tiles, raw RGBA pixels, temporary images.
//! - Files: `GET /api/t/file` (previews and originals, Range-capable),
//!   `/api/t/download`, `/api/t/zip`, `/api/t/upload`, `/api/t/browse`.
//! - `GET /api/events`: server-sent events (progress).

use std::convert::Infallible;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use axum::body::{Body, Bytes};
use axum::extract::{Path as UrlPath, Query, Request, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::stream::Stream;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tower::ServiceExt;
use tower_http::services::ServeFile;

use super::{
    batch, batch_tools, export, images, paths, projects, settings, system, transfer, CmdResult,
    Tools,
};

type Shared = Arc<Tools>;

/// All tool routes. Merge into the authenticated part of the server's router.
pub fn routes(t: Shared) -> Router {
    Router::new()
        .route("/api/events", get(events))
        .route("/api/t/export_tile", post(export_tile))
        .route("/api/t/save_temp_image", post(save_temp_image))
        .route("/api/t/load_image_rgba", post(load_image_rgba))
        .route("/api/t/file", get(file))
        .route("/api/t/download", get(download))
        .route("/api/t/zip", post(zip))
        .route("/api/t/upload", post(upload))
        .route("/api/t/browse", get(browse))
        .route("/api/t/{command}", post(command))
        // Export tiles (raw RGBA) and upload chunks are large.
        .layer(axum::extract::DefaultBodyLimit::max(512 * 1024 * 1024))
        .with_state(t)
}

fn error(status: StatusCode, msg: impl Into<String>) -> Response {
    (status, Json(serde_json::json!({ "error": msg.into() }))).into_response()
}

fn json_result<T: Serialize>(r: CmdResult<T>) -> Response {
    match r {
        Ok(v) => {
            let mut res = Json(v).into_response();
            res.headers_mut()
                .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
            res
        }
        Err(e) => error(StatusCode::BAD_REQUEST, e),
    }
}

/// Run blocking tool work off the async runtime.
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> CmdResult<T> + Send + 'static,
) -> CmdResult<T> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| format!("internal error: {e}"))?
}

fn arg<T: DeserializeOwned>(v: &Value) -> CmdResult<T> {
    serde_json::from_value(v.clone()).map_err(|e| format!("bad arguments: {e}"))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PathsArg {
    paths: Vec<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PathArg {
    path: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IdArg {
    id: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct JobArg {
    job: u32,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PdfArg {
    pdf: u32,
}

async fn command(State(t): State<Shared>, UrlPath(cmd): UrlPath<String>, body: Bytes) -> Response {
    let args: Value = if body.is_empty() {
        Value::Object(Default::default())
    } else {
        match serde_json::from_slice(&body) {
            Ok(v) => v,
            Err(e) => return error(StatusCode::BAD_REQUEST, format!("bad JSON: {e}")),
        }
    };
    let t2 = t.clone();
    let res: CmdResult<Value> = blocking(move || dispatch(&t2, &cmd, &args)).await;
    match res {
        Err(e) if e.starts_with("unknown command") => error(StatusCode::NOT_FOUND, e),
        r => json_result(r),
    }
}

fn to<T: Serialize>(v: T) -> CmdResult<Value> {
    serde_json::to_value(v).map_err(|e| e.to_string())
}

fn dispatch(t: &Tools, cmd: &str, a: &Value) -> CmdResult<Value> {
    match cmd {
        "app_info" => to(system::app_info(t)),
        "ensure_dir" => to(system::ensure_dir(t, arg::<PathArg>(a)?.path)?),
        "check_updates" => to(system::check_updates()?),
        "cache_size" => to(system::cache_size(t)),
        "clear_cache" => to(system::clear_cache(t)),
        "load_settings" => to(settings::load_settings(t)),
        "save_settings" => {
            #[derive(Deserialize)]
            struct A {
                settings: settings::Settings,
            }
            to(settings::save_settings(t, arg::<A>(a)?.settings)?)
        }
        "import_images" => to(images::import_images(t, arg::<PathsArg>(a)?.paths)),
        "probe_files" => to(images::probe_files(t, arg::<PathsArg>(a)?.paths)),
        "read_metadata" => to(images::read_metadata(t, arg::<PathArg>(a)?.path)?),
        "export_begin" => {
            #[derive(Deserialize)]
            struct A {
                width: u32,
                height: u32,
            }
            let A { width, height } = arg(a)?;
            to(export::export_begin(t, width, height)?)
        }
        "export_finish" => {
            #[derive(Deserialize)]
            struct A {
                job: u32,
                path: String,
                format: String,
                quality: u8,
                dpi: u32,
                unique: bool,
            }
            let A {
                job,
                path,
                format,
                quality,
                dpi,
                unique,
            } = arg(a)?;
            to(export::export_finish(
                t, job, path, format, quality, dpi, unique,
            )?)
        }
        "export_cancel" => to(export::export_cancel(t, arg::<JobArg>(a)?.job)?),
        "pdf_begin" => to(export::pdf_begin(t)?),
        "pdf_add_page" => {
            #[derive(Deserialize)]
            struct A {
                pdf: u32,
                job: u32,
                quality: u8,
                dpi: u32,
            }
            let A {
                pdf,
                job,
                quality,
                dpi,
            } = arg(a)?;
            to(export::pdf_add_page(t, pdf, job, quality, dpi)?)
        }
        "pdf_finish" => {
            #[derive(Deserialize)]
            struct A {
                pdf: u32,
                path: String,
                unique: bool,
            }
            let A { pdf, path, unique } = arg(a)?;
            to(export::pdf_finish(t, pdf, path, unique)?)
        }
        "pdf_cancel" => to(export::pdf_cancel(t, arg::<PdfArg>(a)?.pdf)?),
        "convert_images" => {
            #[derive(Deserialize)]
            struct A {
                files: Vec<String>,
                options: tessera_imaging::ConvertOptions,
            }
            let A { files, options } = arg(a)?;
            to(batch::convert_images(t, files, options))
        }
        "compress_images" => {
            #[derive(Deserialize)]
            struct A {
                files: Vec<String>,
                options: tessera_imaging::CompressOptions,
            }
            let A { files, options } = arg(a)?;
            to(batch::compress_images(t, files, options))
        }
        "compress_preview" => {
            #[derive(Deserialize)]
            struct A {
                path: String,
                options: tessera_imaging::CompressOptions,
            }
            let A { path, options } = arg(a)?;
            to(batch::compress_preview(t, path, options)?)
        }
        "cancel_batch" => {
            batch::cancel_batch(t);
            Ok(Value::Null)
        }
        "batch_process" => {
            #[derive(Deserialize)]
            struct A {
                jobs: Vec<batch_tools::BatchJob>,
                options: batch_tools::BatchOptions,
            }
            let A { jobs, options } = arg(a)?;
            to(batch_tools::batch_process(t, jobs, options)?)
        }
        "list_projects" => to(projects::list_projects(t)?),
        "load_project" => to(projects::load_project(t, arg::<PathArg>(a)?.path)?),
        "save_project" => {
            #[derive(Deserialize)]
            struct A {
                id: String,
                json: String,
                thumbnail: Option<String>,
            }
            let A {
                id,
                json,
                thumbnail,
            } = arg(a)?;
            to(projects::save_project(t, id, json, thumbnail)?)
        }
        "save_project_file" => {
            #[derive(Deserialize)]
            struct A {
                path: String,
                json: String,
            }
            let A { path, json } = arg(a)?;
            to(projects::save_project_file(t, path, json)?)
        }
        "delete_project" => to(projects::delete_project(t, arg::<IdArg>(a)?.id)?),
        "areas" => to(t.guard.areas()),
        "resolve" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct A {
                library_id: String,
                path: String,
            }
            let A { library_id, path } = arg(a)?;
            to(transfer::resolve_ref(t, &library_id, &path)?)
        }
        "writable" => {
            let p = arg::<PathArg>(a)?.path;
            to(t.guard.writable(&paths::expand(&p)).is_ok())
        }
        _ => Err(format!("unknown command {cmd}")),
    }
}

fn header_str<'a>(h: &'a HeaderMap, name: &str) -> CmdResult<&'a str> {
    h.get(name)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| format!("missing header {name}"))
}

async fn export_tile(State(t): State<Shared>, headers: HeaderMap, body: Bytes) -> Response {
    let job = match header_str(&headers, "x-job") {
        Ok(v) => v.to_string(),
        Err(e) => return error(StatusCode::BAD_REQUEST, e),
    };
    let rect = match header_str(&headers, "x-rect") {
        Ok(v) => v.to_string(),
        Err(e) => return error(StatusCode::BAD_REQUEST, e),
    };
    json_result(blocking(move || export::export_tile(&t, &job, &rect, &body)).await)
}

async fn save_temp_image(State(t): State<Shared>, body: Bytes) -> Response {
    json_result(blocking(move || batch_tools::save_temp_image(&t, &body)).await)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RgbaArgs {
    path: String,
    max_width: u32,
    max_height: u32,
}

async fn load_image_rgba(State(t): State<Shared>, Json(a): Json<RgbaArgs>) -> Response {
    match blocking(move || images::load_image_rgba(&t, a.path, a.max_width, a.max_height)).await {
        Ok(buf) => (
            [
                (header::CONTENT_TYPE, "application/octet-stream"),
                (header::CACHE_CONTROL, "no-store"),
            ],
            buf,
        )
            .into_response(),
        Err(e) => error(StatusCode::BAD_REQUEST, e),
    }
}

#[derive(Deserialize)]
struct FileQuery {
    path: String,
    /// Cache-busting version from the UI; when present the file is immutable.
    v: Option<String>,
}

async fn serve_path(path: &Path, req: Request, cache: &'static str) -> Response {
    // ServeFile never fails (its error type is Infallible); I/O errors become responses.
    let Ok(mut res) = ServeFile::new(path).oneshot(req).await;
    res.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    res.into_response()
}

/// Previews, thumbnails and originals for the UI (replaces Tauri's asset protocol).
async fn file(State(t): State<Shared>, Query(q): Query<FileQuery>, req: Request) -> Response {
    let p = match t.guard.readable(&paths::expand(&q.path)) {
        Ok(p) => p,
        Err(e) => return error(StatusCode::FORBIDDEN, e),
    };
    if !p.is_file() {
        return error(StatusCode::NOT_FOUND, "not found");
    }
    let cache = if q.v.is_some() {
        "private, max-age=31536000, immutable"
    } else {
        "private, no-cache"
    };
    serve_path(&p, req, cache).await
}

fn attachment(res: &mut Response, name: &str) {
    let ascii: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_graphic() || c == ' ' {
                c
            } else {
                '_'
            }
        })
        .filter(|c| *c != '"' && *c != '\\')
        .collect();
    let encoded: String = name
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect();
    if let Ok(v) = HeaderValue::from_str(&format!(
        "attachment; filename=\"{ascii}\"; filename*=UTF-8''{encoded}"
    )) {
        res.headers_mut().insert(header::CONTENT_DISPOSITION, v);
    }
}

#[derive(Deserialize)]
struct DownloadQuery {
    path: String,
    name: Option<String>,
}

async fn download(
    State(t): State<Shared>,
    Query(q): Query<DownloadQuery>,
    req: Request,
) -> Response {
    let p = match t.guard.readable(&paths::expand(&q.path)) {
        Ok(p) => p,
        Err(e) => return error(StatusCode::FORBIDDEN, e),
    };
    if !p.is_file() {
        return error(StatusCode::NOT_FOUND, "not found");
    }
    let name = q.name.unwrap_or_else(|| {
        p.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "download".into())
    });
    let mut res = serve_path(&p, req, "no-store").await;
    attachment(&mut res, &name);
    res
}

#[derive(Deserialize)]
struct ZipArgs {
    paths: Vec<String>,
    name: Option<String>,
}

/// Accepts JSON, or a form field `json` holding the same JSON (a plain form
/// POST lets the browser stream the zip straight to disk).
async fn zip(State(t): State<Shared>, headers: HeaderMap, body: Bytes) -> Response {
    let form = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("application/x-www-form-urlencoded"));
    let parsed: Result<ZipArgs, String> = if form {
        url::form_urlencoded::parse(&body)
            .find(|(k, _)| k == "json")
            .ok_or_else(|| "missing json field".to_string())
            .and_then(|(_, v)| serde_json::from_str(&v).map_err(|e| e.to_string()))
    } else {
        serde_json::from_slice(&body).map_err(|e| e.to_string())
    };
    let a = match parsed {
        Ok(a) => a,
        Err(e) => return error(StatusCode::BAD_REQUEST, e),
    };
    let t2 = t.clone();
    let paths = a.paths;
    let path = match blocking(move || transfer::make_zip(&t2, &paths)).await {
        Ok(p) => p,
        Err(e) => return error(StatusCode::BAD_REQUEST, e),
    };
    let file = match tokio::fs::File::open(&path).await {
        Ok(f) => f,
        Err(e) => return error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    let len = file.metadata().await.map(|m| m.len()).unwrap_or(0);
    // The file stays open while streaming; unlinking it now frees the space afterwards.
    let _ = tokio::fs::remove_file(&path).await;
    let stream = tokio_util::io::ReaderStream::new(file);
    let mut res = Response::new(Body::from_stream(stream));
    res.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/zip"),
    );
    if let Ok(v) = HeaderValue::from_str(&len.to_string()) {
        res.headers_mut().insert(header::CONTENT_LENGTH, v);
    }
    attachment(&mut res, a.name.as_deref().unwrap_or("Tessera.zip"));
    res
}

async fn upload(
    State(t): State<Shared>,
    Query(p): Query<transfer::UploadParams>,
    body: Bytes,
) -> Response {
    json_result(blocking(move || transfer::upload_chunk(&t, &p, &body)).await)
}

#[derive(Deserialize)]
struct BrowseQuery {
    #[serde(default)]
    path: String,
}

async fn browse(State(t): State<Shared>, Query(q): Query<BrowseQuery>) -> Response {
    json_result(blocking(move || transfer::browse(&t, &q.path)).await)
}

async fn events(State(t): State<Shared>) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = t.events.subscribe();
    let stream = futures_util::StreamExt::filter_map(
        tokio_stream::wrappers::BroadcastStream::new(rx),
        |m| async move { m.ok().map(|data| Ok(Event::default().data(data))) },
    );
    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(20)))
}

/// Background housekeeping: delete day-old temporary uploads every hour.
pub fn spawn_cleanup(t: Shared) {
    tokio::spawn(async move {
        loop {
            let t2 = t.clone();
            let _ = tokio::task::spawn_blocking(move || {
                transfer::clean_uploads(&t2, Duration::from_secs(24 * 3600))
            })
            .await;
            tokio::time::sleep(Duration::from_secs(3600)).await;
        }
    });
}
