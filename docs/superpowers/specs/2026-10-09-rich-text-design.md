# `<Span>` rich text design (issue #147)

## Goal

Text children are strings and numbers, so emphasizing one word means splitting
the paragraph into several `Text` layers placed by estimated widths. Separate
layers cannot wrap, kern, or share a baseline as one paragraph.

`<Span>` changes the fill color, font weight, or font family of part of a
`<Text>`, and the whole text is still laid out and shaped as one paragraph.

```tsx
<Text style={{ fontSize: 48, fill: { type: 'solid', color: '#ffffff' } }}>
  速い、<Span style={{ fill: '#28A34A', fontWeight: 700 }}>カンタン</Span>、頼もしい。
</Text>
```

#99 (PR #186) already added `TextStyle.colorRuns`: solid colors over
code-point ranges, painted after full-text shaping on CPU, GPU, and browser.
This design reuses it for color and adds font runs, which change advances and
therefore take part in shaping, measurement, and wrapping.

Font stacks (`fontFamily: ['Inter', 'Noto Sans JP']`, choosing a family per
character) are out of scope; they are #204, built on the per-range font
mechanism added here.

## Scope

In scope: solid `fill`, `fontWeight`, and `fontFamily` per span, on `<Text>`,
`<TextBox>`, `<TextReveal>`, `<Dialogue>`, and `<DialogueSeries>` lines; the
measurement APIs and `useTypewriter`; native (CPU/GPU) rendering as the
reference; browser rendering as close as practical.

Out of scope: per-span `fontSize`, gradient fills, strokes, italics (no
`fontStyle` exists), synthetic bold, font stacks (#204), and any change to
`@celesta/code`.

## Existing behavior is unchanged

Every composition that renders today renders identically:

- Text without spans produces the same scene JSON: `fontRuns` and `colorRuns`
  are only emitted when spans produce them.
- With empty `fontRuns`, native shaping input and cache keys are unchanged,
  and the baseline is cosmic-text's own `line_y`. The baseline recomputation
  below runs only when `fontRuns` is non-empty.
- Font warnings judge the same family for every character when there are no
  runs, so their entries and messages are unchanged.
- The browser runtime takes its existing path when `fontRuns` is empty.
- Measurement requests for string input are unchanged, and so are their keys.

Changes are limited to input that used to throw (`null`, `undefined`,
booleans, and fragments as `<Text>` / `<Dialogue>` children; spans anywhere),
additive fields and parameter widenings, and the type of `DialogueLine.text`,
which has no runtime effect.

Tests assert this: span-free text keeps the same scene JSON and reuses the
same cached layout as a style without runs, and the existing renderer, GPU,
React, and browser suites, which check rendered pixels, pass unchanged.

## API

```ts
export interface SpanStyle {
  /** A hex color or a solid `Paint`. Gradients are rejected. */
  fill?: string | Paint;
  fontWeight?: number;
  fontFamily?: string;
}
export interface SpanProps {
  style?: SpanStyle;
  children?: ReactNode;
}
export function Span(props: SpanProps): ReactElement;
```

- `<Span>` is valid only inside rich text content: the children of `<Text>`,
  `<TextBox>`, `<TextReveal>`, and `<Dialogue>`, a `<DialogueSeries>` line's
  `text`, and the content arguments of the measurement APIs and
  `useTypewriter`. Rendered anywhere else (directly in a `<Group>`, say), it
  throws.
- Spans nest. Each property of an inner span overrides the outer value; unset
  properties inherit.
- Rich text content may contain strings, numbers, arrays, fragments, and
  `<Span>`. `null`, `undefined`, and booleans render nothing (as `TextBox`
  already allows). Any other element, including a user component that returns
  a `<Span>`, throws: rich text content is read as props, not rendered by
  React. The error names what is allowed.
- `fill` that is not solid throws.
- A `<Text>` whose `style.colorRuns` is non-empty and whose children contain a
  span with `fill` throws, rather than defining a precedence between them.
- Offsets are Unicode code points, as `colorRuns` already uses.
- A `fontWeight` the family has no face for uses the nearest face, as the
  text's own `fontWeight` does since #45. No bold is synthesized; variable
  fonts get the weight through their `wght` axis.

### Measurement and typing

- `useTextMetrics`, `measureText`, and `useFitText` take rich text content
  (`ReactNode`) as their first argument. Strings work as before.
- `useTypewriter` takes rich text content and counts code points of its plain
  text. `text` stays the typed plain string. For spans, pass `length` to
  `style.visibleCharacters`, which reveals the text without changing its
  layout:

  ```tsx
  const content = <>速い、<Span style={{ fontWeight: 700 }}>カンタン</Span>。</>;
  const { length } = useTypewriter(content, { from: 10 });
  return <Text style={{ ...style, visibleCharacters: length }}>{content}</Text>;
  ```

### TextReveal

`<TextReveal>` accepts rich text content. It flattens the content, splits the
text at `\n`, and gives each line's `Text` the runs inside that line, with
offsets rebased to the line's start.

### Subtitles

- `SubtitleRenderProps.text` stays the plain string.
- New `SubtitleRenderProps.content: ReactNode` is the line's children as given,
  spans included. A custom `render` keeps emphasis with
  `<Text style={style}>{content}</Text>`.
- `SubtitleRenderProps.metrics` is measured with the line's font runs.
- `DialogueLine.text` widens from `string` to `ReactNode`. Code reading
  `line.text` as a string through the default `DialogueLine` type needs a
  narrowing; a custom `L extends DialogueLine` that declares `text: string`
  is unaffected. Entry bundling does not type-check, so nothing changes at
  run time.

## Scene model

```rust
pub struct TextStyle {
    // ...existing fields...
    /// Font overrides in code-point ranges of the complete text. Ranges must
    /// be ordered and non-overlapping. Unlike `color_runs`, these change
    /// shaping and layout. Written by `<Span>`; not documented for direct use.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub font_runs: Vec<TextFontRun>,
}

/// Font weight and family over a half-open code-point range of the full text.
pub struct TextFontRun {
    pub start: usize,
    pub end: usize,
    pub font_weight: Option<u16>,
    pub font_family: Option<String>,
}
```

- `colorRuns` is unchanged and keeps carrying color, including span fills.
  Layout-affecting overrides live in a separate array so the shaping cache key
  can include `fontRuns` and keep excluding `colorRuns`, which preserves
  #186's reuse of shaped text when only paint changes.
- `fontRuns` is internal: the public API is `<Span>`, so the representation
  can change later without breaking compositions. It is not described in
  `skills/celesta/references/text.md` or the website beyond a note that
  `<Span>` produces it.
- Empty `fontRuns` serializes to nothing, so existing scenes and project files
  are unchanged. TypeScript bindings are regenerated.

## Data flow

1. **Flattening (React).** `extractText` (render.ts), `subtitleText`
   (components.ts), and `textBoxText` (text-fit.ts) become one
   `flattenTextContent(content)` returning `{ text, colorRuns, fontRuns }`.
   It counts code points with `Array.from`, resolves inherited span styles,
   and merges adjacent runs with identical attributes. TextReveal, the
   measurement APIs, and `useTypewriter` use it too.
2. **Scene.** The text layer's style gets the flattened runs merged in.
3. **Measurement.** Requests carry `fontRuns`. `useTextMetrics` already keys
   on the style minus paint fields, so `fontRuns` stays in its key.
4. **Rendering.** CPU and GPU share `TextRasterizer::shaped_buffer`; the
   browser runtime has its own path (below).

## Native rendering

All changes are in `celesta-renderer`; the GPU renderer uses the same
rasterizer and its texture key already includes the whole style.

- **Cache key.** `shaped_buffer`'s key adds `font_runs`; `color_runs` stays
  out.
- **Validation.** Runs must be ordered, non-overlapping, and within the text's
  code-point length, as for `color_runs`. Violations return a
  `RenderError::InvalidTextFontRun { index, start, end, text_length }`.
  Measurement reports the same error text.
- **Applying runs.** For each buffer line, code-point ranges are converted to
  byte ranges in the line as shaped, accounting for the word joiners that
  `lineBreak: phrase` inserts, including after a segment loses its joiners
  and the line is re-set. Each range gets `Attrs` with the run's family (or
  the text's) and `matched_weight(family, weight)`, via `attrs_list.add_span`.
- **Cluster rule.** An extended grapheme cluster takes the attributes at its
  first code point, matching `colorRuns`' rule. cosmic-text 0.18 already
  applies attributes per grapheme cluster at the cluster's first byte, so
  native needs no boundary adjustment; the browser path implements the rule
  explicitly.
- **Existing spans inherit run attributes.** The color-emoji span and the
  word-joiner span (letter spacing 0) are currently cloned from the base
  attributes. They must be derived from the attributes in effect at their
  position; otherwise a joiner inside a bold run would switch back to the
  regular face and split shaping there (cosmic-text splits shaping runs where
  family, weight, stretch, or style change; letter spacing alone does not).
  Emoji spans keep the color emoji family with the run's matched weight.
- **Kerning and ligatures** do not cross a font-run boundary, since the faces
  differ. They are kept inside runs and across color-only boundaries.

### Baseline

cosmic-text places each line's baseline from the largest ascent and descent of
the glyphs on that line, so a span in a taller font would move its line down
and make line spacing uneven. Spans do not move the baseline:

- When `fontRuns` is non-empty, each line's baseline is recomputed as
  cosmic-text does, `line_top + (line_height - (ascent + descent)) / 2 +
  ascent`, but with `ascent` and `descent` taken over the glyphs outside font
  runs on that line.
- A line that has glyphs, all inside font runs, uses the largest ascent and
  descent of the glyphs outside font runs anywhere in the text. A line with
  no glyphs (an empty line) keeps cosmic-text's baseline.
- Text entirely inside font runs keeps cosmic-text's baseline.
- Line height stays fixed by `Metrics`. Measure and rasterize use the same
  recomputed baselines, so they agree.
- Run glyphs can still reach past their line's boxes (a span in a taller
  family). With font runs, rasterize pads the image above and below by how
  far any glyph's ascent or descent reaches past the layout box, so nothing
  is clipped; the image is that much taller, and the anchor box and the
  returned baseline account for the padding.

Glyph ascent and descent come from each glyph's face metrics (`font_id`) at
its font size.

A run splits shaping, and cosmic-text picks fallback fonts per shaped piece.
Without `lang`, the characters next to a run can fall back to a different
font than the whole phrase did (for example a Chinese font for kanji), which
changes their glyphs and can move the line. With the text's language,
fallback follows it on both sides of the run, so CJK text with spans should
set `lang`.

### Font warnings

`font_fallback` and `missing_glyphs` check only the text's own family today,
so characters drawn from a span's family would be reported as missing. Both
check each character against the family and weight in effect at its position
and return one entry per family/weight (`Option` → `Vec`). An empty run
covers nothing and asks for no face. A family with no loaded face is
reported only by `font_fallback`; `missing_glyphs` leaves its characters out,
as it does for the text's own family today. The GPU renderer's
de-duplication of reported entries is updated accordingly.

## Browser rendering

The browser runtime aims to match native but is not pixel-identical; native
is the reference.

- Text without `fontRuns`, and each line no run reaches, takes the existing
  path unchanged.
- **Wrapping and measurement.** `textLineRanges` and `textMeasurer` measure
  each run's piece with that run's font and sum the widths. Kerning is kept
  inside runs and lost across boundaries, as on native. A line's ink box is
  the union of its pieces' boxes. The measurer validates run ranges as
  scene rendering does.
- **Drawing.** Run pieces are split at grapheme cluster boundaries. Bidi
  reordering can split a piece or put its parts out of source order, so each
  piece is cut into segments whose clusters sit next to each other on the
  shaped line, left to right or right to left. Each segment is drawn with
  its run's `ctx.font` (`fillText`, and `strokeText` for the outline), left
  to right from its left edge or with `direction: 'rtl'` from its right
  edge. A single-cluster segment takes its direction from its neighbors on
  the line, so a mirrored character (a parenthesis in right-to-left text)
  keeps its mirroring.
- **Positions.** Character extents come from an SVG `<text>` with one
  `<tspan>` per run, so color regions and reveals keep working from the same
  extents. `getExtentOfChar` reports positions across `<tspan>` children
  that agree with summed per-run `measureText` widths (checked in Chromium
  152).
- **Baseline.** The browser already places every line's baseline from the
  text's own font, which matches the native rule above.
- **Documented differences.** Kerning at run boundaries and the existing
  platform font and rasterization differences.

## Compatibility

- See "Existing behavior is unchanged".
- `@celesta/code` keeps using `colorRuns` and does not change.
- `font_fallback` / `missing_glyphs` return types change inside the Rust
  workspace; their callers (gpu-renderer, exporter, editor) are updated.

## Testing

- **Unchanged behavior.** Span-free text: identical scene JSON and the same
  cached layout as a style without runs; existing suites pass unchanged.
- **Renderer (Rust).** Family runs with the repository's Bebas Neue and IBM
  Plex Mono fonts: advances change, measured width equals rasterized width,
  and the shaping cache separates texts that differ only in font runs while
  still sharing across color-only changes. Baselines: a span in a taller
  family does not move its line, an all-span wrapped line matches the other
  lines, and fully spanned text keeps cosmic-text's baseline. Weight runs are
  asserted where the system has a bold face and skipped otherwise, like the
  color emoji tests. Also: a boundary inside a combining sequence, phrase line
  breaking with a run (no split at joiners), emoji inside a run, invalid
  ranges, and warnings for a span family that lacks characters.
- **GPU.** Font-run text matches the CPU reference, as existing text tests do.
- **React.** Offsets with surrogate pairs, nested spans, merged adjacent runs,
  the errors (gradient fill, foreign element, `colorRuns` conflict, span
  outside rich text), `Dialogue` `content` and `metrics`, `TextBox` fitting
  with a bold span, `DialogueSeries` lines with spans, `TextReveal` runs
  rebased per line, measurement APIs with content, and `useTypewriter`
  counting rich content.
- **Web.** `text-runs.mjs` in real Chrome with bold and family runs (coverage,
  wrapping, color and reveal over runs, grapheme boundaries), plus unit tests
  for run-aware wrapping.

## Delivery

Stacked PRs with gh-stack, in this order:

1. Scene model and native rendering (`fontRuns`, shaping, baseline, warnings).
2. React API (`<Span>`, flattening, subtitles, TextReveal, measurement,
   `useTypewriter`).
3. Browser rendering.
4. Example and documentation.

Deliverables of the last layer: a new small example (`examples/rich-text/`)
showing an emphasized Japanese subtitle, an explanatory paragraph with mixed
weights and a brand-font word, a TextReveal headline with a span, and typing
with `visibleCharacters`; `skills/celesta/references/text.md` and
`dialogue.md`; the website docs (en/ja); and a handoff note
`docs/handoff/<date>-rich-text.md`.
