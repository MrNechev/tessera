//! Runs Tessera's engine on 127.0.0.1 for the window.
//!
//! TEMPORARY (until the ported library/documents core is merged): serves the
//! tool routes, the embedded UI and a token check itself.

use std::net::SocketAddr;
use std::sync::{Arc, OnceLock};

use axum::body::Body;
use axum::extract::Request;
use axum::http::{header, HeaderValue, StatusCode, Uri};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Redirect, Response};
use axum::Router;
use tauri::{AppHandle, Manager};
use tessera_core::tools::{self, Mode, Tools, ToolsConfig};

pub struct Started {
    pub addr: SocketAddr,
    pub token: String,
    #[allow(dead_code)]
    pub tools: Arc<Tools>,
}

impl Started {
    /// What the window opens. In development the UI comes from Vite (which
    /// proxies the API to the engine on a fixed port).
    pub fn window_url(&self) -> url::Url {
        let base = if cfg!(debug_assertions) {
            "http://localhost:1420/".to_string()
        } else {
            format!("http://{}/", self.addr)
        };
        let mut u = url::Url::parse(&base).expect("valid url");
        u.query_pairs_mut().append_pair("token", &self.token);
        u
    }
}

static APP: OnceLock<AppHandle> = OnceLock::new();
static TOKEN: OnceLock<String> = OnceLock::new();

fn random_token() -> String {
    let mut b = [0u8; 24];
    getrandom::fill(&mut b).expect("random");
    b.iter().map(|x| format!("{x:02x}")).collect()
}

pub fn tools_config(app: &AppHandle) -> ToolsConfig {
    let p = app.path();
    let data = p
        .app_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir().join("tessera-data"));
    let cache = p
        .app_cache_dir()
        .unwrap_or_else(|_| std::env::temp_dir().join("tessera"));
    let config = p.app_config_dir().unwrap_or_else(|_| data.clone());
    ToolsConfig {
        mode: Mode::Desktop,
        uploads_dir: cache.join("uploads"),
        outputs_dir: tools::paths::default_save_dir(),
        inbox_dir: None,
        data_dir: data,
        config_dir: config,
        cache_dir: cache,
    }
}

pub async fn start(app: AppHandle) -> anyhow::Result<Started> {
    let _ = APP.set(app.clone());
    let token = random_token();
    let _ = TOKEN.set(token.clone());
    let tools = Arc::new(Tools::new(tools_config(&app)));
    let router = Router::new()
        .merge(tools::api::routes(tools.clone()))
        .fallback(static_file)
        .layer(middleware::from_fn(check_token));
    let port: u16 = if cfg!(debug_assertions) { 1430 } else { 0 };
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    let addr = listener.local_addr()?;
    tauri::async_runtime::spawn(async move {
        if let Err(e) = axum::serve(listener, router).await {
            log::error!("engine server stopped: {e}");
        }
    });
    Ok(Started { addr, token, tools })
}

fn cookie_token(req: &Request) -> Option<String> {
    let cookies = req.headers().get(header::COOKIE)?.to_str().ok()?;
    cookies.split(';').find_map(|c| {
        let (k, v) = c.trim().split_once('=')?;
        (k == "tessera_token").then(|| v.to_string())
    })
}

async fn check_token(req: Request, next: Next) -> Response {
    let Some(token) = TOKEN.get() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    // DNS-rebinding guard: only our own loopback names.
    let host_ok = req
        .headers()
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .map(|h| h.starts_with("127.0.0.1:") || h.starts_with("localhost:"))
        .unwrap_or(false);
    if !host_ok {
        return StatusCode::FORBIDDEN.into_response();
    }
    let query_token = req.uri().query().and_then(|q| {
        url::form_urlencoded::parse(q.as_bytes())
            .find(|(k, _)| k == "token")
            .map(|(_, v)| v.into_owned())
    });
    if let Some(t) = query_token {
        if &t == token {
            // Set the cookie and drop the token from the address bar.
            let uri = strip_token(req.uri());
            let mut res = if req.uri().path().starts_with("/api/") {
                next.run(req).await
            } else {
                Redirect::to(&uri).into_response()
            };
            if let Ok(v) = HeaderValue::from_str(&format!(
                "tessera_token={t}; Path=/; HttpOnly; SameSite=Strict"
            )) {
                res.headers_mut().append(header::SET_COOKIE, v);
            }
            return res;
        }
    }
    let ok = cookie_token(&req).as_deref() == Some(token.as_str())
        || req
            .headers()
            .get("x-tessera-token")
            .and_then(|v| v.to_str().ok())
            == Some(token.as_str());
    if !ok {
        return (StatusCode::UNAUTHORIZED, "Tessera: not authorised").into_response();
    }
    next.run(req).await
}

fn strip_token(uri: &Uri) -> String {
    let rest: Vec<(String, String)> = uri
        .query()
        .map(|q| {
            url::form_urlencoded::parse(q.as_bytes())
                .filter(|(k, _)| k != "token")
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect()
        })
        .unwrap_or_default();
    if rest.is_empty() {
        uri.path().to_string()
    } else {
        let q: String = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(rest)
            .finish();
        format!("{}?{q}", uri.path())
    }
}

/// The UI, embedded in the binary by Tauri (same files as `dist/`).
async fn static_file(uri: Uri) -> Response {
    let Some(app) = APP.get() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let path = uri.path().trim_start_matches('/');
    let resolver = app.asset_resolver();
    let asset = resolver
        .get(if path.is_empty() {
            "index.html".into()
        } else {
            path.to_string()
        })
        .or_else(|| resolver.get("index.html".into()));
    match asset {
        Some(a) => {
            let cache = if path.starts_with("assets/") {
                "public, max-age=31536000, immutable"
            } else {
                "no-cache"
            };
            Response::builder()
                .header(header::CONTENT_TYPE, a.mime_type)
                .header(header::CACHE_CONTROL, cache)
                .body(Body::from(a.bytes))
                .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}
