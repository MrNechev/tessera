//! Listeners: plain HTTP and HTTPS (HTTP/2 via ALPN) with graceful shutdown, the desktop
//! app's `127.0.0.1:0` listener, and the certificate resolver (private CA, re-checked daily).

use std::future::Future;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Context;
use axum::extract::Request;
use axum::Router;
use hyper::body::Incoming;
use hyper_util::rt::{TokioExecutor, TokioIo, TokioTimer};
use hyper_util::server::conn::auto::Builder;
use hyper_util::server::graceful::{GracefulShutdown, Watcher};
use hyper_util::service::TowerToHyperService;
use rustls::server::{ClientHello, ResolvesServerCert};
use rustls::sign::CertifiedKey;
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tower::ServiceExt;

use super::http::ConnInfo;
use crate::engine::Core;
use crate::tls;

/// The serving certificate (cached for a day) and the CA file for `/ca.crt`.
#[derive(Default)]
pub struct TlsState {
    cert: Mutex<Option<(Arc<CertifiedKey>, Instant)>>,
    ca_cert: Mutex<Option<PathBuf>>,
}

impl std::fmt::Debug for TlsState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TlsState")
    }
}

impl TlsState {
    /// The private CA certificate, once HTTPS uses the generated certificate.
    pub fn ca_cert(&self) -> Option<PathBuf> {
        self.ca_cert
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// The current certificate; re-checks (and renews) it every 24 hours.
    fn get(&self, cfg: &crate::config::Config) -> anyhow::Result<Arc<CertifiedKey>> {
        let mut g = self.cert.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((c, at)) = g.as_ref() {
            if at.elapsed() < tls::RECHECK {
                return Ok(c.clone());
            }
        }
        let loaded = match (&cfg.tls_cert, &cfg.tls_key) {
            (Some(c), Some(k)) => tls::load_pair(c, k),
            _ => tls::ensure(&cfg.data_dir.join("tls"), &cfg.tls_hosts).map(|(ck, files)| {
                *self.ca_cert.lock().unwrap_or_else(|e| e.into_inner()) = Some(files.ca_cert);
                ck
            }),
        };
        match loaded.and_then(|ck| {
            let key = rustls::crypto::ring::sign::any_supported_type(&ck.key)
                .map_err(|e| anyhow::anyhow!("TLS key: {e}"))?;
            Ok(Arc::new(CertifiedKey::new(ck.chain, key)))
        }) {
            Ok(c) => {
                *g = Some((c.clone(), Instant::now()));
                Ok(c)
            }
            Err(e) => match g.as_ref() {
                Some((c, _)) => {
                    tracing::warn!(err = %e, "TLS certificate renewal failed; keeping the old one");
                    Ok(c.clone())
                }
                None => Err(e),
            },
        }
    }
}

#[derive(Debug)]
struct Resolver(Arc<Core>);

impl ResolvesServerCert for Resolver {
    fn resolve(&self, _: ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
        self.0.tls.get(&self.0.cfg).ok()
    }
}

impl std::fmt::Debug for Core {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Core")
    }
}

/// Go-style listen address (":8470" = all interfaces).
async fn bind(addr: &str) -> anyhow::Result<TcpListener> {
    if let Some(port) = addr.strip_prefix(':') {
        if let Ok(l) = TcpListener::bind(format!("[::]:{port}")).await {
            return Ok(l);
        }
        return TcpListener::bind(format!("0.0.0.0:{port}"))
            .await
            .with_context(|| format!("listen {addr}"));
    }
    TcpListener::bind(addr)
        .await
        .with_context(|| format!("listen {addr}"))
}

fn builder() -> Builder<TokioExecutor> {
    let mut b = Builder::new(TokioExecutor::new());
    b.http1()
        .timer(TokioTimer::new())
        .header_read_timeout(Duration::from_secs(15))
        .max_buf_size(1 << 20);
    b.http2().timer(TokioTimer::new()).enable_connect_protocol();
    b
}

async fn handle_conn(
    stream: TcpStream,
    remote: SocketAddr,
    tls: Option<tokio_rustls::TlsAcceptor>,
    router: Router,
    watcher: Watcher,
) {
    let _ = stream.set_nodelay(true);
    let is_tls = tls.is_some();
    let svc = router.map_request(move |mut req: Request<Incoming>| {
        req.extensions_mut().insert(ConnInfo {
            remote,
            tls: is_tls,
        });
        req
    });
    let svc = TowerToHyperService::new(svc);
    let b = builder();
    match tls {
        Some(acc) => {
            let Ok(Ok(s)) = tokio::time::timeout(Duration::from_secs(15), acc.accept(stream)).await
            else {
                return;
            };
            let conn = b.serve_connection_with_upgrades(TokioIo::new(s), svc);
            let _ = watcher.watch(conn.into_owned()).await;
        }
        None => {
            let conn = b.serve_connection_with_upgrades(TokioIo::new(stream), svc);
            let _ = watcher.watch(conn.into_owned()).await;
        }
    }
}

async fn accept_loop(
    listener: TcpListener,
    tls: Option<tokio_rustls::TlsAcceptor>,
    router: Router,
    graceful: Arc<GracefulShutdown>,
    stop: CancellationToken,
) {
    loop {
        tokio::select! {
            _ = stop.cancelled() => return,
            acc = listener.accept() => {
                let Ok((stream, remote)) = acc else {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    continue;
                };
                tokio::spawn(handle_conn(stream, remote, tls.clone(), router.clone(), graceful.watcher()));
            }
        }
    }
}

/// Binds the configured listeners and serves until `shutdown` completes (then drains
/// connections for up to 10 seconds).
pub async fn serve(
    core: Arc<Core>,
    router: Router,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> anyhow::Result<()> {
    let graceful = Arc::new(GracefulShutdown::new());
    let stop = CancellationToken::new();
    let mut tasks = Vec::new();
    if let Some(addr) = core.cfg.https_addr.clone() {
        {
            let c = core.clone();
            tokio::task::spawn_blocking(move || c.tls.get(&c.cfg).map(|_| ())).await??;
        }
        let mut sc = rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()?
        .with_no_client_auth()
        .with_cert_resolver(Arc::new(Resolver(core.clone())));
        sc.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
        let acc = tokio_rustls::TlsAcceptor::from(Arc::new(sc));
        let l = bind(&addr).await?;
        if let Ok(a) = l.local_addr() {
            core.add_local_port(a.port());
        }
        tracing::info!(addr = %addr, "listening (HTTPS, HTTP/2)");
        tasks.push(tokio::spawn(accept_loop(
            l,
            Some(acc),
            router.clone(),
            graceful.clone(),
            stop.clone(),
        )));
    }
    if let Some(addr) = core.cfg.http_addr.clone() {
        let l = bind(&addr).await?;
        if let Ok(a) = l.local_addr() {
            core.add_local_port(a.port());
        }
        tracing::info!(addr = %addr, "listening (HTTP)");
        tasks.push(tokio::spawn(accept_loop(
            l,
            None,
            router.clone(),
            graceful.clone(),
            stop.clone(),
        )));
    }
    if tasks.is_empty() {
        anyhow::bail!("no listen address configured");
    }
    let core_cancel = core.cancellation();
    tokio::select! {
        _ = shutdown => {}
        _ = core_cancel.cancelled() => {}
    }
    stop.cancel();
    for t in tasks {
        let _ = t.await;
    }
    if let Ok(g) = Arc::try_unwrap(graceful) {
        let _ = tokio::time::timeout(Duration::from_secs(10), g.shutdown()).await;
    }
    Ok(())
}

/// Serves `router` on `127.0.0.1:0` (desktop app). The task stops when the core shuts down.
pub async fn bind_local(
    core: Arc<Core>,
    router: Router,
    port: u16,
) -> anyhow::Result<(SocketAddr, JoinHandle<()>)> {
    let l = TcpListener::bind(("127.0.0.1", port)).await?;
    let addr = l.local_addr()?;
    core.add_local_port(addr.port());
    tracing::info!(addr = %addr, "listening (HTTP, local)");
    let stop = core.cancellation();
    let h = tokio::spawn(async move {
        let graceful = Arc::new(GracefulShutdown::new());
        accept_loop(l, None, router, graceful.clone(), stop).await;
        if let Ok(g) = Arc::try_unwrap(graceful) {
            let _ = tokio::time::timeout(Duration::from_secs(5), g.shutdown()).await;
        }
    });
    Ok((addr, h))
}
