# Architecture

Tessera is a [Tauri 2](https://tauri.app) application: a Svelte 5 + TypeScript
user interface running in the system WebView (WebKitGTK on Linux), and a Rust
backend that does all file and pixel work.

```
┌──────────────────────────── src/ (Svelte + TS) ────────────────────────────┐
│ views/        Home · Editor · Convert · Compress · Settings                │
│ components/   editor canvas, panels, inspector, UI kit                      │
│ editor/       editor state + undo history, session (new/open/save), export  │
│ collage/      auto-layout engines (pure functions, unit tested)            │
│ render/       THE renderer: document → Canvas2D (preview and export)       │
│ model/        document types, factories, page presets                       │
│ templates/    template DSL + catalogue                                      │
└───────────────▲───────────────────────────────────────────┬────────────────┘
                │ invoke() / events                          │ raw RGBA tiles
┌───────────────┴──────────── src-tauri/ (Rust) ────────────▼────────────────┐
│ images.rs   import: folder walk, probe, cached proxies & thumbnails         │
│ export.rs   assemble tiles → encode (JPEG/PNG/WebP/AVIF/TIFF/PDF)           │
│ batch.rs    convert / compress jobs (rayon), progress, cancel, previews     │
│ projects.rs settings.rs system.rs (updates, cache, file manager)           │
└───────────────┬───────────────────────────────────────────────────────────┘
                │
┌───────────────▼──── crates/tessera-imaging (Rust library) ─────────────────┐
│ decode (EXIF orientation, HEIC/JXL/AVIF), encode (mozjpeg, libwebp, rav1e,  │
│ oxipng, imagequant), metadata policies, SSIM, smart compression, PDF        │
└────────────────────────────────────────────────────────────────────────────┘
```

## The collage document

A collage is a plain JSON document (`src/lib/model/types.ts`). Geometry is in
*page units* — the pixels of the final image at 100 % — so a document is
resolution independent only through `page.dpi` and the export scale.

- `elements` are drawn bottom to top: `photo`, `text`, `sticker`, `shape`.
- A photo shows an *asset* (an imported file, referenced by path in
  `doc.assets`) through a `mask`, an optional `frame` (polaroid, film…),
  `border`, `shadow` and colour `adjust`ments. `crop` stores the focal point
  (`x`, `y` in 0..1, like PhotoCollage's offsets) and `zoom`.
- Photos with `slot: true` belong to the **auto-layout** described by
  `doc.layout` (`engine`, `seed`, `spacing`, `margin`, optional `region`…).

Documents are immutable: every edit goes through `editor.update()` (Immer),
which makes undo/redo a list of snapshots. Drags are wrapped in
`begin()`/`end()` transactions, and slider changes coalesce by key, so one
gesture is one undo step.

## One renderer, preview = export

`render/renderer.ts` draws a document onto any Canvas2D context. The editor
calls it with low-resolution proxies at screen scale; the exporter calls it,
tile by tile (2048 px), with full-quality photos loaded through
`load_image_rgba`, and streams the raw pixels to Rust, which encodes the final
file. Text, patterns, shadows and frames are therefore identical on screen and
in the saved image.

Canvas tainting is avoided by loading proxies through Tauri's asset protocol
(which sends CORS headers) with `crossOrigin = "anonymous"`, and export pixels
over IPC.

## Layout engines

`collage/engines/*` are pure functions `(items, area, gap, rng, settings) →
placements`, tested in `collage/layout.test.ts`:

| Engine | Idea |
| --- | --- |
| `columns` (Classic) | Port of PhotoCollage: fill the shortest column, wide photos may span two columns, remove bottom holes, equalise column heights. |
| `mosaic` | Random slicing floorplans (binary trees of H/V splits). A subtree's aspect ratio is exact, so we sample trees and keep the one matching the page — near-zero cropping. |
| `rows` | Justified rows. |
| `grid` | Uniform cells, best cols × rows for the photos' median aspect. |
| `featured` | A hero photo plus a mosaic of the rest. |
| `shape` | Square cells whose centres fall inside a heart/circle/star/… outline; photos repeat to fill. |
| `scatter` | Jittered grid of rotated polaroids. |
| `ring` | Centre circle and concentric rings of circles. |

`relayout()` re-runs an engine and keeps each managed photo's id and styling;
**Regenerate** just changes the seed (and the order).

## Image engine

`crates/tessera-imaging` has its own [README](../crates/tessera-imaging/README.md)
with the format/metadata support matrix and the smart-compression calibration.
It has no Tauri dependency and can be reused by other tools.

## Packaging

- `npm run tauri build` → deb, rpm and AppImage (`src-tauri/tauri.conf.json`).
  `scripts/prepare-bundle.mjs` adjusts the AppStream file for these packages.
- `packaging/flatpak/` → Flatpak on the GNOME runtime, bundling libheif with
  the HEVC decoder.
- `.github/workflows/release.yml` builds everything when a `v*` tag is pushed.

## End-to-end smoke tests

When the `TESSERA_AUTOTEST` environment variable points at a JSON list of
steps, `src/lib/autotest.ts` replays them on start-up (create collages, apply
templates, export, convert, compress, take screenshots). This is how the
screenshots in the README are produced; see the file for the step vocabulary.
