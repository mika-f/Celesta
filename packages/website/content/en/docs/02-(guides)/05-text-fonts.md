## Text

`Text` renders its children: strings, numbers, and `<Span>` elements. Style it with `style`:

```tsx title="An outlined, wrapping caption"
<Text x={960} y={900} anchorX={0.5} anchorY={0.5} maxWidth={1400}
  style={{
    fontFamily: 'Hiragino Sans',
    fontSize: 64,
    fontWeight: 700,
    lineHeight: 80,
    align: 'center',
    fill: { type: 'solid', color: '#ffffff' },
    stroke: { paint: { type: 'solid', color: '#57456c' }, width: 8 },
  }}>
  Words that wrap onto a second line when they reach maxWidth.
</Text>
```

- `fontFamily` names a font installed on the computer, or one loaded with `<Font>`. When the family has no face at the requested `fontWeight`, Celesta uses its nearest weight, as CSS does. Characters the font lacks, such as emoji, fall back to another installed font. A JSON project can also bundle a font file as a `font` asset.
- `stroke` outlines the glyphs. Pass a list for several outlines around one another, such as a white inner and a colored outer outline: `stroke: [{ paint: outer, width: 14 }, { paint: inner, width: 7 }]`. Each `width` is measured from the glyph outline, so the outer stroke is the wider one, in whatever order the list gives them.
- `lang` selects the language used for fallback fonts when `fontFamily` is omitted or lacks a character. Set `<Composition lang="ja-JP">` for the whole composition. Children inherit the nearest `Composition`, `Group`, or `Sequence` with a `lang` prop, including subtitles, project timeline text, `useTextMetrics()`, `useFitText()`, and component previews. These scopes can change language between frames; measurement and rendering use the same value. `<Text lang="zh-Hant">` overrides ancestors, and an explicit `style.lang` takes priority over that prop. An empty `lang` selects the system locale. Tags such as `ja-JP`, `ko-KR`, `zh-Hans`, `zh-Hant`, and `zh-Hant-HK` are supported. When neither sets a language, Celesta uses the system locale; installed or bundled fonts still determine the available glyphs. The browser preview applies `lang` in browsers supporting Canvas language selection.

```tsx title="A Japanese default with a Chinese group"
<Composition width={1920} height={1080} fps={30} durationInFrames={90} lang="ja-JP">
  <Text>漢字とかな</Text>
  <Group lang="zh-Hant">
    <Text y={80}>繁體中文</Text>
    <Text y={160} lang="ko-KR">한국어</Text>
  </Group>
</Composition>
```

`prepare()` runs before `<Composition>` is mounted. Pass `lang` explicitly to `measureText()` when measuring: `measureText('漢字とかな', { lang: 'ja-JP' })`.

To use a font file that is not installed, declare it with `<Font>`. A relative `src` is resolved against the entry file’s folder, and `fontFamily` uses the family name stored inside the file, not the file name. TrueType, OpenType, WOFF, and WOFF2 files all work, and `src` can be an `http://` or `https://` URL.

```tsx title="Load a font file next to the entry"
<Composition width={1920} height={1080} fps={30} durationInFrames={90}>
  <Assets>
    <Font src="./fonts/MPLUSRounded1c-Bold.ttf" />
  </Assets>
  <Text style={{ fontFamily: 'M PLUS Rounded 1c', fontWeight: 700, fontSize: 96 }}>
    Hello
  </Text>
</Composition>
```

`src` can also be a web font stylesheet, such as a Google Fonts CSS link. Celesta loads at most one font file for each `@font-face` rule, and `fontFamily` can use the stylesheet’s family name. It reads the rule’s last `src` that has an entry Celesta supports, and takes the first `url()` there whose `format()` hint names TrueType, OpenType, WOFF, or WOFF2, or that has no hint. Entries marked as EOT, SVG fonts, or font collections, and entries with a `tech()` Celesta cannot draw (such as COLRv1 color glyphs), are skipped. `local()` fonts are not used, so a rule whose `src` lists only `local()` fonts loads nothing. Unlike a browser, Celesta does not try the next entry when a file fails to load. Include every weight you use in the link: a weight the stylesheet does not provide is drawn with the nearest weight it does.

```tsx title="Load a Google Fonts family"
<Assets>
  <Font google="M PLUS Rounded 1c" weights={[400, 700]} subsets={['japanese']} />
</Assets>
<Text style={{ fontFamily: 'M PLUS Rounded 1c', fontWeight: 700, fontSize: 96 }}>
  こんにちは
</Text>
```

`google` takes one Google Fonts family name and replaces `src`; choose either one. `weights` defaults to `[400]` and accepts static weights from 100 to 900 in steps of 100. List every weight your text uses. Without `subsets`, every script the family has is loaded, the same as a CSS2 API link. `subsets` narrows the request to Google Fonts script names, such as `latin`, `cyrillic`, or `japanese`; Latin is included when available. These options use the [Google Fonts CSS API](https://developers.google.com/fonts/docs/getting_started). Google may return all Unicode ranges for clients supporting `unicode-range`, such as the browser preview, regardless of `subsets`. Celesta loads all returned faces, including Japanese fonts split across multiple files. Use a CSS URL in `src` for italic styles or variable weight ranges.

A Japanese family is split into many files, and most of them hold characters a video never draws. `text` loads only the characters you pass, as one small file per weight:

```tsx title="Load only the characters a video draws"
const captions = ['字幕の組版を改善', '日本語の折り返し'];

<Assets>
  <Font google="Noto Sans JP" weights={[500, 700]} text={captions.join('')} />
</Assets>
```

Characters are deduplicated and sorted, so passing the same characters in another order reuses the same request. A character that is not in `text` is drawn by the next family in the `fontFamily` stack, or by a system fallback font, and the export warns that the family has no glyph for it. Include every string the video draws, including ones chosen by props or data. `text` cannot be combined with `subsets`, and must contain at least one character.

The desktop app and exporter cache both the stylesheet and each font file in the existing remote asset cache. Once a request has loaded successfully, it works offline on later runs, including in a new process. Changing weights, subsets, or text creates a different request; its first use needs network access. Files are not revalidated. The browser preview uses the browser's HTTP cache and does not guarantee offline availability. An unknown family or unavailable weight reports a Google Fonts error; network failures remain download errors.

For measurements in `prepare()`, use the same URL helper:

```tsx
import { googleFont, measureText } from '@celesta/react';

export async function prepare() {
  await measureText('こんにちは', { fontFamily: 'Noto Sans JP', fontWeight: 700 }, {
    fonts: [googleFont('Noto Sans JP', { weights: [700], subsets: ['japanese'] })],
  });
}
```

### Google Fonts licenses

Fonts keep their own licenses; downloading or caching them does not relicense them under Celesta's license. Check the selected family's license on [Google Fonts](https://fonts.google.com) or in the [Google Fonts repository](https://github.com/google/fonts). When redistributing font files with a project or application, include the applicable license and copyright notices and follow its terms, including any reserved font names. Celesta's cache contains the downloaded font files and CSS, and does not automatically fetch license files. Rendered images and videos do not bundle the font files; consult the font's license for permitted uses. See Google's [font licensing guidance](https://github.com/google/fonts/blob/main/cc-by-sa/knowledge/glossary/terms/licensing/content.md).

## Mix Latin and Japanese fonts

`fontFamily` also takes a list of families, a font stack. Each character is drawn with the first family in the list that has it, so Latin letters can come from a Latin font and Japanese from a Japanese one. A letter and its accents or variation selector always come from the same font.

```tsx title="A Latin and a Japanese font in one line"
<Composition width={1920} height={1080} fps={30} durationInFrames={90} lang="ja-JP">
  <Assets>
    <Font google="Inter" weights={[500]} />
    <Font google="Noto Sans JP" weights={[500]} subsets={['japanese']} />
  </Assets>
  <Text style={{ fontFamily: ['Inter', 'Noto Sans JP'], fontWeight: 500, fontSize: 64 }}>
    Celesta 0.5.0 で字幕の組版を改善
  </Text>
</Composition>
```

Families that are not installed or loaded are skipped. The first family that is sets the baselines, so a paragraph keeps even line spacing whether or not a line has Japanese. Characters that no family in the list has fall back to an installed system font chosen for `lang`. By default, fonts loaded with `<Font>` are not used as fallbacks, so list them in the stack, or set `fontFallback` as described below. Measuring and wrapping follow the stack, and `<Code>` takes one too: `style={{ fontFamily: ['JetBrains Mono', 'Noto Sans JP'] }}`. `<Span style={{ fontFamily }}>` accepts a stack as well. The export warns when no family in the stack is installed or loaded, and names the characters that none of them has. The browser preview passes the list to the browser as a CSS font-family list, so characters that no family has can look different from the export.

- `maxWidth` wraps text at word boundaries, and `align` positions each line within that width. Without `maxWidth`, `align` positions each line within the widest line. Use `\n` in a string to break a line yourself.
- Japanese normally wraps between most characters, splitting a word such as フレーム, while line-break rules keep punctuation such as 、 and 。 off the start of a line. Set `lineBreak: 'phrase'` in the style to wrap only between phrases, found with [BudouX](https://github.com/google/budoux)’s Japanese model; `lang` currently selects fallback fonts only. A phrase wider than `maxWidth` wraps inside itself as normal text does. The browser preview ignores `lineBreak` and wraps only at whitespace.
- Single-line text is anchored vertically by its visible glyphs, so `anchorY={0.5}` centers the letters themselves rather than an invisible line box. Its width is the advance width, so leading and trailing spaces still take up room. Text with several lines, broken with a newline or wrapped at maxWidth, is anchored by its line boxes instead, where measureText puts them.
- `anchorY="baseline"` anchors text on its first line’s baseline instead. Text layers with the same `y` then share a baseline, whatever their letters or font sizes.

### Fall back to loaded fonts

Set `fontFallback="loaded"` to try the fonts loaded with `<Font>` before the system's, without listing them in every stack. A character that no family of `fontFamily` has is then drawn with the first loaded font, in `<Font>` declaration order, that has it, and with a system font only when none does. The glyphs then no longer depend on the fonts installed where the video is exported.

```tsx title="Japanese from the loaded font, not the system's"
<Composition width={1920} height={1080} fps={30} durationInFrames={90} lang="ja-JP" fontFallback="loaded">
  <Assets>
    <Font google="JetBrains Mono" />
    <Font google="Noto Sans JP" subsets={['japanese']} />
  </Assets>
  <Text style={{ fontFamily: 'JetBrains Mono', fontSize: 64 }}>let 名前 = "値";</Text>
</Composition>
```

- `<Group>` and `<Sequence>` also take `fontFallback`, and children inherit the nearest one, like `lang`. `style.fontFallback` takes priority. The default is `'system'`.
- It applies only to text with a `fontFamily`, its own or a `<Span>`'s; text without one keeps the system's default font.
- `measureText()` and `fitText()` in `prepare()` run before the composition, so pass `fontFallback: 'loaded'` in their style there, as with `lang`.
- Measuring, wrapping, and baselines follow the same choice as drawing.
- The export does not warn about characters a loaded font draws. Characters that no loaded font has are still reported.
- The browser preview adds the loaded families to the end of the CSS font-family list. The browser picks a font per character rather than per cluster, so an accent can come from a different font than its letter.

## Emphasize part of a text

`<Span>` changes the `fill`, `fontWeight`, `fontFamily`, or `fontFeatures` of part of a text. The whole text is still laid out as one paragraph: it wraps, aligns, and measures as one, and its baseline stays where the text’s own letters put it. Spans nest; a weight the family lacks uses its nearest weight.

```tsx title="An emphasized word in one paragraph"
<Text style={{ fontSize: 48, fill: { type: 'solid', color: '#ffffff' } }}>
  速い、<Span style={{ fill: '#28A34A', fontWeight: 700 }}>カンタン</Span>、頼もしい。
</Text>
```

Spans work in `<Text>`, `<TextBox>`, `<TextReveal>`, and `<Dialogue>`, in `DialogueSeries` lines, and in `useTextMetrics()`, `measureText()`, `useFitText()`, `fitText()`, and `useTypewriter()`; pass the typed length to `visibleCharacters` to type rich text without changing its layout. A component that returns a span cannot be used inside text. Set `lang` on Japanese, Chinese, or Korean text with spans: a span splits shaping, and without a language the characters next to it can fall back to a different font. The browser preview draws each span with its own font and can differ slightly from export at span edges.

## Turn ligatures and other OpenType features on or off

`fontFeatures` switches a font’s OpenType features, as CSS `font-feature-settings` does. A coding font such as JetBrains Mono joins `===`, `->`, and `!=` into single symbols with its `liga` and `calt` features. Turn them off to show code as it is written:

```tsx title="Code as written, without ligatures"
<Assets>
  <Font google="JetBrains Mono" weights={[400]} />
</Assets>
<Text style={{ fontFamily: 'JetBrains Mono', fontSize: 48, fontFeatures: { liga: false, calt: false } }}>
  {'a === b'}
</Text>
```

Each key is a four-character feature tag, such as `liga`, `calt`, `kern`, `zero`, `tnum`, or `ss01`, and each value is `true`, `false`, or the number of an alternate (`salt: 2`). A tag the font does not have changes nothing; a malformed tag or value throws. `<Span style={{ fontFeatures }}>` changes features for part of a text, even inside a word, merged tag by tag over the text’s. Measurements shape with the same features, so measured and drawn widths agree. `<Code>` draws with the font’s ligatures; pass the same `fontFeatures` in its `style` to show code as written.

The browser preview applies features only to fonts loaded with `<Font>`, and throws for text with `fontFeatures` that uses an installed font or no `fontFamily`. Export applies them to installed fonts too.

## Measure and fit text

In the desktop app and CLI, measureText(text, style, options) runs asynchronously in prepare(); useTextMetrics(text, style, options) measures during rendering. Both return width, height, lines, baseline metrics, and glyph positions using the same shaping as Text. Pass maxWidth for wrapping. In prepare(), supply font files through options.fonts because Font declarations have not mounted yet. Hooks inherit the current language and declared fonts.

In the desktop app and CLI, TextBox chooses a font size between minFontSize and maxFontSize to fit width, height, and optional maxLines. Its x/y is the box’s top-left; verticalAlign is top, middle, or bottom. If the text does not fit at the minimum size, overflow is clip (default), visible, or error. For the fitted style and a fits flag without drawing, use fitText() in prepare() or useFitText() while rendering.

```tsx file=examples/fitted-text.tsx title="fitted-text.tsx"
```
