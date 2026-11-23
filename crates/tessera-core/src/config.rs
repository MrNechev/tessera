//! Settings: read from environment variables (server) or built in code (desktop app).
//!
//! Every setting has a default derived from the machine where that makes sense (CPU count,
//! etc.). Environment variables are read as `TESSERA_<X>` first and fall back to the old
//! `LETOPIS_<X>` name, so an existing Letopis `.env` keeps working.

use std::borrow::Cow;
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{bail, Context};

use crate::util::{num_cpu, parse_go_duration};

/// One media root. Libraries are read-only unless `writable` is set (server: listed in
/// `TESSERA_WRITABLE_LIBRARIES`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Library {
    pub id: String,
    pub name: String,
    /// Absolute path on disk.
    pub root: PathBuf,
    pub writable: bool,
}

/// How clients authenticate.
#[derive(Clone, PartialEq, Eq)]
pub enum AuthMode {
    /// No login (anyone who can reach the server has access).
    None,
    /// Single-password login with a signed `tessera_session` cookie.
    Password(String),
    /// Desktop app: a random token in the `tessera_token` cookie or `X-Tessera-Token`
    /// header. `GET /<page>?token=<t>` sets the cookie. Only `127.0.0.1`/`localhost` Host
    /// headers are accepted (blocks DNS rebinding).
    Token(String),
}

impl fmt::Debug for AuthMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuthMode::None => f.write_str("None"),
            AuthMode::Password(_) => f.write_str("Password(***)"),
            AuthMode::Token(_) => f.write_str("Token(***)"),
        }
    }
}

/// Lookup function for embedded web assets: path relative to the SPA root (e.g.
/// `index.html`, `assets/app-1234.js`) → file contents.
pub type AssetLookup = fn(&str) -> Option<Cow<'static, [u8]>>;

/// Where the static web UI (the built Svelte SPA) comes from.
#[derive(Clone)]
pub enum WebAssets {
    /// A folder with the built SPA (`index.html`, `assets/…`).
    Dir(PathBuf),
    /// Files compiled into the binary.
    Embedded(AssetLookup),
}

impl fmt::Debug for WebAssets {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WebAssets::Dir(p) => f.debug_tuple("Dir").field(p).finish(),
            WebAssets::Embedded(_) => f.write_str("Embedded(..)"),
        }
    }
}

impl WebAssets {
    /// Reads one file (relative path, no leading slash). `None` if missing or a directory.
    pub fn read(&self, rel: &str) -> Option<Cow<'static, [u8]>> {
        match self {
            WebAssets::Dir(dir) => {
                if rel.split('/').any(|s| s == "..") {
                    return None;
                }
                let p = dir.join(rel);
                let md = std::fs::metadata(&p).ok()?;
                if md.is_dir() {
                    return None;
                }
                std::fs::read(&p).ok().map(Cow::Owned)
            }
            WebAssets::Embedded(f) => f(rel),
        }
    }

    /// Whether `rel` exists as a directory (Dir assets only).
    pub fn is_dir(&self, rel: &str) -> bool {
        match self {
            WebAssets::Dir(dir) => dir.join(rel).is_dir(),
            WebAssets::Embedded(_) => false,
        }
    }
}

/// Everything the core needs to run.
#[derive(Debug, Clone)]
pub struct Config {
    /// Documents, index DB, secret, TLS.
    pub data_dir: PathBuf,
    /// Thumbnails, GIF videos, proxies (fast storage).
    pub cache_dir: PathBuf,

    /// Plain HTTP listen address (`None` = off), e.g. ":8470" or "127.0.0.1:0".
    pub http_addr: Option<String>,
    /// HTTPS listen address (`None` = off).
    pub https_addr: Option<String>,
    /// Optional user-provided certificate; otherwise a private CA in `<data>/tls`.
    pub tls_cert: Option<PathBuf>,
    pub tls_key: Option<PathBuf>,
    /// SANs for the generated server certificate.
    pub tls_hosts: Vec<String>,

    pub auth: AuthMode,
    /// Optional; generated and stored in `<data>/session.secret` when empty.
    pub session_secret: String,
    pub session_max_age: Duration,

    pub libraries: Vec<Library>,

    /// Periodic full rescan (zero = off).
    pub scan_interval: Duration,
    /// Filesystem watcher.
    pub watch: bool,
    pub watch_max_dirs: usize,
    pub scan_workers: usize,
    pub thumb_workers: usize,
    pub probe_workers: usize,
    /// GIF conversions + proxies.
    pub video_workers: usize,
    /// Generate grid thumbnails for the whole library after scans.
    pub background_gen: bool,
    /// Name or path patterns (Go `path.Match` syntax) never indexed, e.g. "*_files",
    /// "Private/*", "*.tmp". Matched against each folder/file name and its library path.
    pub exclude: Vec<String>,
    /// Run background jobs under `nice -n 10`.
    pub nice: bool,

    /// auto | vaapi | none
    pub hw_accel: String,
    pub vaapi_device: String,
    /// Codecs the client browser can decode.
    pub playable_vcodecs: Vec<String>,
    pub playable_acodecs: Vec<String>,
    /// In-document preview proxy height (1080).
    pub preview_height: i64,
    /// Videos taller than this get a preview proxy.
    pub preview_min_height: i64,
    /// … or with a bitrate above this (bits/s).
    pub preview_min_bitrate: i64,
    /// Queue proxies for videos used in documents on save.
    pub proxy_doc_videos: bool,

    /// Previous versions kept per document.
    pub history_keep: usize,
    /// Soft limit for the derived cache (0 = unlimited; informational).
    pub max_cache_gb: f64,
    pub log_level: String,

    /// The web UI.
    pub web: WebAssets,
    /// Version string reported by the API.
    pub version: String,
}

// ---------- environment ----------

/// Raw value of `TESSERA_<key>` or, if unset, `LETOPIS_<key>`.
pub fn env_raw(key: &str) -> Option<String> {
    std::env::var(format!("TESSERA_{key}"))
        .ok()
        .or_else(|| std::env::var(format!("LETOPIS_{key}")).ok())
}

/// Trimmed non-empty value of `TESSERA_<key>`, else `LETOPIS_<key>`.
pub fn env(key: &str) -> Option<String> {
    for prefix in ["TESSERA_", "LETOPIS_"] {
        if let Ok(v) = std::env::var(format!("{prefix}{key}")) {
            let t = v.trim();
            if !t.is_empty() {
                return Some(t.to_string());
            }
        }
    }
    None
}

fn env_or(key: &str, def: &str) -> String {
    env(key).unwrap_or_else(|| def.to_string())
}

fn env_int(key: &str, def: i64) -> i64 {
    env(key).and_then(|v| v.parse().ok()).unwrap_or(def)
}

fn env_float(key: &str, def: f64) -> f64 {
    env(key).and_then(|v| v.parse().ok()).unwrap_or(def)
}

fn env_bool(key: &str, def: bool) -> bool {
    match env(key).map(|v| v.to_lowercase()).as_deref() {
        Some("1" | "true" | "yes" | "on") => true,
        Some("0" | "false" | "no" | "off") => false,
        _ => def,
    }
}

fn env_duration(key: &str, def: Duration) -> Duration {
    let Some(v) = env(key) else { return def };
    if let Some(d) = parse_go_duration(&v) {
        return d;
    }
    // plain number = minutes
    if let Ok(n) = v.parse::<i64>() {
        return Duration::from_secs(n.max(0) as u64 * 60);
    }
    def
}

fn env_list(key: &str, def: &str) -> Vec<String> {
    env_or(key, def)
        .split(',')
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .map(|p| p.to_lowercase())
        .collect()
}

/// Splits "a, b; c" into ["a", "b", "c"].
fn split_list(v: &str) -> Vec<String> {
    v.split([',', ';'])
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .map(String::from)
        .collect()
}

fn workers(key: &str, def: usize) -> usize {
    env_int(key, def as i64).max(1) as usize
}

fn addr(key: &str, def: &str) -> Option<String> {
    let v = env_or(key, def);
    if v.is_empty() || v.eq_ignore_ascii_case("off") {
        None
    } else {
        Some(v)
    }
}

impl Config {
    /// Reads the configuration from `TESSERA_*` (falling back to `LETOPIS_*`) variables.
    pub fn from_env() -> anyhow::Result<Config> {
        let cpus = num_cpu();
        let data_dir = PathBuf::from(env_or("DATA_DIR", "/data"));
        let cache_dir = env("CACHE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| data_dir.join("cache"));
        let password = env_raw("PASSWORD").unwrap_or_default();
        let http_addr = addr("HTTP_ADDR", ":8470");
        let https_addr = addr("HTTPS_ADDR", ":8471");
        if http_addr.is_none() && https_addr.is_none() {
            bail!("both TESSERA_HTTP_ADDR and TESSERA_HTTPS_ADDR are off");
        }
        let writable: Vec<String> = split_list(&env_or("WRITABLE_LIBRARIES", ""));
        let mut libraries = load_libraries()?;
        for l in &mut libraries {
            l.writable = writable.iter().any(|w| w == &l.id);
        }
        Ok(Config {
            data_dir,
            cache_dir,
            http_addr,
            https_addr,
            tls_cert: env("TLS_CERT").map(PathBuf::from),
            tls_key: env("TLS_KEY").map(PathBuf::from),
            tls_hosts: env_list("TLS_HOSTS", "localhost,127.0.0.1"),
            auth: if password.is_empty() {
                AuthMode::None
            } else {
                AuthMode::Password(password)
            },
            session_secret: env_or("SESSION_SECRET", ""),
            session_max_age: env_duration(
                "SESSION_MAX_AGE",
                Duration::from_secs(365 * 24 * 3600),
            ),
            libraries,
            scan_interval: env_duration("SCAN_INTERVAL", Duration::from_secs(30 * 60)),
            watch: env_bool("WATCH", true),
            watch_max_dirs: env_int("WATCH_MAX_DIRS", 50000).max(0) as usize,
            scan_workers: workers("SCAN_WORKERS", cpus),
            thumb_workers: workers("THUMB_WORKERS", cpus),
            probe_workers: workers("PROBE_WORKERS", cpus),
            video_workers: workers("VIDEO_WORKERS", 2.max(cpus / 4)),
            background_gen: env_bool("BACKGROUND_THUMBS", true),
            exclude: split_list(&env_or("EXCLUDE", "")),
            nice: env_bool("NICE", true),
            hw_accel: env_or("HWACCEL", "auto").to_lowercase(),
            vaapi_device: env_or("VAAPI_DEVICE", "/dev/dri/renderD128"),
            playable_vcodecs: env_list("PLAYABLE_VIDEO_CODECS", "h264,vp8,vp9,av1"),
            playable_acodecs: env_list(
                "PLAYABLE_AUDIO_CODECS",
                "aac,mp3,opus,vorbis,flac,pcm_s16le,pcm_f32le",
            ),
            preview_height: env_int("PREVIEW_HEIGHT", 1080),
            preview_min_height: env_int("PREVIEW_MIN_HEIGHT", 1440),
            preview_min_bitrate: (env_float("PREVIEW_MIN_MBPS", 20.0) * 1e6) as i64,
            proxy_doc_videos: env_bool("PROXY_DOCUMENT_VIDEOS", true),
            history_keep: env_int("HISTORY_KEEP", 30).max(0) as usize,
            max_cache_gb: env_float("MAX_CACHE_GB", 0.0),
            log_level: env_or("LOG_LEVEL", "info").to_lowercase(),
            web: WebAssets::Dir(PathBuf::from(env_or("WEB_DIR", "/usr/share/tessera/web"))),
            version: env!("CARGO_PKG_VERSION").to_string(),
        })
    }

    /// Defaults for the desktop app: listens on `127.0.0.1:0` (HTTP only), token auth with a
    /// fresh random token, background work on about half the cores (under `nice`).
    pub fn desktop(
        data_dir: impl Into<PathBuf>,
        cache_dir: impl Into<PathBuf>,
        libraries: Vec<Library>,
    ) -> Config {
        let cpus = num_cpu();
        let half = (cpus / 2).max(1);
        let token = {
            use rand::RngCore;
            let mut b = [0u8; 32];
            rand::rng().fill_bytes(&mut b);
            hex::encode(b)
        };
        Config {
            data_dir: data_dir.into(),
            cache_dir: cache_dir.into(),
            http_addr: Some("127.0.0.1:0".into()),
            https_addr: None,
            tls_cert: None,
            tls_key: None,
            tls_hosts: vec!["localhost".into(), "127.0.0.1".into()],
            auth: AuthMode::Token(token),
            session_secret: String::new(),
            session_max_age: Duration::from_secs(365 * 24 * 3600),
            libraries,
            scan_interval: Duration::from_secs(30 * 60),
            watch: true,
            watch_max_dirs: 50000,
            scan_workers: half,
            thumb_workers: half,
            probe_workers: half,
            video_workers: (cpus / 8).max(1),
            background_gen: true,
            exclude: Vec::new(),
            nice: true,
            hw_accel: "auto".into(),
            vaapi_device: "/dev/dri/renderD128".into(),
            playable_vcodecs: ["h264", "vp8", "vp9", "av1"].map(String::from).to_vec(),
            playable_acodecs: [
                "aac",
                "mp3",
                "opus",
                "vorbis",
                "flac",
                "pcm_s16le",
                "pcm_f32le",
            ]
            .map(String::from)
            .to_vec(),
            preview_height: 1080,
            preview_min_height: 1440,
            preview_min_bitrate: 20_000_000,
            proxy_doc_videos: true,
            history_keep: 30,
            max_cache_gb: 0.0,
            log_level: "info".into(),
            web: WebAssets::Dir(PathBuf::from("/usr/share/tessera/web")),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    /// The configured password (Password mode), else "".
    pub fn password(&self) -> &str {
        match &self.auth {
            AuthMode::Password(p) => p,
            _ => "",
        }
    }

    /// Path of the index database.
    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("tessera.db")
    }
}

/// Validates a library id (letters, digits, `-` and `_`, at most 64 characters).
pub fn valid_library_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.chars()
            .all(|r| r.is_ascii_alphanumeric() || r == '-' || r == '_')
}

/// Lexically absolute path (Go `filepath.Abs`: joins with the cwd and cleans, no symlink
/// resolution).
pub fn abs_path(p: &Path) -> anyhow::Result<PathBuf> {
    let joined = if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir()?.join(p)
    };
    let s = joined
        .to_str()
        .context("library path is not valid UTF-8")?
        .to_string();
    Ok(PathBuf::from(crate::util::path_clean(&s)))
}

/// Reads `TESSERA_LIBRARIES` ("id:Name:/path;id2:Name 2:/path2") and/or discovers every
/// non-hidden subdirectory of `TESSERA_LIBRARIES_ROOT` (default /libraries) as a library
/// whose id is the directory name. Display names for discovered libraries come from
/// `TESSERA_LIBRARY_NAMES` ("id=Name,id2=Name 2").
pub fn load_libraries() -> anyhow::Result<Vec<Library>> {
    let mut libs: Vec<Library> = Vec::new();
    let add = |id: &str, name: &str, root: &Path, libs: &mut Vec<Library>| {
        if !valid_library_id(id) {
            bail!("invalid library id {id:?} (use letters, digits, - and _)");
        }
        if libs.iter().any(|l| l.id == id) {
            bail!("duplicate library id {id:?}");
        }
        libs.push(Library {
            id: id.to_string(),
            name: name.to_string(),
            root: abs_path(root)?,
            writable: false,
        });
        Ok(())
    };

    let mut names = std::collections::HashMap::new();
    for p in env_or("LIBRARY_NAMES", "").split(',') {
        if let Some((k, v)) = p.split_once('=') {
            names.insert(k.trim().to_string(), v.trim().to_string());
        }
    }

    for spec in env_or("LIBRARIES", "").split(';') {
        let spec = spec.trim();
        if spec.is_empty() {
            continue;
        }
        let parts: Vec<&str> = spec.splitn(3, ':').collect();
        if parts.len() != 3 {
            bail!("TESSERA_LIBRARIES entry {spec:?} must be id:Name:/path");
        }
        add(parts[0], parts[1], Path::new(parts[2]), &mut libs)?;
    }

    let root = PathBuf::from(env_or("LIBRARIES_ROOT", "/libraries"));
    if let Ok(rd) = std::fs::read_dir(&root) {
        let mut entries: Vec<String> = rd
            .flatten()
            .filter_map(|e| e.file_name().to_str().map(String::from))
            .collect();
        entries.sort();
        for name in entries {
            if name.starts_with('.') || libs.iter().any(|l| l.id == name) {
                continue;
            }
            let full = root.join(&name);
            if !std::fs::metadata(&full).map(|m| m.is_dir()).unwrap_or(false) {
                continue;
            }
            let display = names
                .get(&name)
                .filter(|n| !n.is_empty())
                .cloned()
                .unwrap_or_else(|| name.clone());
            add(&name, &display, &full, &mut libs)?;
        }
    }
    Ok(libs)
}
