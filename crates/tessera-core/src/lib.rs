//! Tessera's engine, shared by the desktop app (`src-tauri`) and the server
//! (`crates/tessera-server`). Everything is reachable over one HTTP API.
//!
//! Module ownership during the Letopis merge:
//! - ported from Letopis (Go → Rust): config, store, jobs, media, library, geo, docs,
//!   prefs, auth, tls, server (HTTP API with Letopis's routes)
//! - moved from src-tauri: tools (images, export, batch, projects, settings, system)

pub mod auth;
pub mod config;
pub mod docs;
pub mod geo;
pub mod jobs;
pub mod library;
pub mod media;
pub mod prefs;
pub mod store;
pub mod tls;
pub mod util;
pub mod zipstream;
