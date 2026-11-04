//! Low-level container parsing for the formats whose metadata we read and
//! write ourselves (JPEG, PNG, WebP).

pub(crate) mod jpeg;
pub(crate) mod png;
pub(crate) mod webp;

/// Strip an optional 4-byte big-endian "TIFF header offset" prefix (as used in
/// HEIF / JPEG XL Exif boxes) or an "Exif\0\0" prefix, returning the bare TIFF
/// structure.
pub(crate) fn normalize_exif(raw: &[u8]) -> Option<Vec<u8>> {
    let is_tiff = |b: &[u8]| b.starts_with(b"II*\0") || b.starts_with(b"MM\0*");
    if is_tiff(raw) {
        return Some(raw.to_vec());
    }
    if let Some(rest) = raw.strip_prefix(b"Exif\0\0") {
        return is_tiff(rest).then(|| rest.to_vec());
    }
    if raw.len() > 4 {
        let off = u32::from_be_bytes([raw[0], raw[1], raw[2], raw[3]]) as usize;
        if let Some(rest) = raw.get(4 + off..) {
            if is_tiff(rest) {
                return Some(rest.to_vec());
            }
            if let Some(r) = rest.strip_prefix(b"Exif\0\0") {
                if is_tiff(r) {
                    return Some(r.to_vec());
                }
            }
        }
    }
    // Last resort: search the first bytes for a TIFF header.
    (0..raw.len().saturating_sub(4).min(32))
        .find(|&i| is_tiff(&raw[i..]))
        .map(|i| raw[i..].to_vec())
}
