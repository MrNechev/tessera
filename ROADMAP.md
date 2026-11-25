# Tessera roadmap

Status: ✅ done · 🚧 in progress · 📋 planned · 💡 idea (not decided) · ⏸️ not now

Sizes: **S** = an evening, **M** = a few days, **L** = a week or more.

Updated on 2026-11-25, after **0.7.0** ("One app, one toolkit"). Collages and stories
now share one home screen, one command palette, one library and one photo toolkit. Next:
finish the loose ends of 0.7, then the open 0.6.x items, then the server side and the
library. Released work is described in detail in [CHANGELOG.md](CHANGELOG.md).

## Ground rules

- **Libraries are read-only unless the user decides otherwise**, per folder (Settings →
  Libraries on the desktop, `.env` on a server). Stories only reference media; tools save to
  Outputs. New features follow this rule.
- **Every feature works in both the desktop app and the web app**, unless it physically
  can't (and then the README's *Known limitations* says so).
- Breaking the story or collage formats needs an automatic migration.

## Done

| | Release | What |
|---|---|---|
| ✅ | 0.1–0.5.1 | Collage studio, templates, stickers, RAW/colour management, convert/compress/batch, multi-page projects, photo books, calendars, printing |
| ✅ | 0.6.0 | Letopis merged in: library, documents, canvas, presentations, exports; Tessera as a Docker web app; write guard; desktop library setup |
| ✅ | 0.7.0 | One app: home, headers, Ctrl+K, library in the collage editor, settings; Stories (diary, calendar, on this day, story starter, live collages); one photo toolkit everywhere; cut-outs; memes; MP4 slideshows; `:edge` image |
| ✅ | 0.7.1 | Polish: Library folders on the left and easier multi-select, a photo side panel and media size slider in stories, canvas stacking order, typed text sizes, middle-click paste fix |

---

## 0.7.x — Finish what 0.7 started

Gaps found while building and testing 0.7.0.

| | Item | Why / what | Size |
|---|---|---|---|
| 📋 | **Check 0.7 in the desktop window** | The new features were tested in Chromium against the Docker image only. Check them in WebKitGTK: cut-outs (WASM in a worker), photo masks and frames (container units), the Edit photo dialog, MP4 export. | S |
| 📋 | **Cut-outs on real photos** | Tested on generated images only. Try people, pets and objects from a real library; tune the mask threshold and feathering. | S |
| 📋 | **Cleaner cut-out edges** | Edges are soft around hair and fur. A matting model (or a refine step), plus an erase/restore brush to fix the mask by hand. | M |
| 📋 | **Sound from video clips** | MP4 slideshows play only the music track; clips are silent. Mix each clip's own sound in, with a volume per clip and music ducking. | M |
| 📋 | **Collages in the offline package** | The offline package shows a live collage section as text. Render it to an image when packaging. | S |
| 📋 | **Story covers from collage templates** | Make a story's cover from a collage template, filled with the story's photos. | S |
| 📋 | **Tools on library selections, finished** | Convert, compress and batch already work on Library selections. Missing: insert the results into the open story or collage in one click. | S |

## Still open from 0.6.x / 0.7

| | Item | Why / what | Size |
|---|---|---|---|
| 📋 | **A better server file picker** (web) | Show folder and library names instead of raw paths (`/libraries/nextcloud` → *Nextcloud*), thumbnails for images, a search box, remember the last folder per purpose, and shift-click range selection. | M |
| 📋 | **Upload progress** (web) | A progress panel for big uploads (per file and total, speed, cancel, resume after a dropped connection) instead of a line of text. | S |
| 📋 | **Outputs tidy-up** | Save results into dated subfolders (`Outputs/2026-11/…`) or per tool; "Open in Library" after saving; a size meter and "delete old results" for Outputs on the System page (Outputs only, never libraries). | S |
| 📋 | **Per-browser video support** | The server decides "can this video play?" from a fixed list (`PLAYABLE_VIDEO_CODECS`). Ask each browser/window what it can play (`canPlayType`) and serve the original or a proxy per client. Fixes HEVC in browsers that can play it, and the desktop window on systems with fewer codecs. | M |
| 📋 | **ffmpeg in the Flatpak** | Build a small ffmpeg (decoders + H.264/VP9/AV1 encoders) into the Flatpak so library video thumbnails, previews, proxies and MP4 slideshows work there too. | M |
| 📋 | **Web-app tests in CI** | The 0.7.0 Playwright suites (login, library, stories, collages, photo toolkit, cut-outs, video export) run by hand against a throwaway container. Move them into the repo and run them in CI against the Docker image. | M |
| 📋 | **Small fixes found during the merge** | Story print layouts on the desktop (paper size), keyboard focus in the server picker, the System page's "browser video support" list for the desktop window, typed errors instead of strings in the API. | S |
| 📋 | **One image engine** | Library thumbnails, previews and RAW/HEIC decoding move to `tessera-imaging` (colour-managed, real RAW development, no libvips needed); the collage editor uses the library's cached previews instead of making its own. One cache, less disk, less work. | L |
| 📋 | **One project store** | Collages get what stories have: version history, restore, conflict detection when the same project is open in two places (two browser tabs, two computers), trash, and backup in the same zip. | M |
| 📋 | **Shared interaction layer** | Selection, handles, snapping guides, align/distribute, groups, lock, undo history and clipboard shared by the canvas and the collage editor (today each has its own). Copy elements between them. | L |

## 0.8 — Tessera on a server

For the Docker web app (the part used every day on slavlab).

| | Item | Why / what | Size |
|---|---|---|---|
| 📋 | **Install as an app** (PWA) | Add to the desktop/dock from Vivaldi/Chrome: own window and icon, offline start page. | S |
| 📋 | **Phones and tablets** | A touch-friendly layout for the Library, stories (reading and light editing) and presenting; today only the phone remote is built for phones. | L |
| 📋 | **Read-only share links** | Share one story, presentation or collage with a link (optional expiry and password), without giving out the main password. | M |
| 📋 | **Backups built in** | A nightly backup of stories, projects and settings into a folder of choice; "last backup" and "restore" on the System page. | S |
| 📋 | **Update notice for servers** | *Check now* already finds new releases. Add the changelog, show the update command (`docker compose pull && docker compose up -d`), and tell `:edge` users when `main` has moved on. | S |
| 📋 | **Homepage widget** | A small metrics endpoint (files, stories, queues, cache, GPU) for a Homepage tile. | S |
| 📋 | **Real HTTPS certificates** | Optional certificates via Tailscale (`*.ts.net`) or ACME, so no private CA has to be trusted. | S |
| 📋 | **Smaller image** | Today ~260 MB, plus the cut-out model compressed: trim ffmpeg/Mesa to what's used, split the GPU drivers into an optional variant. | M |
| 📋 | **Share the desktop app on the network** | A Settings switch (off by default; turns on password login) so phones can use the presentation remote with the desktop app too. | M |
| 💡 | **User accounts** | Several people with their own stories and projects, shared libraries. Only if the single password stops being enough. | L |

## 0.9 — A smarter library

| | Item | Why / what | Size |
|---|---|---|---|
| 📋 | **More libraries** | An Immich library (read its folders the same way), and more slots in the compose file. | S |
| 📋 | **Places map** | Photos and videos on a map (GPS is already indexed); pick a region → collage or story. | M |
| 📋 | **Smart collections** | Saved searches ("2025 · Canon · videos > 1 min") and per-trip groups made from dates and places. | M |
| ✅ | **Story starter** (0.7.0) | From a trip or a date range, Tessera drafts a story (sections by day, best photos, captions with places and dates) for you to edit. | M |
| 💡 | **Face and object search** | Local models only (nothing leaves the machine): "people", "beach", "dog". Would also let *Hide areas* find faces by itself. | L |
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
