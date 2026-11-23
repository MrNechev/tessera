//! Runs Tessera's engine (`tessera-core`) on 127.0.0.1 for the window.

use std::borrow::Cow;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use tauri::{AppHandle, Manager};
use tessera_core::tools::settings::{self, Settings};
use tessera_core::tools::{self, Mode, Tools, ToolsConfig, OUTPUTS_ID};
use tessera_core::{Config, Core, Library, WebAssets};

pub struct Started {
    pub addr: SocketAddr,
    pub token: String,
    #[allow(dead_code)]
    pub core: Arc<Core>,
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

/// The UI, embedded in the binary by Tauri (the same files as `dist/`).
fn embedded(path: &str) -> Option<Cow<'static, [u8]>> {
    let app = APP.get()?;
    let path = if path.is_empty() { "index.html" } else { path };
    app.asset_resolver()
        .get(path.to_string())
        .map(|a| Cow::Owned(a.bytes))
}

fn dirs(app: &AppHandle) -> (PathBuf, PathBuf, PathBuf) {
    let p = app.path();
    let data = p
        .app_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir().join("tessera-data"));
    let cache = p
        .app_cache_dir()
        .unwrap_or_else(|_| std::env::temp_dir().join("tessera"));
    let config = p.app_config_dir().unwrap_or_else(|_| data.clone());
    (data, cache, config)
}

/// The engine's libraries for these settings: the user's folders plus the
/// save folder as "Outputs" (results show up in the Library).
fn libraries_for(s: &Settings) -> Vec<Library> {
    let mut libs: Vec<Library> = s
        .libraries
        .iter()
        .filter(|l| tessera_core::config::valid_library_id(&l.id) && l.id != OUTPUTS_ID)
        .map(|l| Library {
            id: l.id.clone(),
            name: l.name.clone(),
            root: tools::paths::expand(&l.path),
            writable: l.writable,
        })
        .collect();
    if !s.save_directory.trim().is_empty() {
        let root = tools::paths::expand(&s.save_directory);
        let _ = std::fs::create_dir_all(&root);
        libs.push(Library {
            id: OUTPUTS_ID.into(),
            name: "Outputs".into(),
            root,
            writable: true,
        });
    }
    libs.retain(|l| l.root.is_dir());
    libs
}

pub async fn start(app: AppHandle) -> anyhow::Result<Started> {
    let _ = APP.set(app.clone());
    let (data, cache, config) = dirs(&app);
    let tools = Arc::new(Tools::new(ToolsConfig {
        mode: Mode::Desktop,
        uploads_dir: cache.join("uploads"),
        outputs_dir: tools::paths::default_save_dir(),
        inbox_dir: None,
        max_upload: None,
        data_dir: data.clone(),
        config_dir: config,
        cache_dir: cache.clone(),
    }));
    let s = settings::load_settings(&tools);
    let mut cfg = Config::desktop(data, cache.join("library"), libraries_for(&s));
    cfg.web = WebAssets::Embedded(embedded);
    cfg.version = env!("CARGO_PKG_VERSION").to_string();
    // A bundled ffmpeg next to the executable wins over the system one.
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(PathBuf::from))
    {
        for (name, slot) in [("ffmpeg", &mut cfg.ffmpeg), ("ffprobe", &mut cfg.ffprobe)] {
            let bundled = dir.join(name);
            if bundled.is_file() {
                *slot = bundled.to_string_lossy().into_owned();
            }
        }
    }
    let core = Core::start(cfg).await?;
    tools.guard.set_areas(tools::areas_for(&core.libraries()));
    {
        let core = core.clone();
        let t = Arc::downgrade(&tools);
        tools.on_settings_saved(Box::new(move |s| {
            let libs = libraries_for(s);
            if libs != core.libraries() {
                core.set_libraries(libs.clone());
            }
            if let Some(t) = t.upgrade() {
                t.guard.set_areas(tools::areas_for(&libs));
            }
        }));
    }
    tools::api::spawn_cleanup(tools.clone());
    let router = core.router_with(tools::api::routes(tools.clone()));
    let (addr, _) = if cfg!(debug_assertions) {
        core.bind_local_port(router, 1430).await?
    } else {
        core.bind_local(router).await?
    };
    let token = core.token().unwrap_or_default().to_string();
    Ok(Started {
        addr,
        token,
        core,
        tools,
    })
}
