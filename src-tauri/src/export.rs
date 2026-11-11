//! Collage export: the UI renders tiles and streams raw RGBA here; we assemble
//! the full image and encode it to the requested format.

use std::collections::HashMap;
use std::time::Instant;

use image::{DynamicImage, RgbaImage};
use serde::Serialize;
use tauri::ipc::{InvokeBody, Request};
use tauri::State;
use tessera_imaging::{Chroma, EncodeOptions, ImageFormat};

use crate::{err, paths, AppState, CmdResult};

/// Largest export we accept (≈ 200 MP, 800 MB of RGBA).
const MAX_PIXELS: u64 = 200_000_000;

pub struct Job {
    image: RgbaImage,
    started: Instant,
}

#[derive(Default)]
pub struct Jobs {
    next: u32,
    jobs: HashMap<u32, Job>,
    pdfs: HashMap<u32, Vec<PdfPage>>,
}

/// One page of a multi-page PDF, already JPEG-encoded to keep memory low.
pub struct PdfPage {
    jpeg: Vec<u8>,
    width: u32,
    height: u32,
    dpi: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub path: String,
    pub bytes: u64,
    pub width: u32,
    pub height: u32,
}

#[tauri::command]
pub fn export_begin(state: State<'_, AppState>, width: u32, height: u32) -> CmdResult<u32> {
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err(format!("Unsupported export size {width}×{height}"));
    }
    let mut jobs = state.exports.lock().map_err(err)?;
    // Drop abandoned jobs (e.g. the UI was reloaded mid-export).
    jobs.jobs
        .retain(|_, j| j.started.elapsed().as_secs() < 3600);
    jobs.next = jobs.next.wrapping_add(1);
    let id = jobs.next;
    jobs.jobs.insert(
        id,
        Job {
            image: RgbaImage::new(width, height),
            started: Instant::now(),
        },
    );
    Ok(id)
}

fn header<'a>(req: &'a Request<'_>, name: &str) -> CmdResult<&'a str> {
    req.headers()
        .get(name)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| format!("missing header {name}"))
}

#[tauri::command]
pub fn export_tile(state: State<'_, AppState>, request: Request<'_>) -> CmdResult<()> {
    let job: u32 = header(&request, "x-job")?.parse().map_err(err)?;
    let rect: Vec<u32> = header(&request, "x-rect")?
        .split(',')
        .map(|v| v.trim().parse::<u32>())
        .collect::<Result<_, _>>()
        .map_err(err)?;
    let [x, y, w, h] = rect[..] else {
        return Err("bad tile rectangle".into());
    };
    let InvokeBody::Raw(data) = request.body() else {
        return Err("tile must be sent as raw bytes".into());
    };
    if data.len() != (w as usize) * (h as usize) * 4 {
        return Err(format!(
            "tile has {} bytes, expected {}",
            data.len(),
            w * h * 4
        ));
    }
    let mut jobs = state.exports.lock().map_err(err)?;
    let job = jobs.jobs.get_mut(&job).ok_or("unknown export job")?;
    let (iw, ih) = job.image.dimensions();
    if x + w > iw || y + h > ih {
        return Err("tile outside the image".into());
    }
    let stride = iw as usize * 4;
    let buf: &mut [u8] = &mut job.image;
    for row in 0..h as usize {
        let src = &data[row * w as usize * 4..(row + 1) * w as usize * 4];
        let off = (y as usize + row) * stride + x as usize * 4;
        buf[off..off + w as usize * 4].copy_from_slice(src);
    }
    Ok(())
}

#[tauri::command]
pub fn export_cancel(state: State<'_, AppState>, job: u32) -> CmdResult<()> {
    state.exports.lock().map_err(err)?.jobs.remove(&job);
    Ok(())
}

#[tauri::command]
pub async fn export_finish(
    state: State<'_, AppState>,
    job: u32,
    path: String,
    format: String,
    quality: u8,
    dpi: u32,
    unique: bool,
) -> CmdResult<ExportResult> {
    let job = state
        .exports
        .lock()
        .map_err(err)?
        .jobs
        .remove(&job)
        .ok_or("unknown export job")?;
    tauri::async_runtime::spawn_blocking(move || {
        let img = job.image;
        let (width, height) = img.dimensions();
        let bytes = if format == "pdf" {
            tessera_imaging::rgba_to_pdf(&img, dpi.max(1), quality.clamp(1, 100)).map_err(err)?
        } else {
            let fmt = ImageFormat::from_extension(&format)
                .ok_or_else(|| format!("Unsupported format {format}"))?;
            let opts = EncodeOptions {
                format: fmt,
                quality: quality.clamp(1, 100),
                lossless: matches!(fmt, ImageFormat::Png | ImageFormat::Tiff),
                progressive: true,
                chroma: Chroma::Auto,
                // Exports can be large; favour reasonable speed.
                effort: if matches!(fmt, ImageFormat::Avif) {
                    35
                } else {
                    50
                },
                dpi: Some(dpi.max(1)),
                background: [255, 255, 255],
            };
            tessera_imaging::encode(&DynamicImage::ImageRgba8(img), &opts).map_err(err)?
        };
        let mut target = paths::expand(&path);
        if unique {
            target = paths::unique_path(&target);
        }
        paths::write_atomic(&target, &bytes)
            .map_err(|e| format!("Could not write {}: {e}", target.display()))?;
        Ok(ExportResult {
            path: target.to_string_lossy().into_owned(),
            bytes: bytes.len() as u64,
            width,
            height,
        })
    })
    .await
    .map_err(err)?
}

/* ------------------------------------------------------------ multi-page PDF */

#[tauri::command]
pub fn pdf_begin(state: State<'_, AppState>) -> CmdResult<u32> {
    let mut jobs = state.exports.lock().map_err(err)?;
    jobs.next = jobs.next.wrapping_add(1);
    let id = jobs.next;
    jobs.pdfs.insert(id, Vec::new());
    Ok(id)
}

/// Move a finished export job (all tiles received) into a PDF as the next page.
#[tauri::command]
pub async fn pdf_add_page(
    state: State<'_, AppState>,
    pdf: u32,
    job: u32,
    quality: u8,
    dpi: u32,
) -> CmdResult<()> {
    let job = state
        .exports
        .lock()
        .map_err(err)?
        .jobs
        .remove(&job)
        .ok_or("unknown export job")?;
    let page = tauri::async_runtime::spawn_blocking(move || -> CmdResult<PdfPage> {
        let (width, height) = job.image.dimensions();
        let opts = EncodeOptions {
            format: ImageFormat::Jpeg,
            quality: quality.clamp(1, 100),
            lossless: false,
            progressive: false,
            chroma: Chroma::Auto,
            effort: 50,
            dpi: Some(dpi.max(1)),
            background: [255, 255, 255],
        };
        let jpeg =
            tessera_imaging::encode(&DynamicImage::ImageRgba8(job.image), &opts).map_err(err)?;
        Ok(PdfPage {
            jpeg,
            width,
            height,
            dpi: dpi.max(1),
        })
    })
    .await
    .map_err(err)??;
    state
        .exports
        .lock()
        .map_err(err)?
        .pdfs
        .get_mut(&pdf)
        .ok_or("unknown PDF")?
        .push(page);
    Ok(())
}

#[tauri::command]
pub fn pdf_cancel(state: State<'_, AppState>, pdf: u32) -> CmdResult<()> {
    state.exports.lock().map_err(err)?.pdfs.remove(&pdf);
    Ok(())
}

/// Assemble a PDF with one image per page, each page sized to its pixels at its DPI.
pub fn build_pdf(pages: &[PdfPage]) -> Vec<u8> {
    use pdf_writer::{Content, Filter, Finish, Name, Pdf, Rect, Ref};
    let mut pdf = Pdf::new();
    let catalog_id = Ref::new(1);
    let tree_id = Ref::new(2);
    let info_id = Ref::new(3);
    let page_ids: Vec<Ref> = (0..pages.len())
        .map(|i| Ref::new(4 + i as i32 * 3))
        .collect();
    pdf.catalog(catalog_id).pages(tree_id);
    pdf.pages(tree_id)
        .kids(page_ids.iter().copied())
        .count(pages.len() as i32);
    for (i, p) in pages.iter().enumerate() {
        let page_id = page_ids[i];
        let image_id = Ref::new(page_id.get() + 1);
        let content_id = Ref::new(page_id.get() + 2);
        let name = Name(b"Im1");
        let pw = p.width as f32 * 72.0 / p.dpi as f32;
        let ph = p.height as f32 * 72.0 / p.dpi as f32;
        let mut page = pdf.page(page_id);
        page.media_box(Rect::new(0.0, 0.0, pw, ph));
        page.parent(tree_id);
        page.contents(content_id);
        page.resources().x_objects().pair(name, image_id);
        page.finish();
        let mut img = pdf.image_xobject(image_id, &p.jpeg);
        img.filter(Filter::DctDecode);
        img.width(p.width as i32);
        img.height(p.height as i32);
        img.color_space().device_rgb();
        img.bits_per_component(8);
        img.finish();
        let mut content = Content::new();
        content.save_state();
        content.transform([pw, 0.0, 0.0, ph, 0.0, 0.0]);
        content.x_object(name);
        content.restore_state();
        pdf.stream(content_id, &content.finish());
    }
    pdf.document_info(info_id)
        .producer(pdf_writer::TextStr("Tessera"));
    pdf.finish()
}

#[tauri::command]
pub async fn pdf_finish(
    state: State<'_, AppState>,
    pdf: u32,
    path: String,
    unique: bool,
) -> CmdResult<ExportResult> {
    let pages = state
        .exports
        .lock()
        .map_err(err)?
        .pdfs
        .remove(&pdf)
        .ok_or("unknown PDF")?;
    if pages.is_empty() {
        return Err("The PDF has no pages".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let bytes = build_pdf(&pages);
        let mut target = paths::expand(&path);
        if unique {
            target = paths::unique_path(&target);
        }
        paths::write_atomic(&target, &bytes)
            .map_err(|e| format!("Could not write {}: {e}", target.display()))?;
        Ok(ExportResult {
            path: target.to_string_lossy().into_owned(),
            bytes: bytes.len() as u64,
            width: pages[0].width,
            height: pages[0].height,
        })
    })
    .await
    .map_err(err)?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multi_page_pdf_has_every_page() {
        let img = DynamicImage::ImageRgba8(RgbaImage::from_pixel(
            30,
            20,
            image::Rgba([200, 30, 30, 255]),
        ));
        let opts = EncodeOptions {
            format: ImageFormat::Jpeg,
            quality: 80,
            ..Default::default()
        };
        let jpeg = tessera_imaging::encode(&img, &opts).unwrap();
        let pages: Vec<PdfPage> = (0..3)
            .map(|_| PdfPage {
                jpeg: jpeg.clone(),
                width: 30,
                height: 20,
                dpi: 72,
            })
            .collect();
        let pdf = build_pdf(&pages);
        let text = String::from_utf8_lossy(&pdf);
        assert!(text.starts_with("%PDF"));
        assert!(text.contains("/Count 3"));
        assert_eq!(text.matches("/MediaBox [0 0 30 20]").count(), 3);
    }
}
