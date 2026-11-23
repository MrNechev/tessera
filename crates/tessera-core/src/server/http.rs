//! HTTP helpers: JSON responses and bodies, query strings, request info, and a port of Go's
//! `http.ServeContent` (Range requests incl. multipart, If-None-Match / If-Modified-Since /
//! If-Range / If-Match / If-Unmodified-Since, Last-Modified, HEAD).

use std::collections::HashMap;
use std::io::SeekFrom;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::SystemTime;

use axum::body::Body;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode, Version};
use axum::response::Response;
use bytes::Bytes;
use chrono::{DateTime, Utc};
use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio::io::{AsyncReadExt, AsyncSeekExt};

use crate::util::{http_date, parse_http_date};

// ---------- JSON ----------

/// A JSON response like Go's `writeJSON` (no-store, trailing newline).
pub fn json<T: Serialize + ?Sized>(status: u16, v: &T) -> Response {
    let mut body = match serde_json::to_vec(v) {
        Ok(b) => b,
        Err(e) => return err(500, &e.to_string()),
    };
    body.push(b'\n');
    let mut r = Response::new(Body::from(body));
    *r.status_mut() = StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let h = r.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json; charset=utf-8"),
    );
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    r
}

/// `{"error": msg}`.
pub fn err(status: u16, msg: &str) -> Response {
    json(status, &serde_json::json!({ "error": msg }))
}

/// Plain-text error like Go's `http.Error`.
pub fn text_err(status: u16, msg: &str) -> Response {
    let mut r = Response::new(Body::from(format!("{msg}\n")));
    *r.status_mut() = StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    r.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    r.headers_mut().insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    r
}

/// Reads a JSON body (at most `limit` bytes). Like Go's `json.Decoder.Decode`, only the
/// first JSON value counts; trailing data is ignored.
pub async fn read_json<T: DeserializeOwned>(body: Body, limit: usize) -> Result<T, String> {
    let b = axum::body::to_bytes(body, limit)
        .await
        .map_err(|e| e.to_string())?;
    let mut it = serde_json::Deserializer::from_slice(&b).into_iter::<T>();
    match it.next() {
        Some(Ok(v)) => Ok(v),
        Some(Err(e)) => Err(e.to_string()),
        None => Err("EOF".into()),
    }
}

/// Runs blocking work (SQLite, filesystem, external tools) off the async threads.
pub async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    match tokio::task::spawn_blocking(f).await {
        Ok(v) => v,
        Err(e) if e.is_panic() => std::panic::resume_unwind(e.into_panic()),
        Err(e) => panic!("blocking task failed: {e}"),
    }
}

// ---------- query ----------

/// Query parameters (first value wins, like Go's `Query().Get`).
#[derive(Debug, Default, Clone)]
pub struct Query(pub HashMap<String, String>);

impl Query {
    pub fn parse(q: Option<&str>) -> Query {
        let mut m = HashMap::new();
        if let Some(q) = q {
            for (k, v) in form_urlencoded::parse(q.as_bytes()) {
                m.entry(k.into_owned()).or_insert_with(|| v.into_owned());
            }
        }
        Query(m)
    }
    pub fn get(&self, k: &str) -> &str {
        self.0.get(k).map(|s| s.as_str()).unwrap_or("")
    }
    pub fn i64(&self, k: &str) -> i64 {
        self.get(k).parse().unwrap_or(0)
    }
    pub fn f64(&self, k: &str) -> f64 {
        self.get(k).parse().unwrap_or(0.0)
    }
}

impl<S: Send + Sync> FromRequestParts<S> for Query {
    type Rejection = std::convert::Infallible;
    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        Ok(Query::parse(parts.uri.query()))
    }
}

// ---------- request info ----------

/// Per-connection facts, inserted as a request extension by the listeners.
#[derive(Debug, Clone, Copy)]
pub struct ConnInfo {
    pub remote: SocketAddr,
    pub tls: bool,
}

/// What handlers need to know about the request beyond its path and body.
#[derive(Debug, Clone)]
pub struct ReqInfo {
    pub tls: bool,
    /// "HTTP/1.1", "HTTP/2.0", …
    pub proto: &'static str,
    /// Client IP (X-Real-IP / X-Forwarded-For aware, like chi's RealIP).
    pub ip: String,
    /// Host (or :authority) incl. port.
    pub host: String,
    pub headers: HeaderMap,
}

impl ReqInfo {
    pub fn from_parts(parts: &Parts) -> ReqInfo {
        let conn = parts.extensions.get::<ConnInfo>().copied();
        let h = &parts.headers;
        let hv = |k: &str| {
            h.get(k)
                .and_then(|v| v.to_str().ok())
                .map(str::trim)
                .unwrap_or("")
        };
        let mut ip = hv("true-client-ip").to_string();
        if ip.is_empty() {
            ip = hv("x-real-ip").to_string();
        }
        if ip.is_empty() {
            ip = hv("x-forwarded-for")
                .split(',')
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
        }
        if ip.is_empty() {
            ip = conn.map(|c| c.remote.ip().to_string()).unwrap_or_default();
        }
        ReqInfo {
            tls: conn.map(|c| c.tls).unwrap_or(false),
            proto: proto_str(parts.version),
            ip,
            host: request_host(parts),
            headers: h.clone(),
        }
    }

    pub fn cookies(&self) -> String {
        self.headers
            .get_all(header::COOKIE)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .collect::<Vec<_>>()
            .join("; ")
    }

    pub fn token_header(&self) -> Option<String> {
        self.headers
            .get(crate::auth::TOKEN_HEADER)
            .and_then(|v| v.to_str().ok())
            .map(String::from)
    }
}

impl<S: Send + Sync> FromRequestParts<S> for ReqInfo {
    type Rejection = std::convert::Infallible;
    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        Ok(ReqInfo::from_parts(parts))
    }
}

pub fn proto_str(v: Version) -> &'static str {
    match v {
        Version::HTTP_09 => "HTTP/0.9",
        Version::HTTP_10 => "HTTP/1.0",
        Version::HTTP_2 => "HTTP/2.0",
        Version::HTTP_3 => "HTTP/3.0",
        _ => "HTTP/1.1",
    }
}

pub fn request_host(parts: &Parts) -> String {
    if let Some(h) = parts
        .headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
    {
        return h.to_string();
    }
    parts
        .uri
        .authority()
        .map(|a| a.as_str().to_string())
        .unwrap_or_default()
}

// ---------- ServeContent ----------

/// Content type by extension (Go's builtin table, then mime_guess).
pub fn type_by_extension(name: &str) -> Option<String> {
    let ext = crate::util::path_ext(name).to_lowercase();
    let t = match ext.as_str() {
        ".avif" => "image/avif",
        ".css" => "text/css; charset=utf-8",
        ".gif" => "image/gif",
        ".htm" | ".html" => "text/html; charset=utf-8",
        ".jpeg" | ".jpg" => "image/jpeg",
        ".js" | ".mjs" => "text/javascript; charset=utf-8",
        ".json" => "application/json",
        ".pdf" => "application/pdf",
        ".png" => "image/png",
        ".svg" => "image/svg+xml",
        ".wasm" => "application/wasm",
        ".webp" => "image/webp",
        ".xml" => "text/xml; charset=utf-8",
        ".woff2" => "font/woff2",
        ".woff" => "font/woff",
        ".ico" => "image/vnd.microsoft.icon",
        ".webmanifest" => "application/manifest+json",
        ".txt" => "text/plain; charset=utf-8",
        "" => return None,
        _ => {
            return mime_guess::from_ext(&ext[1..])
                .first()
                .map(|m| m.to_string())
        }
    };
    Some(t.to_string())
}

/// What to serve.
pub enum Source {
    File(PathBuf),
    Bytes(Bytes),
}

#[derive(PartialEq)]
enum Cond {
    None,
    True,
    False,
}

fn etag_strong_match(a: &str, b: &str) -> bool {
    a == b && !a.is_empty() && !a.starts_with("W/")
}

fn etag_weak_match(a: &str, b: &str) -> bool {
    a.trim_start_matches("W/") == b.trim_start_matches("W/")
}

/// Splits an If-Match / If-None-Match list into entity tags.
fn etag_list(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = s.trim();
    while !rest.is_empty() {
        rest = rest.trim_start_matches([',', ' ', '\t']);
        if rest.is_empty() {
            break;
        }
        if rest.starts_with('*') {
            out.push("*".into());
            rest = &rest[1..];
            continue;
        }
        let (weak, r) = match rest.strip_prefix("W/") {
            Some(r) => (true, r),
            None => (false, rest),
        };
        if !r.starts_with('"') {
            break;
        }
        match r[1..].find('"') {
            Some(end) => {
                let tag = &r[..end + 2];
                out.push(if weak {
                    format!("W/{tag}")
                } else {
                    tag.to_string()
                });
                rest = &r[end + 2..];
            }
            None => break,
        }
    }
    out
}

fn hdr(h: &HeaderMap, k: header::HeaderName) -> &str {
    h.get(k).and_then(|v| v.to_str().ok()).unwrap_or("")
}

fn unix_secs(t: SystemTime) -> i64 {
    match t.duration_since(SystemTime::UNIX_EPOCH) {
        Ok(d) => d.as_secs() as i64,
        Err(e) => -(e.duration().as_secs() as i64),
    }
}

/// One byte range.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Range {
    start: u64,
    length: u64,
}

enum RangeErr {
    Invalid,
    NoOverlap,
}

fn parse_range(s: &str, size: u64) -> Result<Vec<Range>, RangeErr> {
    if s.is_empty() {
        return Ok(Vec::new());
    }
    let Some(spec) = s.strip_prefix("bytes=") else {
        return Err(RangeErr::Invalid);
    };
    let mut ranges = Vec::new();
    let mut no_overlap = false;
    for ra in spec.split(',') {
        let ra = ra.trim_matches([' ', '\t']);
        if ra.is_empty() {
            continue;
        }
        let Some((a, b)) = ra.split_once('-') else {
            return Err(RangeErr::Invalid);
        };
        let (a, b) = (a.trim_matches([' ', '\t']), b.trim_matches([' ', '\t']));
        let r = if a.is_empty() {
            // suffix: last N bytes
            if b.is_empty() || b.starts_with('-') {
                return Err(RangeErr::Invalid);
            }
            let mut i: u64 = b.parse().map_err(|_| RangeErr::Invalid)?;
            if i > size {
                i = size;
            }
            Range {
                start: size - i,
                length: i,
            }
        } else {
            let i: u64 = a.parse().map_err(|_| RangeErr::Invalid)?;
            if i >= size {
                no_overlap = true;
                continue;
            }
            let length = if b.is_empty() {
                size - i
            } else {
                let mut j: u64 = b.parse().map_err(|_| RangeErr::Invalid)?;
                if i > j {
                    return Err(RangeErr::Invalid);
                }
                if j >= size {
                    j = size - 1;
                }
                j - i + 1
            };
            Range { start: i, length }
        };
        ranges.push(r);
    }
    if no_overlap && ranges.is_empty() {
        return Err(RangeErr::NoOverlap);
    }
    Ok(ranges)
}

/// Go's `http.Error` on a response whose headers were already set: keeps them, replaces
/// the content type, drops the length.
fn error_with(mut headers: HeaderMap, status: u16, msg: &str) -> Response {
    let mut r = text_err(status, msg);
    headers.remove(header::CONTENT_LENGTH);
    for (k, v) in r.headers() {
        headers.insert(k.clone(), v.clone());
    }
    *r.headers_mut() = headers;
    r
}

fn status_only(headers: HeaderMap, status: StatusCode) -> Response {
    let mut r = Response::new(Body::empty());
    *r.status_mut() = status;
    *r.headers_mut() = headers;
    r
}

fn not_modified(mut headers: HeaderMap) -> Response {
    headers.remove(header::CONTENT_TYPE);
    headers.remove(header::CONTENT_LENGTH);
    headers.remove(header::CONTENT_ENCODING);
    if headers.contains_key(header::ETAG) {
        headers.remove(header::LAST_MODIFIED);
    }
    let mut r = Response::new(Body::empty());
    *r.status_mut() = StatusCode::NOT_MODIFIED;
    *r.headers_mut() = headers;
    r
}

/// Serves `src` like Go's `http.ServeContent`. `headers` are the response headers set so
/// far (Content-Type, ETag, Cache-Control, …); `name` picks a Content-Type by extension when
/// none is set; `modtime` drives Last-Modified and If-Modified-Since.
pub async fn serve_content(
    method: &Method,
    req: &HeaderMap,
    mut headers: HeaderMap,
    name: &str,
    modtime: Option<SystemTime>,
    src: Source,
) -> Response {
    let size = match &src {
        Source::Bytes(b) => b.len() as u64,
        Source::File(p) => match tokio::fs::metadata(p).await {
            Ok(m) => m.len(),
            Err(_) => return err(404, "not found"),
        },
    };
    let modtime = modtime.filter(|t| unix_secs(*t) > 0);
    if let Some(t) = modtime {
        let dt: DateTime<Utc> = t.into();
        if let Ok(v) = HeaderValue::from_str(&http_date(dt)) {
            headers.insert(header::LAST_MODIFIED, v);
        }
    }
    let etag = headers
        .get(header::ETAG)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let is_get_head = method == Method::GET || method == Method::HEAD;
    let mod_secs = modtime.map(unix_secs);

    // checkIfMatch / checkIfUnmodifiedSince
    let mut ch = Cond::None;
    let im = hdr(req, header::IF_MATCH);
    if !im.is_empty() {
        ch = if etag_list(im)
            .iter()
            .any(|t| t == "*" || etag_strong_match(t, &etag))
        {
            Cond::True
        } else {
            Cond::False
        };
    } else {
        let ius = hdr(req, header::IF_UNMODIFIED_SINCE);
        if let (false, Some(ms)) = (ius.is_empty(), mod_secs) {
            if let Some(t) = parse_http_date(ius) {
                ch = if ms <= t.timestamp() {
                    Cond::True
                } else {
                    Cond::False
                };
            }
        }
    }
    if ch == Cond::False {
        return status_only(headers, StatusCode::PRECONDITION_FAILED);
    }
    // checkIfNoneMatch / checkIfModifiedSince
    let inm = hdr(req, header::IF_NONE_MATCH);
    if !inm.is_empty() {
        let matched = etag_list(inm)
            .iter()
            .any(|t| t == "*" || (!etag.is_empty() && etag_weak_match(t, &etag)));
        if matched {
            if is_get_head {
                return not_modified(headers);
            }
            return status_only(headers, StatusCode::PRECONDITION_FAILED);
        }
    } else if is_get_head {
        let ims = hdr(req, header::IF_MODIFIED_SINCE);
        if let (false, Some(ms)) = (ims.is_empty(), mod_secs) {
            if let Some(t) = parse_http_date(ims) {
                if ms <= t.timestamp() {
                    return not_modified(headers);
                }
            }
        }
    }
    let mut range_hdr = hdr(req, header::RANGE).to_string();
    if !range_hdr.is_empty() && is_get_head {
        let ir = hdr(req, header::IF_RANGE);
        if !ir.is_empty() {
            let ok = if ir.starts_with('"') || ir.starts_with("W/") {
                etag_strong_match(ir, &etag)
            } else {
                match (parse_http_date(ir), mod_secs) {
                    (Some(t), Some(ms)) => t.timestamp() == ms,
                    _ => false,
                }
            };
            if !ok {
                range_hdr.clear();
            }
        }
    } else if !is_get_head {
        range_hdr.clear();
    }

    if !headers.contains_key(header::CONTENT_TYPE) {
        let ct = type_by_extension(name).unwrap_or_else(|| "application/octet-stream".into());
        if let Ok(v) = HeaderValue::from_str(&ct) {
            headers.insert(header::CONTENT_TYPE, v);
        }
    }

    let mut ranges = match parse_range(&range_hdr, size) {
        Ok(r) => r,
        Err(RangeErr::NoOverlap) => {
            set(
                &mut headers,
                header::CONTENT_RANGE,
                &format!("bytes */{size}"),
            );
            return error_with(headers, 416, "invalid range: failed to overlap");
        }
        Err(RangeErr::Invalid) => return error_with(headers, 416, "invalid range"),
    };
    if ranges.iter().map(|r| r.length).sum::<u64>() > size {
        // the total number of bytes in all the ranges is larger than the size of the
        // file: ignore the range request
        ranges.clear();
    }
    let mut status = StatusCode::OK;
    let send_size;
    let body: Body;
    let head = method == Method::HEAD;
    match ranges.len() {
        1 => {
            let ra = ranges[0];
            status = StatusCode::PARTIAL_CONTENT;
            send_size = ra.length;
            if let Ok(v) = HeaderValue::from_str(&format!(
                "bytes {}-{}/{size}",
                ra.start,
                ra.start + ra.length - 1
            )) {
                headers.insert(header::CONTENT_RANGE, v);
            }
            body = if head {
                Body::empty()
            } else {
                match body_range(&src, ra.start, ra.length).await {
                    Ok(b) => b,
                    Err(_) => return err(404, "not found"),
                }
            };
        }
        n if n > 1 => {
            status = StatusCode::PARTIAL_CONTENT;
            let boundary = {
                let mut b = [0u8; 30];
                rand::RngCore::fill_bytes(&mut rand::rng(), &mut b);
                hex::encode(b)
            };
            let ctype = headers
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_string();
            let part_headers: Vec<Vec<u8>> = ranges
                .iter()
                .enumerate()
                .map(|(i, r)| {
                    let lead = if i == 0 { "" } else { "\r\n" };
                    format!(
                        "{lead}--{boundary}\r\nContent-Range: bytes {}-{}/{size}\r\nContent-Type: {ctype}\r\n\r\n",
                        r.start,
                        r.start + r.length - 1
                    )
                    .into_bytes()
                })
                .collect();
            let closing = format!("\r\n--{boundary}--\r\n").into_bytes();
            send_size = part_headers.iter().map(|h| h.len() as u64).sum::<u64>()
                + ranges.iter().map(|r| r.length).sum::<u64>()
                + closing.len() as u64;
            if let Ok(v) =
                HeaderValue::from_str(&format!("multipart/byteranges; boundary={boundary}"))
            {
                headers.insert(header::CONTENT_TYPE, v);
            }
            body = if head {
                Body::empty()
            } else {
                let (tx, rx) = tokio::sync::mpsc::channel::<Result<Bytes, std::io::Error>>(4);
                let src = match &src {
                    Source::File(p) => Source::File(p.clone()),
                    Source::Bytes(b) => Source::Bytes(b.clone()),
                };
                tokio::spawn(async move {
                    for (r, h) in ranges.iter().zip(part_headers) {
                        if tx.send(Ok(Bytes::from(h))).await.is_err() {
                            return;
                        }
                        match read_range(&src, r.start, r.length).await {
                            Ok(b) => {
                                if tx.send(Ok(b)).await.is_err() {
                                    return;
                                }
                            }
                            Err(e) => {
                                let _ = tx.send(Err(e)).await;
                                return;
                            }
                        }
                    }
                    let _ = tx.send(Ok(Bytes::from(closing))).await;
                });
                Body::from_stream(futures_util::stream::unfold(rx, |mut rx| async move {
                    rx.recv().await.map(|v| (v, rx))
                }))
            };
        }
        _ => {
            send_size = size;
            body = if head {
                Body::empty()
            } else {
                match body_range(&src, 0, size).await {
                    Ok(b) => b,
                    Err(_) => return err(404, "not found"),
                }
            };
        }
    }
    headers.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    if !headers.contains_key(header::CONTENT_ENCODING) {
        headers.insert(header::CONTENT_LENGTH, HeaderValue::from(send_size));
    }
    let mut r = Response::new(body);
    *r.status_mut() = status;
    *r.headers_mut() = headers;
    r
}

async fn read_range(src: &Source, start: u64, len: u64) -> std::io::Result<Bytes> {
    match src {
        Source::Bytes(b) => Ok(b.slice(start as usize..(start + len) as usize)),
        Source::File(p) => {
            let mut f = tokio::fs::File::open(p).await?;
            f.seek(SeekFrom::Start(start)).await?;
            let mut buf = Vec::with_capacity(len.min(64 << 20) as usize);
            f.take(len).read_to_end(&mut buf).await?;
            Ok(Bytes::from(buf))
        }
    }
}

async fn body_range(src: &Source, start: u64, len: u64) -> std::io::Result<Body> {
    match src {
        Source::Bytes(b) => Ok(Body::from(b.slice(start as usize..(start + len) as usize))),
        Source::File(p) => {
            let mut f = tokio::fs::File::open(p).await?;
            if start > 0 {
                f.seek(SeekFrom::Start(start)).await?;
            }
            let stream = tokio_util::io::ReaderStream::with_capacity(f.take(len), 256 << 10);
            Ok(Body::from_stream(stream))
        }
    }
}

/// Adds a header if the value is valid.
pub fn set(h: &mut HeaderMap, k: header::HeaderName, v: &str) {
    if let Ok(v) = HeaderValue::from_str(v) {
        h.insert(k, v);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges() {
        let ok = |s: &str| match parse_range(s, 100) {
            Ok(v) => v.iter().map(|r| (r.start, r.length)).collect::<Vec<_>>(),
            Err(_) => vec![(999, 999)],
        };
        assert_eq!(ok("bytes=0-9"), vec![(0, 10)]);
        assert_eq!(ok("bytes=90-"), vec![(90, 10)]);
        assert_eq!(ok("bytes=-5"), vec![(95, 5)]);
        assert_eq!(ok("bytes=95-200"), vec![(95, 5)]);
        assert_eq!(ok("bytes=0-1,5-6"), vec![(0, 2), (5, 2)]);
        assert!(matches!(
            parse_range("bytes=200-", 100),
            Err(RangeErr::NoOverlap)
        ));
        assert!(matches!(
            parse_range("items=0-1", 100),
            Err(RangeErr::Invalid)
        ));
        assert!(matches!(
            parse_range("bytes=5-1", 100),
            Err(RangeErr::Invalid)
        ));
        assert_eq!(etag_list(r#""a", W/"b", *"#), vec!["\"a\"", "W/\"b\"", "*"]);
    }

    #[tokio::test]
    async fn serve_ranges_and_conditionals() {
        let src = || Source::Bytes(Bytes::from_static(b"0123456789"));
        let mut req = HeaderMap::new();
        req.insert(header::RANGE, HeaderValue::from_static("bytes=2-4"));
        let mut pre = HeaderMap::new();
        pre.insert(header::ETAG, HeaderValue::from_static("\"x\""));
        let r = serve_content(
            &Method::GET,
            &req,
            pre.clone(),
            "a.txt",
            Some(SystemTime::now()),
            src(),
        )
        .await;
        assert_eq!(r.status(), 206);
        assert_eq!(r.headers()[header::CONTENT_RANGE], "bytes 2-4/10");
        assert_eq!(r.headers()[header::CONTENT_LENGTH], "3");
        let b = axum::body::to_bytes(r.into_body(), 100).await.unwrap();
        assert_eq!(&b[..], b"234");

        let mut req = HeaderMap::new();
        req.insert(header::IF_NONE_MATCH, HeaderValue::from_static("\"x\""));
        let r = serve_content(&Method::GET, &req, pre.clone(), "a.txt", None, src()).await;
        assert_eq!(r.status(), 304);

        let mut req = HeaderMap::new();
        req.insert(header::RANGE, HeaderValue::from_static("bytes=20-"));
        let r = serve_content(&Method::GET, &req, pre.clone(), "a.txt", None, src()).await;
        assert_eq!(r.status(), 416);
        assert_eq!(r.headers()[header::CONTENT_RANGE], "bytes */10");

        let mut req = HeaderMap::new();
        req.insert(header::RANGE, HeaderValue::from_static("bytes=0-0,8-9"));
        let r = serve_content(&Method::GET, &req, pre, "a.txt", None, src()).await;
        assert_eq!(r.status(), 206);
        let len: usize = r.headers()[header::CONTENT_LENGTH]
            .to_str()
            .unwrap()
            .parse()
            .unwrap();
        let b = axum::body::to_bytes(r.into_body(), 10000).await.unwrap();
        assert_eq!(b.len(), len);
        assert!(String::from_utf8_lossy(&b).contains(
            "Content-Range: bytes 8-9/10\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n89"
        ));
    }
}
