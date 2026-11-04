Code looks best when it arrives a character at a time. This chapter reveals [`Code`](/en/docs/code/) as if it were typed, puts a caret where the typing is, and shows the lower-level tools behind both.

## Typing code in

Pass `useTypewriter()`’s `length` to `visibleCharacters`:

```tsx title="Type the source out"
import { useTypewriter } from '@celesta.studio/text';
import { Code } from '@celesta.studio/code';

function Typed() {
  const { length } = useTypewriter(source, { from: 15, framesPerChar: 0.5 });
  return <Code x={64} y={64} language="ts" style={style} visibleCharacters={length}>{source}</Code>;
}
```

The whole source is tokenized once, when the source or language changes, and typing only reveals the result. Colors therefore reflect the **finished** code: a half-typed string is already colored as a string, and nothing flickers as a quote is closed.

## What visibleCharacters counts

- Unicode **code points of the original source**, matching `useTypewriter().length`. A single-code-point emoji counts once; a joined, flag, or skin-tone sequence counts once per code point.
- A tab costs one character and an LF costs one. A **CRLF costs two**.
- Fractions round down, a negative count shows nothing, and the default `Infinity` shows everything.

## A caret that follows the typing

`useCodePoint(source, { line, column }, style?, tabSize?)` measures a position in the source with the same font and tab expansion as `Code`, so you can place a caret, underline, or callout there. Lines and columns are **one-based**, and columns count original characters (a tab or a single-code-point emoji is one column). The column may be one past the last character, to sit at a line’s end. An invalid position throws.

```tsx title="A blinking caret at the end of the typed text"
import { Group, Rect } from '@celesta.studio/react'; import { useTypewriter } from '@celesta.studio/text';
import { Code, useCodePoint } from '@celesta.studio/code';

function TypedWithCaret() {
  const { length, caretVisible } = useTypewriter(source, { from: 15, framesPerChar: 0.5 });
  const typed = Array.from(source).slice(0, length).join('').split(/\r\n|\r|\n/);
  const caret = useCodePoint(source, {
    line: typed.length,
    column: Array.from(typed[typed.length - 1]).length + 1,
  }, style);

  return (
    <Group x={64} y={64}>
      <Code language="ts" style={style} visibleCharacters={length}>{source}</Code>
      {caretVisible && <Rect x={caret.x} y={caret.y} width={2} height={caret.lineHeight} fill="#a68bbf" />}
    </Group>
  );
}
```

Wrapping both in one `Group` keeps the caret aligned with the code however the group is moved or scaled. The hook returns `x`, the line’s top `y`, the `baseline`, and `lineHeight`, all relative to `Code`’s top-left corner, before any group transform.

## Revealing whole lines

`codeCharacterCount(source, { line, column })` turns a source position into the number of characters before it, which is exactly what `visibleCharacters` wants. It is a plain function, so it also works outside React. To show the first three lines of a four-line snippet:

```tsx title="Show the first three lines"
import { Code, codeCharacterCount } from '@celesta.studio/code';

function FirstThreeLines({ source }: { source: string }) {
  const count = codeCharacterCount(source, { line: 4, column: 1 });
  return <Code language="ts" visibleCharacters={count}>{source}</Code>;
}
```

A position at the end of a line leaves out that line’s newline; use column 1 of the next line to include it. Pair this with [`useCue`](/en/docs/motion-toolkit/) to build up a snippet one step at a time, one cue per line.

## Tokens

`tokenizeCode(source, language?)` returns the tokens behind `Code`: each one has the original `text`, a `type` (twinkleplop’s name, or `plain`), and its `start` and `end` offsets in UTF-16 units, end exclusive. Tokens preserve all of the source, whitespace included, and need neither HTML nor a DOM. Use them for your own rendering or to analyze the code:

```tsx title="Count the comments"
import { tokenizeCode } from '@celesta.studio/code';

const comments = tokenizeCode(source, 'ts').filter((token) => token.type === 'comment');
```

## Drawing the lines yourself

`highlightCode(source, { language, theme, tabSize })` returns the lines and color runs that `Code` draws, so you can keep its colors and stage the lines your own way: one line per beat, an accent beside a line, a band out to the frame edge. It follows `Code`’s rules and defaults (`'text'`, `codeThemes.dark`, and a tab size of `2`): LF, CRLF, and CR end a line, tabs expand to tab stops, and token types the theme does not list use `foreground`. It is a plain function, so call it once outside the component when the source is fixed.

**Each line**

| API | Use it for |
| --- | --- |
| `text` | The line with tabs expanded, without its line break. An empty line is an empty string and keeps its row. |
| `start` | Original source characters before the line, counted as `visibleCharacters` counts them. |
| `columns` | The expanded column after each original character of the line: `columns[0]` is `0`, and the last entry is the length of `text`. It turns a count of source characters into `Text`’s `visibleCharacters`, which counts the expanded text. |
| `runs` | Runs of `start`, `end`, `color`, and `type` over the expanded text, in order and covering all of it. Pass them as `style.colorRuns` as they are. Neighboring tokens of one color share a run, which keeps the first token’s `type`. |

Draw each non-empty line as one `Text` with the same style as `Code`. `Code` puts each line’s baseline at the ascent of `'M'` below the line’s top, read with `useTextMetrics`; doing the same draws exactly what `Code` draws. This example types one line per beat:

```tsx file=examples/code-lines.tsx title="Type one line per beat"
```

To color with `<Span style={{ fill: run.color }}>` instead, slice `Array.from(line.text)` by each run’s start and end. A Text takes `colorRuns` or Span fills, not both.

`highlightCode` does not know how wide a glyph is. Measure positions, such as a caret or where a line’s text ends: `useCodePoint()` for a source position, or `useTextMetrics()` with `textCaret()` on a line.

`codeLines(tokens, tabSize?)` is the step before coloring. It splits `tokenizeCode()`’s tokens into the same lines, with an uncolored run for each stretch of adjacent tokens of one type: its `type`, tab-expanded `text`, and expanded start `column`. Use it when you need token types rather than merged colors.

## Limits

- Each non-empty line is one `Text` layer with color runs. Typing rasterizes the line being typed again for each new character; its shaping is cached. Keep snippets to what fits on screen.
- Kerning, ligatures, and combining characters are shaped across color boundaries; a shaped cluster takes its first character’s color and appears as a whole. A line wider than the GPU texture limit is rasterized at a smaller scale and can look softer.
- Proportional fonts are not a supported layout guarantee.
- Empty lines get no `Text` layer but keep their row.

Browser support is tracked in [issue 100](https://github.com/mika-f/Celesta/issues/100).
