# Sharing one font database across SVG assets (2026-11-06)

Issue #700: `ImageSources::load` (`crates/renderer/src/image_source.rs`)
gave every SVG asset fresh `usvg::Options` and called
`fontdb_mut().load_system_fonts()` on them, so each SVG walked every system
font directory and parsed each face's header again, on the preview worker
before the first frame. Each parsed `usvg::Tree` also kept its own copy of
the database.

## Specification

- `ImageSources` keeps `svg_fonts: Option<Arc<usvg::fontdb::Database>>`. The
  first SVG it loads builds it with `load_system_fonts`; every SVG after it
  gets `options.fontdb = Arc::clone(&fonts)`, so the system fonts are
  scanned once per `ImageSources` (one editor document, or one export run).
- Fonts installed after the first SVG loads are not seen by later SVGs
  until a new `ImageSources` (a reopened document, or the next export run).
  The per-SVG scan did see them; the trade-off is intended.
- SVG text still resolves with usvg's default font resolver against the
  same system fonts, so what an SVG draws does not change.

## Implementation notes

- The database cannot be the text rasterizer's: usvg 0.49 uses fontdb 0.24
  and cosmic-text 0.18.2 uses fontdb 0.23, so the two types differ.
- The default font resolver only reads the database, so sharing it through
  the `Arc` never clones it (`fontdb_mut` would, through `Arc::make_mut`).

## Checks

- `cargo test -p celesta-renderer --lib image_source`
  - `svgs_share_one_font_database` loads two SVGs through one
    `ImageSources` and checks `Arc::ptr_eq` on their trees' `fontdb()`. It
    fails with the previous per-SVG loading.
  - `svg_text_draws_with_a_system_font` loads a blank SVG, then renders a
    `<text>` SVG (`DejaVu Sans, Arial, Helvetica, sans-serif`, one of which
    each CI OS has) and checks it drew opaque pixels. It fails when the
    shared database is left empty.
- `cargo clippy -p celesta-renderer --all-targets -- -D warnings`
