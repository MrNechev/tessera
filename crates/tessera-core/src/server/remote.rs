//! Phone remote: the presentation and any paired phones share a small message channel per
//! document (server-sent events down, POST up). A phone pairs by scanning a QR code that
//! holds a one-time token; it gets a cookie that only opens this channel.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::Response;
use bytes::Bytes;
use tokio::sync::mpsc;

use super::http::{blocking, err, json, read_json, set, ReqInfo};
use super::St;

#[derive(Default)]
struct Room {
    subs: HashMap<u64, mpsc::Sender<Bytes>>,
    /// Last "state" message, replayed to new subscribers.
    state: Option<Bytes>,
}

/// Rooms and pairing tokens.
#[derive(Default)]
pub struct Hub {
    rooms: Mutex<HashMap<String, Room>>,
    tokens: Mutex<HashMap<String, (String, Instant)>>,
    next: AtomicU64,
}

impl Hub {
    fn subscribe(&self, doc: &str) -> (u64, mpsc::Receiver<Bytes>, Option<Bytes>) {
        let (tx, rx) = mpsc::channel(32);
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let mut g = self.rooms.lock().unwrap_or_else(|e| e.into_inner());
        let r = g.entry(doc.to_string()).or_default();
        r.subs.insert(id, tx);
        (id, rx, r.state.clone())
    }

    fn unsubscribe(&self, doc: &str, id: u64) {
        let mut g = self.rooms.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(r) = g.get_mut(doc) {
            r.subs.remove(&id);
            if r.subs.is_empty() && r.state.is_none() {
                g.remove(doc);
            }
        }
    }

    fn publish(&self, doc: &str, msg: Bytes, is_state: bool) -> usize {
        let mut g = self.rooms.lock().unwrap_or_else(|e| e.into_inner());
        let r = g.entry(doc.to_string()).or_default();
        if is_state {
            r.state = Some(msg.clone());
        }
        // slow client: drop
        r.subs
            .values()
            .filter(|tx| tx.try_send(msg.clone()).is_ok())
            .count()
    }

    fn new_token(&self, doc: &str) -> String {
        let mut g = self.tokens.lock().unwrap_or_else(|e| e.into_inner());
        g.retain(|_, (_, exp)| Instant::now() < *exp);
        let mut b = [0u8; 16];
        rand::RngCore::fill_bytes(&mut rand::rng(), &mut b);
        let tok = hex::encode(b);
        g.insert(
            tok.clone(),
            (
                doc.to_string(),
                Instant::now() + Duration::from_secs(15 * 60),
            ),
        );
        tok
    }

    /// Reusable until it expires (a second phone may pair).
    fn use_token(&self, tok: &str) -> Option<String> {
        let g = self.tokens.lock().unwrap_or_else(|e| e.into_inner());
        g.get(tok)
            .filter(|(_, exp)| Instant::now() < *exp)
            .map(|(d, _)| d.clone())
    }
}

fn split_port(addr: &str) -> String {
    addr.rsplit_once(':')
        .map(|(_, p)| p.to_string())
        .unwrap_or_default()
}

fn split_host(host: &str) -> String {
    if let Some(rest) = host.strip_prefix('[') {
        if let Some((h, _)) = rest.split_once("]:") {
            return h.to_string();
        }
    }
    match host.rsplit_once(':') {
        Some((h, p)) if !h.contains(':') && p.chars().all(|c| c.is_ascii_digit()) => h.to_string(),
        _ => host.to_string(),
    }
}

pub async fn pair(State(c): St, Path(doc): Path<String>, ri: ReqInfo) -> Response {
    let c2 = c.clone();
    let d2 = doc.clone();
    if blocking(move || c2.docs.get(&d2)).await.is_err() {
        return err(404, "document not found");
    }
    let tok = c.remote.new_token(&doc);
    // Addresses a phone on the same network can use. Plain HTTP avoids the certificate
    // warning on phones that don't trust the private CA.
    let mut urls: Vec<String> = Vec::new();
    let mut add = |u: String| {
        if !urls.contains(&u) {
            urls.push(u);
        }
    };
    let mut http_port = c
        .cfg
        .http_addr
        .as_deref()
        .map(split_port)
        .unwrap_or_default();
    if http_port == "0" {
        http_port = c
            .local_ports
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .first()
            .map(|p| p.to_string())
            .unwrap_or_default();
    }
    let req_host = split_host(&ri.host);
    let hosts = std::iter::once(req_host).chain(c.cfg.tls_hosts.iter().cloned());
    for h in hosts {
        if h.is_empty() || h == "localhost" || h.starts_with("127.") || h == "::1" {
            continue;
        }
        let hh = if h.contains(':') {
            format!("[{h}]")
        } else {
            h.clone()
        };
        if !http_port.is_empty() {
            add(format!("http://{hh}:{http_port}/remote/{tok}"));
        }
    }
    let scheme = if ri.tls { "https" } else { "http" };
    add(format!("{scheme}://{}/remote/{tok}", ri.host));
    json(200, &serde_json::json!({"token": tok, "urls": urls}))
}

/// Opened by the phone's camera app: swaps the token for a remote cookie and opens the
/// remote page.
pub async fn join(State(c): St, Path(tok): Path<String>, ri: ReqInfo) -> Response {
    let Some(doc) = c.remote.use_token(&tok) else {
        let mut r = Response::new(Body::from(
            r#"<!doctype html><meta name="viewport" content="width=device-width"><body style="font:16px system-ui;padding:2em;background:#121117;color:#eee">This pairing code has expired. Press <b>P → Phone remote</b> in the presentation again and scan the new code.</body>"#,
        ));
        *r.status_mut() = StatusCode::GONE;
        set(
            r.headers_mut(),
            header::CONTENT_TYPE,
            "text/html; charset=utf-8",
        );
        return r;
    };
    let mut r = Response::new(Body::empty());
    *r.status_mut() = StatusCode::FOUND;
    set(
        r.headers_mut(),
        header::LOCATION,
        &format!("/#/remote/{doc}"),
    );
    set(
        r.headers_mut(),
        header::SET_COOKIE,
        &c.auth
            .remote_cookie(&doc, Duration::from_secs(12 * 3600), ri.tls),
    );
    r
}

fn remote_allowed(c: &crate::engine::Core, ri: &ReqInfo, doc: &str) -> bool {
    c.authed(ri) || c.auth.valid_remote(&ri.cookies(), doc)
}

pub async fn stream(State(c): St, Path(doc): Path<String>, ri: ReqInfo) -> Response {
    if !remote_allowed(&c, &ri, &doc) {
        return err(401, "login required");
    }
    let (id, rx, state) = c.remote.subscribe(&doc);
    struct Guard {
        hub: std::sync::Arc<crate::engine::Core>,
        doc: String,
        id: u64,
    }
    impl Drop for Guard {
        fn drop(&mut self) {
            self.hub.remote.unsubscribe(&self.doc, self.id);
        }
    }
    let mut first = Vec::new();
    if let Some(s) = state {
        first.extend_from_slice(b"data: ");
        first.extend_from_slice(&s);
        first.extend_from_slice(b"\n\n");
    }
    first.extend_from_slice(b": hello\n\n");
    let guard = Guard {
        hub: c.clone(),
        doc,
        id,
    };
    let mut ping = tokio::time::interval_at(
        tokio::time::Instant::now() + Duration::from_secs(25),
        Duration::from_secs(25),
    );
    ping.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let init = Some(Bytes::from(first));
    let st = futures_util::stream::unfold(
        (init, rx, ping, guard),
        |(mut init, mut rx, mut ping, guard)| async move {
            if let Some(b) = init.take() {
                return Some((Ok::<_, std::io::Error>(b), (init, rx, ping, guard)));
            }
            tokio::select! {
                m = rx.recv() => {
                    let m = m?;
                    let mut b = b"data: ".to_vec();
                    b.extend_from_slice(&m);
                    b.extend_from_slice(b"\n\n");
                    Some((Ok(Bytes::from(b)), (init, rx, ping, guard)))
                }
                _ = ping.tick() => Some((Ok(Bytes::from_static(b": ping\n\n")), (init, rx, ping, guard))),
            }
        },
    );
    let mut r = Response::new(Body::from_stream(st));
    set(r.headers_mut(), header::CONTENT_TYPE, "text/event-stream");
    set(r.headers_mut(), header::CACHE_CONTROL, "no-store");
    set(
        r.headers_mut(),
        header::HeaderName::from_static("x-accel-buffering"),
        "no",
    );
    r
}

pub async fn send(State(c): St, Path(doc): Path<String>, ri: ReqInfo, body: Body) -> Response {
    if !remote_allowed(&c, &ri, &doc) {
        return err(401, "login required");
    }
    let Ok(msg) = read_json::<serde_json::Map<String, serde_json::Value>>(body, 64 << 10).await
    else {
        return err(400, "bad request");
    };
    let typ = msg.get("type").and_then(|v| v.as_str()).unwrap_or("");
    // phones may only send navigation; state comes from the presentation (full session)
    let full = c.authed(&ri);
    match typ {
        "nav" | "goto" | "hello" | "black" | "pointer" => {}
        "state" => {
            if !full {
                return err(403, "forbidden");
            }
        }
        _ => return err(400, "unknown message"),
    }
    let b = serde_json::to_vec(&msg).unwrap_or_default();
    let n = c.remote.publish(&doc, Bytes::from(b), typ == "state");
    json(200, &serde_json::json!({"delivered": n}))
}
