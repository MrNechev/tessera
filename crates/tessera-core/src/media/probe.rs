//! Reading metadata: images natively where possible (no process spawn); everything else
//! through ffprobe (or vipsheader for exotic image formats).

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::time::Duration;

use serde::Deserialize;
use sha1::{Digest, Sha1};
use tokio_util::sync::CancellationToken;

use super::cmd::CmdError;
use super::exif::{
    camera_name, jpeg_exif, parse_exif_time, parse_iso6709, parse_tiff, parse_vips_gps,
    valid_gps, vips_string, Meta,
};
use super::kinds::{is_raw, KIND_AUDIO, KIND_GIF, KIND_IMAGE};
use super::motion::embedded_motion;
use super::raw::{jpeg_config, raw_preview};
use super::Manager;
use crate::store::Probe;
use crate::util::truncate_bytes;

impl Manager {
    /// Reads metadata for a file (blocking).
    pub fn probe(&self, cancel: &CancellationToken, abs: &Path, kind: &str, ext: &str) -> Probe {
        let mut p = self.probe_kind(cancel, abs, kind, ext);
        if p.error.is_empty() {
            p.chash = content_hash(abs);
            if kind == KIND_IMAGE && matches!(ext, "jpg" | "jpeg" | "heic" | "heif") {
                let off = embedded_motion(abs);
                if off > 0 {
                    p.motion = format!("embed:{off}");
                }
            }
        }
        p
    }

    fn probe_kind(&self, cancel: &CancellationToken, abs: &Path, kind: &str, ext: &str) -> Probe {
        match kind {
            KIND_GIF => {
                // ffprobe gives the duration (animated vs. still); fall back to the header
                if let Ok(p) = self.probe_ff(cancel, abs) {
                    return p;
                }
                if let Some(p) = probe_image_native(abs, ext) {
                    return p;
                }
                Probe::error("unreadable gif")
            }
            KIND_IMAGE => {
                if is_raw(ext) {
                    return probe_raw(abs);
                }
                if let Some(p) = probe_image_native(abs, ext) {
                    return p;
                }
                if self.has_vips {
                    if let Ok(p) = self.probe_vips(cancel, abs) {
                        return p;
                    }
                }
                match self.probe_ff(cancel, abs) {
                    Ok(mut p) => {
                        p.duration = 0.0;
                        p
                    }
                    Err(e) => Probe::error(e.to_string()),
                }
            }
            KIND_AUDIO => match self.probe_ff(cancel, abs) {
                Ok(p) => p,
                Err(FfErr { msg, partial }) => match partial {
                    Some(mut p) if !p.acodec.is_empty() => {
                        p.error.clear();
                        p
                    }
                    _ => Probe::error(msg),
                },
            },
            _ => match self.probe_ff(cancel, abs) {
                Ok(p) => p,
                Err(e) => Probe::error(e.to_string()),
            },
        }
    }

    fn probe_vips(&self, cancel: &CancellationToken, abs: &Path) -> anyhow::Result<Probe> {
        let ex = self.exec(cancel, Duration::from_secs(30), false);
        let (out, _) = ex.output("vipsheader", &["-a".into(), path_arg(abs)])?;
        let out = String::from_utf8_lossy(&out);
        let mut p = Probe::default();
        let mut orientation = 1;
        let (mut mk, mut model, mut taken, mut offset, mut lat_ref, mut lon_ref) = (
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
        );
        let (mut lat, mut lon) = (None, None);
        for line in out.split('\n') {
            let Some((k, v)) = line.split_once(':') else {
                continue;
            };
            let (k, v) = (k.trim(), v.trim());
            match k {
                "width" => p.width = v.parse().unwrap_or(0),
                "height" => p.height = v.parse().unwrap_or(0),
                "orientation" => orientation = v.parse().unwrap_or(0),
                "vips-loader" => {
                    p.format = v.strip_suffix("load").unwrap_or(v).to_string();
                    p.vcodec = p.format.clone();
                }
                "exif-ifd0-Make" => mk = vips_string(v),
                "exif-ifd0-Model" => model = vips_string(v),
                "exif-ifd2-DateTimeOriginal" => taken = vips_string(v),
                "exif-ifd2-OffsetTimeOriginal" => offset = vips_string(v),
                "exif-ifd3-GPSLatitude" => lat = parse_vips_gps(v),
                "exif-ifd3-GPSLongitude" => lon = parse_vips_gps(v),
                "exif-ifd3-GPSLatitudeRef" => lat_ref = vips_string(v),
                "exif-ifd3-GPSLongitudeRef" => lon_ref = vips_string(v),
                _ => {}
            }
        }
        let mut mt = Meta {
            camera: camera_name(&mk, &model),
            taken: parse_exif_time(&taken, &offset),
            ..Default::default()
        };
        if let (Some(mut la), Some(mut lo)) = (lat, lon) {
            if lat_ref.starts_with('S') {
                la = -la;
            }
            if lon_ref.starts_with('W') {
                lo = -lo;
            }
            if valid_gps(la, lo) {
                mt.lat = la;
                mt.lon = lo;
                mt.has_gps = true;
            }
        }
        apply_meta(&mut p, &mt);
        if p.width == 0 {
            anyhow::bail!("vipsheader: no size");
        }
        if (5..=8).contains(&orientation) {
            std::mem::swap(&mut p.width, &mut p.height);
            p.rotation = 90;
        }
        Ok(p)
    }

    /// ffprobe: format, streams, rotation, HDR and container metadata.
    pub(crate) fn probe_ff(&self, cancel: &CancellationToken, abs: &Path) -> Result<Probe, FfErr> {
        let ex = self.exec(cancel, Duration::from_secs(60), false);
        let args: Vec<String> = [
            "-v",
            "error",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
        ]
        .iter()
        .map(|s| s.to_string())
        .chain([path_arg(abs)])
        .collect();
        let out = match ex.output("ffprobe", &args) {
            Ok((out, _)) => out,
            Err(e) => {
                let msg = match &e {
                    CmdError::Exit { stderr, status, .. } => {
                        let s = String::from_utf8_lossy(stderr).trim().to_string();
                        if s.is_empty() {
                            super::cmd::exit_string(status)
                        } else {
                            truncate_bytes(&s, 300).to_string()
                        }
                    }
                    CmdError::Timeout => "signal: killed".into(),
                    other => other.to_string(),
                };
                return Err(FfErr {
                    msg: format!("ffprobe: {msg}"),
                    partial: None,
                });
            }
        };
        let fo: FfprobeOut = serde_json::from_slice(&out).map_err(|e| FfErr {
            msg: e.to_string(),
            partial: None,
        })?;
        let p = parse_ffprobe(&fo);
        if p.vcodec.is_empty() {
            return Err(FfErr {
                msg: "no video stream".into(),
                partial: Some(p),
            });
        }
        Ok(p)
    }
}

/// An ffprobe failure, possibly with the metadata that was read (audio files have no video
/// stream).
#[derive(Debug)]
pub(crate) struct FfErr {
    pub msg: String,
    pub partial: Option<Probe>,
}

impl std::fmt::Display for FfErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.msg)
    }
}

pub(crate) fn path_arg(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

fn apply_meta(p: &mut Probe, mt: &Meta) {
    if let Some(t) = mt.taken {
        p.taken = t;
    }
    p.camera = mt.camera.clone();
    if mt.has_gps {
        p.lat = mt.lat;
        p.lon = mt.lon;
        p.has_gps = true;
    }
}

/// Metadata of a camera RAW (dimensions of its embedded preview).
pub fn probe_raw(abs: &Path) -> Probe {
    let (jpg, mt) = match raw_preview(abs) {
        Ok(v) => v,
        Err(e) => return Probe::error(format!("raw: {e}")),
    };
    let (w, h) = match jpeg_config(&jpg) {
        Ok(v) => v,
        Err(e) => return Probe::error(format!("raw preview: {e}")),
    };
    let mut p = Probe {
        width: w,
        height: h,
        format: "raw".into(),
        vcodec: "jpeg".into(),
        ..Default::default()
    };
    if (5..=8).contains(&mt.orientation) {
        std::mem::swap(&mut p.width, &mut p.height);
        p.rotation = 90;
    }
    apply_meta(&mut p, &mt);
    p
}

/// Identifies a file's content cheaply (size + first and last 64 KiB) for duplicate
/// detection; collisions between different files of identical size and edges are
/// practically impossible for camera media.
pub fn content_hash(path: &Path) -> String {
    let Ok(mut f) = std::fs::File::open(path) else {
        return String::new();
    };
    let Ok(md) = f.metadata() else {
        return String::new();
    };
    let size = md.len();
    let mut h = Sha1::new();
    h.update(format!("{size}|").as_bytes());
    const CHUNK: usize = 64 << 10;
    let mut buf = vec![0u8; CHUNK];
    let n = read_full(&mut f, &mut buf);
    h.update(&buf[..n]);
    if size > 2 * CHUNK as u64 && f.seek(SeekFrom::End(-(CHUNK as i64))).is_ok() {
        let n = read_full(&mut f, &mut buf);
        h.update(&buf[..n]);
    }
    hex::encode(h.finalize())[..24].to_string()
}

fn read_full(r: &mut impl Read, buf: &mut [u8]) -> usize {
    let mut n = 0;
    while n < buf.len() {
        match r.read(&mut buf[n..]) {
            Ok(0) | Err(_) => break,
            Ok(k) => n += k,
        }
    }
    n
}

/// Go `image.DecodeConfig` for the formats it registers: (format name, width, height).
fn decode_config(path: &Path) -> Option<(&'static str, i64, i64)> {
    let mut f = std::fs::File::open(path).ok()?;
    let mut head = vec![0u8; 256 << 10];
    let n = read_full(&mut f, &mut head);
    head.truncate(n);
    let fmt = if head.starts_with(&[0xFF, 0xD8]) {
        "jpeg"
    } else if head.starts_with(b"\x89PNG\r\n\x1a\n") {
        "png"
    } else if head.len() >= 6 && &head[..4] == b"GIF8" && (head[4] == b'7' || head[4] == b'9') && head[5] == b'a' {
        "gif"
    } else if head.len() >= 15 && &head[..4] == b"RIFF" && &head[8..15] == b"WEBPVP8" {
        "webp"
    } else if head.len() >= 10 && &head[..2] == b"BM" && head[6..10] == [0, 0, 0, 0] {
        "bmp"
    } else {
        return None;
    };
    if fmt == "jpeg" {
        return match jpeg_config(&head) {
            Ok((w, h)) => Some((fmt, w, h)),
            Err(_) if head.len() == 256 << 10 => {
                let all = std::fs::read(path).ok()?;
                jpeg_config(&all).ok().map(|(w, h)| (fmt, w, h))
            }
            Err(_) => None,
        };
    }
    let ifmt = match fmt {
        "png" => image::ImageFormat::Png,
        "gif" => image::ImageFormat::Gif,
        "webp" => image::ImageFormat::WebP,
        _ => image::ImageFormat::Bmp,
    };
    let f = std::fs::File::open(path).ok()?;
    let reader = image::ImageReader::with_format(std::io::BufReader::new(f), ifmt);
    let (w, h) = reader.into_dimensions().ok()?;
    Some((fmt, w as i64, h as i64))
}

/// Native image probe (no process spawn). `None` if the format isn't handled natively.
pub fn probe_image_native(abs: &Path, ext: &str) -> Option<Probe> {
    if !matches!(ext, "jpg" | "jpeg" | "jpe" | "jfif" | "png" | "gif" | "webp" | "bmp") {
        return None;
    }
    if let Err(e) = std::fs::File::open(abs) {
        return Some(Probe::error(go_open_error(abs, &e)));
    }
    let (format, w, h) = decode_config(abs)?;
    let mut p = Probe {
        width: w,
        height: h,
        format: format.into(),
        vcodec: format.into(),
        ..Default::default()
    };
    if format == "jpeg" {
        if let Ok(f) = std::fs::File::open(abs) {
            let mt = jpeg_exif(f).map(|t| parse_tiff(&t)).unwrap_or_default();
            if (5..=8).contains(&mt.orientation) {
                // 90°/270° rotations: displayed dimensions are swapped
                std::mem::swap(&mut p.width, &mut p.height);
                p.rotation = 90;
            } else if mt.orientation == 3 || mt.orientation == 4 {
                p.rotation = 180;
            }
            apply_meta(&mut p, &mt);
        }
    }
    Some(p)
}

/// Go's `*PathError` text: "open <path>: <reason>".
pub(crate) fn go_open_error(p: &Path, e: &std::io::Error) -> String {
    let reason = match e.kind() {
        std::io::ErrorKind::NotFound => "no such file or directory".to_string(),
        std::io::ErrorKind::PermissionDenied => "permission denied".to_string(),
        _ => e.to_string(),
    };
    format!("open {}: {reason}", p.display())
}

// ---------- ffprobe ----------

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub(crate) struct FfprobeOut {
    streams: Vec<FfStream>,
    format: FfFormat,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct FfStream {
    codec_type: String,
    codec_name: String,
    width: i64,
    height: i64,
    pix_fmt: String,
    color_transfer: String,
    duration: String,
    tags: HashMap<String, serde_json::Value>,
    disposition: HashMap<String, serde_json::Value>,
    side_data_list: Vec<FfSideData>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct FfSideData {
    rotation: Option<serde_json::Value>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct FfFormat {
    format_name: String,
    duration: String,
    bit_rate: String,
    tags: HashMap<String, serde_json::Value>,
}

fn value_str(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn value_f64(v: &serde_json::Value) -> f64 {
    match v {
        serde_json::Value::Number(n) => n.as_f64().unwrap_or(0.0),
        serde_json::Value::String(s) => s.parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

fn parse_ffprobe(fo: &FfprobeOut) -> Probe {
    let mut p = Probe {
        format: fo.format.format_name.clone(),
        duration: fo.format.duration.parse().unwrap_or(0.0),
        bitrate: fo.format.bit_rate.parse().unwrap_or(0),
        ..Default::default()
    };
    for s in &fo.streams {
        match s.codec_type.as_str() {
            "video" => {
                let attached = s
                    .disposition
                    .get("attached_pic")
                    .map(|v| value_f64(v) == 1.0)
                    .unwrap_or(false);
                if !p.vcodec.is_empty() || attached {
                    continue;
                }
                p.vcodec = s.codec_name.clone();
                p.width = s.width;
                p.height = s.height;
                p.pix_fmt = s.pix_fmt.clone();
                p.hdr = s.color_transfer == "smpte2084" || s.color_transfer == "arib-std-b67";
                let mut rot = 0.0;
                if let Some(r) = s.tags.get("rotate") {
                    rot = value_str(r).parse().unwrap_or(0.0);
                }
                for sd in &s.side_data_list {
                    if let Some(r) = &sd.rotation {
                        let v = value_f64(r);
                        if v != 0.0 {
                            rot = v;
                        }
                    }
                }
                let mut r = (rot.round() as i64) % 360;
                if r < 0 {
                    r += 360;
                }
                p.rotation = r;
                if r == 90 || r == 270 {
                    std::mem::swap(&mut p.width, &mut p.height);
                }
                if p.duration == 0.0 {
                    p.duration = s.duration.parse().unwrap_or(0.0);
                }
            }
            "audio" => {
                if p.acodec.is_empty() {
                    p.acodec = s.codec_name.clone();
                }
            }
            _ => {}
        }
    }
    let tags: HashMap<String, String> = fo
        .format
        .tags
        .iter()
        .map(|(k, v)| (k.clone(), value_str(v)))
        .collect();
    video_meta(&mut p, &tags);
    p
}

/// Capture time, place and camera from container tags (QuickTime/Apple, Android, generic
/// MP4).
fn video_meta(p: &mut Probe, tags: &HashMap<String, String>) {
    if tags.is_empty() {
        return;
    }
    let get = |keys: &[&str]| -> String {
        for k in keys {
            for (tk, v) in tags {
                if tk.eq_ignore_ascii_case(k) && !v.trim().is_empty() {
                    return v.trim().to_string();
                }
            }
        }
        String::new()
    };
    let mut mt = Meta::default();
    let s = get(&["com.apple.quicktime.creationdate"]);
    if !s.is_empty() {
        if let Ok(t) = chrono::DateTime::parse_from_str(&s, "%Y-%m-%dT%H:%M:%S%z") {
            mt.taken = Some(t.timestamp_millis());
        } else if let Ok(t) = chrono::DateTime::parse_from_rfc3339(&s) {
            mt.taken = Some(t.timestamp_millis());
        }
    }
    if mt.taken.is_none() {
        let s = get(&["creation_time", "date"]);
        if !s.is_empty() {
            if let Ok(t) = chrono::DateTime::parse_from_rfc3339(&s) {
                use chrono::Datelike;
                if t.year() > 1971 {
                    mt.taken = Some(t.timestamp_millis());
                }
            }
        }
    }
    let s = get(&["com.apple.quicktime.location.ISO6709", "location", "location-eng"]);
    if !s.is_empty() {
        if let Some((la, lo)) = parse_iso6709(&s) {
            mt.lat = la;
            mt.lon = lo;
            mt.has_gps = true;
        }
    }
    mt.camera = camera_name(
        &get(&["com.apple.quicktime.make", "make", "com.android.manufacturer"]),
        &get(&["com.apple.quicktime.model", "model", "com.android.model"]),
    );
    apply_meta(p, &mt);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ffprobe_parsing() {
        let js = r#"{"streams":[{"codec_type":"video","codec_name":"hevc","width":1920,"height":1080,"pix_fmt":"yuv420p10le","color_transfer":"arib-std-b67","duration":"3.5","side_data_list":[{"rotation":-90}]},{"codec_type":"audio","codec_name":"aac"}],
          "format":{"format_name":"mov,mp4","duration":"3.533","bit_rate":"12000000","tags":{"com.apple.quicktime.make":"Apple","com.apple.quicktime.model":"iPhone 15 Pro","com.apple.quicktime.creationdate":"2024-07-14T18:22:05+0200","com.apple.quicktime.location.ISO6709":"+64.1466-021.9426+010.000/"}}}"#;
        let fo: FfprobeOut = serde_json::from_str(js).unwrap();
        let p = parse_ffprobe(&fo);
        assert_eq!((p.width, p.height, p.rotation), (1080, 1920, 270));
        assert!(p.hdr);
        assert_eq!(p.vcodec, "hevc");
        assert_eq!(p.acodec, "aac");
        assert_eq!(p.camera, "Apple iPhone 15 Pro");
        assert_eq!(p.bitrate, 12_000_000);
        assert!(p.has_gps && (p.lat - 64.1466).abs() < 1e-9);
        assert_eq!(p.taken, 1720974125000);
    }

    #[test]
    fn native_probe_png_and_jpeg() {
        let dir = tempfile::tempdir().unwrap();
        let png = dir.path().join("a.png");
        image::RgbImage::new(33, 17).save(&png).unwrap();
        let p = probe_image_native(&png, "png").unwrap();
        assert_eq!((p.width, p.height, p.format.as_str()), (33, 17, "png"));
        let jpg = dir.path().join("b.jpg");
        image::RgbImage::new(40, 30).save(&jpg).unwrap();
        let p = probe_image_native(&jpg, "jpg").unwrap();
        assert_eq!((p.width, p.height, p.vcodec.as_str()), (40, 30, "jpeg"));
        assert!(probe_image_native(&jpg, "heic").is_none());
        assert_eq!(content_hash(&jpg).len(), 24);
    }
}
