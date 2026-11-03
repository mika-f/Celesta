# @celesta.studio/code

Optional syntax-highlighted code for Celesta React compositions. It uses
twinkleplop to tokenize source and Celesta's `Text` and `Rect` to draw it.
`@celesta.studio/react` does not depend on or re-export this package.

## Build and try

Building `@celesta.studio/cli` as described in the root README also builds
`@celesta.studio/code` and stages its TypeScript support. From the repository root:

```sh
pnpm install
pnpm --dir packages/code run test
pnpm --dir packages/code run check:examples
cargo run -p celesta-editor --release -- packages/code/examples/highlight.tsx
cargo run -p celesta-editor --release -- packages/code/examples/lines.tsx
```

Like `@celesta.studio/react`, this package is currently private and has not been
published to npm. It depends on the matching `@celesta.studio/react` runtime and
React 19 as peers. Celesta's desktop app ships it as a separate package;
the CLI resolves it from that runtime when a composition imports it, sharing
the CLI's React and Celesta instances.

Choose **File > Set Up TypeScript** with a React composition open. Its
`.celesta/` support directory automatically includes the `@celesta.studio/code`
declarations and import mapping, alongside the other Celesta packages.
You can import `Code` without adding a dependency or installing from npm,
including in repository compositions under `examples/<name>`.

Other external packages still need dependencies in the composition's project.
General example dependency setup and a possible `celesta-editor --init`
command are tracked in
[#107](https://github.com/mika-f/Celesta/issues/107).

The browser playground currently only accepts `@celesta.studio/react`, `@celesta.studio/math`, and `react`
imports, and does not support this optional package. Browser integration is
tracked in [#100](https://github.com/mika-f/Celesta/issues/100).

## Code

```tsx
import { useTypewriter } from '@celesta.studio/text';
import { Code, codeThemes } from '@celesta.studio/code';

const source = `const message: string = 'Hello, Celesta.';`;
const style = { fontFamily: 'IBM Plex Mono', fontSize: 24, lineHeight: 36 };

function Example() {
  const { length } = useTypewriter(source, { from: 15, framesPerChar: 0.5 });
  return (
    <Code
      x={64} y={64}
      language="ts"
      style={style}
      theme={codeThemes.dark}
      visibleCharacters={length}
      highlightLines={[1]}
      highlightWidth={1152}
    >
      {source}
    </Code>
  );
}
```

Load the named monospaced font with Celesta's `<Font src="..." />`, or install
it on the rendering machine. Defaults are JetBrains Mono, 24 px, and a line
height of 1.5 × font size. If the font is missing, Celesta uses its normal font
fallback. The example loads the repository's IBM Plex Mono font explicitly.

- `children` is a source string, including indentation and trailing newlines.
- `language` is `tsx`, `ts`, `json`, `bash`, or `text` (the default).
- `style` is Celesta's `TextStyle`. Text is left aligned and does not wrap.
  The theme supplies each run's fill color. In a font with coding ligatures,
  `fontFeatures: { liga: false, calt: false }` shows `===` and `->` as
  written.
- `visibleCharacters` counts Unicode code points in the **original source**,
  matching `useTypewriter().length`. A tab costs one character, LF costs one,
  and CRLF costs two. Fractions are rounded down, negative counts show nothing,
  and the default `Infinity` shows everything.
- `highlightLines` uses one-based line numbers. Bands occupy an entire line
  box and the full source width, even when only a prefix is visible.
- `highlightWidth` overrides the band width in pixels, for example to reach
  the edge of a surrounding panel. It must be finite and non-negative; zero
  gives the band no width. It does not change text positioning or wrapping.
- `tabSize` defaults to 2. Tabs advance to the next tab stop, counted in code
  points rather than pixels; LF, CRLF, and CR are supported as line separators.
- Group props are supported: `x`/`y`, `scale`/`scaleX`/`scaleY`, `rotation`,
  `anchorX`/`anchorY`, `opacity`, `blendMode`, `blur`, `shadow`, and `glow`.

The complete source is tokenized only when source or language changes. Line
preparation also updates when tab size changes. Typing reveals the cached result;
it does not
re-tokenize an incomplete program. Colors therefore reflect the finished code.
Code measures the full source and a baseline once, independently of token count.
Each non-empty source line is one Text layer with `style.colorRuns`; changing
visibility, theme or line highlights keeps the complete line's shaping and
reuses its measurements. Adjacent tokens with the same color share a run.

## Themes

`codeThemes.dark` and `codeThemes.light` contain token colors, a foreground
fallback, and the line highlight color. They do not draw a panel background;
place a `Rect` behind `Code` when needed.

```tsx
const theme = {
  ...codeThemes.dark,
  highlightLine: '#a68bbf30',
  tokens: { ...codeThemes.dark.tokens, keyword: '#e0b7ff' },
};
```

Token names are twinkleplop's names, such as `keyword`, `string`, `comment`,
`number`, `tag_name`, and `attr_name`. JSON object keys use `property`, including
their escaped fragments; string values retain `string` and `string_escape`.
In bash, hyphens, dots, and slashes inside a word stay in its `identifier`
(`Celesta-export`, `scripts/migrate-packages.mjs`), so `keyword` and `builtin`
only mark whole words, and a flag such as `--bit-depth` is one `parameter`.
Unspecified names use `foreground`.

## Carets and annotations

`useCodePoint(source, { line, column }, style?, tabSize?)` measures a source
position from the full-source glyph layout using the same font and tab expansion
as `Code`. Moving the caret does not measure prefixes. Both line and column
are one-based; columns count original code points, so tabs and emoji each count
once. `column` may be one past the final character to place a caret at line end.
Invalid positions throw an error. This hook requires the native editor/exporter
runtime; browser renders cannot provide shaped glyph metrics and throw an
explicit `useCodePoint()` unsupported error. `Code` rendering itself works in
the browser.

```tsx
const caret = useCodePoint(source, { line: 1, column: 7 }, style);

return <Group x={64} y={64}>
  <Code language="ts" style={style}>{source}</Code>
  <Rect x={caret.x} y={caret.y} width={2} height={caret.lineHeight} fill="#a68bbf" />
</Group>;
```

The hook returns `x`, the line box's top `y`, the absolute `baseline` within
Code, and `lineHeight`, all relative to Code's top-left before group transforms.
See `examples/highlight.tsx` for a moving caret driven by `useTypewriter()`.

`codeCharacterCount(source, { line, column })` converts the same one-based
source position into the number of original code points before that position.
It is a plain function and can be used outside React. For example, to reveal
the first three lines when the source has a fourth line:

```tsx
import { Code, codeCharacterCount } from '@celesta.studio/code';

function FirstThreeLines({ source }: { source: string }) {
  const count = codeCharacterCount(source, { line: 4, column: 1 });
  return <Code language="ts" visibleCharacters={count}>{source}</Code>;
}
```

Tabs and emoji each count once, LF and CR each count once, and CRLF counts
twice. A position at line end excludes its following newline. To include
that newline, use column 1 of the next line. Empty sources and trailing empty
lines accept column 1; invalid positions throw as they do in `useCodePoint()`.

`tokenizeCode(source, language?)` also exposes the original token text, type,
and UTF-16 `[start, end)` offsets for custom rendering or analysis. It preserves
all source text, including whitespace, and requires neither HTML nor a DOM.

## Drawing lines yourself

`highlightCode(source, { language?, theme?, tabSize? })` returns the lines and
color runs that `Code` draws, so you can stage them differently, for example
typing one line per beat or adding an accent beside a line. It uses `Code`'s
rules and defaults (`'text'`, `codeThemes.dark`, and a tab size of 2): LF, CRLF,
and CR end lines, tabs expand to tab stops, and token types missing from the
theme use `foreground`. `Code` itself colors its lines with the same function.

Each `HighlightedCodeLine` has:

- `text`: the line with tabs expanded, without its line break. Empty lines are
  returned as `''` and keep their row.
- `start`: the original source code points before the line, counted like
  `visibleCharacters`.
- `columns`: the expanded column at each original code-point boundary of the
  line. `columns[0]` is 0 and the last entry is the length of `text` in code
  points. It converts a source count into `Text`'s `visibleCharacters`, which
  counts the expanded text.
- `runs`: `HighlightedCodeRun`s with `start`, `end`, `color`, and `type`, in
  expanded code-point columns. They cover the whole `text` in order and can be
  passed as `style.colorRuns` as they are. Adjacent tokens of the same color
  share one run, which keeps the first token's `type`.

Draw each non-empty line as one `Text` with `Code`'s style, its baseline at the
ascent of `'M'` below the line's top, to match `Code` exactly:

```tsx
import { Group, Text, useTextMetrics } from '@celesta.studio/react';
import { codeThemes, highlightCode } from '@celesta.studio/code';

const style = { fontFamily: 'IBM Plex Mono', fontSize: 24, lineHeight: 36 };
const theme = codeThemes.dark;
const lines = highlightCode(source, { language: 'ts', theme });

function OwnCode({ visible = Infinity }: { visible?: number }) {
  const { ascent } = useTextMetrics('M', style);
  return <Group x={64} y={64}>
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

`<Span style={{ fill: run.color }}>` around each run's code points works in
place of `colorRuns`. See `examples/lines.tsx` for lines typed on separate
beats. `highlightCode` does not assume glyph widths; measure positions with
`useCodePoint()`, or `useTextMetrics()` and `textCaret()`.

`codeLines(tokens, tabSize?)` is the step before coloring. It splits
`tokenizeCode()` output into the same lines, with an uncolored `CodeRun` for
each stretch of adjacent same-type tokens (`type`, tab-expanded `text`, and the
expanded start `column`), for work that needs token types rather than merged
colors.

## Current limits

Color boundaries preserve kerning, ligatures, combining sequences and bidirectional
shaping. A shaped cluster takes its first code point's color and is revealed
as a whole when that first code point becomes visible. Positions inside a
cluster interpolate its advance; they are not font-specific ligature caret stops.
Left alignment and unwrapped lines remain intentional. Monospaced fonts are
recommended for conventional code indentation; proportional fonts retain their
natural shaping. Extremely wide lines are rasterized at a smaller scale on the
GPU to fit its texture limit, which can reduce sharpness.

The same `TextStyle.colorRuns` and `visibleCharacters` work on ordinary `Text`
and subtitles. See [the API and benchmark note](../../docs/performance/text-color-runs.md).
