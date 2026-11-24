//! App info, file-manager integration, update checks and cache management.

use std::fs;
use std::path::Path;

use serde::Serialize;
use tessera_imaging::FormatCapabilities;

use super::{err, paths, CmdResult, Mode, Tools};

const REPO: &str = "MrNechev/tessera";
pub const CHANNEL: &str = "alpha";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub channel: String,
    pub platform: String,
    pub install_kind: String,
    pub heif: bool,
    pub formats: Vec<FormatCapabilities>,
    pub input_extensions: Vec<String>,
    pub data_dir: String,
    pub cache_dir: String,
    /// "desktop" or "server".
    pub mode: Mode,
    /// Server mode: where results are saved by default.
    pub outputs_dir: String,
    pub has_inbox: bool,
    /// Build label of images that follow main ("edge-<sha>"); None for releases.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build: Option<String>,
}

/// How this copy of Tessera was installed (decides how updates are offered).
pub fn install_kind() -> &'static str {
    if std::env::var_os("TESSERA_DOCKER").is_some() {
        "docker"
    } else if std::env::var_os("APPIMAGE").is_some() {
        "appimage"
    } else if Path::new("/.flatpak-info").exists() {
        "flatpak"
    } else if cfg!(debug_assertions) {
        "dev"
    } else if Path::new("/var/lib/dpkg/info/tessera.list").exists() {
        "deb"
    } else if std::process::Command::new("rpm")
        .args(["-q", "tessera"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        "rpm"
    } else {
        "unknown"
    }
}

pub fn app_info(t: &Tools) -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION").into(),
        channel: CHANNEL.into(),
        platform: std::env::consts::OS.into(),
        // May run `rpm -q`.
        install_kind: install_kind().into(),
        heif: cfg!(feature = "heif"),
        formats: tessera_imaging::capabilities(),
        input_extensions: tessera_imaging::input_extensions()
            .into_iter()
            .map(String::from)
            .collect(),
        data_dir: t.data_dir().to_string_lossy().into_owned(),
        cache_dir: t.cache_dir().to_string_lossy().into_owned(),
        mode: t.cfg.mode,
        outputs_dir: t.cfg.outputs_dir.to_string_lossy().into_owned(),
        has_inbox: t.cfg.inbox_dir.is_some(),
        build: std::env::var("TESSERA_BUILD")
            .ok()
            .filter(|b| !b.is_empty()),
    }
}

pub fn ensure_dir(t: &Tools, path: String) -> CmdResult<String> {
    let p = t.guard.writable(&paths::expand(&path))?;
    fs::create_dir_all(&p).map_err(|e| format!("{}: {e}", p.display()))?;
    Ok(p.to_string_lossy().into_owned())
}

pub fn cache_size(t: &Tools) -> u64 {
    [
        t.proxies(),
        t.thumbs(),
        t.previews(),
        t.cache_dir().join("watermarks"),
    ]
    .iter()
    .map(|d| paths::dir_size(d))
    .sum()
}

pub fn clear_cache(t: &Tools) -> u64 {
    let mut freed = 0;
    for dir in [t.proxies(), t.thumbs(), t.previews()] {
        freed += paths::dir_size(&dir);
        let _ = fs::remove_dir_all(&dir);
    }
    freed
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current: String,
    pub latest: Option<String>,
    pub has_update: bool,
    pub url: Option<String>,
    pub notes: Option<String>,
    pub published_at: Option<String>,
    pub install_kind: String,
}

fn parse_version(tag: &str) -> Option<semver::Version> {
    semver::Version::parse(tag.trim_start_matches('v')).ok()
}

/// Ask GitHub for the newest published release. Only the release list is
/// requested; nothing about the user or the machine is sent.
pub fn check_updates() -> CmdResult<UpdateInfo> {
    {
        let current = env!("CARGO_PKG_VERSION");
        let url = format!("https://api.github.com/repos/{REPO}/releases?per_page=20");
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(std::time::Duration::from_secs(15)))
            .build()
            .into();
        let mut resp = agent
            .get(&url)
            .header("User-Agent", &format!("Tessera/{current}"))
            .header("Accept", "application/vnd.github+json")
            .call()
            .map_err(|e| format!("Could not reach GitHub: {e}"))?;
        let releases: serde_json::Value = resp.body_mut().read_json().map_err(err)?;
        let cur = parse_version(current).ok_or("bad current version")?;
        let best = releases
            .as_array()
            .into_iter()
            .flatten()
            .filter(|r| !r.get("draft").and_then(|d| d.as_bool()).unwrap_or(false))
            .filter_map(|r| {
                let tag = r.get("tag_name")?.as_str()?;
                Some((parse_version(tag)?, r))
            })
            .max_by(|a, b| a.0.cmp(&b.0));
        let (latest, url, notes, published) = match best {
            Some((v, r)) => (
                Some(v.to_string()),
                r.get("html_url").and_then(|u| u.as_str()).map(String::from),
                r.get("body")
                    .and_then(|u| u.as_str())
                    .map(|s| s.chars().take(2000).collect()),
                r.get("published_at")
                    .and_then(|u| u.as_str())
                    .map(String::from),
            ),
            None => (None, None, None, None),
        };
        let has_update = latest
            .as_deref()
            .and_then(parse_version)
            .map(|v| v > cur)
            .unwrap_or(false);
        Ok(UpdateInfo {
            current: current.into(),
            latest,
            has_update,
            url,
            notes,
            published_at: published,
            install_kind: install_kind().into(),
        })
    }
}
