//! The HTTP API (same routes, JSON and headers as Letopis), media streaming and the static
//! web UI. Handlers are split like the Go files: `api_media`, `api_docs`, `api_extra`,
//! `offline`, `remote`; `listen` binds HTTP/HTTPS; `http` holds the shared helpers.

pub mod api_docs;
pub mod api_extra;
pub mod api_media;
pub mod http;
pub mod listen;
pub mod offline;
pub mod remote;

use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode, Version};
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::routing::{get, post};
use axum::Router;
use bytes::Bytes;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::compression::CompressionLayer;

use crate::config::AuthMode;
use crate::engine::Core;
use http::{err, json, read_json, ReqInfo};

pub(crate) type St = State<Arc<Core>>;

/// Builds the router: public routes, the authenticated group (with `extra` merged in), the
/// static UI fallback and the middleware stack.
pub(crate) fn router(core: Arc<Core>, extra: Router) -> Router {
    let protected = Router::new()
        .route("/api/status", get(api_media::status))
        .route("/api/libraries", get(api_media::libraries))
        .route("/api/libraries/{lib}/tree", get(api_media::tree))
        .route("/api/files", get(api_media::files))
        .route("/api/files/{id}", get(api_media::file))
        .route("/api/days", get(api_media::days))
        .route("/api/resolve", post(api_media::resolve))
        .route("/api/proxy", post(api_media::proxy))
        .route("/api/scan", post(api_media::scan))
        .route(
            "/api/prefs",
            get(api_extra::prefs_get).put(api_extra::prefs_put),
        )
        .route("/api/duplicates", get(api_extra::duplicates))
        .route("/api/cameras", get(api_extra::cameras))
        .route("/api/usage", get(api_extra::usage))
        .route("/api/search", get(api_extra::search))
        .route("/api/match", post(api_extra::match_files))
        .route("/api/prewarm", post(api_extra::prewarm))
        .route("/api/remote/{id}/pair", post(remote::pair))
        .route("/api/documents", get(api_docs::list).post(api_docs::create))
        .route("/api/documents/import", post(api_docs::import))
        .route("/api/documents/stress-test", post(api_docs::stress_test))
        .route(
            "/api/documents/{id}",
            get(api_docs::get_doc)
                .put(api_docs::save)
                .delete(api_docs::delete_doc),
        )
        .route("/api/documents/{id}/duplicate", post(api_docs::duplicate))
        .route("/api/documents/{id}/snapshot", post(api_docs::snapshot))
        .route("/api/documents/{id}/history", get(api_docs::history))
        .route(
            "/api/documents/{id}/history/{version}",
            get(api_docs::version),
        )
        .route(
            "/api/documents/{id}/history/{version}/restore",
            post(api_docs::restore),
        )
        .route("/api/documents/{id}/export", get(api_docs::export))
        .route("/api/documents/{id}/offline", post(offline::offline))
        .route("/api/export", get(api_docs::export_all))
        .route("/media/{lib}/{*path}", get(api_media::original))
        .route("/d/{key}/{name}", get(api_media::derived))
        .with_state(core.clone())
        .merge(extra)
        .layer(middleware::from_fn_with_state(core.clone(), require_auth));

    let public = Router::new()
        .route("/api/session", get(session))
        .route("/api/login", post(login))
        .route("/api/logout", post(logout))
        .route("/ca.crt", get(ca))
        .route("/healthz", get(|| async { "ok" }))
        .route("/remote/{token}", get(remote::join))
        .route("/api/remote/{id}/stream", get(remote::stream))
        .route("/api/remote/{id}/send", post(remote::send))
        .fallback(static_files)
        .with_state(core.clone());

    let types = [
        "application/json",
        "text/html",
        "text/css",
        "text/javascript",
        "application/javascript",
        "image/svg+xml",
        "text/plain",
    ];
    let compress = CompressionLayer::new()
        .gzip(true)
        .deflate(true)
        .no_br()
        .no_zstd()
        .compress_when(
            move |status: StatusCode, _: Version, h: &HeaderMap, _: &axum::http::Extensions| {
                if status == StatusCode::PARTIAL_CONTENT || h.contains_key(header::CONTENT_ENCODING)
                {
                    return false;
                }
                let ct = h
                    .get(header::CONTENT_TYPE)
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("");
                let base = ct.split(';').next().unwrap_or("").trim();
                types.contains(&base)
            },
        );

    public
        .merge(protected)
        .layer(middleware::from_fn_with_state(core.clone(), token_gate))
        .layer(middleware::map_response(security_headers))
        .layer(compress)
        .layer(CatchPanicLayer::new())
}

async fn security_headers(mut r: Response) -> Response {
    let h = r.headers_mut();
    h.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    h.insert("referrer-policy", HeaderValue::from_static("same-origin"));
    h.insert("x-frame-options", HeaderValue::from_static("SAMEORIGIN"));
    r
}

impl Core {
    /// Whether the request is authenticated (session cookie, desktop token, or no auth).
    pub(crate) fn authed(&self, ri: &ReqInfo) -> bool {
        self.auth.valid(&ri.cookies(), ri.token_header().as_deref())
    }
}

async fn require_auth(State(c): St, req: Request, next: Next) -> Response {
    let (parts, body) = req.into_parts();
    let ri = ReqInfo::from_parts(&parts);
    if !c.authed(&ri) {
        return err(401, "login required");
    }
    next.run(Request::from_parts(parts, body)).await
}

/// Token mode (desktop): Host check against DNS rebinding, and `?token=` → cookie.
async fn token_gate(State(c): St, req: Request, next: Next) -> Response {
    let AuthMode::Token(token) = &c.cfg.auth else {
        return next.run(req).await;
    };
    let (parts, body) = req.into_parts();
    let host = http::request_host(&parts);
    let ports = c
        .local_ports
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    let host_ok = ports.iter().any(|p| {
        host == format!("127.0.0.1:{p}") || host.eq_ignore_ascii_case(&format!("localhost:{p}"))
    });
    if !host_ok {
        return err(403, "forbidden host");
    }
    // Desktop development: the page comes from Vite, which proxies the API;
    // the UI hands the token over with `GET /api/session?token=…`.
    if parts.method == Method::GET && parts.uri.path() == "/api/session" {
        let valid = parts.uri.query().is_some_and(|q| {
            form_urlencoded::parse(q.as_bytes())
                .any(|(k, v)| k == "token" && crate::auth::ct_eq(&v, token))
        });
        if valid {
            let mut r = next.run(Request::from_parts(parts, body)).await;
            http::set(
                r.headers_mut(),
                header::SET_COOKIE,
                &format!(
                    "{}={token}; Path=/; HttpOnly; SameSite=Strict",
                    crate::auth::TOKEN_COOKIE
                ),
            );
            return r;
        }
    }
    if parts.method == Method::GET && !parts.uri.path().starts_with("/api/") {
        if let Some(q) = parts.uri.query() {
            let pairs: Vec<(String, String)> = form_urlencoded::parse(q.as_bytes())
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect();
            if let Some((_, t)) = pairs.iter().find(|(k, _)| k == "token") {
                if crate::auth::ct_eq(t, token) {
                    let rest: Vec<_> = pairs.iter().filter(|(k, _)| k != "token").collect();
                    let mut loc = parts.uri.path().to_string();
                    if !rest.is_empty() {
                        let mut ser = form_urlencoded::Serializer::new(String::new());
                        for (k, v) in rest {
                            ser.append_pair(k, v);
                        }
                        loc = format!("{loc}?{}", ser.finish());
                    }
                    let mut r = Response::new(Body::empty());
                    *r.status_mut() = StatusCode::FOUND;
                    http::set(r.headers_mut(), header::LOCATION, &loc);
                    http::set(
                        r.headers_mut(),
                        header::SET_COOKIE,
                        &format!(
                            "{}={token}; Path=/; HttpOnly; SameSite=Strict",
                            crate::auth::TOKEN_COOKIE
                        ),
                    );
                    http::set(r.headers_mut(), header::CACHE_CONTROL, "no-store");
                    return r;
                }
            }
        }
    }
    next.run(Request::from_parts(parts, body)).await
}

// ---------- session ----------

async fn session(State(c): St, ri: ReqInfo) -> Response {
    let authed = c.authed(&ri);
    let auth_enabled = match &c.cfg.auth {
        AuthMode::Token(_) => !authed,
        _ => c.auth.password_enabled(),
    };
    json(
        200,
        &serde_json::json!({
            "authenticated": authed,
            "authEnabled": auth_enabled,
            "version": c.cfg.version,
            "https": ri.tls,
            "httpsAddr": c.cfg.https_addr.clone().unwrap_or_default(),
            "proto": ri.proto,
        }),
    )
}

#[derive(serde::Deserialize)]
struct LoginBody {
    #[serde(default)]
    password: String,
}

async fn login(State(c): St, ri: ReqInfo, body: Body) -> Response {
    let Ok(b) = read_json::<LoginBody>(body, 4096).await else {
        return err(400, "bad request");
    };
    match &c.cfg.auth {
        AuthMode::Password(p) if !p.is_empty() => {}
        AuthMode::Token(_) => return err(401, "Open Tessera from the desktop app."),
        _ => return json(200, &serde_json::json!({"ok": true})),
    }
    match c.auth.login(&ri.ip, &b.password, ri.tls).await {
        Ok(cookie) => {
            let mut r = json(200, &serde_json::json!({"ok": true}));
            http::set(r.headers_mut(), header::SET_COOKIE, &cookie);
            r
        }
        Err(msg) => err(401, &msg),
    }
}

async fn logout(State(c): St, ri: ReqInfo) -> Response {
    let mut r = json(200, &serde_json::json!({"ok": true}));
    http::set(
        r.headers_mut(),
        header::SET_COOKIE,
        &c.auth.logout_cookie(ri.tls),
    );
    r
}

async fn ca(State(c): St) -> Response {
    let Some(p) = c.tls.ca_cert() else {
        return http::text_err(404, "404 page not found");
    };
    match tokio::fs::read(&p).await {
        Ok(b) => {
            let mut r = Response::new(Body::from(b));
            http::set(
                r.headers_mut(),
                header::CONTENT_TYPE,
                "application/x-x509-ca-cert",
            );
            http::set(
                r.headers_mut(),
                header::CONTENT_DISPOSITION,
                r#"attachment; filename="tessera-ca.crt""#,
            );
            r
        }
        Err(_) => http::text_err(404, "404 page not found"),
    }
}

// ---------- static web UI ----------

async fn static_files(
    State(c): St,
    method: Method,
    uri: axum::http::Uri,
    req_headers: HeaderMap,
) -> Response {
    let path = uri.path().to_string();
    if path.starts_with("/api/") || path.starts_with("/media/") || path.starts_with("/d/") {
        return err(404, "not found");
    }
    let clean = crate::util::path_clean(&path);
    let mut p = clean.trim_start_matches('/').to_string();
    if p.is_empty() {
        p = "index.html".into();
    }
    let web = c.cfg.web.clone();
    let p2 = p.clone();
    let found = http::blocking(move || {
        if web.is_dir(&p2) {
            return Err(true);
        }
        match web.read(&p2) {
            Some(b) => Ok((p2, b)),
            None => match web.read("index.html") {
                // SPA fallback
                Some(b) => Ok(("index.html".to_string(), b)),
                None => Err(false),
            },
        }
    })
    .await;
    let (p, data) = match found {
        Ok(v) => v,
        Err(true) => return http::text_err(404, "404 page not found"),
        Err(false) => return http::text_err(500, "web UI not built"),
    };
    let mut h = HeaderMap::new();
    if p.starts_with("assets/") {
        http::set(
            &mut h,
            header::CACHE_CONTROL,
            "public, max-age=31536000, immutable",
        );
    } else {
        http::set(&mut h, header::CACHE_CONTROL, "no-cache");
    }
    let bytes = match data {
        std::borrow::Cow::Borrowed(b) => Bytes::from_static(b),
        std::borrow::Cow::Owned(v) => Bytes::from(v),
    };
    http::serve_content(
        &method,
        &req_headers,
        h,
        &p,
        Some(c.started_at),
        http::Source::Bytes(bytes),
    )
    .await
}
