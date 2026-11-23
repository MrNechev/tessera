//! End-to-end tests of the embeddable core: token auth (desktop), Host check, extra routes
//! inside the auth group, embedded web assets, missing ffmpeg, runtime library changes and
//! the filesystem watcher.

use std::borrow::Cow;
use std::path::Path;
use std::time::Duration;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::routing::get;
use tessera_core::{AuthMode, Config, Core, Library, WebAssets};
use tower::ServiceExt;

fn assets(p: &str) -> Option<Cow<'static, [u8]>> {
    match p {
        "index.html" => Some(Cow::Borrowed(
            b"<!doctype html><html><title>T</title></html>",
        )),
        "assets/app.js" => Some(Cow::Borrowed(b"console.log(1)")),
        _ => None,
    }
}

fn write_png(p: &Path, w: u32, h: u32) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    image::RgbImage::from_fn(w, h, |x, y| image::Rgb([(x * 7) as u8, (y * 3) as u8, 90]))
        .save(p)
        .unwrap();
}

struct Ctx {
    router: axum::Router,
    host: String,
    token: String,
}

impl Ctx {
    async fn call(
        &self,
        method: &str,
        uri: &str,
        token: bool,
        body: Option<&str>,
    ) -> (StatusCode, axum::http::HeaderMap, String) {
        let mut b = Request::builder()
            .method(method)
            .uri(uri)
            .header(header::HOST, &self.host);
        if token {
            b = b.header("X-Tessera-Token", &self.token);
        }
        let req = match body {
            Some(s) => b
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(s.to_string()))
                .unwrap(),
            None => b.body(Body::empty()).unwrap(),
        };
        let resp = self.router.clone().oneshot(req).await.unwrap();
        let status = resp.status();
        let headers = resp.headers().clone();
        let bytes = axum::body::to_bytes(resp.into_body(), 64 << 20)
            .await
            .unwrap();
        (
            status,
            headers,
            String::from_utf8_lossy(&bytes).into_owned(),
        )
    }

    async fn json(&self, uri: &str) -> serde_json::Value {
        let (s, _, b) = self.call("GET", uri, true, None).await;
        assert_eq!(s, 200, "{uri}: {b}");
        serde_json::from_str(&b).unwrap()
    }

    async fn wait_total(&self, lib: &str, want: i64) {
        for _ in 0..200 {
            let v = self
                .json(&format!("/api/files?library={lib}&recursive=1"))
                .await;
            if v["total"] == want {
                return;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        panic!("library {lib} never reached {want} files");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn desktop_core_end_to_end() {
    let tmp = tempfile::tempdir().unwrap();
    let lib1 = tmp.path().join("lib1");
    let lib2 = tmp.path().join("lib2");
    write_png(&lib1.join("a.png"), 64, 48);
    write_png(&lib1.join("sub/b.png"), 30, 40);
    std::fs::write(lib1.join("broken.mp4"), b"not a video").unwrap();
    write_png(&lib2.join("c.png"), 20, 20);

    let mut cfg = Config::desktop(
        tmp.path().join("data"),
        tmp.path().join("cache"),
        vec![Library {
            id: "main".into(),
            name: "Main".into(),
            root: lib1.clone(),
            writable: false,
        }],
    );
    cfg.ffmpeg = "/nonexistent/ffmpeg".into();
    cfg.ffprobe = "/nonexistent/ffprobe".into();
    cfg.web = WebAssets::Embedded(assets);
    cfg.background_gen = false;
    let token = match &cfg.auth {
        AuthMode::Token(t) => t.clone(),
        _ => unreachable!(),
    };
    // a Letopis index is renamed on start
    std::fs::create_dir_all(tmp.path().join("data")).unwrap();
    drop(tessera_core::store::Store::open(&tmp.path().join("data/letopis.db")).unwrap());

    let core = Core::start(cfg).await.expect("start without ffmpeg");
    assert!(tmp.path().join("data/tessera.db").exists());
    assert!(!tmp.path().join("data/letopis.db").exists());
    let extra = axum::Router::new().route("/api/tools/ping", get(|| async { "pong" }));
    let router = core.router_with(extra);
    let (addr, task) = core.bind_local(router.clone()).await.unwrap();
    assert!(addr.ip().is_loopback());
    let ctx = Ctx {
        router,
        host: format!("127.0.0.1:{}", addr.port()),
        token: token.clone(),
    };

    // ---- auth ----
    let (s, _, _) = ctx.call("GET", "/api/libraries", false, None).await;
    assert_eq!(s, 401);
    let (s, _, _) = ctx.call("GET", "/api/tools/ping", false, None).await;
    assert_eq!(s, 401, "extra routes are inside the auth group");
    let (s, _, b) = ctx.call("GET", "/api/tools/ping", true, None).await;
    assert_eq!((s.as_u16(), b.as_str()), (200, "pong"));
    let bad_host = Ctx {
        router: ctx.router.clone(),
        host: "evil.example:80".into(),
        token: token.clone(),
    };
    let (s, _, _) = bad_host.call("GET", "/api/libraries", true, None).await;
    assert_eq!(s, 403, "DNS rebinding guard");
    let (s, h, _) = ctx
        .call("GET", &format!("/?token={token}&x=1"), false, None)
        .await;
    assert_eq!(s, 302);
    assert_eq!(h[header::LOCATION], "/?x=1");
    let cookie = h[header::SET_COOKIE].to_str().unwrap().to_string();
    assert!(
        cookie.starts_with(&format!("tessera_token={token}"))
            && cookie.contains("HttpOnly")
            && cookie.contains("SameSite=Strict")
    );
    let req = Request::builder()
        .uri("/api/session")
        .header(header::HOST, format!("localhost:{}", addr.port()))
        .header(header::COOKIE, format!("tessera_token={token}"))
        .body(Body::empty())
        .unwrap();
    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(resp.into_body(), 1 << 20)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        (v["authenticated"].as_bool(), v["authEnabled"].as_bool()),
        (Some(true), Some(false))
    );
    let (_, _, b) = ctx.call("GET", "/api/session", false, None).await;
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    assert_eq!(
        (v["authenticated"].as_bool(), v["authEnabled"].as_bool()),
        (Some(false), Some(true))
    );

    // ---- static UI (embedded) ----
    let (s, h, b) = ctx.call("GET", "/", false, None).await;
    assert_eq!(s, 200);
    assert!(b.contains("<title>T</title>"));
    assert_eq!(h[header::CACHE_CONTROL], "no-cache");
    let (s, _, b2) = ctx.call("GET", "/documents/123", false, None).await;
    assert_eq!((s.as_u16(), b2), (200, b));
    let (s, h, _) = ctx.call("GET", "/assets/app.js", false, None).await;
    assert_eq!(s, 200);
    assert!(h[header::CONTENT_TYPE]
        .to_str()
        .unwrap()
        .starts_with("text/javascript"));
    assert_eq!(
        h[header::CACHE_CONTROL],
        "public, max-age=31536000, immutable"
    );
    let (s, _, b) = ctx.call("GET", "/api/nope", true, None).await;
    assert_eq!((s.as_u16(), b.trim()), (404, r#"{"error":"not found"}"#));

    // ---- scanning without ffmpeg ----
    ctx.wait_total("main", 3).await;
    let libs = ctx.json("/api/libraries").await;
    assert_eq!(libs[0]["writable"], false);
    let status = ctx.json("/api/status").await;
    assert_eq!(status["ffmpeg"], false);
    assert_eq!(status["tools"]["ffprobe"], false);
    let mut probed = serde_json::Value::Null;
    for _ in 0..100 {
        let v = ctx
            .json("/api/files?library=main&recursive=1&sort=name")
            .await;
        if v["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|f| f["probed"] == true)
        {
            probed = v;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let items = probed["items"].as_array().expect("all probed");
    let a = items.iter().find(|f| f["path"] == "a.png").unwrap();
    assert_eq!(
        (a["width"].as_i64(), a["height"].as_i64()),
        (Some(64), Some(48))
    );
    let broken = items.iter().find(|f| f["path"] == "broken.mp4").unwrap();
    assert!(
        broken["probeError"]
            .as_str()
            .unwrap()
            .contains("tool missing"),
        "{broken}"
    );
    let key = broken["key"].as_str().unwrap();
    let (s, _, b) = ctx
        .call("GET", &format!("/d/{key}/t320.webp"), true, None)
        .await;
    assert_eq!(
        (s.as_u16(), b.trim()),
        (500, r#"{"error":"thumbnail failed"}"#)
    );
    // the original is still streamed (Range)
    let req = Request::builder()
        .uri("/media/main/broken.mp4")
        .header(header::HOST, &ctx.host)
        .header("X-Tessera-Token", &token)
        .header(header::RANGE, "bytes=4-6")
        .body(Body::empty())
        .unwrap();
    let resp = ctx.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 206);
    assert_eq!(resp.headers()[header::CONTENT_RANGE], "bytes 4-6/11");
    assert_eq!(
        &axum::body::to_bytes(resp.into_body(), 100).await.unwrap()[..],
        b"a v"
    );

    // ---- documents round trip ----
    let (s, _, b) = ctx
        .call("POST", "/api/documents", true, Some(r#"{"title":"Desk"}"#))
        .await;
    assert_eq!(s, 201);
    let d: serde_json::Value = serde_json::from_str(&b).unwrap();
    assert_eq!(d["sections"][0]["layout"], "text-only");

    // ---- runtime library changes ----
    core.set_libraries(vec![
        Library {
            id: "main".into(),
            name: "Main".into(),
            root: lib1.clone(),
            writable: false,
        },
        Library {
            id: "two".into(),
            name: "Two".into(),
            root: lib2.clone(),
            writable: true,
        },
    ]);
    ctx.wait_total("two", 1).await;
    assert_eq!(core.libraries().len(), 2);
    // the watcher picks up new files in the new library
    write_png(&lib2.join("new/d.png"), 10, 10);
    ctx.wait_total("two", 2).await;
    core.set_libraries(vec![Library {
        id: "two".into(),
        name: "Two".into(),
        root: lib2.clone(),
        writable: true,
    }]);
    ctx.wait_total("main", 0).await;
    let tree = ctx.json("/api/libraries/two/tree").await;
    assert_eq!(tree["total"], 2);

    core.shutdown().await;
    let _ = tokio::time::timeout(Duration::from_secs(5), task).await;
}

#[test]
fn env_fallback() {
    // SAFETY-free in edition 2021; only this test touches these names.
    std::env::set_var("LETOPIS_ZZ_TEST_KEY", " old ");
    assert_eq!(
        tessera_core::config::env("ZZ_TEST_KEY").as_deref(),
        Some("old")
    );
    std::env::set_var("TESSERA_ZZ_TEST_KEY", "new");
    assert_eq!(
        tessera_core::config::env("ZZ_TEST_KEY").as_deref(),
        Some("new")
    );
}
