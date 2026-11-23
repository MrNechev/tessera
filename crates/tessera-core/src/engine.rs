//! [`Core`]: owns the index, media manager, library scanner, documents, preferences and
//! authentication, and serves the HTTP API. Used by the server binary and embedded by the
//! desktop app.

use std::future::Future;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime};

use anyhow::Context;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::config::{Config, Library};
use crate::server;
use crate::{auth, docs, library, media, prefs, store};

/// Everything Tessera's engine owns. Create with [`Core::start`].
pub struct Core {
    pub cfg: Arc<Config>,
    pub db: Arc<store::Store>,
    pub media: Arc<media::Manager>,
    pub lib: Arc<library::Manager>,
    pub docs: Arc<docs::Store>,
    pub prefs: Arc<prefs::Store>,
    pub auth: Arc<auth::Auth>,
    pub(crate) remote: server::remote::Hub,
    pub(crate) tls: server::listen::TlsState,
    pub(crate) started: Instant,
    pub(crate) started_at: SystemTime,
    pub(crate) local_ports: Mutex<Vec<u16>>,
    cancel: CancellationToken,
}

/// Renames a Letopis index (`letopis.db` + `-wal`/`-shm`) to `tessera.db`.
fn migrate_db_name(data_dir: &Path) -> anyhow::Result<()> {
    let old = data_dir.join("letopis.db");
    let new = data_dir.join("tessera.db");
    if old.exists() && !new.exists() {
        std::fs::rename(&old, &new)?;
        for suffix in ["-wal", "-shm"] {
            let o = data_dir.join(format!("letopis.db{suffix}"));
            if o.exists() {
                std::fs::rename(&o, data_dir.join(format!("tessera.db{suffix}")))?;
            }
        }
        tracing::info!("renamed letopis.db to tessera.db");
    }
    Ok(())
}

impl Core {
    /// Opens the index, loads documents and preferences, and starts scanning, probing and
    /// watching the libraries. Must run inside a (multi-threaded) Tokio runtime.
    pub async fn start(cfg: Config) -> anyhow::Result<Arc<Core>> {
        let cfg = Arc::new(cfg);
        tracing::info!(
            version = %cfg.version,
            data = %cfg.data_dir.display(),
            cache = %cfg.cache_dir.display(),
            libraries = cfg.libraries.len(),
            "starting Tessera"
        );
        for l in &cfg.libraries {
            tracing::info!(id = %l.id, name = %l.name, root = %l.root.display(), writable = l.writable, "library");
        }
        if cfg.libraries.is_empty() {
            tracing::warn!(
                "no libraries configured: set TESSERA_LIBRARIES or mount folders under /libraries"
            );
        }
        if matches!(cfg.auth, crate::config::AuthMode::None) {
            tracing::warn!("TESSERA_PASSWORD is empty: no login required");
        }
        let rt = tokio::runtime::Handle::current();
        let cancel = CancellationToken::new();
        let c2 = cancel.clone();
        let cfg2 = cfg.clone();
        let parts = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
            let cfg = cfg2;
            for d in [&cfg.data_dir, &cfg.cache_dir] {
                std::fs::create_dir_all(d).with_context(|| format!("create {}", d.display()))?;
            }
            migrate_db_name(&cfg.data_dir)?;
            let db = Arc::new(store::Store::open(&cfg.db_path())?);
            let mm = Arc::new(media::Manager::new(cfg.clone(), rt)?);
            mm.clean_temp();
            let lib = Arc::new(library::Manager::new(
                cfg.clone(),
                db.clone(),
                mm.clone(),
                c2,
            ));
            let ds = Arc::new(docs::Store::new(
                &cfg.data_dir,
                db.clone(),
                cfg.history_keep,
            )?);
            ds.reindex()?;
            let a = Arc::new(auth::Auth::new(
                &cfg.data_dir,
                cfg.auth.clone(),
                &cfg.session_secret,
                cfg.session_max_age,
            )?);
            let ps = Arc::new(prefs::Store::open(&cfg.data_dir).context("prefs")?);
            lib.set_hidden(server::api_extra::hidden_map(&ps.get()));
            Ok((db, mm, lib, ds, a, ps))
        })
        .await??;
        let (db, mm, lib, ds, a, ps) = parts;
        lib.start();
        Ok(Arc::new(Core {
            cfg,
            db,
            media: mm,
            lib,
            docs: ds,
            prefs: ps,
            auth: a,
            remote: server::remote::Hub::default(),
            tls: server::listen::TlsState::default(),
            started: Instant::now(),
            started_at: SystemTime::now(),
            local_ports: Mutex::new(Vec::new()),
            cancel,
        }))
    }

    /// All Tessera routes. `extra` is merged inside the authenticated group (its routes
    /// require the same login/token as the rest of the API).
    pub fn router_with(self: &Arc<Self>, extra: axum::Router) -> axum::Router {
        server::router(self.clone(), extra)
    }

    /// All Tessera routes.
    pub fn router(self: &Arc<Self>) -> axum::Router {
        self.router_with(axum::Router::new())
    }

    /// Binds the HTTP and/or HTTPS listeners from the config and serves `router` until
    /// `shutdown` completes. HTTPS negotiates HTTP/2 via ALPN.
    pub async fn serve(
        self: &Arc<Self>,
        router: axum::Router,
        shutdown: impl Future<Output = ()> + Send + 'static,
    ) -> anyhow::Result<()> {
        server::listen::serve(self.clone(), router, shutdown).await
    }

    /// Desktop: serves `router` over plain HTTP on `127.0.0.1:0`; returns the bound address
    /// and the server task (abort it or call [`Core::shutdown`] to stop).
    pub async fn bind_local(
        self: &Arc<Self>,
        router: axum::Router,
    ) -> anyhow::Result<(SocketAddr, JoinHandle<()>)> {
        server::listen::bind_local(self.clone(), router, 0).await
    }

    /// Like [`Core::bind_local`] on a fixed port (desktop development: Vite proxies to it).
    pub async fn bind_local_port(
        self: &Arc<Self>,
        router: axum::Router,
        port: u16,
    ) -> anyhow::Result<(SocketAddr, JoinHandle<()>)> {
        server::listen::bind_local(self.clone(), router, port).await
    }

    /// The current libraries.
    pub fn libraries(&self) -> Vec<Library> {
        self.lib.libraries()
    }

    /// Replaces the libraries at runtime: new ones are scanned, rows of removed ones are
    /// dropped and the watcher follows.
    pub fn set_libraries(&self, libs: Vec<Library>) {
        self.lib.set_libraries(libs)
    }

    /// The desktop token (Token auth mode).
    pub fn token(&self) -> Option<&str> {
        match &self.cfg.auth {
            crate::config::AuthMode::Token(t) => Some(t),
            _ => None,
        }
    }

    /// A token that is cancelled when the core shuts down.
    pub fn cancellation(&self) -> CancellationToken {
        self.cancel.clone()
    }

    /// Stops background work (scanner, watcher, prober, worker pools).
    pub async fn shutdown(&self) {
        self.cancel.cancel();
        self.lib.stop();
        let mm = self.media.clone();
        let _ = tokio::task::spawn_blocking(move || mm.close()).await;
    }

    pub(crate) fn add_local_port(&self, p: u16) {
        self.local_ports
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(p);
    }
}
