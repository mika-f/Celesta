# Text, fonts, and measuring

Drawing text in a React composition: the `<Text>` layer, `TextStyle`,
fonts and fallback, line breaking (including Japanese phrases), text
animation helpers, measuring text, and fitting text into a box. `Text` and
measuring come from `@celesta/react`; `TextReveal`, `useTypewriter`,
`useCountUp`, `TextBox`, `useFitText`, and `fitText` from `@celesta/text`;
`Badge` from `@celesta/components`.
Syntax-highlighted code is in [code.md](code.md).

## Contents

- [The Text layer](#the-text-layer)
- [Shorthand props and `<Theme>`](#shorthand-props-and-theme)
- [Rich text with `<Span>`](#rich-text-with-span)
- [TextStyle](#textstyle)
- [Line breaking](#line-breaking)
- [Fonts, fallback, and emoji](#fonts-fallback-and-emoji)
- [Font stacks (和欧混植)](#font-stacks-和欧混植)
- [Language and fallback fonts](#language-and-fallback-fonts)
- [Text motion](#text-motion): TextReveal, useTypewriter, useCountUp
- [Measure text](#measure-text): useTextMetrics, measureText
- [Fit text into a box](#fit-text-into-a-box): TextBox, useFitText, fitText
- [Labels on a background](#labels-on-a-background): Badge
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

`<Theme>` names fonts and colors for the text inside it, so a film's
palette and type live in one place instead of a `Label` wrapper:

```tsx
<Theme fonts={{ display: 'Inter', mono: ['JetBrains Mono', 'Noto Sans JP'] }}
       colors={{ paper: '#F4F1EA', ink: '#111111', accent: '#FF4D1F' }}>
  <Text x={56} y={56} fontFamily="mono" fontSize={18} fontWeight={700} color="paper">
    CELESTA <Span color="accent">REEL</Span>
  </Text>
</Theme>
```

- A font name may stand for a [font stack](#font-stacks-和欧混植). Colors
  are `#RRGGBB` or `#RRGGBBAA`; anything else in a `<Theme>` throws.
- Names work in the shorthand props, in `style.fontFamily`, in solid
  `style.fill`, `style.stroke` paints and `style.colorRuns`, and in a
  `<Span>`'s `fontFamily` and fill (gradient stops are not looked up).
- A name no theme defines is used as it is: a font name as a family name
  (so `fontFamily="Georgia"` still works inside a theme), a color as a
  color. The `color` shorthand must then be a hex color, or the text
  throws with the theme's color names.
- A nested `<Theme>` adds to the one around it, its own names winning. It
  passes through `<Group>`, `<Sequence>`, and every other component, like
  any React context.
- `useTheme()` returns the merged `{ fonts, colors }`, for anything that is
  not text: `<Rect fill={useTheme().colors.accent}>`.
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

`<Span style={{ fill, fontWeight, fontFamily }}>` changes part of a text.
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
- Kerning and ligatures do not cross a change of weight or family; they do
  cross a change of color.
- Set `lang` on Japanese, Chinese, or Korean text with spans (or on the
  composition). A weight or family span splits shaping, and fallback fonts
  are chosen per shaped piece: without a language, the characters next to a
  span can fall back to a different font than the rest of the phrase (for
  example, a Chinese font for kanji), which also shifts the line.
- `<Span fill>` cannot be combined with `style.colorRuns` on the same text.
- The browser preview draws each span with its own font and can differ
  slightly from export at span edges.

See [`examples/rich-text`](../../../examples/rich-text/film.tsx).

## TextStyle

```ts
type TextStyle = {
  fontFamily?: string | readonly string[];  // installed family, or one loaded with <Font>; a list is a font stack
  fontSize?: number;       // px, > 0
  fontWeight?: number;     // 100–900
  fill?: Paint;                                   // { type: 'solid', color } or a gradient
  stroke?: Stroke | readonly Stroke[];            // outline(s); Stroke = { paint: Paint; width: number }
  align?: 'left' | 'center' | 'right';
  lineHeight?: number;     // px, > 0 (TextBox's lineHeight is a multiple instead)
  letterSpacing?: number | { em: number };  // px, or a multiple of fontSize, after each glyph; may be negative
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

### `useTypewriter(text, { from?, framesPerChar?, blinkFrames? })`

Returns `{ text, length, done, caretVisible }`. Counts by code point, so kana
and emoji count once. `framesPerChar` below 1 types several characters per
frame. The caret is steady while typing and blinks (period `blinkFrames`,
default one second) otherwise. To place a caret after the typed text,
measure it: `useTextMetrics(text, style).width`. For code, pass `length` to
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
`decimals`. Format it yourself, e.g. `value.toLocaleString('en-US')`.

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

## Labels on a background

`<Badge>` (from `@celesta/components`) draws a short label (a chapter tag,
a status chip, an API name) on a background sized to the measured text plus
`padding`. The width comes from the renderer's shaping, so Japanese, Latin,
and proportional fonts fit without estimating from the character count, and
the box follows the text as it changes. Use it instead of a `Rect` sized by
`useTextMetrics` by hand.

```tsx
import { Badge } from '@celesta/components';

<Badge x={120} y={80} padding={{ horizontal: 20, vertical: 10 }}
  fill="#204060" fontFamily="sans" fontSize={28} fontWeight={700}>第1章</Badge>

<Badge x={120} y={840} anchorY={0.5} cornerRadius="pill" fill="ink" color="paper"
  leading={<Rect y={-4} width={8} height={8} cornerRadius={4} fill="#FF4D1F" />} leadingWidth={8} gap={12}
  fontFamily="mono" fontSize={22}>{api}</Badge>
```

| Prop | Notes |
| --- | --- |
| children | Text as for `<Text>`: strings, numbers, and `<Span>`. |
| `style`, `fontFamily`, `fontSize`, `fontWeight`, `color` | As for `<Text>`; `style` wins over the shorthand. Defaults: `fontSize` 24, `color` `#ffffff`. |
| `padding` | A number for every side, or `{ horizontal, vertical, top, right, bottom, left }` (sides win over axes; missing ones are 0). Default `{ horizontal: 16, vertical: 8 }`. |
| `fill`, `stroke`, `strokeWidth` | The background and its outline: a hex color, a `<Theme>` color name, or a `Paint`. `fill` defaults to `#1f1f1f`. |
| `cornerRadius` | Pixels, or `'pill'` for half the height. Default 0. |
| `leading`, `leadingWidth`, `gap` | Drawn before the text (a dot, an icon) with its origin at the left padding edge and the middle of the text's lines, so draw it from `y = -height / 2`. It takes `leadingWidth` (default 0) plus `gap` (default 8) of the row. |
| `maxWidth` | Widest the whole badge may be, padding included. Longer text wraps, as `<Text maxWidth>` does, and the badge grows taller; a word wider than the room is cut off. Default: no limit. |
| `anchorX`, `anchorY` | The point of the badge `x`/`y` place, and that `rotation`/`scale` turn about: 0 left/top, 1 right/bottom. Default 0. |
| common props | `x`, `y`, `rotation`, `scale`, `opacity`, `shadow`, effects, … apply to the background and the text together. |

- The height is the text's line boxes plus `padding`, and the text sits on
  its first line's baseline, so badges with the same style line up and keep
  their height whatever their letters.
- Empty text (or no children) keeps the height of one line, so the badge
  does not collapse while its text changes, and drops the `gap` after
  `leading`.
- Wrapped left-aligned text hugs its widest line; with `style.align`
  `'center'` or `'right'`, the text area is the whole room, since the lines
  align across it.
- There is no ellipsis: for long text, set `maxWidth` to wrap, or fit the
  size with `useFitText` and pass its `fontSize`.

The full example is `packages/cli/examples/with-badge.tsx`.

## Verifying text

`scripts/inspect.mjs` shows each text layer's string, position, font, and
color. **An entry that uses `useTextMetrics`, `measureText`, `TextBox`,
`useFitText`, `fitText`, `Badge`, or `@celesta/code` is inspected through the
exporter**, which shapes text like an export; `inspect.mjs` finds it or
takes `--native <Celesta-export>`. Without one, those frames report
`UNSUPPORTED inspection` (unless `prepare()` catches the error and falls
back). See
[verify.md](verify.md#measured-text---native). Check anything about how
text looks (font, wrapping, overflow, fallback) with a PNG frame export;
see [verify.md](verify.md#look-at-real-frames).
