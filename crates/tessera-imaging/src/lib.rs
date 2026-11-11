//! Tessera imaging engine: decoding (with orientation and metadata), fast
//! previews, encoding for every writable format, metadata policies, SSIM and
//! perceptual ("smart") compression, plus high-level convert/compress jobs.
//!
//! See `README.md` in this crate for the format/metadata support matrix and
//! the calibration of the smart compression levels.

mod color;
mod container;
mod decode;
mod encode;
mod error;
mod exif_util;
mod format;
#[cfg(feature = "heif")]
mod heif;
mod jobs;
mod jpegtran;
mod metadata;
mod quality;
mod raw;
mod resize;
mod smart;
mod tiff_out;

pub use color::{icc_is_srgb, ColorMode, LoadOptions};
pub use container::jpeg::image_data_fingerprint as jpeg_image_data;
pub use decode::{
    load, load_bytes, load_bytes_with, load_scaled_rgba, load_with, make_preview, probe,
    PreviewInfo, ProbeInfo, RawMetadata, SourceImage,
};
pub use encode::{encode, rgba_to_pdf, Chroma, EncodeOptions};
pub use error::{Error, Result};
pub use format::{capabilities, input_extensions, FormatCapabilities, ImageFormat, RAW_EXTENSIONS};
pub use jobs::{
    compress_file, compress_preview, convert_file, CompressLevel, CompressOptions, CompressPreview,
    ConflictPolicy, ConvertOptions, JobResult, JobStatus, OutputSpec, QualityMode, TargetFormat,
};
pub use metadata::{
    embed, filter_exif, filter_exif_sized, icc_description, read_summary, summarize_exif,
    MetadataField, MetadataMode, MetadataPolicy, MetadataSummary,
};
pub use quality::ssim;
pub use resize::{apply_resize, fit_dimensions, target_dimensions, ResizeMode};

/// Smart-compression SSIM thresholds `(visually_lossless, balanced, small)`.
pub fn smart_thresholds() -> (f64, f64, f64) {
    (
        smart::VISUALLY_LOSSLESS.threshold,
        smart::BALANCED.threshold,
        smart::SMALL.threshold,
    )
}

/// Lossless JPEG optimisation (Huffman re-optimisation, optional progressive
/// conversion). Metadata segments are not copied.
pub fn jpeg_optimize_lossless(jpeg: &[u8], progressive: bool) -> Result<Vec<u8>> {
    jpegtran::optimize(jpeg, progressive)
}

/// Internals exposed for the calibration example; not a stable API.
#[doc(hidden)]
pub mod __internal {
    use crate::{Chroma, ImageFormat, Result};
    use image::DynamicImage;

    /// Mean SSIM over the smart-search tiles at quality `q`.
    pub fn search_metric(
        reference: &DynamicImage,
        f: ImageFormat,
        q: u8,
        chroma: Chroma,
    ) -> Result<f64> {
        crate::smart::search_metric(reference, f, q, chroma)
    }

    /// Encode with the lossy settings used by smart compression.
    pub fn encode_lossy(
        img: &DynamicImage,
        f: ImageFormat,
        q: u8,
        chroma: Chroma,
    ) -> Result<Vec<u8>> {
        crate::smart::encode_lossy(img, f, q, chroma, false, [255, 255, 255])
    }
}
