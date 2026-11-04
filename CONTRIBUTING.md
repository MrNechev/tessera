# Contributing to Tessera

Thanks for wanting to help! Bug reports, template and sticker ideas, code,
translations (soon) and design feedback are all welcome.

## Getting set up

See [Building from source](README.md#building-from-source) for the system
packages. Then:

```bash
npm ci
npm run tauri dev
```

Useful commands:

| Command | What it does |
| --- | --- |
| `npm run check` | Type-check the Svelte/TypeScript code |
| `npm test` | Unit tests for layouts and geometry (Vitest) |
| `cargo test --workspace --release` | Rust tests (imaging engine, metadata, codecs) |
| `cargo clippy --workspace --all-targets -- -D warnings` | Lints |
| `cargo fmt --all` | Format Rust code |
| `npm run tauri build` | Build deb, rpm and AppImage |

CI runs all of these on every pull request.

## Where things live

Read [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) first — it explains the
document model, the shared renderer and the layout engines.

- New **template**: add it to `src/lib/templates/catalog.ts`. Coordinates are
  fractions of the page, font sizes fractions of the shorter side. Check it in
  the app at several page sizes.
- New **sticker**: add an SVG to `public/stickers/<category>/` and an entry to
  `public/stickers/manifest.json`. Stickers must be your own work and are
  released under CC0. Keep the `viewBox` tight, set `width`/`height`, and use no
  text, filters, masks or external references.
- New **layout engine**: add a pure function in `src/lib/collage/engines/`,
  register it in `collage/layout.ts` and add it to the test matrix.
- New **image format or metadata behaviour**: `crates/tessera-imaging`, with
  tests in its `tests/` folder.

## Guidelines

- Keep the editor preview and the export identical: draw through
  `render/renderer.ts`, never with ad-hoc canvas code in components.
- Every document change goes through `editor.update()`/`patch()` so undo works.
- User-facing errors should say what happened and what to do next.
- Never modify a user's original files unless they explicitly asked for it.
- Commit messages: a short imperative summary line, then the why.

## Licensing

By contributing you agree that your contributions are licensed under the
GPL-3.0-or-later (code) or CC0-1.0 (stickers, patterns, templates).
