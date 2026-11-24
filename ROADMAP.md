# Tessera roadmap

Status: ✅ done · 🚧 in progress · 📋 planned · 💡 idea (not decided) · ⏸️ not now

Sizes: **S** = an evening, **M** = a few days, **L** = a week or more.

Written on 2026-11-24, right after **0.6.0** ("Letopis joins Tessera"). Two things fit
together in 0.6.0 that used to be separate apps, and both still show it. The plan below
first makes them feel like **one** app, then makes them **work together** ("build once,
use everywhere"), then grows the server side and the library.

## Ground rules

- **Libraries are read-only unless the user decides otherwise**, per folder (Settings →
  Libraries on the desktop, `.env` on a server). Documents only reference media; tools save
  to Outputs. New features follow this rule.
- **Every feature works in both the desktop app and the web app**, unless it physically
  can't (and then the README's *Known limitations* says so).
- Breaking the document or project formats needs an automatic migration.

## Done

| | Release | What |
|---|---|---|
| ✅ | 0.1–0.5.1 | Collage studio, templates, stickers, RAW/colour management, convert/compress/batch, multi-page projects, photo books, calendars, printing |
| ✅ | 0.6.0 | Letopis merged in: library, documents, canvas, presentations, exports; Tessera as a Docker web app; write guard; desktop library setup |
| ✅ | 0.7.0 | One app: home, headers, Ctrl+K, library in the collage editor, settings; Stories (diary, calendar, on this day, story starter, live collages); one photo toolkit everywhere; cut-outs; memes; MP4 slideshows; `:edge` image |

---

## 0.6.x — Feel like one app (quality of life)

The seams of the merge, visible today.

| | Item | Why / what | Size |
|---|---|---|---|
| ✅ | **One home screen** | Home shows *Recent collages* only; documents live behind their own start page. Merge them: one "Recent" list (collages and documents, with thumbnails), one "New…" menu (collage, photo book, calendar, document, canvas, from template), templates of both kinds. | M |
| ✅ | **One header style** | The Documents page and the document editor still carry Letopis's top bar (its own logo, a home button, a System button, a theme toggle) next to Tessera's rail. Drop the duplicates, use Tessera's typography and button style in the documents views, keep Import/Backup in a "…" menu. | M |
| ✅ | **Command palette everywhere** | <kbd>Ctrl</kbd>+<kbd>K</kbd> only knows document actions. Add Tessera's: new collage/photo book/calendar, open a recent project, go to any view, convert/compress the selection, open Settings, search library files. | S |
| ✅ | **Library inside the collage editor** | The collage editor's Photos panel still adds photos through a file dialog. Add the library panel there (as in the document editor): search, filters, drag photos onto the page. Keep "Add from computer" for files outside the library. | M |
| 📋 | **A better server file picker** (web) | Show folder and library names instead of raw paths (`/libraries/nextcloud` → *Nextcloud*), thumbnails for images, a search box, remember the last folder per purpose, and shift-click range selection. | M |
| 📋 | **Upload progress** (web) | A progress panel for big uploads (per file and total, speed, cancel, resume after a dropped connection) instead of a line of text. | S |
| 📋 | **Outputs tidy-up** | Save results into dated subfolders (`Outputs/2026-11/…`) or per tool; "Open in Library" after saving; a size meter and "delete old results" for Outputs on the System page (Outputs only, never libraries). | S |
| ✅ | **Settings in one place** | Letopis's preferences (transitions, hidden folders, favourites, blur, library tile size) are scattered in menus. Give them a *Documents & library* section in Settings, next to Saving and Libraries. | S |
| 📋 | **Per-browser video support** | The server decides "can this video play?" from a fixed list (`PLAYABLE_VIDEO_CODECS`). Ask each browser/window what it can play (`canPlayType`) and serve the original or a proxy per client. Fixes HEVC in browsers that can play it, and the desktop window on systems with fewer codecs. | M |
| 📋 | **ffmpeg in the Flatpak** | Build a small ffmpeg (decoders + VP9/AV1 encoders) into the Flatpak so library video thumbnails, previews and proxies work there too. | M |
| 📋 | **Web-app tests in CI** | Run a headless Chromium (Playwright) against the Docker image in CI: login, library, documents, collage via the server picker, save + download, tools. The desktop already has its autotest. | M |
| 📋 | **Small fixes found during the merge** | Document print layouts on the desktop (paper size), keyboard focus in the server picker, the System page's "browser video support" list for the desktop window, typed errors instead of strings in the API. | S |

## 0.7 — Build once, use everywhere

What the merge was for: features written once, used by collages and documents alike.

| | Item | Why / what | Size |
|---|---|---|---|
| ✅ | **Collages from the library** | Select photos in the Library → *Make a collage / photo book / calendar* with the layout engines and templates. Also: *Add to a document* from the collage editor, *Make a collage* from a document section. | S |
| ✅ | **A collage inside a document** | A section (or canvas card, or slide) can be a Tessera collage page: edit it in the collage editor, it renders live in the document and the presentation, and exports to PDF/PPTX. | L |
| ✅ | **One photo editing toolkit** | Crop, rotate/flip, adjustments (brightness, contrast, warmth…), filters and frames from the collage editor, usable on every photo in a document and on the canvas. Non-destructive (stored as settings, originals untouched). Documents' crop dialog becomes this toolkit. *Done in 0.7.0, plus hide areas, duotone, colour pop, cut-outs and Edit a copy.* | L |
| 📋 | **One image engine** | Library thumbnails, previews and RAW/HEIC decoding move to `tessera-imaging` (colour-managed, real RAW development, no libvips needed); the collage editor uses the library's cached previews instead of making its own. One cache, less disk, less work. | L |
| 📋 | **Tools on library selections** | Convert / compress / batch photos picked in the Library; results land in Outputs and can be inserted into the open document in one click. | M |
| 📋 | **One project store** | Collages get what documents have: version history, restore, conflict detection when the same project is open in two places (two browser tabs, two computers), trash, and backup in the same zip. | M |
| 📋 | **Shared interaction layer** | Selection, handles, snapping guides, align/distribute, groups, lock, undo history and clipboard shared by the canvas and the collage editor (today each has its own). Copy elements between them. | L |

## 0.8 — Tessera on a server

For the Docker web app (the part used every day on slavlab).

| | Item | Why / what | Size |
|---|---|---|---|
| 📋 | **Install as an app** (PWA) | Add to the desktop/dock from Vivaldi/Chrome: own window and icon, offline start page. | S |
| 📋 | **Phones and tablets** | A touch-friendly layout for the Library, documents (reading and light editing) and presenting; today only the phone remote is built for phones. | L |
| 📋 | **Read-only share links** | Share one document, presentation or collage with a link (optional expiry and password), without giving out the main password. | M |
| 📋 | **Backups built in** | A nightly backup of documents, projects and settings into a folder of choice; "last backup" and "restore" on the System page. | S |
| 📋 | **Update notice for servers** | The System page says when a new release is out (with the changelog) and shows the two commands to update. | S |
| 📋 | **Homepage widget** | A small metrics endpoint (files, documents, queues, cache, GPU) for a Homepage tile. | S |
| 📋 | **Real HTTPS certificates** | Optional certificates via Tailscale (`*.ts.net`) or ACME, so no private CA has to be trusted. | S |
| 📋 | **Smaller image** | Today ~260 MB compressed: trim ffmpeg/Mesa to what's used, split the GPU drivers into an optional variant. | M |
| 📋 | **Share the desktop app on the network** | A Settings switch (off by default; turns on password login) so phones can use the presentation remote with the desktop app too. | M |
| 💡 | **User accounts** | Several people with their own documents and projects, shared libraries. Only if the single password stops being enough. | L |

## 0.9 — A smarter library

| | Item | Why / what | Size |
|---|---|---|---|
| 📋 | **More libraries** | An Immich library (read its folders the same way), and more slots in the compose file. | S |
| 📋 | **Places map** | Photos and videos on a map (GPS is already indexed); pick a region → collage or document. | M |
| 📋 | **Smart collections** | Saved searches ("2025 · Canon · videos > 1 min"), On this day, and per-trip groups made from dates and places. | M |
| ✅ | **Story starter** | From a trip or a date range, Tessera drafts a document (sections by day, best photos, captions with places and dates) for you to edit. | M |
| 💡 | **Face and object search** | Local models only (nothing leaves the machine): "people", "beach", "dog". | L |
| 💡 | **Unreadable files report** | List files the server can't read (permissions) with a hint how to fix them. | S |

## 1.0 — Stable

| | Item | Size |
|---|---|---|
| 📋 | Leave alpha: settle the formats, the HTTP API and the folder layout, with migrations for everything since 0.6 | M |
| 📋 | Translations (Bulgarian first), with dates and numbers in the user's locale | M |
| 📋 | Flathub listing (vendored npm/cargo sources) | M |
| 📋 | In-place updates for the AppImage | M |
| 📋 | User guide (desktop and web), with screenshots of the merged app | M |

## Later

| | Item | Size |
|---|---|---|
| 💡 | Windows and macOS builds of the desktop app | L |
| 💡 | A Chromium-based desktop window (Tauri's CEF runtime, once stable) to lift the desktop video limits | M |
| 💡 | Plugins: layout engines and templates as installable packs | L |
