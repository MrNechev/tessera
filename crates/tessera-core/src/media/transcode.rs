//! Browser-playable proxies.
//!
//! Variants:
//! - `preview` — short side ≤ PreviewHeight (1080), for in-document playback of heavy videos
//! - `full`    — browser-compatible H.264 at up to 2160p, only for videos the browser can't
//!   play

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;

use super::kinds::KIND_VIDEO;
use super::probe::path_arg;
use super::{exists, publish, shard, tmp_for, tonemap_prefix, Manager};
use crate::jobs::{JobCtx, Priority};
use crate::store::File;

pub const VARIANT_PREVIEW: &str = "preview";
pub const VARIANT_FULL: &str = "full";

/// A proxy's state for the UI.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ProxyState {
    pub variant: String,
    /// ready | queued | running | failed | none
    pub status: String,
    #[serde(skip_serializing_if = "is_zero")]
    pub progress: f64,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub error: String,
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

impl ProxyState {
    fn new(variant: &str, status: &str) -> ProxyState {
        ProxyState {
            variant: variant.into(),
            status: status.into(),
            progress: 0.0,
            error: String::new(),
        }
    }
}

fn short_side(f: &File) -> i64 {
    f.width.min(f.height)
}

fn proxy_job_key(key: &str, variant: &str) -> String {
    format!("{key}-{variant}")
}

impl Manager {
    /// Whether the client browser can decode the original directly.
    pub fn playable(&self, f: &File) -> bool {
        if f.kind != KIND_VIDEO {
            return true;
        }
        if !f.probed || !f.probe_error.is_empty() {
            return false;
        }
        if !self.cfg.playable_vcodecs.contains(&f.vcodec) {
            return false;
        }
        if !f.acodec.is_empty() && !self.cfg.playable_acodecs.contains(&f.acodec) {
            return false;
        }
        match f.ext.as_str() {
            "mp4" | "m4v" | "mov" | "webm" => {}
            "mkv" => {
                // Chromium plays Matroska only as WebM: VP8/VP9/AV1 video
                if !matches!(f.vcodec.as_str(), "vp8" | "vp9" | "av1") {
                    return false;
                }
            }
            _ => return false,
        }
        if f.vcodec == "h264" {
            // browsers only decode 8-bit 4:2:0 H.264
            if f.pix_fmt.contains("10") || f.pix_fmt.contains("422") || f.pix_fmt.contains("444") {
                return false;
            }
        }
        true
    }

    /// Playable, but heavy enough that a lighter proxy helps in-document playback.
    pub fn needs_preview(&self, f: &File) -> bool {
        f.kind == KIND_VIDEO
            && f.probed
            && f.probe_error.is_empty()
            && (short_side(f) > self.cfg.preview_min_height
                || (self.cfg.preview_min_bitrate > 0 && f.bitrate > self.cfg.preview_min_bitrate))
    }

    /// Which proxy (if any) the document view uses. "" = original.
    pub fn doc_variant(&self, f: &File) -> &'static str {
        if f.kind != KIND_VIDEO || !f.probed || !f.probe_error.is_empty() {
            return "";
        }
        if self.playable(f) {
            if self.needs_preview(f) {
                return VARIANT_PREVIEW;
            }
            return "";
        }
        if short_side(f) > self.cfg.preview_height {
            return VARIANT_PREVIEW;
        }
        VARIANT_FULL
    }

    /// Which proxy (if any) presentations use. "" = original.
    pub fn present_variant(&self, f: &File) -> &'static str {
        if f.kind != KIND_VIDEO || !f.probed || !f.probe_error.is_empty() || self.playable(f) {
            return "";
        }
        VARIANT_FULL
    }

    pub fn proxy_path(&self, key: &str, variant: &str) -> PathBuf {
        self.cache_dir
            .join("proxy")
            .join(shard(key))
            .join(format!("{key}-{variant}.mp4"))
    }

    pub fn proxy_status(&self, f: &File, variant: &str) -> ProxyState {
        let mut st = ProxyState::new(variant, "none");
        if exists(&self.proxy_path(&f.key, variant)) {
            st.status = "ready".into();
            return st;
        }
        let jk = proxy_job_key(&f.key, variant);
        if let Some(j) = self.transcode.get(&jk) {
            st.status = "queued".into();
            let p = j.progress();
            if p > 0.0 || self.transcode.is_running(&jk) {
                st.status = "running".into();
                st.progress = p;
            }
            return st;
        }
        if let Some(e) = self.transcode.last_error(&jk) {
            st.status = "failed".into();
            st.error = e.to_string();
        }
        st
    }

    /// The proxy file if it exists.
    pub fn proxy_path_if_ready(&self, key: &str, variant: &str) -> Option<PathBuf> {
        let p = self.proxy_path(key, variant);
        exists(&p).then_some(p)
    }

    /// Queues a proxy transcode. Requests from the UI use High priority.
    pub fn request_proxy(
        self: &Arc<Self>,
        f: &File,
        abs: &Path,
        variant: &str,
        prio: Priority,
        retry: bool,
    ) -> ProxyState {
        if variant != VARIANT_PREVIEW && variant != VARIANT_FULL {
            let mut st = ProxyState::new(variant, "failed");
            st.error = "unknown variant".into();
            return st;
        }
        let out = self.proxy_path(&f.key, variant);
        if exists(&out) {
            return ProxyState::new(variant, "ready");
        }
        let jk = proxy_job_key(&f.key, variant);
        if retry {
            self.transcode.clear_failure(&jk);
        }
        let m = self.clone();
        let fc = f.clone();
        let abs = abs.to_path_buf();
        let v = variant.to_string();
        let res = self.transcode.submit(&jk, prio, false, move |ctx| {
            if exists(&out) {
                return Ok(());
            }
            m.make_proxy(ctx, &fc, &abs, &v, &out)
        });
        if let Err(e) = res {
            let mut st = ProxyState::new(variant, "failed");
            st.error = e.to_string();
            return st;
        }
        self.proxy_status(f, variant)
    }

    fn make_proxy(
        &self,
        ctx: &JobCtx,
        f: &File,
        abs: &Path,
        variant: &str,
        out: &Path,
    ) -> anyhow::Result<()> {
        if let Some(d) = out.parent() {
            std::fs::create_dir_all(d)?;
        }
        let (mut target, qp) = if variant == VARIANT_FULL {
            (2160, 21)
        } else {
            (self.cfg.preview_height, 24)
        };
        let ss = short_side(f);
        if ss < target {
            target = ss;
        }
        target -= target % 2;
        // width/height for the (display-oriented) frame: scale the short side to target
        let (w, h) = if f.width < f.height {
            (target.to_string(), "-2".to_string())
        } else {
            ("-2".to_string(), target.to_string())
        };

        let audio: Vec<String> = if f.acodec.is_empty() {
            vec!["-an".into()]
        } else if f.acodec == "aac" || f.acodec == "mp3" {
            vec!["-c:a".into(), "copy".into()]
        } else {
            ["-c:a", "aac", "-b:a", "192k"].map(String::from).to_vec()
        };
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        let common = |input: Vec<String>, vf: String, venc: Vec<String>| -> Vec<String> {
            let mut a = s(&["-v", "error", "-nostdin", "-progress", "pipe:1", "-nostats"]);
            a.extend(input);
            a.extend(["-i".into(), path_arg(abs)]);
            a.extend(s(&["-map", "0:v:0", "-map", "0:a:0?", "-vf"]));
            a.push(vf);
            a.extend(venc);
            a.extend(audio.clone());
            a.extend(s(&["-movflags", "+faststart", "-map_metadata", "-1", "-y"]));
            a
        };

        let mut attempts: Vec<(&str, Vec<String>)> = Vec::new();
        if self.hw == "vaapi" {
            let dev = self.cfg.vaapi_device.clone();
            let venc: Vec<String> = vec![
                "-c:v".into(),
                "h264_vaapi".into(),
                "-qp".into(),
                qp.to_string(),
                "-profile:v".into(),
                "high".into(),
            ];
            let hw_decodable = ["h264", "hevc", "vp8", "vp9", "av1", "mpeg2video"]
                .contains(&f.vcodec.as_str())
                && !f.pix_fmt.contains("422")
                && !f.pix_fmt.contains("444");
            if f.rotation == 0 && hw_decodable && !f.hdr {
                // everything on the GPU: decode → scale → encode
                attempts.push((
                    "vaapi",
                    common(
                        vec![
                            "-hwaccel".into(),
                            "vaapi".into(),
                            "-hwaccel_device".into(),
                            dev.clone(),
                            "-hwaccel_output_format".into(),
                            "vaapi".into(),
                        ],
                        format!("scale_vaapi=w={w}:h={h}:format=nv12"),
                        venc.clone(),
                    ),
                ));
            }
            // CPU decode (handles rotation and odd codecs) → GPU encode
            attempts.push((
                "cpu-decode+vaapi",
                common(
                    vec!["-vaapi_device".into(), dev],
                    format!(
                        "{}scale={w}:{h}:flags=bicubic,format=nv12,hwupload",
                        tonemap_prefix(f.hdr)
                    ),
                    venc,
                ),
            ));
        }
        attempts.push((
            "libx264",
            common(
                Vec::new(),
                format!(
                    "{}scale={w}:{h}:flags=bicubic,format=yuv420p",
                    tonemap_prefix(f.hdr)
                ),
                vec![
                    "-c:v".into(),
                    "libx264".into(),
                    "-preset".into(),
                    "veryfast".into(),
                    "-crf".into(),
                    (qp - 3).to_string(),
                    "-profile:v".into(),
                    "high".into(),
                    "-threads".into(),
                    "0".into(),
                ],
            ),
        ));

        let ex = self.exec(
            &ctx.cancel,
            Duration::from_secs(7 * 24 * 3600),
            ctx.background(),
        );
        let mut last_err = anyhow::anyhow!("no attempts");
        for (name, mut args) in attempts {
            let tmp = tmp_for(out);
            args.push(path_arg(&tmp));
            let start = Instant::now();
            let duration = f.duration;
            let job = ctx.job.clone();
            let mut err = ex
                .run_lines("ffmpeg", &args, |line| {
                    if let Some(v) = line.strip_prefix("out_time_us=") {
                        if duration > 0.0 {
                            if let Ok(us) = v.parse::<i64>() {
                                job.set_progress(us as f64 / 1e6 / duration);
                            }
                        }
                    }
                })
                .err();
            if err.is_none() {
                match publish(&tmp, out) {
                    Ok(()) => {
                        tracing::info!(file = %f.path, variant, method = name, took = ?start.elapsed(), "proxy ready");
                        return Ok(());
                    }
                    Err(e) => err = Some(e),
                }
            }
            let _ = std::fs::remove_file(&tmp);
            let err = err.unwrap_or_else(|| anyhow::anyhow!("unknown error"));
            if ctx.cancel.is_cancelled() {
                return Err(err);
            }
            tracing::warn!(file = %f.path, method = name, err = %err, "proxy attempt failed");
            last_err = err;
            ctx.set_progress(0.0);
        }
        Err(last_err)
    }
}
