# Text, fonts, and measuring

Drawing text in a React composition: the `<Text>` layer, `TextStyle`,
fonts and fallback, line breaking (including Japanese phrases), text
animation helpers, measuring text, and fitting text into a box. `Text` and
measuring come from `@celesta/react`; `TextReveal`, `useTypewriter`,
`useCountUp`, `TextBox`, `useFitText`, `fitText`, the emphasis ranges, and
the timed ranges from `@celesta/text`;
`TitleCard`, `LowerThird`, and `TypewriterText` from `@celesta/components`.
Syntax-highlighted code is in [code.md](code.md).

## Contents

- [The Text layer](#the-text-layer)
- [Shorthand props and `<Theme>`](#shorthand-props-and-theme)
- [Rich text with `<Span>`](#rich-text-with-span)
- [Emphasis ranges](#emphasis-ranges): emphasisContent, resolveEmphasis, graphemes
- [Timed ranges](#timed-ranges): resolveTimedRanges, timedRangeAt
- [TextStyle](#textstyle)
- [OpenType features](#opentype-features): ligatures, alternates, `fontFeatures`
- [Line breaking](#line-breaking)
- [Fonts, fallback, and emoji](#fonts-fallback-and-emoji)
- [Font stacks (和欧混植)](#font-stacks-和欧混植)
- [Language and fallback fonts](#language-and-fallback-fonts)
- [Text motion](#text-motion): TextReveal, TypewriterText, useTypewriter, useCountUp
- [Measure text](#measure-text): useTextMetrics, measureText
- [Fit text into a box](#fit-text-into-a-box): TextBox, useFitText, fitText
- [Title cards](#title-cards): TitleCard
- [Lower thirds](#lower-thirds): LowerThird
- [Verifying text](#verifying-text)

## The Text layer

| Prop | Notes |
| --- | --- |
| children | Strings, numbers, and [`<Span>`](#rich-text-with-span) elements (arrays and fragments of those are joined); `null`, `undefined`, and booleans draw nothing. Use template literals to combine values: `` {`${name} · Lv.${level}`} ``. `\n` breaks a line. No other elements inside `Text`. |
| `style` | A `TextStyle`, see below. |
| `fontFamily`, `fontSize`, `fontWeight`, `color` | Shorthand for `style`; see [Shorthand props and `<Theme>`](#shorthand-props-and-theme). |
| `lang` | Text language (e.g. `"zh-Hant"`), overriding enclosing scopes; `style.lang` takes priority. |
| `maxWidth` | Wrap lines to fit this width; `style.align` positions each line inside it. A word wider than `maxWidth` is not split; the part past `maxWidth` is cut off. See [Line breaking](#line-breaking). |
| common props | `x`, `y`, `anchorX`, `anchorY`, `opacity`, effects, … |

Anchoring:

- Single-line text is anchored vertically by its visible glyph bounds, so
  `anchorY={0.5}` centers the letters themselves. Horizontally it keeps its
  advance width, so leading and trailing spaces take up room.
- Text with several lines, whether broken with `\n` or wrapped at
  `maxWidth`, is anchored by its line boxes instead: with `anchorY={0}` the
  first line box's top is at `y` and the first baseline `ascent` below it,
  exactly where [`useTextMetrics`/`measureText`](#measure-text) put them.
- `anchorY="baseline"` anchors the text on its first line's baseline
  instead. Use it to line up separate `Text` layers (per-character
  animation, mixed font sizes) at one `y`, and with measured layouts
  (`y={padding + metrics.ascent}`). For a differently colored or weighted
  word inside a line, use [`<Span>`](#rich-text-with-span) instead.

## Shorthand props and `<Theme>`

`fontFamily`, `fontSize`, and `fontWeight` on `<Text>` set the same
`style` entries; `color` sets a solid `style.fill`. Where a prop and
`style` set the same thing, `style` wins. Both spellings draw the same
text:

```tsx
<Text fontFamily="Inter" fontSize={18} fontWeight={700} color="#F4F1EA">A</Text>
<Text style={{ fontFamily: 'Inter', fontSize: 18, fontWeight: 700, fill: { type: 'solid', color: '#F4F1EA' } }}>A</Text>
```

`<Theme>` names fonts and colors for everything inside it, so a film's
palette and type live in one place instead of a `Label` wrapper:

```tsx
<Theme fonts={{ display: 'Inter', mono: ['JetBrains Mono', 'Noto Sans JP'] }}
       colors={{ paper: '#F4F1EA', ink: '#111111', accent: '#FF4D1F' }}>
  <Rect width={1920} height={1080} fill="ink" />
  <Text x={56} y={56} fontFamily="mono" fontSize={18} fontWeight={700} color="paper">
    CELESTA <Span color="accent">REEL</Span>
  </Text>
</Theme>
```

- A font name may stand for a [font stack](#font-stacks-和欧混植). Colors
  are `#RRGGBB` or `#RRGGBBAA`; anything else in a `<Theme>` throws.
- Font names work in:
  - `<Text>`'s `fontFamily` and `style.fontFamily`;
  - a `<Span>`'s `fontFamily` and `style.fontFamily`;
  - the `fontFamily` of the components below and of `@celesta/components`.
- Color names work in:
  - `<Text>`'s `color`, and `style.fill`, `style.stroke` paints and
    `style.colorRuns`;
  - a `<Span>`'s `color` and `style.fill`;
  - `<Rect>` and `<Path>` `fill` and `stroke`, and so `<Line>`,
    `<Polyline>`, `<Circle>`, `<Ellipse>`, `<Arrow>`, `<Triangle>`,
    `<Polygon>`, `<Star>`, `<Pie>`, and `<Heart>` from `@celesta/shapes`;
  - the color props of `@celesta/components` (`Panel`, `Badge`,
    `StatCard`, `PatternBackground`, …; each prop's doc says so).

  A paint's color and each gradient stop's color are looked up alike.
- A name no theme defines is used as it is: a font name as a family name
  (so `fontFamily="Georgia"` still works inside a theme), a color as a
  color. The `color` shorthand, a shape's `fill` and `stroke`, and the
  components' colors must then be hex colors, or the element throws with
  its name and the theme's color names:
  `<Rect> fill "inc" is neither a #RRGGBB or #RRGGBBAA color nor a <Theme> color name (paper, ink, accent)`.
  The error comes from the frame that draws the element; to find such
  mistakes in every frame before exporting, run
  `inspect.mjs film.tsx --every 1` ([verify.md](verify.md#inspectmjs)).
  `style.fill` and the other text paints are not checked here: a name they
  do not find reaches the renderer, which fails the frame.
- A nested `<Theme>` adds to the one around it, its own names winning. It
  passes through `<Group>`, `<Sequence>`, and every other component, like
  any React context.
- `useTheme()` returns the merged `{ fonts, colors }`, for anything that
  does not look names up, such as `interpolateColor()` or `<Instances>`:
  `interpolateColor(t, [0, 1], [colors.ink, colors.accent])`.
- `<TextBox>`, `<TextReveal>`, `useTypewriter()`, and `<Dialogue>`
  (its line and its character's `subtitle.style`) look up the names of
  the `<Theme>`s around them in their `style` and spans, as `<Text>` does.
- `useTextMetrics()` and `useFitText()` measure with the names of the
  `<Theme>`s around them. `measureText()` and `fitText()` run in
  `prepare()`, outside every `<Theme>`, so pass the names in their
  options: `measureText('M', { fontFamily: 'mono' }, { theme: { fonts } })`.

See [`packages/cli/examples/with-theme.tsx`](../../../packages/cli/examples/with-theme.tsx)
and [`examples/reel`](../../../examples/reel/film.tsx).

## Rich text with `<Span>`

`<Span style={{ fill, fontWeight, fontFamily, fontFeatures }}>` changes part of a text.
The whole text is still shaped as one paragraph: it wraps, aligns, and
measures as one, and its baseline stays where the text's own glyphs put it.

```tsx
<Text style={{ fontSize: 48, fill: { type: 'solid', color: '#ffffff' } }}>
  速い、<Span style={{ fill: '#28A34A', fontWeight: 700 }}>カンタン</Span>、頼もしい。
</Text>
```

- `fill` is a hex color or a solid paint; gradients are rejected.
- `fontWeight` uses the family's nearest face when it has no face at that
  weight, as the text's own `fontWeight` does. No bold is synthesized.
- `fontFamily` must be installed or loaded with `<Font>`, like the text's.
  It may be a [font stack](#font-stacks-和欧混植), which replaces the text's
  whole stack inside the span.
- `fontFeatures` turns [OpenType features](#opentype-features) on or off
  inside the span, merged tag by tag over the text's and outer spans'.
- Spans nest; an inner span overrides only the properties it sets.
- `fontFamily`, `fontWeight`, and `color` props are shorthand for the
  span's `style`, which wins where both set the same thing; inside a
  [`<Theme>`](#shorthand-props-and-theme) they take its names:
  `<Span color="accent">`.
- `<Span>` works in `<Text>`, `<TextBox>`, `<TextReveal>`, `<Dialogue>`,
  `DialogueSeries` lines, `useTextMetrics`, `measureText`, `useFitText`,
  `fitText`, and `useTypewriter`. Anywhere else it throws, and so does a
  component that returns a `<Span>` inside text: text children are read,
  not rendered.
- A grapheme cluster (a letter with its accents, an emoji sequence) takes
  the style at its first character.
- Kerning and ligatures do not cross a change of weight, family, or
  features; they do cross a change of color.
- Set `lang` on Japanese, Chinese, or Korean text with spans (or on the
  composition). A weight or family span splits shaping, and fallback fonts
  are chosen per shaped piece: without a language, the characters next to a
  span can fall back to a different font than the rest of the phrase (for
  example, a Chinese font for kanji), which also shifts the line.
- `<Span fill>` cannot be combined with `style.colorRuns` on the same text.
- The browser preview draws each span with its own font and can differ
  slightly from export at span edges.

See [`examples/rich-text`](../../../examples/rich-text/film.tsx).

## Emphasis ranges

For text that comes as data (captions, dialogue lines), `@celesta/text`
marks emphasis as ranges instead of `<Span>` elements, and turns them into
spans. `<CaptionTrack>` and `<Dialogue>` subtitles take the same shape.

```tsx
import { emphasisContent } from '@celesta/text';

const styles = { danger: { color: '#ff6060', fontWeight: 700 } };
<Text style={captionStyle}>
  {emphasisContent('HPが1です。', [{ start: 3, end: 4, style: 'danger' }], { styles, theme: useTheme() })}
</Text>
```

- `TextEmphasis` is `{ start, end, style }`: grapheme clusters `[start,
  end)`, counted from 0. `graphemes(text)` splits a text the same way: an
  emoji sequence (`👨‍👩‍👧`), a flag (`🇯🇵`), a letter with combining marks
  (`é`), or a surrogate pair (`𠮷`) is one cluster, so a range can
  never cut one.
- `style` is an `EmphasisStyle`, `{ color?, fontWeight? }`, or the name of
  one in `options.styles`. `color` is a hex color or, with `options.theme`
  (`useTheme()`), a `<Theme>` color name. An emphasis with an empty style
  changes nothing but is still reported, for a custom render to style.
- `resolveEmphasis(text, emphasis, { styles?, theme?, what? })` checks the
  ranges and returns them sorted, each with its `text`, its style (color
  resolved to hex), and its `name` when it named one. It throws, naming
  `what` and the emphasis by index, for a range that is empty, not whole
  numbers, or outside the text's clusters; ranges that overlap; an unknown
  style name; a style property other than `color` and `fontWeight`; and a
  color or weight a `<Span>` cannot draw.
- `emphasisContent(text, emphasis, options)` returns the text with a
  `<Span>` per range (the text itself when there are none), for `<Text>`,
  `<TextBox>`, `useTextMetrics`, or anything else that takes text. Draw the
  whole text in one `<Text>` so it still wraps and keeps one baseline.
- `sliceEmphasis(resolved, start, end)` cuts resolved emphasis to clusters
  `[start, end)` and counts it from `start`: the emphasis of one page or
  line of a longer text.
- Emphasis needs the text as a string. Text that already holds `<Span>`
  uses spans throughout instead.

## Japanese phrases

`phrases(text)` from `@celesta/text` splits Japanese into the phrases
(文節) `lineBreak: 'phrase'` keeps on one line, with the same BudouX model
the renderer wraps by; `phraseBoundaries(text)` gives where they start, as
code-point offsets. Joined, the phrases are the text again. Text without
Japanese stays whole: split it at spaces yourself.

```ts
phrases('世界中の情報を整理し、') // ['世界中の', '情報を', '整理し、']
```

`useTextMeasurer()` returns a function that measures text during render,
`(content, style, maxWidth?) => TextMetrics`, as `useTextMetrics()` does,
for a component that has to measure a number of texts it only knows while
rendering (candidate pages of a caption, say). Each call measures again, so
keep the results in `React.useMemo()`.

## Timed ranges

`resolveTimedRanges(items, options)` checks things shown for a while
(captions, subtitle pages) and `timedRangeAt(ranges, frame)` finds the one
on a frame. `<CaptionTrack>` and dialogue subtitle pages use them, so they
share these rules:

- An item `{ from, durationInFrames? }` is shown on frames
  `[from, from + durationInFrames)`: on `from`, not on `from +
  durationInFrames`. `from` is a whole number and `durationInFrames` a whole
  number of at least 1.
- Without `durationInFrames`, an item lasts until the next item's `from`;
  the last one until `options.end`. Without `end`, every item but the ones
  something follows needs a duration.
- Frames no item covers (before the first, between two, after the last)
  have none: `timedRangeAt` returns null.
- With `end`, items must fit `[0, end)`. Without it they may sit anywhere,
  including before frame 0, as a `<Sequence>` may.
- Overlapping items throw unless `overlap: 'latest'`, which lets the one
  starting later (or, starting together, the one later in the list) cut
  the other short. Touching items (one ends where the next starts) do not
  overlap.
- Items are sorted by `from`; `ordered: true` requires them listed in order.
- Errors name `options.what` and the item: `label(index)`, or `item <index>`.

See [`packages/cli/examples/with-text-emphasis.tsx`](../../../packages/cli/examples/with-text-emphasis.tsx).

## TextStyle

```ts
type TextStyle = {
  fontFamily?: string | readonly string[];  // installed family, or one loaded with <Font>; a list is a font stack
  fontSize?: number;       // px, > 0
  fontWeight?: number;     // 100–900
  fill?: Paint;                                   // { type: 'solid', color } or a gradient
  stroke?: Stroke | readonly Stroke[];            // outline(s); Stroke = { paint: Paint; width: number }
  align?: 'left' | 'center' | 'right';  // each line within maxWidth, or within the widest line without it
  lineHeight?: number;     // px, > 0 (TextBox's lineHeight is a multiple instead)
  letterSpacing?: number | { em: number };  // px, or a multiple of fontSize, after each glyph; may be negative
  fontFeatures?: { [tag: string]: boolean | number };  // OpenType features, e.g. { liga: false, calt: false }
  lineBreak?: 'normal' | 'phrase';  // where maxWidth may wrap; default 'normal'
  lang?: string;           // language for fallback fonts, e.g. 'ja-JP'; overrides inherited lang
  fontFallback?: 'system' | 'loaded';  // 'loaded': <Font> fonts before system fonts; overrides inherited
};
```

A list of strokes draws several outlines around one another, such as the
white inner and colored outer outline common in subtitles. Each `width` is
measured from the glyph outline, so the outer stroke is the wider one; the
order of the list does not matter:

```tsx
stroke: [
  { paint: { type: 'solid', color: '#2E6B1F' }, width: 14 },  // outer
  { paint: { type: 'solid', color: '#FFFFFF' }, width: 7 },   // inner
],
```

`fill` and stroke `paint`s are paint objects, never bare hex strings:
`fill: { type: 'solid', color: '#ffffff' }`. Gradients are described in
[react-core.md](react-core.md#rect-and-paints-gradients); their coordinates
are relative to the text's layout box. The same style fields are used by
character subtitles and `.celesta.json` text items. `<Span>` writes
`colorRuns` and an internal `fontRuns` field; set them through `<Span>`.

`letterSpacing: { em: -0.02 }` tightens by 2% of `fontSize` and follows
the size wherever it changes: in `<TextBox>`, `fitText()`, and
`useFitText()` each size tried is measured with its own spacing, so the
size chosen is the largest that fits with that spacing. A plain number
stays the same px at every size. `<Span>` has no size of its own, so its
text takes the same spacing.

## OpenType features

`fontFeatures` turns a font's OpenType features on and off, as CSS
`font-feature-settings` does. A coding font such as JetBrains Mono joins
`===`, `->`, and `!=` into one symbol through its `liga` and `calt`
features; turn them off to draw code as it is written:

```tsx
<Font google="JetBrains Mono" weights={[400]} />
<Text style={{ fontFamily: 'JetBrains Mono', fontSize: 48, fontFeatures: { liga: false, calt: false } }}>
  {'a === b'}
</Text>
```

- Each key is a feature tag of four ASCII characters: `liga` (standard
  ligatures), `calt` (contextual alternates), `dlig`, `kern`, `smcp`,
  `zero`, `tnum`, `frac`, `ss01`–`ss20`, `cv01`–`cv99`, and any other tag a
  font defines. Each value is `true`, `false`, or the number of an
  alternate (`salt: 2`). A tag the font does not have changes nothing.
- A malformed tag (`lig`, `ligatures`) or value (`-1`, `1.5`, `'off'`)
  throws at the `<Text>`, `<Span>`, or measurement that sets it.
- `<Span style={{ fontFeatures }}>` changes features for part of a text,
  merged tag by tag over the text's: inside `{ liga: false }` text,
  `<Span style={{ fontFeatures: { liga: true } }}>` turns ligatures back on
  for that part only, even in the middle of a word.
- Measuring (`useTextMetrics`, `measureText`, `fitText`, `<TextBox>`)
  shapes with the same features, so measured and drawn widths agree.
- `<Code>` draws with the font's ligatures; pass the same `fontFeatures`
  in its `style` to show code as written ([code.md](code.md)).
- The browser preview applies features only to fonts loaded with
  `<Font>`, from a file or from a font stylesheet's `local()` fonts: text
  with `fontFeatures` and no `fontFamily`, or with an installed family
  named directly, throws there (export applies them to installed fonts
  too). Characters a fallback font draws do not get them in the preview.

## Line breaking

With `maxWidth`, lines break where Unicode line breaking (UAX #14) allows:
at spaces in Latin text, and between most characters in Japanese and
Chinese, which keeps punctuation such as `、` and `。` off the start of a
line.

`lineBreak: 'phrase'` wraps Japanese only between phrases (文節), found with
[BudouX](https://github.com/google/budoux)'s Japanese model, so a word such
as `フレーム` or a trailing `の。` is not split across lines. **Use it for
Japanese titles and subtitles.** Lines still break at spaces, at `\n`, and
at explicit break characters (an ideographic space `　`, a dash), so
`第一章　はじめに` can wrap after `　`. A phrase wider than `maxWidth` wraps
inside itself as `'normal'` text would. It works the same for `<Text>`,
character subtitles (`subtitle={{ maxWidth, style: { lineBreak: 'phrase' } }}`),
`TextBox`, `<TextReveal maxWidth>`, `measureText()`/`useTextMetrics()`, and
`.celesta.json` text styles. The browser canvas renderer (`@celesta/web`) ignores it and wraps
only at whitespace.

## Fonts, fallback, and emoji

Load font files with `<Font src>` inside `<Assets>` (see
[react-core.md](react-core.md#font-and-assets)) and refer to them by the
**family name stored inside the font**, not the file name. For reproducible
output, ship the font file next to the entry rather than relying on
installed fonts.

- Weights match within the family first: when `fontFamily` has no face at
  the requested `fontWeight`, its nearest weight is used, picked as CSS font
  matching does. A family loaded only in Bold therefore draws `fontWeight`
  400, or no weight, in Bold. Bold is never synthesized.
- With Google Fonts, list every weight you use in the URL; a missing weight
  uses the family's nearest one. A `text=` subset (`<Font google text>`)
  must include every character the video draws.
- A `fontFamily` with no installed or loaded face falls back to another
  font. The app lists it with the preview's warnings, and the exporter
  prints `warning: font family "…" (weight …) is not installed or loaded;
  text layer "…" uses a fallback font`, once per family and weight.
- Characters the family lacks fall back per glyph to an installed system
  font chosen for the text's [`lang`](#language-and-fallback-fonts), or
  are drawn as a missing-glyph box (tofu) when no font has them. The
  exporter prints `warning: font family "…" (weight …) has no glyph for
  "…"; text layer "…" draws them with a fallback font` (the first 10
  characters, then `and N more characters`). Emoji drawn by a color emoji
  font, whitespace, and invisible characters are not reported.
- **By default, fonts loaded with `<Font>` are not fallback candidates.**
  With `fontFamily: 'JetBrains Mono'` and Noto Sans JP loaded, Japanese is
  still drawn with a system font (Hiragino Sans on macOS), so the result
  depends on the machine that exports. Either list the loaded font in a
  [font stack](#font-stacks-和欧混植), or set `fontFallback="loaded"`:

  ```tsx
  <Composition width={1920} height={1080} fps={30} durationInFrames={90}
    lang="ja-JP" fontFallback="loaded">
    <Font google="Noto Sans JP" />
    <Text style={{ fontFamily: 'JetBrains Mono' }}>let 名前 = "値";</Text>
  </Composition>
  ```

  A character no family of the stack has is then drawn from the fonts the
  composition loads, tried in `<Font>` declaration order (the first with
  every character of the cluster wins), and from a system font only when
  none has it. Characters a loaded font draws are not reported as missing.
  It applies only to text with a `fontFamily` (its own or a `<Span>`'s).
  `measureText()`/`fitText()` in `prepare()` run outside the composition,
  so pass `fontFallback: 'loaded'` in their style there, as with `lang`.
  `<Group>` and `<Sequence>`
  take `fontFallback` too and their children inherit it, like `lang`;
  `style.fontFallback` takes priority. Measuring and wrapping follow it.
  The browser preview appends the loaded families to the CSS font list,
  so the browser picks per character rather than per cluster, and a
  combining mark can come from a different font than its base.
- Emoji meant to look like emoji (🎉, a character followed by U+FE0F such
  as ❤️ or 1️⃣, flags, skin tones, ZWJ sequences) are drawn with the first
  installed color emoji font of Apple Color Emoji, Segoe UI Emoji, Noto
  Color Emoji, Twemoji Mozilla, Twemoji, Twitter Color Emoji, JoyPixels, and
  EmojiOne Color (a `<Font>` with one of these families counts), even when a
  text font has a plain glyph for them. U+FE0E keeps a character as text.
- Minimal Linux containers may have no fonts at all; load files with
  `<Font>`.

`scripts/inspect.mjs` does not check fonts. Export a PNG frame and read the
exporter's warnings (see [verify.md](verify.md#look-at-real-frames)).

## Font stacks (和欧混植)

`fontFamily` also takes a list of families, a font stack. Each grapheme
cluster is drawn with the first family in the list that has a glyph for it,
so Latin can come from one font and Japanese from the next:

```tsx
<Composition width={1920} height={1080} fps={30} durationInFrames={90} lang="ja-JP">
  <Assets>
    <Font src="https://fonts.googleapis.com/css2?family=Inter:wght@500" />
    <Font src="https://fonts.googleapis.com/css2?family=Noto+Sans+JP:wght@500" />
  </Assets>
  <Text style={{ fontFamily: ['Inter', 'Noto Sans JP'], fontWeight: 500, fontSize: 64 }}>
    Celesta 0.5.0 で字幕の組版を改善
  </Text>
</Composition>
```

- A cluster takes the first family with a glyph for every visible
  character of it, so a combining mark or a variation selector is never
  drawn from another font than its base character. When no family has the
  whole cluster, it takes the first family with its first character; a
  system font that has the whole cluster may draw it instead, as with one
  family, so the export can differ between operating systems.
- Families with no installed or loaded face are skipped. The stack's
  **primary family** is its first family with a face.
- Characters no family in the stack has fall back to a system font, as
  with one family (loaded fonts are not candidates; see above).
- Baselines come from the primary family, as in CSS: neither glyphs from
  later families nor fallback glyphs (an emoji, a character no family has)
  move a line, so a paragraph keeps even line spacing whether or not a
  line has Japanese. Their ink may reach above or below the line box when
  their font is taller. With one family, fallback glyphs still move their
  line, as before.
- Emoji stay with the color emoji font, unless the stack itself lists a
  color emoji family that has them.
- Weights match within each family on its own: with `fontWeight: 400` and
  Noto Sans JP loaded only at 500, the Japanese is drawn at 500.
- Measuring (`useTextMetrics`, `measureText()`, `useFitText`, `TextBox`),
  wrapping, and drawing use the same choice, and so does `@celesta/code`:
  `<Code style={{ fontFamily: ['JetBrains Mono', 'Noto Sans JP'] }}>` draws
  Japanese comments and strings in Noto Sans JP, and its line highlight
  covers them. Mixing a proportional font into code breaks column
  alignment, so keep the first family monospaced.
- `<Span style={{ fontFamily: [...] }}>` takes a stack too, which replaces
  the text's stack inside the span.
- Name real families. CSS generic names such as `sans-serif` and `serif`
  have no face in export (`fontFamily: 'sans-serif'` alone already gets
  the fallback warning), so a stack skips them.
- Warnings judge the whole stack: `none of the font families "A", "B"
  (weight …) is installed or loaded` when no family has a face, and `none
  of the font families "A", "B" (weight …) has a glyph for "…"` for
  characters no family has. A family missing from a stack whose other
  families have faces is no warning; check the exporter's warnings when
  the Japanese comes out in a system font.
- The browser preview (`@celesta/web`) passes the stack to the browser as
  a CSS `font-family` list. The browser picks families per character with
  its own matching and falls back to its own system fonts, so a character
  no family has can look different from export.

## Language and fallback fonts

Han characters (漢字) look different in Japanese, Simplified Chinese,
Traditional Chinese, and Korean fonts. `lang` tells Celesta which language
text is in, so a fallback font for that language is used when `fontFamily`
is omitted or lacks a character. **Set it for any CJK video:**

```tsx
<Composition width={1920} height={1080} fps={30} durationInFrames={90} lang="ja-JP">
  <Text>漢字とかな</Text>
  <Group lang="zh-Hant">
    <Text y={80}>繁體中文</Text>
    <Text y={160} lang="ko-KR">한국어</Text>
  </Group>
</Composition>
```

- Children inherit the nearest `Composition`, `Group`, or `Sequence` with a
  `lang`, including subtitles, project timeline text, `useTextMetrics()`,
  `useFitText()`, and `TextBox`. Scopes may change language between frames;
  measuring and drawing use the same value.
- `<Text lang>` overrides ancestors; `style.lang` takes priority over that.
  An empty `lang` selects the system locale, which is also the default.
- Supported tags include `ja-JP`, `ko-KR`, `zh-Hans`, `zh-Hant`, and
  `zh-Hant-HK`.
- `prepare()` runs before `<Composition>` mounts, so pass the language to
  `measureText()`/`fitText()` there: `measureText('漢字とかな', { lang: 'ja-JP' })`.
- `lang` only picks fallback fonts; the glyphs available still depend on
  installed or loaded fonts, and `lineBreak: 'phrase'` always uses the
  Japanese model. Loading a font for the language with `<Font>` and naming
  it in `fontFamily` (alone, or after a Latin font in a
  [font stack](#font-stacks-和欧混植)) remains the reproducible choice.

## Text motion

### `<TextReveal>`

Each line of a string slides up from behind its own mask (`Group clip`),
staggered. Props: `children` (text, which may hold `<Span>`; a span crossing
a line break continues on the next line), `style`, `maxWidth` (wrap width,
see below), `lineHeight` (mask height and line spacing; defaults to
`style.lineHeight`, then `fontSize`), `baseline` (0.8: baseline position
inside each line box), `align` (0/0.5/1 pivot of each line), `from`,
`stagger` (4), `durationInFrames` (20), `easing` (`easeOutExpo`),
`direction` (`'in'` or `'out'`), plus common props. `x`/`y` are the
top-left of the first line box.

- Without `maxWidth`, lines break only at `\n`: a long line runs past the
  frame instead of wrapping.
- With `maxWidth`, lines also wrap at that width, at the same places as
  `<Text maxWidth>` with the same `style` and content, and each wrapped
  line is revealed on its own. `\n` still forces a break. Set
  `style.lineBreak: 'phrase'` to wrap Japanese only between phrases (文節),
  so copy that changes (a project property, a translated title) wraps
  without hand-placed `\n`. The spaces at a wrap are dropped, so `align`
  pivots the visible text.
- Each line is still drawn as its own single-line `Text` at `x`, pivoted by
  `align`; `style.align` and the `maxWidth` box do not move it. For
  centered lines, use `align={0.5}` with `x` at the center.
- `maxWidth` measures the text during render, like
  [`useTextMetrics`](#measure-text), so it needs the editor, exporter, or
  browser preview runtime. The browser preview ignores `lineBreak` (see
  [Line breaking](#line-breaking)), so its breaks can differ from export.

```tsx
<Composition width={1080} height={1920} fps={30} durationInFrames={90} lang="ja-JP">
  <TextReveal x={540} y={700} align={0.5} maxWidth={880} lineHeight={150}
    style={{ fontFamily: 'Noto Sans JP', fontWeight: 900, fontSize: 120, lineBreak: 'phrase' }}>
    {getProjectProperty('headline')}
  </TextReveal>
</Composition>
```

### `<TypewriterText>`

Text that types itself out with a caret, from `@celesta/components`. It
times the typing with `useTypewriter` (same `from`, `framesPerChar`, and
`blinkFrames`, so it shows what the hook returns on every frame), draws the
complete text once with `style.visibleCharacters`, and places the caret with
`textCaret()` on that layout. No monospaced font is needed: the caret
follows proportional fonts, Japanese, `\n`, `maxWidth` wrapping, and
`style.align`.

```tsx
import { TypewriterText } from '@celesta/components';

<TypewriterText x={160} y={300} from={12} framesPerChar={2}
  caretShape="block" fontFamily="mono" fontSize={40} color="paper">
  pnpm run build
</TypewriterText>
```

Props: `children` (text, which may hold `<Span>`), `style`, the shorthand
`fontFamily`/`fontSize`/`fontWeight`/`color`, `lang`, `maxWidth`, `from`
(0), `framesPerChar` (1), `blinkFrames` (one second), `caret` (`true`),
`caretShape` (`'line'` or `'block'`), `caretColor` (hex or `<Theme>` name;
defaults to the text's solid fill, else white), `caretWidth`,
`caretHeight`, plus common props.

- **Placement**: `anchorX`/`anchorY` pivot on the finished text's box as
  `measureText()` reports it (0 the top of the first line box, 1 the bottom
  of the last), or `anchorY="baseline"` on the first baseline. The box is
  the finished text's, so nothing shifts while it types: `anchorX={0.5}`
  centers the whole line from the first frame.
- **Caret size**: a line is 6% of the font's line height wide (at least
  2px); a block is one `0` wide, one cell in a monospaced font. Both are as
  tall as the font's own line, centered on the line box, so a larger
  `style.lineHeight` does not stretch them.
- **Before `from`** nothing is typed and the caret blinks at the start;
  **while typing** it stays on after the last typed character; **once
  done** it blinks after the last character. Empty text draws only the
  blinking caret. To hide the caret from some frame on, compute `caret`
  (`caret={frame < 60}`).
- **Units**: one Unicode code point per step, as `useTypewriter` counts.
  Emoji with several code points (ZWJ sequences, flags, skin tones) and
  combining marks take several steps; between them the caret sits partway
  across the cluster.
- **Wrapping**: at a `maxWidth` wrap, the caret moves to the next line once
  the character before the wrap is typed. After a typed `\n`, it is at the
  start of the next line.
- **Browser preview**: the browser measurer has no glyph positions, so the
  caret is measured from the typed part of its line instead, without
  `<Span>` fonts.

### `useTypewriter(text, { from?, framesPerChar?, blinkFrames? })`

Returns `{ text, length, done, caretVisible }`. Counts by code point, so kana
and emoji count once. `framesPerChar` below 1 types several characters per
frame. The caret is steady while typing and blinks (period `blinkFrames`,
default one second) otherwise. For typed text with a caret, use
[`<TypewriterText>`](#typewritertext). For code, pass `length` to
`<Code visibleCharacters>` ([code.md](code.md)).

`text` may hold `<Span>`; `text` in the result is then the plain string. To
type rich text without changing its layout, pass `length` to
`style.visibleCharacters` of a `<Text>` with the same content:

```tsx
const content = <>速い、<Span style={{ fontWeight: 700 }}>カンタン</Span>。</>;
const { length } = useTypewriter(content, { from: 10 });
return <Text style={{ ...style, visibleCharacters: length }}>{content}</Text>;
```

### `useCountUp(to, { from?, delay?, durationInFrames?, easing?, decimals? })`

A number that counts to `to` (default 30 frames, `easeOutExpo`), rounded to
`decimals`. Format it yourself, e.g. `value.toLocaleString('en-US')`, or
draw it with `<Counter>` from `@celesta/components`, which counts with the
same defaults and prints with a fixed locale, decimals, and a prefix or
suffix; `<StatCard>` and `<BarChart>` print theirs the same way. See
[react-core.md](react-core.md#counter-statcard-barchart-ranking-numbers-and-bar-charts).

## Measure text

### `useTextMetrics(text, style?, { maxWidth?, fonts?, theme? }?)`

Shapes text exactly as `<Text>` does, synchronously, during render, and
returns
`{ width, height, ascent, descent, lineHeight, capHeight, xHeight, lines, glyphs: [{ text, x, width, line, inkAscent, inkDescent }] }`
in composition pixels. Use it to size backgrounds and place carets without
guessing widths. `text` may hold `<Span>`, measured with the spans' weights
and families. Font names from the [`<Theme>`](#shorthand-props-and-theme)s
around it work in `style` and spans; `theme` adds more.

- `width` is the advance width, including spaces. `height` is the line-box
  height (`<Text>` that fits on one line trims empty rows above and below
  its glyphs; text with several lines keeps its line boxes).
  `ascent` runs from the first line's top to its baseline. `glyphs` has one
  entry per cluster, so per-letter layout is `glyphs[i].x`.
- `glyphs[i].width` is the advance width **with kerning applied**: a pair's
  tightening is taken out of the first glyph's advance, so in "AV" the A is
  narrower than a lone "A" and the gap `next.x - (x + width)` is always 0.
  To show kerning, measure each letter alone and compare.
- Vertical extents, all in pixels from the baseline: `capHeight` and
  `xHeight` (of the first glyph's font; 0 if it has none), and per cluster
  `inkAscent` / `inkDescent`, how far the drawn outline reaches above and
  below (0 for a space). Fit a band or underline to a word with
  `y = baseline - Math.max(...ink.map(g => g.inkAscent))` down to
  `baseline + Math.max(...ink.map(g => g.inkDescent))`, so no per-font
  constants are needed; a Japanese run's ink gives its glyph-face extent.
- Pass the same `style` and `maxWidth` to the hook and the `<Text>`, and
  place the text at `y={padding + metrics.ascent}` with
  `anchorY="baseline"`. This lines up one line and several alike, so shapes
  placed from `glyphs` (`y = padding + line * lineHeight`) sit on the text.
- `<Font>` declarations in the composition are included automatically, even
  when they appear after the measuring component. Relative font paths
  resolve against the entry file. `fonts` lists extra files to load for
  measurement only; also declare them with `<Font>` so drawing uses them.
- Unchanged text, style, width and fonts reuse the last result. Call it at
  the component's top level with a stable number of calls, like any hook.
- Each `<Text>` shapes separately: for a word in another color, weight, or
  family, use one `<Text>` with [`<Span>`](#rich-text-with-span) rather than
  separate layers placed by measured widths.

A pill that grows with its label:

```tsx
const style = { fontFamily: 'sans-serif', fontSize: 40 };
function Pill({ label }: { label: string }) {
  const m = useTextMetrics(label, style);
  const height = m.height + 24;
  return <Group>
    <Rect width={m.width + 48} height={height} cornerRadius={height / 2} fill="#28A34A" />
    <Text x={24} y={12 + m.ascent} anchorY="baseline"
      style={{ ...style, fill: { type: 'solid', color: '#ffffff' } }}>{label}</Text>
  </Group>;
}
```

A pill around a label with an emphasized word measures the same content it
draws:

```tsx
const label = <>New <Span style={{ fontWeight: 700 }}>2.0</Span></>;
const m = useTextMetrics(label, style);
```

For a plain label like these, `<Badge>` from `@celesta/components` does the
measuring and the sizing, with a decoration before the text and wrapping at
a `maxWidth`: `<Badge fill="#28A34A" cornerRadius="pill" padding={{ top: 12,
right: 24, bottom: 12, left: 24 }} style={style} color="#ffffff">{label}</Badge>`.
See [react-core.md](react-core.md#badge-labels-sized-to-their-text).

A caret after typed text, and a second label after the first: measure
instead of multiplying the character count by a font's advance (`0.6 em`
holds for one monospace font, and breaks on CJK, emoji, and any other
family). `textCaret(metrics, offset)` gives the position after `offset`
characters:

```tsx
const style = { fontFamily: 'JetBrains Mono', fontSize: 40 };
function Prompt({ typed }: { typed: string }) {
  const line = useTextMetrics(`$ ${typed}`, style);
  const caret = textCaret(line, 2 + typed.length);
  return <Group>
    <Text style={{ ...style, fill: { type: 'solid', color: '#ffffff' } }}>{`$ ${typed}`}</Text>
    <Rect x={caret.x} y={4} width={useTextMetrics('M', style).width} height={44} fill="#3DDC84" />
  </Group>;
}
```

A centered row: measure each item, add padding and a fixed gap, and offset
the row by `(width - total) / 2`. The full example is
`packages/cli/examples/with-text-metrics.tsx` in the Celesta repository.

### `measureText(text, style, { maxWidth?, fonts?, theme? })`

The same measurement, asynchronously, for `prepare()`; `text` may hold
`<Span>`. `<Font>` files are not loaded during `prepare()`; pass their
`src` in `fonts`. No `<Theme>` is around `prepare()` either: pass font
names in `theme` (`{ fonts: { mono: 'JetBrains Mono' } }`) to use them in
`style` and spans.

```tsx
let titleWidth = 0;
export async function prepare() {
  const m = await measureText('Celesta', { fontFamily: 'Inter', fontSize: 96 }, { fonts: ['./Inter.ttf'] });
  titleWidth = m.width;
}
```

## Fit text into a box

`<TextBox>` draws text at the largest font size from `minFontSize` to
`maxFontSize` that fits `width`, `height`, and `maxLines`, so a caption,
title, or product name can change without retuning its size. Short text
uses `maxFontSize`; longer text shrinks, down to `minFontSize`.

```tsx
<TextBox x={448} y={880} width={1024} height={136}
  minFontSize={28} maxFontSize={64} maxLines={2} lineHeight={1.3}
  verticalAlign="middle" style={{ fontFamily: 'Noto Sans JP', align: 'center', lineBreak: 'phrase' }}>
  {caption}
</TextBox>
```

| Prop | Notes |
| --- | --- |
| `width`, `height` | The box, in px, with its top-left corner at `x`/`y`. Lines wrap at `width` as `<Text maxWidth>` does. |
| `minFontSize`, `maxFontSize` | The font size range, in px. |
| `maxLines` | Most lines the text may take. Default: as many as fit `height`. Lines started by `\n` count too. |
| `lineHeight` | **A multiple of the font size** (like CSS's unitless `line-height`), so it shrinks with the text. Default: the font's own. |
| `step` | Sizes tried are `minFontSize + n * step` and `maxFontSize`. Default `1`. |
| `style` | A `TextStyle` without `fontSize` and `lineHeight`. `align` positions each line inside `width`. |
| `verticalAlign` | `'top'` (default), `'middle'`, or `'bottom'`: where shorter text sits in `height`. |
| `overflow` | When the text does not fit even at `minFontSize`: `'clip'` (default) cuts it off at the box and after `maxLines`; `'visible'` draws all its lines, past the box; `'error'` fails the render, so an export stops instead of shipping cut-off text. |

- The size is chosen with the renderer's own shaping (same fonts,
  `letterSpacing`, `lineBreak`, and wrapping width), so preview and export
  agree. For spacing that scales with the size, use `letterSpacing: { em }`.
  The size is found by bisection; rarely, a larger size could also fit
  past one that does not, and the size chosen fits but is not the largest.
- Empty text, and `null`, `undefined`, or boolean children, draw nothing.
  Children may hold `<Span>`; the fit is measured with the spans' weights
  and families.
- A non-positive or non-finite `width`, `height`, font size, `lineHeight`,
  or `step`, `maxFontSize` below `minFontSize`, or a `maxLines` that is not
  a whole number of at least 1 throws a `RangeError`.
- This fits text to a fixed box. For a box that grows with its text, use
  `useTextMetrics` as above.

`useFitText(text, options)` returns the choice without drawing it, for a
custom layout or overflow treatment: `{ fontSize, style, metrics, fits }`.
`options` are `TextBox`'s `width`, `height`, `minFontSize`, `maxFontSize`,
`maxLines`, `lineHeight`, `step`, and `style`, plus `fonts` and `theme`
(names added to the [`<Theme>`](#shorthand-props-and-theme)s around it).
Draw it with the returned `style`, whose theme names are looked up, and the
same width:

```tsx
function Title({ text }: { text: string }) {
  const fit = useFitText(text, { width: 800, height: 120, minFontSize: 32, maxFontSize: 96, maxLines: 1 });
  return <Group>
    <Text y={fit.metrics.ascent} anchorY="baseline" maxWidth={800} style={fit.style}>{text}</Text>
    {!fit.fits && <Rect width={800} height={4} y={124} fill="#EF402B" />}
  </Group>;
}
```

`fitText(text, options)` does the same in `prepare()` and resolves to the
same result; pass font files in `options.fonts` and font and color names in
`options.theme`, as for `measureText()`.
The full example is `packages/cli/examples/with-text-box.tsx`.

## Title cards

`<TitleCard title subtitle? kicker? chapter? …>` from `@celesta/components`
is a whole title screen: a large chapter number, a small kicker, the title,
and a subtitle, stacked top to bottom in one block (absent pieces take no
space). It lays out in the current area (the canvas, a `<SafeArea>`, or a
`<Fit>`), keeps the title's `\n` breaks, wraps every piece at the column,
and shrinks the title with `useFitText` until the block fits.

```tsx
import { TitleCard } from '@celesta/components';

<TitleCard chapter="01" kicker="GETTING STARTED" kickerRule title={'Code is\nthe cut'}
  subtitle="Celesta 入門" background="ink" color="paper" subtitleColor="soft" accentColor="accent" />
```

| Prop | Notes |
| --- | --- |
| `align` | `'left'` (default), `'center'`, `'right'`: where the column sits and how its lines align. Every piece passes `maxWidth`, so lines align the same in preview and export. |
| `verticalAlign` | `'top'`, `'middle'` (default), `'bottom'`: where the block sits. |
| `background` | A hex color or `<Theme>` color name, or any element (a `<PatternBackground>`, an `<Image>`), over the whole area. Omitted, the card is transparent, for a header over a scene. |
| `safeArea` | As `<SafeArea padding>`, such as `'vertical-social'`: the text keeps clear of the platform UI while the background fills the frame. |
| `padding`, `maxWidth`, `gap` | Space inside the area (a number or per-edge insets), the widest column, and the space between pieces. Defaults scale with the area's shorter side. |
| `fontFamily`, `color`, `subtitleColor`, `accentColor` | Family or `<Theme>` font name for every piece; hex colors or `<Theme>` color names. `accentColor` paints the chapter number, kicker, and `kickerRule`. |
| `titleSize`, `minTitleSize`, `maxTitleLines`, `titleLineHeight` | The title's largest and smallest sizes (defaults 9% of the shorter side, and 40% of that), a line limit while it shrinks, and its line height as a multiple (1.1). |
| `subtitleSize`, `kickerSize`, `chapterSize` | Defaults 3%, 2%, and 16% of the shorter side. |
| `kickerRule` | A short line before the kicker; a number sets its length. |
| `titleStyle`, `subtitleStyle`, `kickerStyle`, `chapterStyle` | Extra `TextStyle` over each piece's defaults (letter spacing, weight, another font). |
| `enter`, `exit`, `stagger` | Each piece is a [`<Motion>`](animation.md#motion) with these `enter`/`exit` effects (`null` cuts), coming in `stagger` frames after the one above. Defaults: `fade-up`, `'fade'`, a fifth of a second. |

- **Calm or staggered.** The defaults bring the pieces in one after another.
  `enter="fade" stagger={0}` fades them in together; `enter={null}
  exit={null}` draws a still card.
- **Timing.** The pieces share the enclosing sequence's window: put the
  card in a `<Sequence>` (or a scene) of the length it should stay, and all
  pieces leave together at its end.
- **Not for scene changes.** To join the card to the next scene with a
  cross-fade or wipe, put both in a `<TransitionSeries>`.

Runnable: [`packages/cli/examples/with-title-card.tsx`](../../../packages/cli/examples/with-title-card.tsx)
(a staggered card, a calm one, and a vertical card inside `'vertical-social'`).

## Lower thirds

`<LowerThird title subtitle? …>` from `@celesta/components` puts a name
and a role on a band in a corner of the current area (the canvas, a
`<SafeArea>`, or a `<Fit>`), inside `safeArea` and `margin`. The band is a
[`<Panel>`](react-core.md#panel-cards-and-boxes-with-padding) as wide as the longer line, with an
accent bar on its outer edge. Each line keeps its `\n` breaks, wraps at the
band's widest, and shrinks with `useFitText` until it fits its line limit.
It needs no character or dialogue: give it the text.

```tsx
import { LowerThird } from '@celesta/components';

<Sequence from={30} durationInFrames={120}>
  <LowerThird title="夏猫" subtitle="ソフトウェアエンジニア" accent="#60A0FF" />
</Sequence>
```

| Prop | Notes |
| --- | --- |
| `placement` | `'bottom-left'` (default), `'bottom-center'`, `'bottom-right'`, `'top-left'`, `'top-right'`. The lines align toward the side the band sits on, and the accent goes on that edge. |
| `safeArea`, `margin` | As `<SafeArea padding>` (such as `'vertical-social'`), then the space to the band. `margin` defaults to 6% of the area's shorter side. |
| `maxWidth`, `padding`, `gap` | The widest the band may be (defaults: 60% of the room on a landscape area, all of it otherwise), the space inside the band, and between the lines. |
| `background`, `cornerRadius`, `accent`, `accentWidth` | The band's fill (hex, `<Theme>` color name, or `Paint`; default `#000000B3`), its rounding, and the accent bar's color (default `color`) and width (`0` for none). |
| `fontFamily`, `color`, `subtitleColor` | Family or `<Theme>` font name for both lines; hex colors or `<Theme>` color names. |
| `titleSize`, `minTitleSize`, `maxTitleLines`, `titleLineHeight` | The title's largest and smallest sizes (defaults 4.5% of the shorter side, and 60% of that), its line limit (2), and its line height as a multiple (1.2). |
| `subtitleSize`, `minSubtitleSize`, `maxSubtitleLines`, `subtitleLineHeight` | Likewise for the subtitle (2.6%, 80% of that, 2, 1.35). |
| `titleStyle`, `subtitleStyle` | Extra `TextStyle` over each line's defaults. |
| `enter`, `exit` | The band's [`<Motion>`](animation.md#motion) effects (`null` cuts). Defaults: fading in while moving toward the middle of the area, and `'fade'`. |
| `textEnter`, `stagger` | How each line arrives inside the band, `stagger` frames after the band and after each other, as [title cards](#title-cards) stagger their pieces. Defaults: a short `fade-up`, a fifth of a second. The lines leave with the band. |

- **Timing.** The caption uses the enclosing sequence's window: wrap it in
  a `<Sequence from durationInFrames>` for the stretch it should show; the
  band leaves at its end.
- **Speaker names in dialogue.** `<DialogueSeries>` draws its own subtitles
  and name plates ([dialogue.md](dialogue.md)); `LowerThird` is for a
  caption you place yourself.

Runnable: [`packages/cli/examples/with-lower-third.tsx`](../../../packages/cli/examples/with-lower-third.tsx)
(bottom-left with a role, bottom-right without, and a long Japanese name in a
vertical composition).

## Verifying text

`scripts/inspect.mjs` shows each text layer's string, position, font, and
color. **An entry that uses `useTextMetrics`, `measureText`, `TextBox`,
`useFitText`, `fitText`, or `@celesta/code` is inspected through the
exporter**, which shapes text like an export; `inspect.mjs` finds it or
takes `--native <Celesta-export>`. Without one, those frames report
`UNSUPPORTED inspection` (unless `prepare()` catches the error and falls
back). See
[verify.md](verify.md#measured-text---native). Check anything about how
text looks (font, wrapping, overflow, fallback) with a PNG frame export;
see [verify.md](verify.md#look-at-real-frames).
