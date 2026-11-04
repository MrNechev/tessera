//! App info, file-manager integration, update checks and cache management.

use std::fs;
use std::path::Path;

use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;
use tessera_imaging::FormatCapabilities;

use crate::{err, paths, CmdResult};

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
}

/// How this copy of Tessera was installed (decides how updates are offered).
pub fn install_kind() -> &'static str {
    if std::env::var_os("APPIMAGE").is_some() {
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

#[tauri::command]
pub fn app_info(app: AppHandle) -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION").into(),
        channel: CHANNEL.into(),
        platform: std::env::consts::OS.into(),
        install_kind: install_kind().into(),
        heif: cfg!(feature = "heif"),
        formats: tessera_imaging::capabilities(),
        input_extensions: tessera_imaging::input_extensions().into_iter().map(String::from).collect(),
        data_dir: paths::data_dir(&app).to_string_lossy().into_owned(),
        cache_dir: paths::cache_dir(&app).to_string_lossy().into_owned(),
    }
}

#[tauri::command]
pub fn reveal_path(app: AppHandle, path: String) -> CmdResult<()> {
    app.opener().reveal_item_in_dir(paths::expand(&path)).map_err(err)
}

#[tauri::command]
pub fn open_path(app: AppHandle, path: String) -> CmdResult<()> {
    app.opener()
        .open_path(paths::expand(&path).to_string_lossy(), None::<&str>)
        .map_err(err)
}

#[tauri::command]
pub fn open_url(app: AppHandle, url: String) -> CmdResult<()> {
    if !url.starts_with("https://") {
        return Err("only https links can be opened".into());
    }
    app.opener().open_url(url, None::<&str>).map_err(err)
}

#[tauri::command]
pub fn ensure_dir(path: String) -> CmdResult<String> {
    let p = paths::expand(&path);
    fs::create_dir_all(&p).map_err(|e| format!("{}: {e}", p.display()))?;
    Ok(p.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn cache_size(app: AppHandle) -> u64 {
    paths::dir_size(&paths::cache_dir(&app))
}

#[tauri::command]
pub fn clear_cache(app: AppHandle) -> u64 {
    let mut freed = 0;
    for dir in [paths::proxies(&app), paths::thumbs(&app), paths::previews(&app)] {
        freed += paths::dir_size(&dir);
        let _ = fs::remove_dir_all(&dir);
    }
    freed
}

/// Development hook: when TESSERA_AUTOTEST points at a JSON script, the UI
/// runs it on start-up (used for end-to-end smoke tests).
#[tauri::command]
pub fn autotest_script() -> Option<String> {
    let path = std::env::var_os("TESSERA_AUTOTEST")?;
    fs::read_to_string(path).ok()
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
#[tauri::command]
pub async fn check_updates() -> CmdResult<UpdateInfo> {
    tauri::async_runtime::spawn_blocking(|| {
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
                r.get("body").and_then(|u| u.as_str()).map(|s| s.chars().take(2000).collect()),
                r.get("published_at").and_then(|u| u.as_str()).map(String::from),
            ),
            None => (None, None, None, None),
        };
        let has_update = latest.as_deref().and_then(parse_version).map(|v| v > cur).unwrap_or(false);
        Ok(UpdateInfo {
            current: current.into(),
            latest,
            has_update,
            url,
            notes,
            published_at: published,
            install_kind: install_kind().into(),
        })
    })
    .await
    .map_err(err)?
}
