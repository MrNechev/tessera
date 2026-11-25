# Changelog

All notable changes to Tessera are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [0.7.1] — 2026-11-25

A small release: polish and bug fixes for the Library, stories and the canvas.

### Changed — polish
- **Library:** on a wide window the folders, search and filters sit in a column
  on the left and the photos fill the rest. The column (or, in the story editor,
  the folder tree and filters) folds away for more photos.
- **Picking several files:** every tile has a tick; once something is selected a
  click adds or removes a file, Shift+click adds a range, *Select* and *All* in
  the status line, Esc clears. Clicking no longer selects text in the grid.
- **The Library's action bar** is bigger and stands out (count, larger buttons,
  accent border).
- **Story editor:** a right-hand panel with a *Photo* tab (filters, light and
  colour, crop/turn/frame, caption, clip range, cover, cut-out, replace, remove)
  for the photo clicked in the story, and the *Outline* tab. It replaces the
  drop-down media options.
- **Media size slider** in the story editor: caps how tall photos, videos and
  collages get, so big photos no longer take several screens.
- **Canvas:** text size can be typed as a number next to S…XXL.

### Fixed
- Canvas *Bring to front / Send to back* did nothing visible (each kind of item
  was drawn in its own layer); every card, note, shape, sticker and drawing can
  now be stacked, also with Ctrl+] / Ctrl+[.
- Panning the canvas with the middle mouse button pasted the last selected text
  (on Linux a middle click pastes), which showed "files are not in a library
  folder yet" errors.
- A double-click in the story editor's library inserted the photo twice; it now
  only previews.

## [0.7.0] — 2026-11-24

**One app, one toolkit.** The collage studio and the former documents (now
**Stories**) feel like one application and share their tools: whatever you can
do to a photo in one editor, you can do in all of them.

### Added — one photo toolkit, in every editor
- **Edit photo** in the collage editor, in stories, on the canvas and in the
  Library (*Edit a copy*: saved as a new file in Outputs/Edited, rendered at
  full resolution, RAW and HEIC included). Originals are never changed.
- New adjustments: exposure, tint, highlights, shadows, vibrance, clarity,
  sharpen, blur, grain; **duotone**, **colour pop**, one-click **Auto**; new
  presets (Crisp, Film, Golden hour, Moody, two duotones, Red pop); presets are
  previewed on the photo itself.
- Crop, quarter turns, straighten and mirror; **hide areas** (blur or pixelate
  faces, number plates, screens); frames (polaroid, instant, mat, film, tape),
  shapes (circle, heart, star…), borders and shadows for story photos.
- One pixel pipeline (`render/look.ts`) shared by the collage renderer, stories,
  exports and copies; videos get an exact SVG twin of the colour stages.
  Crops now also work in presentations in the desktop app.
- **Cut-outs**: remove a photo's background on your own device (U²-Netp, ONNX
  Runtime Web; nothing is uploaded), touch up with a brush, save a transparent
  PNG to Outputs/Cut-outs and use it as a sticker in collages ("My cut-outs"),
  in stories and on the canvas.
- **Memes**: a Memes & fun template category, a Meme text style (Anton), and
  *Meme* from any Library photo. More social page sizes (YouTube thumbnail, X,
  Pinterest, LinkedIn, profile picture, chat sticker).
- **Video out**: an MP4 slideshow (16:9, 9:16 or 1:1; slow zoom and pan,
  crossfades, title cards, music from the library) from Library photos and
  clips, a story or a multi-page collage; saved to Outputs/Videos (H.264 via
  VA-API, x264 or OpenH264).

### Added — Stories (formerly Documents)
- **Live collages** in stories: a section shows a collage project and stays
  editable (*Edit collage* → back to the story); presentations, print/PDF and
  PowerPoint include it. *Add to a story* from the collage editor adds it live
  or as an image; a story section can start a collage of its photos.
- **Diary**: *Today* (today's photos into the diary story, a chapter per day),
  a **calendar** of stories and photos per day, **On this day** (Stories page
  and Home), and a **story starter** that drafts a story from a trip or date
  range (chapters per day, the best photos without near-duplicates, captions
  with date and place). Stories and sections can carry a date and place.
- On the **canvas**: stickers, and text in Tessera's styles (fonts, outline,
  shadow, pill); styled section titles (Script, Poster, Handwritten, Meme…).

### Added — one app
- **One home screen**: one *Recent* list for collages and stories, one set of
  *New* actions, collage and story templates side by side.
- **Library inside the collage editor** (Photos → Library tab): click to add,
  drag onto the page or a frame. Photos fill a template's empty frames first.
- **Library selection** → collage, photo book, calendar, meme, video, cut-out,
  edited copy, convert, compress, batch.
- **Command palette everywhere** (<kbd>Ctrl</kbd>+<kbd>K</kbd>): any view, any
  kind of new project, recent collages, library files and folders, the collage
  editor's commands, actions on the Library selection.
- **Settings → Stories & library**: slide transition, thumbnail size and blur,
  hidden and favourite folders, recently inserted media.
- Docker: `ghcr.io/mrnechev/tessera:edge` follows `main` (Settings shows the
  build); releases stay `:<version>` and `:latest`.

### Changed
- *Documents* are called **Stories** in the interface (files, API and data
  folder unchanged, no migration).
- Theme switch and *Sign out* live in the rail; the old Letopis top bars are
  gone; the Stories and System pages use Tessera's headers.

## [0.6.0] — 2026-11-24

**Letopis joins Tessera.** Tessera gains a media library, long documents, a
canvas and presentations (everything the separate app Letopis did), and it now
runs both as a desktop app and as a self-hosted web app in Docker.

### Added — library, documents and presentations
- **Library**: every photo and video in your library folders, indexed in the
  background (SQLite) and kept up to date by a file watcher: folder tree,
  virtualised grid, search, filters (date taken, camera, resolution,
  orientation, duration, GPS), favourites and recents, duplicates and
  look-alikes, video hover-scrubbing, live/motion photos, full-screen viewer.
  Pick photos to start a collage or send them to Convert, Compress or Batch.
- **Documents**: chaptered documents of rich text, photos and videos (seven
  layouts, captions, crop, before/after compare, clip ranges, embeds,
  templates, tags, find & replace, version history, conflict detection).
- **Canvas**: the same document as a whiteboard (cards, notes, shapes, arrows,
  frames, pen, groups, minimap, auto-arrange).
- **Presentations**: transitions, overview, speaker-notes window, timed and
  looping slideshows, laser and drawing, music, rehearsal, phone remote (web app).
- **Exports**: PDF, PPTX, Markdown, offline package, backup of every document.
- **Video**: thumbnails, posters and hover sprites; proxies for formats the
  viewer can't play and for very large files, made on the GPU (VAAPI) where
  possible; GIFs become small looping videos.
- **System** page (queues, cache, GPU, stress test) and a <kbd>Ctrl</kbd>+<kbd>K</kbd>
  command palette.

### Added — Tessera as a web app (Docker)
- `ghcr.io/mrnechev/tessera` image with `docker-compose.yml` and `.env.example`
  attached to every release; guide in `docs/DOCKER.md`.
- HTTPS with HTTP/2 and a private certificate authority, single-password login.
- Collages and all image tools work in the browser: pick files from the
  server's libraries, Outputs and Inbox, or drop/upload them from your computer
  (files already on the server are found by name and size and not uploaded;
  big files upload in resumable chunks). Results are saved to Outputs and
  downloaded (many files as one zip).
- Reads Letopis's data and settings as they are (`LETOPIS_*` variables still work).

### Added — desktop
- First start asks which folders hold your photos and videos and where to
  save results; **Settings → Libraries** to change them.
- Libraries are **read-only** by default; switch a folder to read-write if the
  image tools should save into it. One write guard checks every file the tools
  write, in the desktop app and on servers.
- Files dropped from the file manager into a document are found in your libraries.

### Changed
- The desktop app is now a thin shell around Tessera's engine
  (`crates/tessera-core`), which it runs privately on your computer; the
  window talks to it like a browser talks to the web app.
- The Flatpak has audio access (needed for video playback); the deb and rpm
  recommend ffmpeg, libvips and GStreamer codecs.

### Known limitations
- In the desktop app about four 1080p videos play smoothly at the same time
  (documents limit autoplay to four); the web app in a Chromium-based browser
  has no such limit. No ffmpeg in the Flatpak yet. See the README.

## [0.5.1] — 2026-11-12

A bug-fix and performance release.

### Fixed — your work
- Autosave could write the project you were editing into a project you had
  just opened (losing the opened one). Saves now go to the right place, one
  at a time.
- Edits made in the last moment before closing the window, or before opening
  another project, are saved. New collages, photo books and calendars appear
  in Recent straight away, and a failing autosave is reported.
- Deleting a project from Recent asks first, and deleting the open project no
  longer brings it back.
- Photos edited after a project was saved show up again instead of empty
  frames; Clear cache rebuilds the previews of photos in use.
- Batch tools: two photos that would get the same output name (IMG_0001.CR2
  and IMG_0001.JPG, or same-named files from two folders) no longer overwrite
  each other, and renaming with "move" can no longer replace another photo of
  the batch.
- Lossless JPEG compression keeps IPTC captions/keywords and comments.

### Fixed — editor
- Moving the spacing or margin slider reshuffled the whole collage on every
  step; layouts now stay put (and Previous/Next layout no longer brings back
  deleted photos).
- Font changes apply to the element that was selected when you picked them.
- Photos pasted from another project export correctly (they were grey boxes).
- Adding a text style is one undo step; calendars honour their opacity; the
  crop view follows flipped photos; export no longer cuts small elements'
  shadows at tile edges.
- Keyboard shortcuts no longer act behind open dialogs or while typing a name.
- Save As adds the right extension to names with dots ("Mum & Dad 25.12").

### Fixed — Convert, Compress and Batch
- Empty or fractional sizes no longer fail a job or shrink photos to 16 px.
- The finishing message says when files failed; only one tool job runs at a
  time (they shared one Cancel); options are kept when you switch views; the
  Compress preview always shows the selected file.
- Colour profiles for CMYK/Lab are no longer embedded into RGB output; XMP
  orientation is reset after rotating; stale EXIF thumbnails are dropped;
  a malformed EXIF block can no longer abort a whole batch.

### Faster
- Layout sliders on a 120-photo collage react in about a millisecond (was a
  third of a second).
- Exports load photos in parallel; the editor no longer decodes full-size
  previews for small photos and limits how many it keeps in memory.
- Import shows its progress everywhere, uses all cores when memory allows, and
  half-written previews from a crash are never reused.
- Heavy file and project operations no longer briefly freeze the window.

[0.5.1]: https://github.com/MrNechev/tessera/releases/tag/v0.5.1

## [0.5.0] — 2026-11-11

### Multi-page projects
- Projects can have any number of pages. A page strip under the canvas shows
  them all: click to switch (or Ctrl+PageUp / Ctrl+PageDown), and duplicate,
  reorder or delete the current page. The File menu has the same actions.
- Templates can be used on the current page instead of starting a new project.
- **Save** writes every page; **Save as…** exports the current page or all
  pages, as one multi-page PDF or as numbered images.

### Photo books and calendars
- **New photo book** on the home screen: a cover with title and subtitle,
  then pages with varied layouts (or a fixed number of photos per page),
  three styles and page numbers, in square 12×12″, A4, US Letter or 8×10″.
- **New photo calendar**: a cover and twelve month pages with one photo each,
  for any year, with the week starting on Monday or Sunday.

### Printing
- **Print** (Ctrl+P, or File → Print) opens the system print dialog with the
  paper set to the project's page size and no margins, so every page prints
  on one sheet. Printers, copies and "Print to File" are all in the dialog,
  which remembers your printer.

### Fixes
- The status bar no longer wraps while saving.
- The file drop areas mention camera RAW.

[0.5.0]: https://github.com/MrNechev/tessera/releases/tag/v0.5.0

## [0.4.0] — 2026-11-11

### Batch tools
- A new **Batch** view: drop in any number of photos (RAW included) and
  resize, watermark and rename them in one go, with a live preview.
- **Resize** by long edge, width, height or percentage; photos are never
  enlarged.
- **Watermark** with text or a logo image: nine positions or tiled across the
  photo, size, opacity and margin; text in any of the app's fonts, with
  colour, bold, outline and shadow.
- **Rename** with patterns (`{name}`, `{n}`, `{date}`, `{time}`, `{year}`,
  `{month}`, `{day}`, `{size}`, `{folder}`), counters with padding, sorting by
  name or date taken, letter case and space handling.
- Save copies to a folder or next to the originals, or move the originals.
  Renames without other changes copy files byte for byte. Edited copies never
  overwrite an original unless "Replace" is chosen.
- Output format: keep, or convert to JPEG, PNG, WebP, AVIF or TIFF, with
  metadata kept, chosen or removed.
- TIFF-based RAW files (NEF, DNG, ARW…) are recognised as RAW and saved as
  JPEG when the format is kept.

[0.4.0]: https://github.com/MrNechev/tessera/releases/tag/v0.4.0

## [0.3.0] — 2026-11-11

### Camera RAW
- Open RAW files from most cameras: Canon CR2/CR3, Nikon NEF, Sony ARW,
  Adobe/phone DNG, Fujifilm RAF, Olympus ORF, Panasonic RW2/RAW, Pentax PEF
  and more. They work everywhere: collages, Convert and Compress.
- RAW photos are developed with the camera's white balance and colour matrix,
  with highlight handling and the orientation from the file.
- Their metadata (camera, lens, exposure, date) is read and can be carried
  over to the converted files.

### Colour management
- Wide-gamut photos (Display P3, Adobe RGB, ProPhoto and others) are
  converted to sRGB through their ICC profile so colours look right in
  browsers and on other devices.
- Convert has a new **Colour** section: convert to sRGB (default) or keep the
  original profile. Compress keeps the profile by default with an option to
  convert to sRGB.
- The metadata panel shows the colour profile and whether it is sRGB.

[0.3.0]: https://github.com/MrNechev/tessera/releases/tag/v0.3.0

## [0.2.0] — 2026-11-11

### Layouts
- **Letters** layout: photos fill the letters of any word, name or number
  (eight bold fonts, repeating photos to keep strokes crisp).
- **Spiral** layout: a golden spiral where each photo is a little smaller
  than the last.

### Calendars
- New **calendar element**: any month and year, week starting Monday or
  Sunday, four styles (clean, lines, boxes, dots), marked days for
  birthdays and holidays, fonts, colours and an optional card background.
- A **Calendar** tool in the editor with six themes, and “Add all 12 months”.
- Templates: Photo Calendar, Desk Calendar, Calendar with Three Photos,
  Year at a Glance, Family Birthdays, and a Save-the-Date card.

### Templates & stickers
- 25 new templates, including L-O-V-E, The Big Number, Name in Photos, MOM,
  2027 in Photos, Monogram, Baby’s First Year, Champions, Festival Nights,
  Thank You, Autumn Walks, Spring Blooms, Foodie Diary, Playtime, Little
  Space Explorer, Nine Grid and Two-tone Split.
- A second sticker pack: gold balloon numbers, sports, music, food & drink,
  space, kids, flowers, celebration extras, seasons and animals.

[0.2.0]: https://github.com/MrNechev/tessera/releases/tag/v0.2.0

## [0.1.0] — 2026-11-05 — first alpha

### Collages
- Quick automatic collages from any number of photos with eight layout engines:
  Mosaic, Classic (PhotoCollage's column algorithm), Rows, Grid, Featured,
  Shape (heart, circle, star, hexagon, diamond), Scattered polaroids and Rings.
- Regenerate with back/forward layout history; adjustable spacing, margins,
  corner radius, borders and shadows.
- Drag a photo inside its frame to re-centre it, onto another photo to swap,
  double-click to zoom and reposition.
- Free-form editing: move, resize, rotate, snapping guides, layers, alignment,
  lock/hide, undo/redo, copy/paste and keyboard shortcuts.
- Photo masks (14 shapes), frames (polaroid, instant, film, mat, tape) with
  captions, borders, shadows, filters and colour adjustments.
- Text with 26 bundled fonts, outline, shadow, label background and curved text.
- 144 original CC0 stickers, decorative shapes and empty photo frames.
- Backgrounds: colours, gradients, 16 patterns, paper textures, blurred photo,
  vignette and 12 page border styles.
- 33 templates for Valentine's Day and loved ones, Christmas, New Year,
  birthdays, family, weddings, babies, travel, holidays, graduation, friends,
  pets and more.
- Page presets for paper (A3–A5, Letter, Legal, Tabloid, posters), photo prints
  and social media, with 72–600 DPI.
- One-click Save into a configurable folder and Save as… to JPEG, PNG, WebP,
  AVIF, TIFF or PDF with DPI metadata; project autosave and `.tessera` files.

### Image tools
- Batch converter with lossless "keep 100 %" mode or a chosen quality
  reduction, resizing, and metadata policies (keep all / remove all / pick
  date, camera, settings, location, author, colour profile, file dates).
- Smart compressor (visually lossless, balanced, small), lossless-only, custom
  quality and target file size, with a before/after preview.
- Import of JPEG, PNG, WebP, AVIF, HEIC/HEIF, TIFF, BMP, GIF, JPEG XL, QOI, TGA, ICO.

### Packaging
- deb, rpm, AppImage and Flatpak builds; GitHub release workflow.
- Daily update check against GitHub releases (can be disabled).

[0.1.0]: https://github.com/MrNechev/tessera/releases/tag/v0.1.0
