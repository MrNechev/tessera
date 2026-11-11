# Changelog

All notable changes to Tessera are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

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
