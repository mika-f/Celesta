# Text, fonts, and measuring

Drawing text in a React composition: the `<Text>` layer, `TextStyle`,
fonts and fallback, line breaking (including Japanese phrases), text
animation helpers, measuring text, and fitting text into a box. All imported
from `@celesta/react`. Syntax-highlighted code is in [code.md](code.md).

## Contents

- [The Text layer](#the-text-layer)
- [Title and Theme](#title-and-theme)
- [TextStyle](#textstyle)
- [Line breaking](#line-breaking)
- [Fonts, fallback, and emoji](#fonts-fallback-and-emoji)
- [Language and fallback fonts](#language-and-fallback-fonts)
- [Text motion](#text-motion): TextReveal, useTypewriter, useCountUp
- [Measure text](#measure-text): useTextMetrics, measureText
- [Fit text into a box](#fit-text-into-a-box): TextBox, useFitText, fitText
- [Verifying text](#verifying-text)

## The Text layer

| Prop | Notes |
| --- | --- |
| children | Strings or numbers only (arrays of them are joined). Use template literals to combine values: `` {`${name} · Lv.${level}`} ``. `\n` breaks a line. No elements inside `Text`. |
| `style` | A `TextStyle`, see below. |
| `lang` | Text language (e.g. `"zh-Hant"`), overriding enclosing scopes; `style.lang` takes priority. |
| `maxWidth` | Wrap lines to fit this width; `style.align` positions each line inside it. A word wider than `maxWidth` is not split; the part past `maxWidth` is cut off. See [Line breaking](#line-breaking). |
| common props | `x`, `y`, `anchorX`, `anchorY`, `opacity`, effects, … |

Anchoring:

- Single-line text is anchored vertically by its visible glyph bounds, so
  `anchorY={0.5}` centers the letters themselves. Horizontally it keeps its
  advance width, so leading and trailing spaces take up room.
- `anchorY="baseline"` anchors the text on its first line's baseline
  instead. Use it to line up separate `Text` layers (colored runs,
  per-character animation, mixed font sizes) at one `y`, and with measured
  layouts (`y={padding + metrics.ascent}`).

## Title and Theme

`Title` is a shortcut for `Text`. It accepts the same text and layer props,
plus `font`, `size`, `weight`, `color` (hex), `align`, `center`, and
[`enter`](animation.md#motion). Defaults: sans-serif, 96 px, white, at (0, 0).

```tsx
<Theme font="Inter" size={64} color="#e4daf0">
  <Title center enter="fade">Hello, Celesta.</Title>
  <Title x={80} y={600} size={24}>A smaller caption.</Title>
</Theme>
```

`center` sets both coordinates to the middle of the current canvas or layout
bounds (`SafeArea`, `Fit`), both anchors to 0.5, and line alignment to center.
Explicit `x`, `y`, anchors, `align`, and `style.align` override these defaults.
Inside a translated `Group`, coordinates still belong to that group's local
space; use ordinary `x`/`y` when its origin is already centered.

`Theme` shares defaults with descendant **Titles only**; existing `Text`,
`TextBox`, and `TextReveal` keep their behavior. Nested themes merge styles.
Precedence: Title defaults → outer/inner Theme → center alignment → Title
shorthands → Title `style`. Both Theme and Title accept a full `TextStyle`
through `style`; its explicit fields win over shorthands on that component.
Load custom fonts with `Font` as usual; a theme does not load them.

`enter` wraps the text in a motion group positioned at the text's x/y, so
scaling pivots around its anchor rather than the canvas origin. Without
`enter`, Title evaluates directly to a Text layer. Use `Text` for the most
explicit form; the two can be mixed freely.

Example: `packages/react/examples/with-authoring.tsx`.

For a 320×240 composition, these declarations evaluate to the same Text
layer (apart from its generated id):

```tsx
<Text x={160} y={120} anchorX={0.5} anchorY={0.5} opacity={0.6} style={{ fontFamily: 'Inter', fontSize: 48, fontWeight: 700, align: 'center', fill: { type: 'solid', color: '#ff8800' } }}>Hello</Text>
<Title center font="Inter" size={48} weight={700} color="#ff8800" opacity={0.6}>Hello</Title>
```

Measured with js-tiktoken 1.0.21, counting only each declaration (no imports):
72 → 31 tokens with cl100k_base (56.9% fewer), or 75 → 31 with o200k_base
(58.7% fewer). Savings vary with the composition and tokenizer. The React
authoring test checks the evaluated-layer equivalence.

## TextStyle

```ts
type TextStyle = {
  fontFamily?: string;     // installed family, or one loaded with <Font>
  fontSize?: number;       // px, > 0
  fontWeight?: number;     // 100–900
  fill?: Paint;                                   // { type: 'solid', color } or a gradient
  stroke?: { paint: Paint; width: number };       // outline
  align?: 'left' | 'center' | 'right';
  lineHeight?: number;     // px, > 0 (TextBox's lineHeight is a multiple instead)
  letterSpacing?: number;  // px added after each glyph; may be negative
  lineBreak?: 'normal' | 'phrase';  // where maxWidth may wrap; default 'normal'
  lang?: string;           // language for fallback fonts, e.g. 'ja-JP'; overrides inherited lang
};
```

`fill` and `stroke.paint` are paint objects, never bare hex strings:
`fill: { type: 'solid', color: '#ffffff' }`. Gradients are described in
[react-core.md](react-core.md#rect-and-paints-gradients); their coordinates
are relative to the text's layout box. The same style fields are used by
character subtitles and `.celesta.json` text items.

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
`TextBox`, `measureText()`/`useTextMetrics()`, and `.celesta.json` text
styles. The browser canvas renderer (`@celesta/web`) ignores it and wraps
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
  uses the family's nearest one. A `text=` subset must include every
  character the video draws.
- A `fontFamily` with no installed or loaded face falls back to another
  font. The app lists it with the preview's warnings, and the exporter
  prints `warning: font family "…" (weight …) is not installed or loaded;
  text layer "…" uses a fallback font`, once per family and weight.
- Characters the family lacks fall back per glyph to another font, or are
  drawn as a missing-glyph box (tofu) when no font has them. The exporter
  prints `warning: font family "…" (weight …) has no glyph for "…"; text
  layer "…" draws them with a fallback font` (the first 10 characters, then
  `and N more characters`). Emoji drawn by a color emoji font, whitespace,
  and invisible characters are not reported.
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
  Japanese model. Loading a font for the language with `<Font>` remains the
  reproducible choice.

## Text motion

### `<TextReveal>`

Each line of a string slides up from behind its own mask (`Group clip`),
staggered. Props: `children` (string, `\n` separates lines), `style`,
`lineHeight` (mask height and line spacing; defaults to `style.lineHeight`,
then `fontSize`), `baseline` (0.8: baseline position inside each line box),
`align` (0/0.5/1 pivot of each line), `from`, `stagger` (4),
`durationInFrames` (20), `easing` (`easeOutExpo`), `direction` (`'in'` or
`'out'`), plus common props. `x`/`y` are the top-left of the first line box.

### `useTypewriter(text, { from?, framesPerChar?, blinkFrames? })`

Returns `{ text, length, done, caretVisible }`. Counts by code point, so kana
and emoji count once. `framesPerChar` below 1 types several characters per
frame. The caret is steady while typing and blinks (period `blinkFrames`,
default one second) otherwise. To place a caret after the typed text,
measure it: `useTextMetrics(text, style).width`. For code, pass `length` to
`<Code visibleCharacters>` ([code.md](code.md)).

### `useCountUp(to, { from?, delay?, durationInFrames?, easing?, decimals? })`

A number that counts to `to` (default 30 frames, `easeOutExpo`), rounded to
`decimals`. Format it yourself, e.g. `value.toLocaleString('en-US')`.

## Measure text

### `useTextMetrics(text, style?, { maxWidth?, fonts? }?)`

Shapes text exactly as `<Text>` does, synchronously, during render, and
returns
`{ width, height, ascent, descent, lineHeight, lines, glyphs: [{ text, x, width, line }] }`
in composition pixels. Use it to size backgrounds, line up colored runs, and
place carets, without guessing widths.

- `width` is the advance width, including spaces. `height` is the line-box
  height (single-line `<Text>` trims empty rows above and below its glyphs).
  `ascent` runs from the first line's top to its baseline. `glyphs` has one
  entry per cluster, so per-letter layout is `glyphs[i].x`.
- Pass the same `style` and `maxWidth` to the hook and the `<Text>`, and
  place the text at `y={padding + metrics.ascent}` with
  `anchorY="baseline"`.
- `<Font>` declarations in the composition are included automatically, even
  when they appear after the measuring component. Relative font paths
  resolve against the entry file. `fonts` lists extra files to load for
  measurement only; also declare them with `<Font>` so drawing uses them.
- Unchanged text, style, width and fonts reuse the last result. Call it at
  the component's top level with a stable number of calls, like any hook.
- Each `<Text>` shapes separately: keep letters that need shared kerning or
  a ligature in one run.

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

Independently colored runs on one baseline:

```tsx
function Heading() {
  const style = { fontFamily: 'sans-serif', fontSize: 64 };
  const first = useTextMetrics('速い、', style);
  const accent = useTextMetrics('Easy', style);
  return <Group y={100}>
    <Text anchorY="baseline" style={style}>速い、</Text>
    <Text x={first.width} anchorY="baseline"
      style={{ ...style, fill: { type: 'solid', color: '#28A34A' } }}>Easy</Text>
    <Text x={first.width + accent.width} anchorY="baseline" style={style}>、頼もしい。</Text>
  </Group>;
}
```

A centered row: measure each item, add padding and a fixed gap, and offset
the row by `(width - total) / 2`. The full example is
`packages/react/examples/with-text-metrics.tsx` in the Celesta repository.

### `measureText(text, style, { maxWidth?, fonts? })`

The same measurement, asynchronously, for `prepare()`. `<Font>` files are
not loaded during `prepare()`; pass their `src` in `fonts`.

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
  agree. It is found by bisection; rarely, a larger size could also fit
  past one that does not, and the size chosen fits but is not the largest.
- Empty text, and `null`, `undefined`, or boolean children, draw nothing.
- A non-positive or non-finite `width`, `height`, font size, `lineHeight`,
  or `step`, `maxFontSize` below `minFontSize`, or a `maxLines` that is not
  a whole number of at least 1 throws a `RangeError`.
- This fits text to a fixed box. For a box that grows with its text, use
  `useTextMetrics` as above.

`useFitText(text, options)` returns the choice without drawing it, for a
custom layout or overflow treatment: `{ fontSize, style, metrics, fits }`.
`options` are `TextBox`'s `width`, `height`, `minFontSize`, `maxFontSize`,
`maxLines`, `lineHeight`, `step`, and `style`, plus `fonts`. Draw it with the
returned `style` and the same width:

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
same result; pass font files in `options.fonts`, as for `measureText()`.
The full example is `packages/react/examples/with-text-box.tsx`.

## Verifying text

`scripts/inspect.mjs` shows each text layer's string, position, font, and
color, but it cannot shape text: **an entry that uses `useTextMetrics`,
`measureText`, `TextBox`, `useFitText`, `fitText`, or `@celesta/code` fails
in `inspect.mjs`** (unless `prepare()` catches the error and falls back).
Check such entries, and anything about how text
looks (font, wrapping, overflow, fallback), with a PNG frame export; see
[verify.md](verify.md#look-at-real-frames).
