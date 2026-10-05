# `interpolateColor` (2026-10-05)

`interpolateColor(input, inputRange, colors, options?)`
(`packages/react/src/animation.ts`) maps a frame onto `#RRGGBB` / `#RRGGBBAA`
colors the way `interpolate()` maps it onto numbers (issue #136).

- It lives in `@celesta/react`, next to `interpolate()`, because it shares
  its segment search (`locateSegment`), `easing`, and extrapolation options.
  `@celesta/math` has no color or easing concepts.
- Colors blend in gamma-encoded sRGB with premultiplied alpha, matching the
  renderer's gradient stops (`sample_stops` in `crates/renderer/src/paint.rs`)
  rather than the `.celesta.json` effect color keyframes
  (`evaluate_effect_color`), which interpolate straight RGBA. The result is
  rounded per channel and always uppercase `#RRGGBBAA`, the format the Rust
  evaluator emits.
- Where the eased position is exactly 0 or 1 the stop color is returned
  unchanged, so a fully transparent stop keeps its RGB (`#FF000000`), which
  premultiplying would otherwise lose.
- Extrapolation defaults to `'clamp'`, unlike `interpolate()`'s `'extend'`.
  `'extend'` and overshooting easings clamp each channel to 0–255; a
  non-positive alpha gives `#00000000`. `'identity'` throws.
- Non-finite input or `inputRange` values, mismatched lengths, a
  non-increasing range, and colors other than `#RRGGBB`/`#RRGGBBAA` throw.
- Text solid fills now honor alpha. cosmic-text's `Buffer::draw` drops the
  base color's alpha for ordinary (mask) glyphs, so `TextRasterizer`
  (`shaping.rs`) draws through its own `Renderer`, `GlyphPixelRenderer`, which
  checks each glyph image's `SwashContent` and applies the alpha to mask
  glyphs only. Color glyphs (emoji) keep their own pixels, and the stroke's
  dilation mask keeps full coverage. Before this, `#FFFFFF80` text rendered
  opaque.
- `interpolateColor` validates arrays by index, so sparse-array holes are
  rejected rather than skipped.
- `examples/with-color.tsx` animates a `Rect` fill, gradient stops, and a
  `Text` fill; `crates/react-bridge/tests/node_integration.rs` checks the
  evaluated colors at frames 0 and 45.
