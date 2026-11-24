//! Derived assets: multi-size thumbnails, GIF→MP4 conversions, H.264 proxies, live-photo
//! clips and video sprite sheets. Every derived file is keyed by the source file's identity
//! key, so its URL never changes meaning and can be cached by browsers forever.
//!
//! Cache layout (same as Letopis): `thumbs/xx/<key>-<size>.webp`, `gif/xx/<key>.mp4`,
//! `proxy/xx/<key>-<variant>.mp4`, `motion/xx/<key>.mp4`, `sprite/xx/<key>.jpg`.

pub mod cmd;
pub mod exif;
pub mod kinds;
pub mod motion;
pub mod phash;
pub mod probe;
pub mod raw;
pub mod sprite;
pub mod transcode;

pub use kinds::{
    browser_image, is_psd, is_raw, kind_of, mime, KIND_AUDIO, KIND_GIF, KIND_IMAGE, KIND_VIDEO,
};
pub use phash::phash;
pub use sprite::{sprite_layout, SPRITE_COLS, SPRITE_TILE_W};
pub use transcode::{ProxyState, VARIANT_FULL, VARIANT_PREVIEW};

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::bail;
use serde::Serialize;
use tokio::runtime::Handle;
use tokio_util::sync::CancellationToken;

use crate::config::Config;
use crate::jobs::{JobCtx, JobError, Pool, Priority};
use crate::store::File;
use crate::util::{first_line, look_path, num_cpu};
use cmd::Exec;
use probe::path_arg;

/// Generated widths. 320 = library grid, 1280 = in-document, 2560 = full screen.
pub const THUMB_SIZES: [i64; 3] = [320, 1280, 2560];

pub fn valid_thumb_size(n: i64) -> bool {
    THUMB_SIZES.contains(&n)
}

/// Hit/miss counter for a cache.
#[derive(Debug, Default)]
pub struct Counter {
    pub hits: AtomicI64,
    pub misses: AtomicI64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CounterStats {
    pub hits: i64,
    pub misses: i64,
    #[serde(rename = "hitRate")]
    pub hit_rate: f64,
}

impl Counter {
    pub fn hit(&self) {
        self.hits.fetch_add(1, Ordering::Relaxed);
    }
    pub fn miss(&self) {
        self.misses.fetch_add(1, Ordering::Relaxed);
    }
    pub fn stats(&self) -> CounterStats {
        let (h, m) = (
            self.hits.load(Ordering::Relaxed),
            self.misses.load(Ordering::Relaxed),
        );
        CounterStats {
            hits: h,
            misses: m,
            hit_rate: if h + m > 0 {
                h as f64 / (h + m) as f64
            } else {
                0.0
            },
        }
    }
}

const CACHE_DIRS: [&str; 5] = ["thumbs", "gif", "proxy", "motion", "sprite"];

/// Generates and locates derived assets.
pub struct Manager {
    cfg: Arc<Config>,
    cache_dir: PathBuf,
    rt: Handle,
    tools: Arc<cmd::Tools>,
    has_ffmpeg: bool,
    has_ffprobe: bool,
    has_vips: bool,
    /// "vaapi" | "none"
    hw: String,
    hw_note: String,

    pub probes: Pool,
    pub thumbs: Pool,
    pub gifs: Pool,
    pub transcode: Pool,

    pub thumb_stats: Counter,
    pub gif_stats: Counter,
    pub proxy_stats: Counter,

    size_cache: Mutex<Option<(BTreeMap<String, i64>, Instant)>>,
}

impl Manager {
    /// Creates the cache folders, checks the external tools and probes GPU encoding.
    /// Blocking (runs a short test encode); `rt` is the runtime used to drive processes.
    pub fn new(cfg: Arc<Config>, rt: Handle) -> anyhow::Result<Manager> {
        let cache_dir = cfg.cache_dir.clone();
        for d in CACHE_DIRS {
            std::fs::create_dir_all(cache_dir.join(d))?;
        }
        let has_ffmpeg = look_path(&cfg.ffmpeg).is_some();
        let has_ffprobe = look_path(&cfg.ffprobe).is_some();
        if !has_ffmpeg {
            tracing::warn!(ffmpeg = %cfg.ffmpeg, "ffmpeg not found: videos, GIFs and audio get no thumbnails or conversions (set TESSERA_FFMPEG)");
        }
        if !has_ffprobe {
            tracing::warn!(ffprobe = %cfg.ffprobe, "ffprobe not found: videos and audio can't be probed (set TESSERA_FFPROBE)");
        }
        let tools = Arc::new(cmd::Tools {
            ffmpeg: cfg.ffmpeg.clone(),
            ffprobe: cfg.ffprobe.clone(),
        });
        let has_vips = look_path("vipsthumbnail").is_some() && look_path("vipsheader").is_some();
        if !has_vips {
            tracing::warn!(
                "libvips tools not found; images are thumbnailed with ffmpeg (no HEIC, slower)"
            );
        }
        let mut m = Manager {
            probes: Pool::new("probe", cfg.probe_workers),
            thumbs: Pool::new("thumbnails", cfg.thumb_workers),
            gifs: Pool::new("gif", (cfg.thumb_workers / 2).max(1)),
            transcode: Pool::new("transcode", cfg.video_workers),
            cfg,
            cache_dir,
            rt,
            tools,
            has_ffmpeg,
            has_ffprobe,
            has_vips,
            hw: "none".into(),
            hw_note: String::new(),
            thumb_stats: Counter::default(),
            gif_stats: Counter::default(),
            proxy_stats: Counter::default(),
            size_cache: Mutex::new(None),
        };
        m.detect_hw();
        Ok(m)
    }

    /// Stops all worker pools.
    pub fn close(&self) {
        self.transcode.close();
        self.gifs.close();
        self.thumbs.close();
        self.probes.close();
    }

    /// GPU mode ("vaapi" | "none") and a human-readable note.
    pub fn hw(&self) -> (&str, &str) {
        (&self.hw, &self.hw_note)
    }

    /// The VA-API device used for GPU encoding.
    pub fn vaapi_device(&self) -> &str {
        &self.cfg.vaapi_device
    }

    pub fn has_vips(&self) -> bool {
        self.has_vips
    }

    pub fn has_ffmpeg(&self) -> bool {
        self.has_ffmpeg
    }

    pub fn has_ffprobe(&self) -> bool {
        self.has_ffprobe
    }

    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    pub(crate) fn exec(&self, cancel: &CancellationToken, timeout: Duration, bg: bool) -> Exec {
        Exec::new(
            self.tools.clone(),
            self.rt.clone(),
            cancel.clone(),
            timeout,
            bg && self.cfg.nice,
        )
    }

    /// Checks whether VAAPI H.264 encoding actually works by encoding a tiny clip.
    fn detect_hw(&mut self) {
        self.hw = "none".into();
        if self.cfg.hw_accel == "none" || self.cfg.hw_accel == "off" {
            self.hw_note = "disabled by TESSERA_HWACCEL".into();
            return;
        }
        let dev = self.cfg.vaapi_device.clone();
        if !self.has_ffmpeg {
            self.hw_note = "ffmpeg not available".into();
            return;
        }
        if std::fs::metadata(&dev).is_err() {
            self.hw_note =
                format!("no {dev} (pass /dev/dri into the container for GPU transcoding)");
            return;
        }
        let ex = self.exec(&CancellationToken::new(), Duration::from_secs(20), false);
        let args: Vec<String> = [
            "-v",
            "error",
            "-vaapi_device",
            &dev,
            "-f",
            "lavfi",
            "-i",
            "testsrc2=s=256x144:d=0.2:r=25",
            "-vf",
            "format=nv12,hwupload",
            "-c:v",
            "h264_vaapi",
            "-f",
            "null",
            "-",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        match ex.output("ffmpeg", &args) {
            Ok(_) => {
                self.hw = "vaapi".into();
                self.hw_note = format!("VAAPI H.264 encoding on {dev}");
                tracing::info!(device = %dev, "GPU transcoding enabled");
            }
            Err(e) => {
                let out = match &e {
                    cmd::CmdError::Exit { stdout, stderr, .. } => {
                        let mut c = stdout.clone();
                        c.extend_from_slice(stderr);
                        String::from_utf8_lossy(&c).into_owned()
                    }
                    other => other.to_string(),
                };
                self.hw_note = format!("VAAPI test encode failed: {}", first_line(&out));
                tracing::warn!(reason = %self.hw_note, "GPU transcoding unavailable, using CPU");
            }
        }
    }

    // ---------- paths ----------

    pub fn thumb_path(&self, key: &str, size: i64) -> PathBuf {
        self.cache_dir
            .join("thumbs")
            .join(shard(key))
            .join(format!("{key}-{size}.webp"))
    }

    pub fn gif_path(&self, key: &str) -> PathBuf {
        self.cache_dir
            .join("gif")
            .join(shard(key))
            .join(format!("{key}.mp4"))
    }

    // ---------- thumbnails ----------

    /// Makes sure the thumbnail exists, generating it at high priority and waiting.
    pub async fn thumb(
        self: &Arc<Self>,
        f: &File,
        abs: &Path,
        size: i64,
    ) -> Result<PathBuf, JobError> {
        let out = self.thumb_path(&f.key, size);
        if exists(&out) {
            self.thumb_stats.hit();
            return Ok(out);
        }
        self.thumb_stats.miss();
        let job = self.thumb_job(f, abs, size, out.clone());
        self.thumbs.wait(&format!("{}-{size}", f.key), job).await?;
        Ok(out)
    }

    /// Queues the thumbnail as background work (returns immediately).
    pub fn thumb_queue(self: &Arc<Self>, f: &File, abs: &Path, size: i64) -> Result<(), JobError> {
        let out = self.thumb_path(&f.key, size);
        if exists(&out) {
            return Ok(());
        }
        let job = self.thumb_job(f, abs, size, out);
        self.thumbs
            .submit(&format!("{}-{size}", f.key), Priority::Low, false, job)
            .map(|_| ())
    }

    fn thumb_job(
        self: &Arc<Self>,
        f: &File,
        abs: &Path,
        size: i64,
        out: PathBuf,
    ) -> impl FnOnce(&JobCtx) -> anyhow::Result<()> + Send + 'static {
        let m = self.clone();
        let fc = f.clone();
        let abs = abs.to_path_buf();
        move |ctx| {
            if exists(&out) {
                return Ok(());
            }
            m.make_thumb(ctx, &fc, &abs, size, &out)
        }
    }

    fn make_thumb(
        &self,
        ctx: &JobCtx,
        f: &File,
        abs: &Path,
        size: i64,
        out: &Path,
    ) -> anyhow::Result<()> {
        if let Some(d) = out.parent() {
            std::fs::create_dir_all(d)?;
        }
        let bg = ctx.background();
        let tmp = tmp_for(out);
        let _g = RemoveOnDrop(tmp.clone());
        let ex = self.exec(&ctx.cancel, Duration::from_secs(180), bg);
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<String>>();

        if f.kind == KIND_IMAGE && is_raw(&f.ext) {
            return self.raw_thumb(&ex, abs, size, &tmp, out);
        }
        if f.kind == KIND_AUDIO {
            // cover art, if the file has one
            let mut a = s(&["-v", "error", "-nostdin", "-i"]);
            a.push(path_arg(abs));
            a.extend(s(&["-an", "-frames:v", "1", "-vf"]));
            a.push(format!("scale='min({size},iw)':-2"));
            a.extend(s(&["-c:v", "libwebp", "-quality", "80", "-y"]));
            a.push(path_arg(&tmp));
            ex.run("ffmpeg", &a)?;
            return publish(&tmp, out);
        }
        if (f.kind == KIND_IMAGE || f.kind == KIND_GIF) && self.has_vips {
            // shrink-on-load + EXIF autorotate; ">" = only ever shrink
            let a = vec![
                path_arg(abs),
                "--size".into(),
                format!("{size}x>"),
                "-o".into(),
                format!("{}[Q=80,strip]", path_arg(&tmp)),
            ];
            match ex.run("vipsthumbnail", &a) {
                Ok(()) => return publish(&tmp, out),
                Err(e) if f.kind == KIND_IMAGE => return Err(e),
                Err(_) => {}
            }
        }
        let scale = format!(
            "{}scale='min({size},iw)':-2:flags=lanczos",
            tonemap_prefix(f.hdr)
        );
        let mut seek = 0.0;
        let mut a = s(&["-v", "error", "-nostdin"]);
        if f.kind == KIND_VIDEO && f.duration > 1.0 {
            seek = (f.duration * 0.1).min(5.0);
            a.push("-ss".into());
            a.push(format!("{seek:.2}"));
        }
        let tail = |a: &mut Vec<String>| {
            a.push("-i".into());
            a.push(path_arg(abs));
            a.extend(s(&["-frames:v", "1", "-vf"]));
            a.push(scale.clone());
            a.extend(s(&["-c:v", "libwebp", "-quality", "80", "-y"]));
            a.push(path_arg(&tmp));
        };
        tail(&mut a);
        let mut res = ex.run("ffmpeg", &a);
        if (res.is_err() || !exists(&tmp)) && seek > 0.0 {
            // seek past a broken index etc. — fall back to the first frame
            let mut a = s(&["-v", "error", "-nostdin"]);
            tail(&mut a);
            res = ex.run("ffmpeg", &a);
        }
        res?;
        publish(&tmp, out)
    }

    /// Thumbnails the camera's embedded JPEG preview, rotated by the RAW's orientation.
    fn raw_thumb(
        &self,
        ex: &Exec,
        abs: &Path,
        size: i64,
        tmp: &Path,
        out: &Path,
    ) -> anyhow::Result<()> {
        let (jpg, mt) = raw::raw_preview(abs)?;
        let out_s = out.to_string_lossy();
        let src = tmp_for(Path::new(&format!(
            "{}-raw.jpg",
            out_s.strip_suffix(".webp").unwrap_or(&out_s)
        )));
        std::fs::write(&src, &jpg)?;
        let _gs = RemoveOnDrop(src.clone());
        if self.has_vips {
            let rot = match mt.orientation {
                3 => "d180",
                6 => "d90",
                8 => "d270",
                _ => "",
            };
            let mut _gm = None;
            let mid = if rot.is_empty() {
                tmp.to_path_buf()
            } else {
                let mid = tmp_for(out);
                _gm = Some(RemoveOnDrop(mid.clone()));
                mid
            };
            ex.run(
                "vipsthumbnail",
                &[
                    path_arg(&src),
                    "--no-rotate".into(),
                    "--size".into(),
                    format!("{size}x>"),
                    "-o".into(),
                    format!("{}[Q=80,strip]", path_arg(&mid)),
                ],
            )?;
            if !rot.is_empty() {
                ex.run(
                    "vips",
                    &[
                        "rot".into(),
                        path_arg(&mid),
                        format!("{}[Q=80,strip]", path_arg(tmp)),
                        rot.into(),
                    ],
                )?;
            }
            return publish(tmp, out);
        }
        let tr = match mt.orientation {
            3 => "transpose=1,transpose=1,",
            6 => "transpose=1,",
            8 => "transpose=2,",
            _ => "",
        };
        ex.run(
            "ffmpeg",
            &[
                "-v".into(),
                "error".into(),
                "-nostdin".into(),
                "-i".into(),
                path_arg(&src),
                "-frames:v".into(),
                "1".into(),
                "-vf".into(),
                format!("{tr}scale='min({size},iw)':-2:flags=lanczos"),
                "-c:v".into(),
                "libwebp".into(),
                "-quality".into(),
                "80".into(),
                "-y".into(),
                path_arg(tmp),
            ],
        )?;
        publish(tmp, out)
    }

    // ---------- GIF → MP4 ----------

    /// Converts (and waits for) a GIF's MP4 version.
    pub async fn gif(self: &Arc<Self>, f: &File, abs: &Path) -> Result<PathBuf, JobError> {
        let out = self.gif_path(&f.key);
        if exists(&out) {
            self.gif_stats.hit();
            return Ok(out);
        }
        self.gif_stats.miss();
        let job = self.gif_job(abs, out.clone());
        self.gifs.wait(&f.key, job).await?;
        Ok(out)
    }

    /// Queues a GIF conversion as background work.
    pub fn gif_queue(self: &Arc<Self>, f: &File, abs: &Path) -> Result<(), JobError> {
        let out = self.gif_path(&f.key);
        if exists(&out) {
            return Ok(());
        }
        let job = self.gif_job(abs, out);
        self.gifs
            .submit(&f.key, Priority::Low, false, job)
            .map(|_| ())
    }

    fn gif_job(
        self: &Arc<Self>,
        abs: &Path,
        out: PathBuf,
    ) -> impl FnOnce(&JobCtx) -> anyhow::Result<()> + Send + 'static {
        let m = self.clone();
        let abs = abs.to_path_buf();
        move |ctx| {
            if exists(&out) {
                return Ok(());
            }
            m.make_gif(ctx, &abs, &out)
        }
    }

    fn make_gif(&self, ctx: &JobCtx, abs: &Path, out: &Path) -> anyhow::Result<()> {
        if let Some(d) = out.parent() {
            std::fs::create_dir_all(d)?;
        }
        let tmp = tmp_for(out);
        let _g = RemoveOnDrop(tmp.clone());
        let ex = self.exec(&ctx.cancel, Duration::from_secs(600), ctx.background());
        // Even dimensions (≥2 px: 1×1 spacer GIFs exist) for yuv420p, cap the width, keep
        // GIF timing (VFR), no audio.
        let mut a: Vec<String> = ["-v", "error", "-nostdin", "-i"].map(String::from).to_vec();
        a.push(path_arg(abs));
        a.extend(
            [
                "-an",
                "-fps_mode",
                "vfr",
                "-vf",
                "scale='max(2,trunc(min(1920,iw)/2)*2)':'max(2,trunc(ow/a/2)*2)':flags=lanczos",
                "-c:v",
                "libx264",
                "-preset",
                "veryfast",
                "-crf",
                "23",
                "-pix_fmt",
                "yuv420p",
                "-threads",
            ]
            .map(String::from),
        );
        a.push((num_cpu() / 4).max(1).to_string());
        a.extend(["-movflags", "+faststart", "-y"].map(String::from));
        a.push(path_arg(&tmp));
        ex.run("ffmpeg", &a)?;
        publish(&tmp, out)
    }

    // ---------- cache size ----------

    /// Bytes per cache subdirectory (recomputed at most every 2 minutes).
    pub fn cache_sizes(&self) -> BTreeMap<String, i64> {
        let mut g = self.size_cache.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((sizes, at)) = g.as_ref() {
            if at.elapsed() < Duration::from_secs(120) {
                return sizes.clone();
            }
        }
        let mut sizes = BTreeMap::new();
        for d in CACHE_DIRS {
            sizes.insert(d.to_string(), dir_size(&self.cache_dir.join(d)));
        }
        *g = Some((sizes.clone(), Instant::now()));
        sizes
    }

    /// Removes leftovers of interrupted jobs.
    pub fn clean_temp(&self) {
        fn walk(p: &Path) {
            let Ok(rd) = std::fs::read_dir(p) else { return };
            for e in rd.flatten() {
                let Ok(ft) = e.file_type() else { continue };
                if ft.is_dir() {
                    walk(&e.path());
                } else if e.file_name().to_string_lossy().contains(".tmp-") {
                    let _ = std::fs::remove_file(e.path());
                }
            }
        }
        walk(&self.cache_dir);
    }
}

fn dir_size(p: &Path) -> i64 {
    let Ok(rd) = std::fs::read_dir(p) else {
        return 0;
    };
    let mut total = 0;
    for e in rd.flatten() {
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_dir() {
            total += dir_size(&e.path());
        } else if let Ok(md) = e.metadata() {
            total += md.len() as i64;
        }
    }
    total
}

// ---------- helpers ----------

pub(crate) fn shard(key: &str) -> &str {
    &key[..2.min(key.len())]
}

/// Whether a non-empty file exists.
pub(crate) fn exists(p: &Path) -> bool {
    std::fs::metadata(p).map(|m| m.len() > 0).unwrap_or(false)
}

/// A temp path next to `final_` that keeps its extension (vips and ffmpeg pick the output
/// format from it).
pub(crate) fn tmp_for(final_: &Path) -> PathBuf {
    let mut b = [0u8; 6];
    rand::RngCore::fill_bytes(&mut rand::rng(), &mut b);
    let s = final_.to_string_lossy();
    let ext = crate::util::path_ext(&s);
    let stem = &s[..s.len() - ext.len()];
    PathBuf::from(format!("{stem}.tmp-{}{ext}", hex::encode(b)))
}

pub(crate) fn publish(tmp: &Path, final_: &Path) -> anyhow::Result<()> {
    let md = std::fs::metadata(tmp)?;
    if md.len() == 0 {
        let _ = std::fs::remove_file(tmp);
        bail!("empty output");
    }
    std::fs::rename(tmp, final_)?;
    Ok(())
}

/// Removes a temp file when dropped.
pub(crate) struct RemoveOnDrop(pub PathBuf);

impl Drop for RemoveOnDrop {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Converts HDR (PQ/HLG, BT.2020) to SDR BT.709 before scaling, so HDR phone videos don't
/// look washed out in proxies, thumbnails and clips.
pub fn tonemap_prefix(hdr: bool) -> &'static str {
    if !hdr {
        return "";
    }
    "zscale=t=linear:npl=100,format=gbrpf32le,zscale=p=bt709,tonemap=tonemap=hable:desat=0,zscale=t=bt709:m=bt709:r=tv,format=yuv420p,"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tmp_keeps_extension() {
        let t = tmp_for(Path::new("/c/thumbs/ab/abc-320.webp"));
        let s = t.to_string_lossy();
        assert!(
            s.starts_with("/c/thumbs/ab/abc-320.tmp-") && s.ends_with(".webp"),
            "{s}"
        );
    }
}
