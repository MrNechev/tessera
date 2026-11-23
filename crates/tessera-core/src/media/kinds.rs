//! Media kinds by file extension, browser-native image formats and MIME types.

pub const KIND_IMAGE: &str = "image";
pub const KIND_GIF: &str = "gif";
pub const KIND_VIDEO: &str = "video";
pub const KIND_AUDIO: &str = "audio";

fn ext_kind(ext: &str) -> &'static str {
    match ext {
        "jpg" | "jpeg" | "jpe" | "jfif" | "png" | "webp" | "avif" | "heic" | "heif" | "tif"
        | "tiff" | "bmp" | "jxl" => KIND_IMAGE,
        "gif" => KIND_GIF,
        "mp4" | "m4v" | "mov" | "mkv" | "webm" | "avi" | "wmv" | "flv" | "mpg" | "mpeg" | "ts"
        | "m2ts" | "mts" | "3gp" | "ogv" => KIND_VIDEO,
        // camera RAW (embedded preview) and Photoshop (flattened composite)
        "cr2" | "cr3" | "nef" | "nrw" | "arw" | "srf" | "sr2" | "dng" | "orf" | "rw2" | "raf"
        | "pef" | "srw" | "x3f" | "3fr" | "iiq" | "psd" | "psb" => KIND_IMAGE,
        // audio (background music for presentations)
        "mp3" | "m4a" | "aac" | "flac" | "ogg" | "oga" | "opus" | "wav" => KIND_AUDIO,
        _ => "",
    }
}

/// The media kind for a file name ("" if it isn't media) and its lower-case extension.
pub fn kind_of(name: &str) -> (&'static str, String) {
    let ext = crate::util::path_ext(name)
        .trim_start_matches('.')
        .to_lowercase();
    (ext_kind(&ext), ext)
}

/// Whether a browser can show the original image file directly.
pub fn browser_image(ext: &str) -> bool {
    matches!(
        ext,
        "jpg" | "jpeg" | "jpe" | "jfif" | "png" | "webp" | "avif" | "gif" | "bmp"
    )
}

/// The content type for an extension.
pub fn mime(ext: &str) -> &'static str {
    match ext {
        "jpg" | "jpeg" | "jpe" | "jfif" => "image/jpeg",
        "png" => "image/png",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "heic" => "image/heic",
        "heif" => "image/heif",
        "tif" | "tiff" => "image/tiff",
        "bmp" => "image/bmp",
        "jxl" => "image/jxl",
        "gif" => "image/gif",
        "mp4" | "m4v" => "video/mp4",
        "mov" => "video/quicktime",
        "mkv" => "video/x-matroska",
        "webm" => "video/webm",
        "avi" => "video/x-msvideo",
        "wmv" => "video/x-ms-wmv",
        "flv" => "video/x-flv",
        "mpg" | "mpeg" => "video/mpeg",
        "ts" | "m2ts" | "mts" => "video/mp2t",
        "3gp" => "video/3gpp",
        "ogv" => "video/ogg",
        "mp3" => "audio/mpeg",
        "m4a" => "audio/mp4",
        "aac" => "audio/aac",
        "flac" => "audio/flac",
        "ogg" | "oga" | "opus" => "audio/ogg",
        "wav" => "audio/wav",
        "psd" => "image/vnd.adobe.photoshop",
        "dng" => "image/x-adobe-dng",
        _ => "application/octet-stream",
    }
}

/// Camera RAW formats (shown via their embedded JPEG preview).
pub fn is_raw(ext: &str) -> bool {
    matches!(
        ext,
        "cr2" | "cr3" | "nef" | "nrw" | "arw" | "srf" | "sr2" | "dng" | "orf" | "rw2" | "raf"
            | "pef" | "srw" | "x3f" | "3fr" | "iiq"
    )
}

/// Photoshop documents are thumbnailed by libvips (flattened composite).
pub fn is_psd(ext: &str) -> bool {
    ext == "psd" || ext == "psb"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds() {
        for (name, want) in [
            ("a.JPG", KIND_IMAGE),
            ("b.mov", KIND_VIDEO),
            ("c.gif", KIND_GIF),
            ("d.txt", ""),
            ("e", ""),
            ("f.HEIC", KIND_IMAGE),
        ] {
            assert_eq!(kind_of(name).0, want, "{name}");
        }
    }
}
