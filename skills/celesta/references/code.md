# Syntax-highlighted code (@celesta.studio/code)

`@celesta.studio/code` draws syntax-highlighted source code with Celesta's `Text`
and `Rect` layers: code walkthroughs, typing animations, highlighted lines,
and carets. It is a separate, optional package; `@celesta.studio/react` does not
re-export it. Celesta's runtime supplies it (like `@celesta.studio/react`), so
**never `npm install` it**, and **File → Set Up TypeScript** / `--init` add
its type declarations.

```tsx
import { Code, codeThemes, useCodePoint, codeCharacterCount, tokenizeCode, highlightCode, codeLines } from '@celesta.studio/code';
```

## Contents

- [Code](#code)
- [Typing animation](#typing-animation)
- [Themes](#themes)
- [Carets and annotations](#carets-and-annotations)
- [Draw it yourself](#draw-it-yourself)
- [Limits](#limits)

## Code

```tsx
import { Assets, Composition, Font, Rect } from '@celesta.studio/react';
import { Code, codeThemes } from '@celesta.studio/code';

const source = `const message: string = 'Hello, Celesta.';\nconsole.log(message);\n`;
const style = { fontFamily: 'JetBrains Mono', fontSize: 32, lineHeight: 48 };

export default function Root() {
  return <Composition width={1920} height={1080} fps={30} durationInFrames={150}>
    <Assets><Font src="./fonts/JetBrainsMono-Regular.ttf" /></Assets>
    <Rect width={1920} height={1080} fill="#18181b" />
    <Code x={160} y={200} language="ts" style={style} theme={codeThemes.dark}
      highlightLines={[2]} highlightWidth={1600}>
      {source}
    </Code>
  </Composition>;
}
```

| Prop | Notes |
| --- | --- |
| `children` | The source string, including indentation and trailing newlines. |
| `language` | `'tsx'`, `'ts'`, `'json'`, `'bash'`, or `'text'` (default, no highlighting). |
| `style` | A `TextStyle`. Defaults: `JetBrains Mono`, 24 px, `lineHeight` 1.5 × `fontSize` (px). In a font with coding ligatures, `fontFeatures: { liga: false, calt: false }` shows `===` and `->` as written ([text.md](text.md#opentype-features)). `align` must be `'left'` (or omitted); text never wraps. The theme supplies each run's fill color. |
| `theme` | `codeThemes.dark` (default), `codeThemes.light`, or your own (see [Themes](#themes)). |
| `visibleCharacters` | How much of the source to show, in Unicode code points of the **original** source (a tab costs 1, LF 1, CRLF 2). Fractions round down; negative shows nothing; default `Infinity`. |
| `highlightLines` | One-based line numbers to band. A band covers the whole line box and the full source width, even while only part of it is typed. |
| `highlightWidth` | Band width in px, e.g. to reach a surrounding panel's edge. Finite and ≥ 0. |
| `tabSize` | Tab stop width in code points. Default `2`. |
| common props | `x`/`y`, `scale`/`scaleX`/`scaleY`, `rotation`, `anchorX`/`anchorY`, `opacity`, `blendMode`, `blur`, `shadow`, `glow`. |

Use a monospaced font, load it with `<Font>` (or rely on it being
installed on every machine that renders), and draw any panel background
yourself with a `Rect` behind `Code`; themes do not draw one.

For Japanese comments and strings, which monospaced Latin fonts lack, give
a [font stack](text.md#font-stacks-和欧混植) with the monospaced font first:
`style={{ fontFamily: ['JetBrains Mono', 'Noto Sans JP'] }}`, with both
loaded by `<Font>` and `lang="ja-JP"` on the composition. Without one, the
Japanese is drawn with a system font even when Noto Sans JP is loaded,
unless the composition (or an enclosing `<Group>`/`<Sequence>`) sets
`fontFallback="loaded"`, which tries loaded fonts first; see
[text.md](text.md#fonts-fallback-and-emoji). The line widths and the
highlight band follow the stack.

## Typing animation

`visibleCharacters` uses the same counting as `useTypewriter().length`:

```tsx
import { useTypewriter } from '@celesta.studio/text';

function Typing() {
  const { length } = useTypewriter(source, { from: 15, framesPerChar: 0.5 });
  return <Code x={64} y={64} language="ts" style={style} visibleCharacters={length}>{source}</Code>;
}
```

The full source is tokenized once, so colors reflect the finished code while
it is being typed, and layout does not shift as characters appear.

To reveal whole lines in steps, convert a position to a count with
`codeCharacterCount(source, { line, column })` (one-based; a plain function,
usable anywhere). Column 1 of line 4 reveals exactly the first three lines:

```tsx
<Code language="ts" visibleCharacters={codeCharacterCount(source, { line: 4, column: 1 })}>{source}</Code>
```

## Themes

`codeThemes.dark` and `codeThemes.light` are
`{ foreground, highlightLine, tokens: { [tokenName]: color } }`. Extend one:

```tsx
const theme = {
  ...codeThemes.dark,
  highlightLine: '#a68bbf30',
  tokens: { ...codeThemes.dark.tokens, keyword: '#e0b7ff' },
};
```

Token names come from the twinkleplop tokenizer: `keyword`, `builtin`,
`function`, `tag_name`, `type`, `namespace`, `string`, `template`, `regex`,
`number`, `constant`, `boolean`, `attr_name`, `property`, `parameter`,
`punctuation`, `operator`, `comment`, and more. JSON object keys are
`property`; string values stay `string`. In bash, a word that starts with a
letter, digit, or `_` is one `identifier` through its hyphens, dots, and
slashes (`Celesta-export`, `scripts/migrate-packages.mjs`), so `keyword` and
`builtin` only mark whole words (`export`, not the `export` in
`Celesta-export`). A flag such as `--bit-depth` is one `parameter`. A leading
`./` or `/` keeps `punctuation`/`operator` (`./test.sh` is `.`, `/`,
`test.sh`), and inside `$((…))` and `${…}`, `-` stays an `operator`. Names a theme does not list use
`foreground`. `tokenizeCode(source, language?)` returns the tokens (text,
type, UTF-16 `[start, end)` offsets) if you need to check names; to draw
code your own way, see [Draw it yourself](#draw-it-yourself).

## Carets and annotations

`useCodePoint(source, { line, column }, style?, tabSize?)` measures a source
position with the same font and tab expansion as `Code` and returns
`{ x, y, baseline, lineHeight }` relative to Code's top-left (before group
transforms). `line` and `column` are one-based; columns count original code
points; `column` may be one past the line's last character for a caret at
line end. Invalid positions throw.

```tsx
const caret = useCodePoint(source, { line: 1, column: 7 }, style);
return <Group x={64} y={64}>
  <Code language="ts" style={style}>{source}</Code>
  <Rect x={caret.x} y={caret.y} width={2} height={caret.lineHeight} fill="#a68bbf" />
</Group>;
```

Pass the same `style` and `tabSize` you give `Code`.

## Draw it yourself

When `Code`'s colors are right but its staging is not (typing one line per
beat, an accent beside a line, a band reaching the frame edge), take its lines
and draw them yourself. `highlightCode(source, { language?, theme?, tabSize? })`
returns the lines `Code` draws, with its rules and defaults (`'text'`,
`codeThemes.dark`, tab size 2): LF, CRLF, and CR end a line, tabs expand to tab
stops, and token types the theme does not list use `foreground`. It is a plain
function; call it once outside the component when the source is fixed.

Each line is `{ text, start, columns, runs }`:

| Field | Meaning |
| --- | --- |
| `text` | The line with tabs expanded, without its line break. Empty lines are `''`; keep them, they hold their row. |
| `start` | Original source code points before the line, counted like `visibleCharacters` (a tab 1, LF 1, CRLF 2). |
| `columns` | `columns[i]` is the expanded column after the line's first `i` original code points. `columns[0]` is 0; the last entry is `text`'s length in code points. Use it to turn a source count into `Text`'s `visibleCharacters`, which counts the expanded text. |
| `runs` | `{ start, end, color, type }` over expanded code-point columns, in order and covering all of `text`. Pass them as `style.colorRuns` as they are. Adjacent tokens of the same color share one run, which keeps the first token's `type`. |

One `Text` per line, with `Code`'s style and its placement (each baseline at
the ascent of `'M'` below the line's top, `anchorY="baseline"`), draws exactly
what `Code` draws. `visible` here is a whole number of original code points,
like `Code`'s `visibleCharacters`:

```tsx
import { Group, Text, useTextMetrics } from '@celesta.studio/react';
import { codeThemes, highlightCode } from '@celesta.studio/code';

const style = { fontFamily: 'JetBrains Mono', fontSize: 32, lineHeight: 48 };
const theme = codeThemes.dark;
const lines = highlightCode(source, { language: 'ts', theme });

function OwnCode({ visible = Infinity }: { visible?: number }) {
  const { ascent } = useTextMetrics('M', style);
  return <Group x={160} y={200}>
    {lines.map((line, i) => {
      if (!line.text || visible <= line.start) return null;
      const shown = Math.min(visible - line.start, line.columns.length - 1);
      return <Text key={i} y={i * style.lineHeight + ascent} anchorY="baseline" style={{
        ...style, fill: { type: 'solid', color: theme.foreground },
        colorRuns: line.runs, visibleCharacters: line.columns[shown],
      }}>{line.text}</Text>;
    })}
  </Group>;
}
```

To color with `<Span>` instead, slice by code point (a `Text` takes
`colorRuns` or `<Span>` fills, not both):

```tsx
const characters = Array.from(line.text);
<Text style={style}>
  {line.runs.map((run, i) => (
    <Span key={i} style={{ fill: run.color }}>{characters.slice(run.start, run.end).join('')}</Span>
  ))}
</Text>
```

Type the lines one per beat, here 20 frames apart at two source code points
per frame (`packages/code/examples/lines.tsx` is a complete version):

```tsx
function LineByLine() {
  const frame = useCurrentFrame();
  const { ascent } = useTextMetrics('M', style);
  return <Group x={160} y={200}>
    {lines.map((line, i) => {
      const typed = Math.max(0, Math.floor((frame - i * 20) * 2));
      if (!line.text || typed === 0) return null;
      return <Text key={i} y={i * style.lineHeight + ascent} anchorY="baseline" style={{
        ...style, fill: { type: 'solid', color: theme.foreground }, colorRuns: line.runs,
        visibleCharacters: line.columns[Math.min(typed, line.columns.length - 1)],
      }}>{line.text}</Text>;
    })}
  </Group>;
}
```

`highlightCode` does not know glyph widths. For x positions (a caret, the end
of a band at a line's text) measure: `useCodePoint()` for a source position,
or `useTextMetrics()` with `textCaret()` on a line ([text.md](text.md)).

For per-token work, `codeLines(tokenizeCode(source, language), tabSize?)`
returns the same lines uncolored, with one run for each stretch of adjacent
same-type tokens: `{ type, text, column }`, where `text` is tab-expanded and
`column` is the expanded start column.

## Limits

- `Code` measures text, so `scripts/inspect.mjs` checks an entry that uses
  it through the exporter ([verify.md](verify.md#measured-text---native));
  without one it reports those frames as `UNSUPPORTED inspection`. Check how it looks with
  a PNG frame export ([verify.md](verify.md#look-at-real-frames)).
- Each non-empty line is one `Text` with `style.colorRuns`, so kerning,
  ligatures, and combining clusters are shaped across color boundaries. A
  shaped cluster takes its first code point's color and appears as a whole.
  Use a monospaced font and left-to-right code for conventional indentation.
- Typing re-rasterizes the line being typed on each new character (its
  shaping is cached). A line wider than the GPU texture limit is rasterized
  below 1x and can look softer. Show the part of a file that matters rather
  than hundreds of lines.
- The browser playground (`@celesta.studio/web`) does not support `@celesta.studio/code`.
