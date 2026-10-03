# @celesta/code

Optional syntax-highlighted code for Celesta React compositions. It uses
twinkleplop to tokenize source and Celesta's `Text` and `Rect` to draw it.
`@celesta/react` does not depend on or re-export this package.

## Build and try

Building `@celesta/react` as described in the root README also builds
`@celesta/code` and stages its TypeScript support. From the repository root:

```sh
pnpm install
pnpm --dir packages/code run test
pnpm --dir packages/code run check:examples
cargo run -p celesta-editor --release -- packages/code/examples/highlight.tsx
```

Like `@celesta/react`, this package is currently private and has not been
published to npm. It depends on the matching `@celesta/react` runtime and
React 18 as peers. Celesta's desktop app ships it as a separate package;
the CLI resolves it from that runtime and bundles it only when a composition
imports it, sharing the CLI's React and Celesta instances.

Choose **File > Set Up TypeScript** with a React composition open. Its
`.celesta/` support directory automatically includes the `@celesta/code`
declarations and import mapping, alongside `@celesta/react` and `@celesta/math`.
You can import `Code` without adding a dependency or installing from npm,
including in repository compositions under `examples/<name>`.

Other external packages still need dependencies in the composition's project.
General example dependency setup and a possible `celesta-editor --init`
command are tracked in
[#107](https://github.com/mika-f/Celesta/issues/107).

The browser playground currently only accepts `@celesta/react`, `@celesta/math`, and `react`
imports, and does not support this optional package. Browser integration is
tracked in [#100](https://github.com/mika-f/Celesta/issues/100).

## Code

```tsx
import { useTypewriter } from '@celesta/react';
import { Code, codeThemes } from '@celesta/code';

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
  The theme supplies each run's fill color.
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
Measurements use the full source and prefixes, so changing visibility or line
highlights keeps layout stable and reuses the mounted component's measurements.
Adjacent tokens with the same color share a Text layer, and blank runs have no
Text layer while retaining their measured spacing.

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
Unspecified names use `foreground`.

## Carets and annotations

`useCodePoint(source, { line, column }, style?, tabSize?)` measures a source
position using the same font and tab expansion as `Code`. Both line and column
are one-based; columns count original code points, so tabs and emoji each count
once. `column` may be one past the final character to place a caret at line end.
Invalid positions throw an error.

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
import { Code, codeCharacterCount } from '@celesta/code';

const count = codeCharacterCount(source, { line: 4, column: 1 });
return <Code language="ts" visibleCharacters={count}>{source}</Code>;
```

Tabs and emoji each count once, LF and CR each count once, and CRLF counts
twice. A position at line end excludes its following newline. To include
that newline, use column 1 of the next line. Empty sources and trailing empty
lines accept column 1; invalid positions throw as they do in `useCodePoint()`.

`tokenizeCode(source, language?)` also exposes the original token text, type,
and UTF-16 `[start, end)` offsets for custom rendering or analysis. It preserves
all source text, including whitespace, and requires neither HTML nor a DOM.

## Current limits

Rendering uses separate Text layers per color run and measures their prefixes.
Long lines with many colors increase initial measurement work, scene size, and
rasterization work. Font shaping across color boundaries (including ligatures,
kerning, combining clusters, and bidirectional text) is not preserved as one
text run. Use a monospaced font and left-to-right code; proportional fonts are
not a supported layout guarantee. Shared styled-text shaping and rendering
belong in the core renderer rather than this optional tokenizer package.
This follow-up is tracked in [#99](https://github.com/mika-f/Celesta/issues/99).
