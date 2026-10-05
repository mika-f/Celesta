# Color emoji glyph rendering (2026-08-28)

Follow-up to the earlier "known limitation" note on the homepage-demo
composition (an emoji rendered as a flat monochrome silhouette instead of
full color). Investigated rather than accepted as a hard crate limitation —
it turned out to be fixable in `celesta-renderer` without touching
dependencies at all.

- **Root cause**: `cosmic-text`/`swash` (already pinned at `0.18.2`/`0.2.10`)
  already decode color glyphs — COLR, `sbix` (how Apple Color Emoji stores
  its glyphs), CBDT/CBLC — into real per-pixel RGBA (`swash::scale::image::
  Content::Color`, surfaced through `cosmic_text::Buffer::draw`'s
  callback). `TextRasterizer::rasterize`'s draw callback
  (`crates/renderer/src/lib.rs`) was just discarding that: it only ever
  extracted `color.a()` into a single-channel coverage mask, later
  recombined with one flat `style.fill` `Paint::Solid` color — a
  representation that is exactly right for ordinary uniformly-colored text
  but structurally cannot hold an emoji's multiple hues. Font fallback to a
  system color-emoji font (`Apple Color Emoji` is already in cosmic-text's
  built-in macOS `common_fallback()` list, `src/font/fallback/macos.rs`) was
  never the missing piece.
- **Fix**: the draw callback now also accumulates a second, parallel RGBA
  buffer (`glyph_pixels`) by alpha-blending each pixel's *actual* `color`
  from cosmic-text (reusing the existing `blend` helper), alongside the
  existing coverage-only `mask` (still needed as-is for the stroke's
  dilation, which only cares about the glyph's silhouette). `buffer.draw`'s
  base color argument changed from hardcoded white to the real resolved
  `fill` color — for an ordinary glyph (`Content::Mask`), cosmic-text
  returns `(fill_rgb, coverage_alpha)` per pixel, so `glyph_pixels`
  reproduces flat-fill text exactly; for a color glyph (`Content::Color`),
  it returns the glyph's own real RGBA, captured as-is. A new
  `composite_rgba` helper (mirroring `composite_mask`, but blending a
  buffer's own per-pixel color instead of one uniform color weighted by a
  mask) replaces the old `composite_mask(&mask, fill, ...)` call for the
  fill step; the stroke step is untouched. No GPU renderer changes were
  needed — it already consumes whatever RGBA `TextRasterizer` produces via
  the same `RasterizedText` type, same as the `Rect` work earlier in this
  document.
- Verified: a new `celesta-renderer` unit test
  (`rasterizes_color_emoji_glyphs_when_a_color_font_is_available`)
  rasterizes 🔥 and asserts its opaque pixels contain more than one distinct
  RGB color — the direct check that would fail if this regressed back to
  flat-fill mask compositing (skips gracefully, matching this codebase's
  existing pattern for GPU-adapter/ffmpeg-unavailable tests, on a machine
  with no color-emoji font at all, though it did *not* skip when run here).
  Existing `renders_text_to_a_png`/`centers_visible_single_line_text_on_its_
  transform` tests still pass unchanged, confirming ordinary flat-fill text
  is pixel-for-pixel unaffected. End to end: re-exporting
  `packages/react/examples/homepage-demo.tsx` now shows the 🔥/🥳/🥲 emoji
  in full color (previously flame/flag/circle silhouettes only), and
  re-exporting `examples/voiceroid.celesta.json` (stroke+fill subtitle text)
  confirmed the stroke/fill combination used by VOICEROID-style subtitles is
  visually unaffected.
