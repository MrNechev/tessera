//! `tessera-server`: Tessera as a self-hosted web app (the Docker image's entry point).
//!
//! Subcommands: `version` prints the version; `health` is the container healthcheck
//! (GET /healthz on the local HTTP or HTTPS port, exit 0 when it answers 200).

use std::sync::Arc;
use std::time::Duration;

use tessera_core::config::env_raw;
use tessera_core::tools::{self, Mode, Tools, ToolsConfig, INBOX_ID, OUTPUTS_ID};
use tessera_core::{Config, Core, Library};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Set at build time with `TESSERA_VERSION=x.y.z cargo build`.
const VERSION: &str = match option_env!("TESSERA_VERSION") {
    Some(v) => v,
    None => env!("CARGO_PKG_VERSION"),
};

fn main() {
    let arg = std::env::args().nth(1);
    match arg.as_deref() {
        Some("version" | "--version") => {
            println!("{VERSION}");
            return;
        }
        Some("health") => {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("runtime");
            std::process::exit(rt.block_on(health()));
        }
        _ => {}
    }
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("runtime");
    if let Err(e) = rt.block_on(run()) {
        tracing::error!(err = %format!("{e:#}"), "fatal");
        eprintln!("fatal: {e:#}");
        std::process::exit(1);
    }
}

async fn run() -> anyhow::Result<()> {
    let mut cfg = Config::from_env()?;
    cfg.version = VERSION.to_string();
    let level = match cfg.log_level.as_str() {
        "debug" => "debug",
        "warn" => "warn",
        "error" => "error",
        _ => "info",
    };
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("TESSERA_LOG")
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(level)),
        )
        .with_target(false)
        // `docker logs` shows raw escape codes: colours only on a terminal.
        .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stdout()))
        .init();
    // Outputs and Inbox are libraries too (writable): results and kept uploads
    // show up in the Library and can be used in documents.
    let outputs = env_raw("OUTPUTS_DIR").unwrap_or_else(|| "/outputs".into());
    let inbox = env_raw("INBOX_DIR").unwrap_or_else(|| "/inbox".into());
    let mut inbox_dir = None;
    for (id, name, dir) in [
        (OUTPUTS_ID, "Outputs", &outputs),
        (INBOX_ID, "Inbox", &inbox),
    ] {
        let root = std::path::PathBuf::from(dir);
        if cfg.libraries.iter().any(|l| l.id == id) {
            tracing::warn!(
                id,
                "a library already uses this id; not adding the built-in folder"
            );
            continue;
        }
        if std::fs::create_dir_all(&root).is_err() || !root.is_dir() {
            tracing::warn!(dir = %root.display(), "{name} folder unavailable");
            continue;
        }
        if id == INBOX_ID {
            inbox_dir = Some(root.clone());
        }
        cfg.libraries.push(Library {
            id: id.into(),
            name: name.into(),
            root,
            writable: true,
        });
    }
    let max_upload = env_raw("MAX_UPLOAD_MB")
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|&mb| mb > 0)
        .map(|mb| mb * 1024 * 1024);
    let tools = Arc::new(Tools::new(ToolsConfig {
        mode: Mode::Server,
        data_dir: cfg.data_dir.clone(),
        config_dir: cfg.data_dir.clone(),
        cache_dir: cfg.cache_dir.join("tools"),
        outputs_dir: std::path::PathBuf::from(&outputs),
        uploads_dir: cfg.cache_dir.join("uploads"),
        inbox_dir,
        max_upload,
    }));
    let core: Arc<Core> = Core::start(cfg).await?;
    tools.guard.set_areas(tools::areas_for(&core.libraries()));
    tools.attach_media(core.media.clone());
    tools::api::spawn_cleanup(tools.clone());
    let router = core.router_with(tools::api::routes(tools));
    let res = core.serve(router, shutdown_signal()).await;
    tracing::info!("shutting down");
    core.shutdown().await;
    res
}

async fn shutdown_signal() {
    let term = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut s) => {
                s.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = term => {}
    }
}

/// The container healthcheck.
async fn health() -> i32 {
    let http = env_raw("HTTP_ADDR").unwrap_or_else(|| ":8470".into());
    let https = env_raw("HTTPS_ADDR").unwrap_or_else(|| ":8471".into());
    for (tls, addr) in [(false, http), (true, https)] {
        if addr.is_empty() || addr.eq_ignore_ascii_case("off") {
            continue;
        }
        let port = addr.rsplit(':').next().unwrap_or("").to_string();
        let ok = tokio::time::timeout(Duration::from_secs(4), probe(tls, &port))
            .await
            .unwrap_or(false);
        if ok {
            return 0;
        }
    }
    1
}

async fn probe(tls: bool, port: &str) -> bool {
    let Ok(tcp) = tokio::net::TcpStream::connect(format!("127.0.0.1:{port}")).await else {
        return false;
    };
    let req = b"GET /healthz HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
    let mut buf = Vec::new();
    if tls {
        let cfg = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .map(|b| {
            b.dangerous()
                .with_custom_certificate_verifier(Arc::new(NoVerify))
                .with_no_client_auth()
        });
        let Ok(cfg) = cfg else { return false };
        let conn = tokio_rustls::TlsConnector::from(Arc::new(cfg));
        let Ok(name) = rustls::pki_types::ServerName::try_from("localhost") else {
            return false;
        };
        let Ok(mut s) = conn.connect(name, tcp).await else {
            return false;
        };
        if s.write_all(req).await.is_err() {
            return false;
        }
        let _ = s.read_to_end(&mut buf).await;
    } else {
        let mut s = tcp;
        if s.write_all(req).await.is_err() {
            return false;
        }
        let _ = s.read_to_end(&mut buf).await;
    }
    buf.starts_with(b"HTTP/1.1 200") || buf.starts_with(b"HTTP/1.0 200")
}

/// Accepts any certificate (healthcheck against our own self-signed listener).
#[derive(Debug)]
struct NoVerify;

impl rustls::client::danger::ServerCertVerifier for NoVerify {
    fn verify_server_cert(
        &self,
        _: &rustls::pki_types::CertificateDer<'_>,
        _: &[rustls::pki_types::CertificateDer<'_>],
        _: &rustls::pki_types::ServerName<'_>,
        _: &[u8],
        _: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }
    fn verify_tls12_signature(
        &self,
        _: &[u8],
        _: &rustls::pki_types::CertificateDer<'_>,
        _: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }
    fn verify_tls13_signature(
        &self,
        _: &[u8],
        _: &rustls::pki_types::CertificateDer<'_>,
        _: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }
    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}
