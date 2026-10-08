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
`<TextBox>`, `<Dialogue>`, and `<DialogueSeries>` lines; native (CPU/GPU)
rendering as the reference; browser rendering as close as practical.

Out of scope: per-span `fontSize`, gradient fills, strokes, italics (no
`fontStyle` exists), font stacks (#204), and any change to `@celesta/code`.

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

- `<Span>` is valid only as a descendant of a text container: `<Text>`,
  `<TextBox>`, `<Dialogue>`, or a `<DialogueSeries>` line's `text`. Rendered
  anywhere else (directly in a `<Group>`, say), it throws.
- Spans nest. Each property of an inner span overrides the outer value; unset
  properties inherit.
- Text children may be strings, numbers, arrays, fragments, and `<Span>`.
  `null`, `undefined`, and booleans render nothing (as `TextBox` already
  allows). Any other element, including a user component that returns a
  `<Span>`, throws: text children are read as props, not rendered by React.
  The error names what is allowed.
- `fill` that is not solid throws.
- A `<Text>` whose `style.colorRuns` is non-empty and whose children contain a
  span with `fill` throws, rather than defining a precedence between them.
- Offsets are Unicode code points, as `colorRuns` already uses.

### Subtitles

- `SubtitleRenderProps.text` stays the plain string.
- New `SubtitleRenderProps.content: ReactNode` is the line's children as given,
  spans included. A custom `render` keeps emphasis with
  `<Text style={style}>{content}</Text>`.
- `SubtitleRenderProps.metrics` is measured with the line's font runs.
- `DialogueLine.text` widens from `string` to `ReactNode`. Code reading
  `line.text` as a string through the default `DialogueLine` type needs a
  narrowing; a custom `L extends DialogueLine` that declares `text: string`
  is unaffected.

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
  `react-api.md` or the website beyond a note that `<Span>` produces it.
- Empty `fontRuns` serializes to nothing, so existing scenes and project files
  are unchanged. TypeScript bindings are regenerated.

## Data flow

1. **Flattening (React).** `extractText` (render.ts), `subtitleText`
   (components.ts), and `textBoxText` (text-fit.ts) become one
   `flattenTextChildren(children)` returning `{ text, colorRuns, fontRuns }`.
   It counts code points with `Array.from`, resolves inherited span styles,
   and merges adjacent runs with identical attributes. A run whose resolved
   values equal the text's own style is still emitted; the renderer treats
   it as a no-op.
2. **Scene.** The text layer's style gets the flattened runs merged in.
3. **Measurement.** `useTextMetrics`, `useFitText`, `TextBox`, and subtitle
   `metrics` send `fontRuns` with the request. `useTextMetrics` already keys
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
  first code point. A run boundary inside a cluster moves to the next cluster
  start, so combining marks and ZWJ sequences are never split between shaping
  runs. This matches `colorRuns`' rule (a shaped cluster uses its first code
  point's color).
- **Existing spans inherit run attributes.** The color-emoji span and the
  word-joiner span (letter spacing 0) are currently cloned from the base
  attributes. They must be derived from the attributes in effect at their
  position; otherwise a joiner inside a bold run would switch back to the
  regular face and split shaping there. Emoji spans keep the color emoji
  family with the run's matched weight.
- **Line height and baseline.** Line height stays fixed by `Metrics`. Where
  families mix in a line, the baseline follows cosmic-text's layout. Measure
  and rasterize use the same buffer, so they agree.
- **Kerning and ligatures** do not cross a font-run boundary, since the faces
  differ. They are kept inside runs and across color-only boundaries.
- **Font warnings.** `font_fallback` and `missing_glyphs` check only the
  text's own family today, so characters drawn from a span's family would be
  reported as missing. Both check each character against the family and
  weight in effect at its position and return one entry per family/weight
  (`Option` → `Vec`). The GPU renderer's de-duplication of reported entries is
  updated accordingly.

## Browser rendering

The browser runtime aims to match native but is not pixel-identical; native
is the reference.

- Text without `fontRuns` takes the existing path unchanged.
- **Wrapping and measurement.** `textLineRanges` and `textMeasurer` measure
  each run's piece with that run's font and sum the widths. Kerning is kept
  inside runs and lost across boundaries, as on native.
- **Drawing.** `styledLine` draws each run's piece with its own `ctx.font`
  (`fillText`, and `strokeText` for the outline) at the x of its first
  character. Character extents come from an SVG `<text>` with one `<tspan>`
  per run, so color regions and reveals keep working from the same extents.
- **Open question.** `getExtentOfChar` across `<tspan>` children is
  unverified. It is checked in real Chrome first; if it does not hold, x
  positions come from accumulated per-run `measureText` widths instead.
- **Documented differences.** Kerning at run boundaries, baseline placement
  in lines that mix families, and the existing platform font differences.

## Compatibility

- Text without spans produces the same scene as before.
- `@celesta/code` keeps using `colorRuns` and does not change.
- `DialogueLine.text` widens to `ReactNode` (see Subtitles).
- `font_fallback` / `missing_glyphs` return types change inside the Rust
  workspace; their callers (gpu-renderer, exporter, editor) are updated.

## Testing

- **Renderer (Rust).** Family runs with the repository's Bebas Neue and IBM
  Plex Mono fonts: advances change, measured width equals rasterized width,
  and the shaping cache separates texts that differ only in font runs while
  still sharing across color-only changes. Weight runs are asserted where the
  system has a bold face and skipped otherwise, like the color emoji tests.
  Also: a boundary inside a combining sequence, phrase line breaking with a
  run (no split at joiners), emoji inside a run, invalid ranges, and warnings
  for a span family that lacks characters.
- **GPU.** Font-run text matches the CPU reference, as existing text tests do.
- **React.** Offsets with surrogate pairs, nested spans, merged adjacent runs,
  the three errors (gradient fill, foreign element, `colorRuns` conflict),
  `Dialogue` `content` and `metrics`, `TextBox` fitting with a bold span, and
  `DialogueSeries` lines with spans.
- **Web.** `text-runs.mjs` in real Chrome with bold and family runs (coverage,
  wrapping, color and reveal over runs), plus unit tests for run-aware
  wrapping.

## Deliverables

- Examples: a Japanese subtitle with emphasis and an explanatory paragraph
  with mixed weights and a brand-font word. Whether this is a new example or
  a `feature-tour` chapter is decided in the plan.
- `skills/celesta/references/react-api.md`, the website docs (en/ja), and a
  handoff note `docs/handoff/<date>-rich-text.md`.
