# Changelog

All notable changes to Tessera are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

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
