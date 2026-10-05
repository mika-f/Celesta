# `Rect` primitive and a Remotion-homepage-style demo composition (2026-08-28)

Requested as a reproduction of Remotion's homepage "Interactive Demo"
(`packages/promo-pages/.../homepage/Demo/Comp.tsx` upstream). Scoped down
after confirming with the user: visuals/animation only (no browser-side
interactive Player — Celesta has none), fixed mock data instead of the
original's live GitHub-trending/weather fetches, and Celesta-native
replacements for Remotion-only packages (`@remotion/animated-emoji`,
`@remotion/media`).

- **`LayerContent::Rect`** (`crates/composition/src/model.rs`): a flat-shaded
  rectangle, `{ width, height, fill: Option<Paint>, stroke: Option<Stroke>,
  corner_radius }`, reusing the existing `Paint`/`Stroke` types `TextStyle`
  already has rather than introducing a new color type. Unlike
  `Image`/`Video`, a rect has no natural size, hence the explicit
  `width`/`height`. This closes a real gap: before this, Celesta had no way to
  draw a flat background or border at all, in a project or a React entry.
- **Shared rasterizer** (`crates/renderer/src/lib.rs`): `pub fn
  rasterize_rect(width, height, corner_radius, fill: Option<&Paint>, stroke:
  Option<&Stroke>) -> Result<RasterizedText, RenderError>` mirrors
  `TextRasterizer::rasterize`'s shape exactly (same `RasterizedText`
  width/height/pixels output) so it composites through the exact same
  `render_image` path text does, and so `celesta-gpu-renderer` can call it
  directly without its own color-parsing code (it already depends on
  `celesta-renderer` for `TextRasterizer`; this is the same precedent). Uses
  Inigo Quilez's rounded-box signed-distance function, anti-aliased over a
  ~1px edge via `smoothstep`-style clamping; the stroke band is a second SDF
  evaluation against the fill rect shrunk by the stroke width. `celesta-editor`
  needed no changes — its `LayerContent` matches already have wildcard `_ =>`
  arms. `celesta-exporter`'s `absolutize_layer_content` needed one match arm
  added (a rect has no asset to absolutize, so it's a no-op alongside
  `Text`/`MissingComponent`).
- **`<Rect>` in `@celesta/react`** (`src/components.ts`, `src/render.ts`):
  `{width, height, fill?: string, stroke?: string, strokeWidth?, cornerRadius?}`
  — plain hex color strings rather than requiring authors to build `Paint`/
  `Stroke` JSON objects by hand, converted in `render.ts`'s new `'rect'`
  branch of `buildLayer` (added to `HOST_TYPES` alongside the others).
- Verified: a new `celesta-renderer` unit test
  (`renders_a_filled_rounded_rect_with_a_stroke`) asserts the fill color at
  the rect's center and that a corner-radius-excluded pixel is neither the
  fill nor the stroke color; a new `celesta-react-bridge` integration test
  (`evaluates_a_rect_with_fill_stroke_and_corner_radius_when_node_is_available`,
  against a new `packages/react/examples/with-rect.tsx`) asserts the
  evaluated `LayerContent::Rect` fields round-trip exactly through the Node
  bridge. `cargo test -p celesta-project -p celesta-composition --features
  codegen` regenerated `packages/react/src/generated/LayerContent.ts` with
  the new `rect` variant; `pnpm run build` type-checks clean against it.
- **`packages/react/examples/homepage-demo.tsx`**: a 640x360/30fps/120-frame
  composition reproducing the four-card layout (GitHub trending, weather,
  current country, an emoji picker) using `<Rect>` for each card's
  background/border, `spring()`-based staggered entrance per card,
  `interpolate()` for the weather card's count-up, and a plain emoji-glyph
  `<Text>` (cycled every 40 frames) in place of the original's Lottie
  animated emoji. `<Audio src="../../../examples/assets/voices/001.wav">`
  (the repo's existing VOICEROID voice fixture) stands in for
  `@remotion/media`'s reaction sound. Verified end to end: `cargo run -p
  celesta-exporter -- --react packages/react/examples/homepage-demo.tsx
  out.mp4` produced an h264+aac MP4 (ffprobe-confirmed); extracted frames at
  10/60/100 were inspected and show the expected staggered card entrance,
  the weather card's temperature counting up to a settled `24°C`, and the
  emoji swapping across its three glyphs. **Known limitation, not introduced
  by this work**: this environment's text rasterizer (`cosmic-text`/`swash`)
  has no color/COLR emoji font support, so emoji glyphs render as their
  monochrome fallback outline (a flame silhouette, a plain flag shape, a
  bare circle for the "pleading face") rather than full-color glyphs — a
  pre-existing constraint of the shared text rasterizer, not of the new
  `Rect` work.
