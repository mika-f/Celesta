# Syntax-highlighted code (@celesta/code)

`@celesta/code` draws syntax-highlighted source code with Celesta's `Text`
and `Rect` layers: code walkthroughs, typing animations, highlighted lines,
and carets. It is a separate, optional package; `@celesta/react` does not
re-export it. Celesta's runtime supplies it (like `@celesta/react`), so
**never `npm install` it**, and **File → Set Up TypeScript** / `--init` add
its type declarations.

```tsx
import { Code, codeThemes, useCodePoint, codeCharacterCount, tokenizeCode } from '@celesta/code';
```

## Contents

- [Code](#code)
- [Typing animation](#typing-animation)
- [Themes](#themes)
- [Carets and annotations](#carets-and-annotations)
- [Limits](#limits)

## Code

```tsx
import { Assets, Composition, Font, Rect } from '@celesta/react';
import { Code, codeThemes } from '@celesta/code';

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
| `style` | A `TextStyle`. Defaults: `JetBrains Mono`, 24 px, `lineHeight` 1.5 × `fontSize` (px). `align` must be `'left'` (or omitted); text never wraps. The theme supplies each run's fill color. |
| `theme` | `codeThemes.dark` (default), `codeThemes.light`, or your own (see [Themes](#themes)). |
| `visibleCharacters` | How much of the source to show, in Unicode code points of the **original** source (a tab costs 1, LF 1, CRLF 2). Fractions round down; negative shows nothing; default `Infinity`. |
| `highlightLines` | One-based line numbers to band. A band covers the whole line box and the full source width, even while only part of it is typed. |
| `highlightWidth` | Band width in px, e.g. to reach a surrounding panel's edge. Finite and ≥ 0. |
| `tabSize` | Tab stop width in code points. Default `2`. |
| common props | `x`/`y`, `scale`/`scaleX`/`scaleY`, `rotation`, `anchorX`/`anchorY`, `opacity`, `blendMode`, `blur`, `shadow`, `glow`. |

Use a monospaced font, load it with `<Font>` (or rely on it being
installed on every machine that renders), and draw any panel background
yourself with a `Rect` behind `Code`; themes do not draw one.

## Typing animation

`visibleCharacters` uses the same counting as `useTypewriter().length`:

```tsx
import { useTypewriter } from '@celesta/react';

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
`property`; string values stay `string`. Names a theme does not list use
`foreground`. `tokenizeCode(source, language?)` returns the tokens (text,
type, UTF-16 `[start, end)` offsets) if you need to check names or render
code your own way.

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

## Limits

- `Code` measures text, so **`scripts/inspect.mjs` cannot load an entry that
  uses it**. Verify with a PNG frame export
  ([verify.md](verify.md#look-at-real-frames)).
- Each color run is its own `Text` layer. Ligatures, kerning, and combining
  clusters across color boundaries are not shaped as one run; use a
  monospaced font and left-to-right code.
- Long lines with many colors cost more to measure and draw. Show the part
  of a file that matters rather than hundreds of lines.
- The browser playground (`@celesta/web`) does not support `@celesta/code`.
