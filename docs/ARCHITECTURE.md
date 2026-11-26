# Architecture

Tessera is one program with two front doors:

- the **desktop app** (`src-tauri`, a thin [Tauri 2](https://tauri.app) shell), and
- the **web app** (`crates/tessera-server`, the Docker image).

Both run the same engine, **`crates/tessera-core`**, which serves one HTTP API,
and the same Svelte 5 + TypeScript user interface (`src/`).

```
┌──────────────────────────────── src/ (Svelte + TS) ─────────────────────────────────┐
│ views/       Home · Library · Documents · Collage · Convert · Compress · Batch ·      │
│              System · Settings                                                        │
│ documents/   library panel, document editor, canvas, presentations, exports          │
│              (ported from Letopis; styles scoped to .lt)                              │
│ editor/ collage/ render/ model/ templates/   the collage editor (see below)          │
│ api/         fetch client: /api/t/<command>, SSE /api/events                          │
│ platform/    desktop (Tauri dialogs, OS drops, GTK print) vs browser (server file     │
│              picker, uploads, downloads)                                              │
└──────────────────────▲──────────────────────────────────────┬────────────────────────┘
                       │ HTTP: JSON · Range media · SSE        │ raw RGBA export tiles
┌──────────────────────┴────── crates/tessera-core (Rust) ────▼────────────────────────┐
│ Library & documents (ported from Letopis): config · store (SQLite index) · library  │
│   (scanner, watcher) · jobs (priority pools) · media (ffmpeg/ffprobe/libvips, VAAPI) │
│   · docs (JSON documents, history, search) · prefs · auth · tls · server (routes)    │
│ tools/ (formerly the Tauri commands): images · export · batch · batch_tools ·        │
│   projects · settings · system · transfer (uploads, downloads, zips, browse) ·       │
│   guard (the write guard) · events (SSE) · api (/api/t/…)                            │
└──────────────┬──────────────────────────────────────────────┬────────────────────────┘
               │ uses                                          │ started by
┌──────────────▼──────────── crates/tessera-imaging ─┐   ┌─────▼──────────────────────────┐
│ decode (EXIF orientation, HEIC/JXL/AVIF, RAW),     │   │ src-tauri: 127.0.0.1:<random>, │
│ encode (mozjpeg, libwebp, rav1e, oxipng,           │   │ per-launch token, window +     │
│ imagequant), metadata, ICC, SSIM, smart compress   │   │ native extras over IPC          │
└────────────────────────────────────────────────────┘   │ tessera-server: :8470 HTTP,    │
                                                         │ :8471 HTTPS+HTTP/2, password    │
                                                         └────────────────────────────────┘
```

## Desktop vs web

The desktop shell starts the engine on `127.0.0.1` with a random port and a
per-launch secret. The window opens `http://127.0.0.1:<port>/?token=…`; the
engine turns the token into an `HttpOnly; SameSite=Strict` cookie and checks
the `Host` header (against DNS rebinding). The UI, its API and all media are
then one origin, exactly as in a browser talking to the Docker server. A Tauri
capability with `remote.urls` keeps IPC available to that origin for the few
native things: file dialogs, drag-and-drop paths, GTK printing, "show in
folder", pop-out windows (speaker notes, print layouts).

`src/lib/platform` hides the differences: on the desktop, pickers return real
paths; in a browser they open an in-app browser of the server's libraries,
Outputs and Inbox, with uploads from the user's computer (matched against the
libraries first, then sent in resumable chunks), and results are downloaded
after they are saved on the server.

## Libraries and the write guard

Every folder Tessera knows is an *area*: a media library, Outputs, or Inbox.
`tools/guard.rs` is the single check for every disk access of the tools:
writing into a read-only library is refused in both modes; in server mode,
paths outside all areas are refused entirely. Libraries are read-only unless
the desktop user switches them to read-write (Settings → Libraries), or the
server's `.env` both mounts them writable and lists them in
`WRITABLE_LIBRARIES`. Documents never write media: they reference files as
`{libraryId, path}`.

## Video in the desktop window

Measured on 2026-11-23 (WebKitGTK 2.54.3 from the GNOME 51 runtime, AMD RX 9070 XT,
VA-API decoders available): single videos of every common format (H.264, HEVC
incl. iPhone MOV with timescale 600, VP9, AV1, up to 4K) play at full speed;
with 8 simultaneous 1080p streams only about half keep up; without an audio
output (no PulseAudio socket) every video runs at 0.3–0.6× speed even when
muted, because WebKit's media clock follows the audio sink. GStreamer alone
decodes the same clips 8–12× faster than real time, so the limit is WebKit's
media pipeline. Hence: the Flatpak has `--socket=pulseaudio`, and the desktop
app limits documents to 4 autoplaying videos (`documents/lib/playback.ts`).

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

**Multi-page documents** (`src/lib/model/multipage.ts`) keep the active
page's content in the usual top-level fields (`background`, `frame`,
`layout`, `elements`), so the editor, renderer and layout engines only ever
see a single page. `doc.pages` lists every page with `null` at `activePage`;
switching pages swaps contents in and out. Page size, DPI and assets are shared
by all pages. Single-page documents simply have no `pages`.

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

Canvas tainting can't happen: proxies, thumbnails and export pixels all come
from the engine's own origin (`/api/t/file`, `/api/t/load_image_rgba`).

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

Multi-page PDFs are assembled in Rust (`pdf_begin` / `pdf_add_page` /
`pdf_finish` in `crates/tessera-core/src/tools/export.rs`, using pdf-writer): each page is
rendered and JPEG-encoded as it arrives, so memory use stays at one page.

Printing renders the pages at up to 300 DPI into a hidden print-only element
and opens the print dialog. WebKitGTK ignores CSS `@page` sizes, so on Linux
`src-tauri/src/print.rs` runs WebKit's print operation itself with a GTK page
setup that matches the project's paper size and has no margins.

## Video out

`tools/video` makes MP4 slideshows (`video_start` / `video_status` /
`video_cancel`, progress as `video` events). The UI (`src/lib/video`,
`components/VideoDialog.svelte`) sends an ordered list of library photos,
clips (with in/out points) and pictures it drew itself (title cards, collage
pages). Photos are decoded by tessera-imaging (orientation, HEIC, RAW, crops)
into plain JPEGs first. ffmpeg holds every still of a filter graph decoded, so
the slideshow is encoded in chunks of six segments (zoompan, xfade, fill or
fit-over-blur) with the final encoder (VA-API, x264 or OpenH264) and the chunks
are joined by the concat demuxer without re-encoding; a chunk also renders the
crossfade into the next chunk's first segment. The music is added in that last
step. Results go to `Outputs/Videos` through the guard. The pure graph
builder is unit-tested in `tools/video/graph.rs`.

## Paint

`src/lib/paint` is a layered raster editor. A document (`doc.svelte.ts`) is
one canvas per layer plus a composite that is re-blended only inside dirty
rectangles, once per frame; the view just scales the composite. Strokes and
shapes are drawn into a scratch overlay and committed as one undo step;
pixel edits record only the 128 px tiles they changed (`lib/history.ts`, with a
memory budget), structural edits (resize, crop, rotate) record snapshots.
Selections are byte masks (`lib/mask.ts`); "Select subject" reuses the
cut-out model. Text layers keep their `TextElement` (Tessera's text styles,
drawn by `render/text.ts`) until they are painted on.

Drawings are OpenRaster files that GIMP and Krita open: the UI writes the zip
(`lib/zip.ts`, `lib/ora.ts`, layer PNGs cached per revision) and posts it to
`/api/t/save_drawing`; the engine (`tools/drawings.rs`) keeps them in
`projects/drawings/<id>.ora` with a name/size sidecar, the thumbnail copied out
of the file, and `exports.json`, which remembers which exported image came from
which drawing. Exports go to `Outputs/Drawings` through the export job API.
Other views send images to Paint with `openPaint({ libraryId, path })` /
`openPaint({ path })`, reopen a drawing with `openPaintDoc(id)` and can follow
exports with `onPaintExport` (`paint/session.svelte.ts`).

## Image engine

`crates/tessera-imaging` has its own [README](../crates/tessera-imaging/README.md)
with the format/metadata support matrix and the smart-compression calibration.
It has no Tauri or HTTP dependency and can be reused by other tools.

## Packaging

- `npm run tauri build` → deb, rpm and AppImage (`src-tauri/tauri.conf.json`).
  `scripts/prepare-bundle.mjs` adjusts the AppStream file for these packages.
- `packaging/flatpak/` → Flatpak on the GNOME runtime, bundling libheif with
  the HEVC decoder.
- `packaging/docker/` → the web app image (Dockerfile, compose file, `.env.example`);
  see [DOCKER.md](DOCKER.md).
- `.github/workflows/release.yml` builds everything when a `v*` tag is pushed,
  including the Docker image (pushed to `ghcr.io/mrnechev/tessera`).

## End-to-end smoke tests

When the `TESSERA_AUTOTEST` environment variable points at a JSON list of
steps, `src/lib/autotest.ts` replays them on start-up (create collages, apply
templates, export, convert, compress, take screenshots). This is how the
screenshots in the README are produced; see the file for the step vocabulary.
