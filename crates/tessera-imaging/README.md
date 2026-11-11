# tessera-imaging

Image engine for Tessera: decoding (orientation + metadata aware), fast
previews, encoders for every writable format, metadata policies, SSIM and
perceptual ("smart") compression, and high-level single-file convert /
compress jobs. GPL-3.0-or-later.

Cargo feature `heif` (off by default) enables HEIC/HEIF decoding through the
system libheif (>= 1.17); when it is on, AVIF is also decoded through libheif
first (for `irot`/`imir` transforms and EXIF), falling back to dav1d. The crate
builds and passes its tests with `--no-default-features`.

Camera RAW decoding uses [`rawler`](https://crates.io/crates/rawler) (pure
Rust, LGPL-2.1) and colour management uses
[`moxcms`](https://crates.io/crates/moxcms) (pure Rust, BSD-3-Clause/Apache-2.0);
both are always enabled.

## Format support

| Format | Decode | Encode | Lossy (quality) | Lossless | Alpha | EXIF r/w | ICC r/w | XMP r/w |
|---|---|---|---|---|---|---|---|---|
| JPEG | yes | mozjpeg | yes | no (q100 4:4:4; JPEG→JPEG copy is lossless) | no (flattened) | yes/yes | yes/yes | yes/yes |
| PNG | yes (first APNG frame) | oxipng | palette (imagequant) | yes (8/16-bit) | yes | yes/yes (eXIf; reads ImageMagick raw profiles) | yes/yes (iCCP) | yes/yes (iTXt) |
| WebP | yes (first frame) | libwebp | yes | yes (exact) | yes | yes/yes | yes/yes | yes/yes |
| AVIF | dav1d (or libheif) | rav1e | yes | near-lossless (q100) | yes | via libheif only / no | yes / no | via libheif only / no |
| TIFF | yes | Deflate + predictor | no | yes (8/16-bit) | yes | yes/yes (Exif + GPS sub-IFDs) | yes/yes | yes/yes |
| GIF | yes (first frame) | imagequant palette | palette | ≤256 colours | 1-bit | no | no | no |
| BMP, QOI, TGA | yes | yes | no | yes | yes | no | no | no |
| ICO | yes (largest entry) | yes (≤256×256, PNG entries) | no | yes | yes | no | no | no |
| HEIC/HEIF | `heif` feature | no | – | – | yes | yes/– | yes/– | yes/– |
| JPEG XL | jxl-oxide | no | – | – | yes | yes/– | yes/– | –/– |
| Camera RAW | rawler (developed to 8-bit sRGB) | no | – | – | no | synthesised/– | –/– | –/– |

Writing metadata to a format that cannot hold it returns a warning (e.g.
"AVIF: metadata (EXIF/ICC/XMP) is not written by this version"); it never
fails the job. `ImageFormat::supports_*` and `capabilities()` expose the same
table to the UI.

## Camera RAW

`ImageFormat::Raw` ("raw", label "Camera RAW"): DNG, CR2, CR3, CRW, NEF, NRW,
ARW, SRF, SR2, RAF, ORF, RW2, RWL, RAW, PEF, SRW, ERF, MEF, MOS, MRW, KDC,
DCR, DCS, 3FR, FFF, IIQ, X3F, ARI, QTK (`RAW_EXTENSIONS`, a superset of
rawler's own list; cameras are limited to what rawler supports).

* **Detection**: `detect(bytes)` recognises DNG (TIFF with DNGVersion in
  IFD0), CR2, CR3 (ISO-BMFF `crx `), CRW, RAF, ORF, RW2/RWL, MRW and X3F by
  content. NEF/ARW/PEF/SRW/... are plain TIFF structures, so `detect` says
  TIFF and the extension decides: `detect_path(path)` (and `load`/`probe`)
  turn "TIFF content + RAW extension" into `Raw`; `load_bytes` does the same
  with `hint = Some(Raw)`.
* **`load`** develops the RAW: black/white levels, demosaic (PPG Bayer,
  bilinear X-Trans), active-area and default crop, camera white balance,
  camera matrix → linear sRGB (rawler), then dcraw-style auto-exposure (99th
  percentile to white, gain 1–4×), sRGB transfer curve and a gentle default
  tone curve (midtone lift + S-curve, close to in-camera JPEG rendering) to
  8-bit RGB, then the orientation from the RAW metadata is applied. 16–24 MP
  develops in 0.15–0.6 s, 24 MP CR3 in ~0.8 s (release, 16 threads).
* **EXIF** is synthesised as a bare TIFF block from rawler's metadata (Make,
  Model, lens, DateTimeOriginal/ModifyDate, exposure, ISO, focal length,
  serials, artist/copyright, GPS IFD, ...; Orientation = 1), so
  `read_summary`, `filter_exif` and all metadata policies work as for
  JPEGs. MakerNotes are not carried. No ICC/XMP is read from RAW files.
* **`probe`/`read_summary`** parse metadata and image geometry only (no
  pixel decompression; 2–20 ms, ~70 ms for CR2). Dimensions are the
  developed (default-crop, upright) size.
* **`make_preview`/`load_scaled_rgba`** use the embedded preview (normally
  the camera JPEG, rotated upright) when it has the frame's aspect ratio
  (±2 %) and covers the requested size; otherwise they develop. In practice
  CR2 (1536 px), NEF (full size), CR3 (full size) and RAF (1920 px) previews
  are used for 1024 px proxies (20–110 ms); ARW/PEF (640 px previews), ORF,
  Panasonic RAW and DNG (rawler exposes no preview) are developed.
* **Convert/compress** with `TargetFormat::Keep` writes JPEG (quality from
  the options; `QualityMode::Original` = q100 4:4:4) with the warning "Camera
  RAW can't be written; developed and saved as JPEG". `To { Raw }` fails.

## Colour management

`ColorMode` (`"srgb"` default, `"keep"`) on `ConvertOptions`,
`CompressOptions` (`color`, absent in old JSON = sRGB) and
`LoadOptions` for `load_with`/`load_bytes_with`.

* **sRGB (default)**: when an image carries an ICC profile that is not
  (near-)sRGB, the pixels are converted to sRGB with moxcms (perceptual
  intent; 8/16-bit integer fast paths, f32 for float images; RGB, RGBA with
  alpha preserved, grey and grey+alpha) and the profile is dropped
  (`metadata.icc = None`, `SourceImage::color_converted = true`), so
  encoders write untagged sRGB. Jobs report "Converted from <profile
  description> to sRGB". A 24 MP 8-bit conversion takes ~45 ms.
* `load`, `load_bytes`, `load_scaled_rgba` and `make_preview` (including the
  JPEG DCT-scaled path) always return sRGB, so previews and collages render
  wide-gamut photos correctly.
* **Keep**: pixels and profile are untouched; the profile is written when
  the metadata policy keeps ICC (previous behaviour).
* **sRGB detection** (`icc_is_srgb`, `MetadataSummary::icc_is_srgb`): RGB
  matrix/TRC profiles must have sRGB colorants (D50-adapted XYZ within
  ±0.004) and sRGB-like tone curves (within 1.2 % over 20 samples; gamma 2.2
  passes), regardless of their name; grey profiles need an sRGB-like TRC;
  LUT-based RGB profiles and profiles moxcms can't parse count as sRGB only
  when their description says "sRGB". Untagged images report
  `icc_is_srgb = true`.
* Profiles that can't be applied (CMYK, Lab, corrupt, or no usable
  transform) leave pixels and profile untouched in both modes.
* Paths that don't decode pixels (JPEG pass-through, lossless JPEG/PNG
  optimisation, "keep original" for `keep_if_larger`) are still taken in
  sRGB mode when the policy keeps the profile, since the output then renders
  correctly. When the policy would drop a non-sRGB profile, the image is
  re-encoded with the conversion instead (no silent colour shift).
* HEIC/AVIF via libheif: the ICC profile (`prof`/`rICC`) is used; when the
  file only has an `nclx` description with non-BT.709 primaries (e.g. Display
  P3) an equivalent ICC is synthesised from the CICP values and treated like
  an embedded profile. PQ/HLG (HDR) transfer functions are not converted.
  AVIF decoded by dav1d (without the `heif` feature) only uses an ICC
  profile; nclx-only AVIFs are treated as sRGB.

## Orientation

* `load`, `load_scaled_rgba`, `make_preview` return upright pixels; `probe`
  reports upright dimensions. EXIF orientation is applied for JPEG/PNG/WebP/
  TIFF; HEIC/AVIF (libheif) and JPEG XL apply their container transforms in
  the decoder.
* Any re-encode writes upright pixels and sets EXIF Orientation = 1 (XMP
  `tiff:Orientation` is reset too).
* Paths that do **not** re-encode (JPEG→JPEG at original quality without
  resizing, lossless JPEG/PNG optimisation, "keep original") leave pixels
  untouched and therefore keep the original orientation value; even with
  `StripAll` a minimal EXIF block holding only Orientation is written when it
  is not 1, so the photo still displays upright.

## Metadata policy

* `KeepAll`: EXIF copied byte-for-byte with Orientation (and Pixel X/Y
  dimensions, when present) patched in place, so MakerNote and the embedded
  thumbnail survive; ICC and XMP kept; JPEG pass-through copies the file
  unchanged.
* `Custom`: EXIF rebuilt from the selected groups — date/time, camera
  (make/model/lens/software/serials), exposure, location (GPS IFD), author
  (artist/copyright/description/user comment/XP tags/rating). Structural tags
  (resolution, colour space, Exif version) are kept; MakerNote (its offsets
  break when re-laid-out), the IFD1 thumbnail and unknown tags are dropped;
  PixelX/YDimension are rewritten to the output size. XMP is dropped (it can
  contain anything, including location); ICC follows `keep_icc`; JPEG
  pass-through also drops COM/IPTC/MPF segments.
* `StripAll`: no EXIF/ICC/XMP (see orientation note above), no file times.
* `keep_file_times` copies atime/mtime (ignored in `StripAll`).
* EXIF larger than one JPEG APP1 segment is rebuilt without thumbnail and
  MakerNote (warning), or dropped if still too large.

## Convert / compress semantics

* Output name `<stem><suffix>.<ext>` in `output.directory` (created) or next to
  the input. `Rename` appends " (1)", " (2)"…, reserving the name atomically
  so parallel jobs never collide; `Skip`; `Overwrite` (also in place).
  Everything is written to a temp file and renamed into place.
* `QualityMode::Original`: lossless where the target can be (PNG, WebP
  lossless, TIFF, BMP, QOI, TGA, ICO); AVIF and JPEG use q100 with a warning.
  JPEG→JPEG without resize never re-encodes (only metadata segments change; the
  image data is byte-identical). `Reduce{p}` = quality 100−p (`p = 0` is the
  same as `Original`); `Fixed{q}`. For PNG/GIF the quality drives imagequant;
  BMP/QOI/TIFF/TGA/ICO ignore it with a warning.
* HEIC/JPEG XL/camera RAW inputs with `TargetFormat::Keep` are saved as JPEG
  (PNG when transparent) with a warning. Alpha is flattened onto `background` for
  formats without alpha.
* `CompressLevel::Lossless`: JPEG input→JPEG is transcoded at the DCT
  coefficient level (Huffman re-optimisation + mozjpeg progressive scan
  optimisation via libjpeg's `jpeg_read/write_coefficients`; pixels are
  bit-identical); PNG input→PNG runs oxipng on the original; other targets use
  their lossless mode.
* `TargetSize{bytes}`: binary search of the quality in 40..=95 (encoded size
  includes metadata), then 10 % downscale steps if quality 40 is still too
  big; PNG tries lossless then palette qualities 90/75/60/40.
* `keep_if_larger` (only when no resize was requested): if the result is not
  smaller than the original (with the metadata policy applied losslessly for
  JPEG/PNG/WebP), the original is kept and reported as "Already optimized".

## Smart compression

`VisuallyLossless`, `Balanced` and `Small` search, for JPEG/WebP/AVIF, the
**lowest** quality whose SSIM against the (resized, flattened) source reaches
a threshold:

* Images ≤ 1.1 MP are evaluated whole. Larger images are evaluated on six
  512×512 full-resolution tiles picked across the distribution of local edge
  energy (busiest tile down to the 75th percentile, skipping the flattest
  quarter); the score is the mean tile SSIM. AVIF searches at rav1e speed 8.
* After the search the full image is encoded once and verified with
  `ssim()`; if it falls short the quality is raised by 2 (up to 4 times).
  `JobResult::ssim` is that full-image value.
* JPEG uses 4:4:4 for VisuallyLossless and 4:2:0 otherwise.
* PNG targets: lossless oxipng, plus an imagequant palette candidate kept
  only when smaller *and* above the threshold. WebP targets from lossless
  sources also try lossless WebP and keep the smaller.

SSIM (`quality::ssim`): luma (BT.601) after compositing on white, 8×8 box
windows at every position (O(N) running sums), standard C1/C2; images above
4 MP are box-downscaled by an integer factor first (this inflates values a
little, which is why the search itself runs on full-resolution tiles).

### Calibrated thresholds

| Level | SSIM threshold | JPEG range | WebP range | AVIF range |
|---|---|---|---|---|
| VisuallyLossless | 0.980 | 85–98 (4:4:4) | 85–100 | 75–98 |
| Balanced | 0.965 | 60–95 (4:2:0) | 65–95 | 50–92 |
| Small | 0.945 | 40–90 (4:2:0) | 45–90 | 35–85 |

Calibration set: 14 freely licensed Wikimedia Commons photos (landscapes,
people, food, animals, buildings, night; 2.6–20 MP; 7 camera/editor originals
at high quality and 7 Commons re-encodes), not committed. Medians of the mean
tile SSIM per quality (`examples/calibrate.rs curves`), 1280 px clean copies:
mozjpeg 4:4:4 q80 0.958, q85 0.967, q90 0.978, q95 0.989; WebP q80 0.977,
q90 0.988; AVIF q70 0.961, q85 0.982. The thresholds put the median photo
around mozjpeg q90 4:4:4 / q80 / q65 respectively, while per-image content
moves the choice (busy, grainy images get higher qualities; soft ones lower).

Measured with `examples/calibrate.rs levels` (release build, metadata
stripped, `keep_if_larger` off):

* JPEG → JPEG, full-resolution originals (camera/editor exports, 7 files):
  VisuallyLossless q85–92, output **26–52 % of the original (median 39 %)**;
  Balanced q74–80, 8–25 % (median 19 %); Small q55–64, 5–18 % (median 12 %).
  Chosen quality over all 14 files: median q86 / q79 / q62. Already-compressed
  web JPEGs save less (median 91 % / 63 % / 46 %) and VisuallyLossless often
  hits the q85 floor — `keep_if_larger` then keeps the original.
* From lossless 1600 px sources (size relative to the PNG): JPEG
  q85–93/q71–87/q55–79, median 25 % / 13 % / 9 %; WebP q85–92/q65–84/q45–71,
  median 15 % / 9 % / 7 %; AVIF q75–89/q56–84/q38–71, median 12 % / 8 % / 6 %.
* Time per image on a 16-thread desktop: JPEG 0.4–4.6 s for 3–20 MP
  (VisuallyLossless is the slowest), WebP ~1–2 s and AVIF ~2–5 s at 2.5 MP.

## Known limitations

* Colour management converts to sRGB only (no wide-gamut output space); HDR
  (PQ/HLG) is not tone-mapped; 16-bit/HDR sources are written as 8-bit
  except to PNG/TIFF.
* Camera RAW: decode only; a fixed default develop (no lens corrections,
  noise reduction, highlight recovery or per-camera base curves); the
  embedded preview may differ from the develop (e.g. lens-corrected framing
  for CR3). Fuji SuperCCD files rotated by the develop may probe with
  unrotated dimensions. Multi-image RAWs use the first image.
* Animated GIF/WebP/APNG: only the first frame is decoded/written.
* AVIF metadata is not written; AVIF EXIF/transforms are only read when the
  `heif` feature is enabled and libheif can decode AV1.
* `embed` into TIFF supports single-page, strip-based files (what this crate
  writes); multi-page TIFFs keep only the first page when converted.
* JPEG pass-through cannot losslessly rotate pixels, so it keeps the
  orientation tag instead (documented above).
* SSIM is luma-only; chroma fidelity at VisuallyLossless is protected by
  4:4:4 sampling for JPEG, not measured. Near-black flat areas can make SSIM
  non-monotonic in quality (the search then errs towards higher quality).
* Lossless JPEG optimisation fails on arithmetic-coded JPEGs (falls back to a
  metadata-only rewrite with a warning).
